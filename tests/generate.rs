use anyhow::{Context, Result};
use qa_intent::{
    bank::{self, ExportFormat, QuestionBank},
    generator::client::{extract_text, Api},
};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn mock(
    api: Api,
    requests: usize,
    invalid_first_batch: bool,
) -> Result<(String, thread::JoinHandle<Result<Vec<Value>>>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    listener.set_nonblocking(true)?;
    let url = format!("http://{}/v1", listener.local_addr()?);
    let handle = thread::spawn(move || -> Result<Vec<Value>> {
        let mut captured = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut invalid_sent = false;
        while captured.len() < requests {
            let (mut stream, _) = match listener.accept() {
                Ok(stream) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    anyhow::ensure!(Instant::now() < deadline, "mock 等待请求超时");
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut reader = BufReader::new(stream.try_clone()?);
            let mut len = 0usize;
            loop {
                let mut line = String::new();
                anyhow::ensure!(reader.read_line(&mut line)? > 0, "请求意外结束");
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                    len = value.trim().parse()?;
                }
            }
            let mut bytes = vec![0; len];
            reader.read_exact(&mut bytes)?;
            let body: Value = serde_json::from_slice(&bytes)?;
            let input = match api {
                Api::Responses => &body["input"],
                Api::Chat => &body["messages"][1]["content"],
            };
            let user: Value = serde_json::from_str(input.as_str().context("缺少提示词")?)?;
            let task = &user["task"];
            let content = if task["stage"] == "taxonomy" {
                json!({"instructions":"识别客服需求","labels":[{"key":"refund","description":"退款"},{"key":"other","description":"其他"}]}).to_string()
            } else if invalid_first_batch && !invalid_sent {
                invalid_sent = true;
                "not valid JSON".into()
            } else {
                let offset = task["offset"].as_u64().context("缺少 offset")?;
                let count = task["count"].as_u64().context("缺少 count")?;
                json!({"items":(0..count).map(|i| json!({"question":format!("客服测试问题{}",offset+i),"answer":"参考资料中的客服答复","intent":"refund","tags":["测试"]})).collect::<Vec<_>> ()}).to_string()
            };
            let reply = match api {
                Api::Responses => json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":content}]}]}),
                Api::Chat => json!({"choices":[{"finish_reason":"stop","message":{"content":content}}]}),
            }.to_string();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len())?;
            captured.push(body);
        }
        Ok(captured)
    });
    Ok((url, handle))
}

#[test]
fn generate_batches_with_fixed_labels_and_export_without_answer_leakage() -> Result<()> {
    let (url, server) = mock(Api::Responses, 3, false)?;
    let directory = tempfile::tempdir()?;
    let context = directory.path().join("rules.txt");
    std::fs::write(&context, "未发货可以退款")?;
    let output = Command::new(env!("CARGO_BIN_EXE_qai"))
        .args([
            "generate",
            "电商客服",
            "--model",
            "test-model",
            "--base-url",
            &url,
            "--count",
            "3",
            "--batch-size",
            "2",
            "--concurrency",
            "2",
            "--context",
        ])
        .arg(context)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bank: QuestionBank = serde_json::from_slice(&output.stdout)?;
    bank.validate()?;
    assert_eq!(bank.items.len(), 3);
    assert_eq!(bank.items[2].id, "qa-000003");
    assert!(bank.items.iter().all(|item| item.needs_review));
    let requests = server
        .join()
        .map_err(|_| anyhow::anyhow!("mock failed"))??;
    assert!(requests
        .iter()
        .all(|body| body["text"]["format"]["type"] == "json_schema"));
    for request in &requests[1..] {
        let input: Value = serde_json::from_str(request["input"].as_str().context("input")?)?;
        assert_eq!(input["task"]["context"], "未发货可以退款");
        assert_eq!(
            input["task"]["taxonomy"]["questions"]["intent_type"]["criteria"]["refund"],
            "退款"
        );
    }
    let mut systemone = Vec::new();
    bank::export(&bank, ExportFormat::Systemone, &mut systemone)?;
    for row in String::from_utf8(systemone)?.lines() {
        let request: Value = serde_json::from_str(row)?;
        assert_eq!(request["state"].as_object().context("state")?.len(), 1);
        assert!(request["state"].get("answer").is_none());
        assert!(request["state"].get("intent").is_none());
    }
    let mut training = Vec::new();
    bank::export(&bank, ExportFormat::Training, &mut training)?;
    let row: Value =
        serde_json::from_str(String::from_utf8(training)?.lines().next().context("row")?)?;
    assert_eq!(row["label_source"], "model_generated");
    assert_eq!(row["target"], "refund");
    assert!(bank.items[0].as_qa_item().expected.is_none());
    Ok(())
}

#[test]
fn chat_json_mode_repairs_invalid_batch_and_writes_file() -> Result<()> {
    let (url, server) = mock(Api::Chat, 3, true)?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("bank.json");
    let output = Command::new(env!("CARGO_BIN_EXE_qai"))
        .args([
            "generate",
            "售后客服",
            "--api",
            "chat",
            "--json-mode",
            "--model",
            "test",
            "--base-url",
            &url,
            "--count",
            "1",
            "--output",
        ])
        .arg(&path)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    QuestionBank::load(&path)?;
    let requests = server
        .join()
        .map_err(|_| anyhow::anyhow!("mock failed"))??;
    assert_eq!(requests[0]["response_format"]["type"], "json_object");
    let repair: Value = serde_json::from_str(
        requests[2]["messages"][1]["content"]
            .as_str()
            .context("prompt")?,
    )?;
    assert!(repair["task"]["validation_error"].is_string());
    let again = Command::new(env!("CARGO_BIN_EXE_qai"))
        .args(["generate", "另一个场景", "--output"])
        .arg(&path)
        .output()?;
    assert!(!again.status.success());
    assert_eq!(QuestionBank::load(&path)?.scenario, "售后客服");
    Ok(())
}

#[test]
fn exhausted_generation_does_not_leave_partial_output() -> Result<()> {
    let (url, server) = mock(Api::Chat, 2, true)?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("bank.json");
    let output = Command::new(env!("CARGO_BIN_EXE_qai"))
        .args([
            "generate",
            "客服",
            "--api",
            "chat",
            "--model",
            "test",
            "--base-url",
            &url,
            "--count",
            "1",
            "--retries",
            "0",
            "--output",
        ])
        .arg(&path)
        .output()?;
    assert!(!output.status.success());
    assert!(!path.exists());
    assert!(output.stdout.is_empty());
    server
        .join()
        .map_err(|_| anyhow::anyhow!("mock failed"))??;
    Ok(())
}

#[test]
fn bank_validation_csv_and_manual_prompt() -> Result<()> {
    let mut bank: QuestionBank =
        serde_json::from_str(include_str!("../examples/customer-service-bank.json"))?;
    bank.items[0].question = "=1+1".into();
    bank.items[0].answer = "包含,逗号和\"引号\"\n换行".into();
    let mut bytes = Vec::new();
    bank::export(&bank, ExportFormat::FaqCsv, &mut bytes)?;
    assert!(bytes.starts_with(b"\xef\xbb\xbf"));
    let mut csv = csv::Reader::from_reader(&bytes[3..]);
    let record = csv.records().next().context("missing row")??;
    assert_eq!(&record[1], "'=1+1");
    assert_eq!(&record[2], bank.items[0].answer);
    bank.items[0].needs_review = false;
    assert_eq!(bank.items[0].as_qa_item().expected, Some(json!("refund")));
    bank.items[1].question = " =1+1 ".into();
    assert!(bank.validate().is_err());
    let prompt = Command::new(env!("CARGO_BIN_EXE_qai"))
        .env_remove("QAI_GENERATOR_MODEL")
        .args(["generate", "客服场景", "--prompt-only", "--count", "10"])
        .output()?;
    assert!(prompt.status.success());
    let text = String::from_utf8(prompt.stdout)?;
    assert!(text.contains("10 条"));
    assert!(text.contains("qa-intent.bank.v1"));
    Ok(())
}

#[test]
fn refuses_truncated_or_refused_responses_and_hides_generator_key() -> Result<()> {
    assert!(extract_text(&json!({"status":"incomplete"}), Api::Responses).is_err());
    assert!(extract_text(
        &json!({"status":"completed","output":[{"content":[{"type":"refusal"}]}]}),
        Api::Responses
    )
    .is_err());
    assert!(extract_text(
        &json!({"choices":[{"finish_reason":"length","message":{"content":"{}"}}]}),
        Api::Chat
    )
    .is_err());
    let output = Command::new(env!("CARGO_BIN_EXE_qai"))
        .env("QAI_GENERATOR_API_KEY", "secret-generator-sentinel")
        .args(["generate", "--help"])
        .output()?;
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret-generator-sentinel"));
    Ok(())
}

#[test]
fn export_accepts_bank_from_stdin_for_language_integration() -> Result<()> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_qai"))
        .args(["export", "-i", "-", "--format", "faq-json"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().context("缺少 stdin")?;
    input.write_all(include_bytes!("../examples/customer-service-bank.json"))?;
    drop(input);
    let output = child.wait_with_output()?;
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout)?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["intent"], "refund");
    assert!(rows[0]["answer"].as_str().is_some());
    Ok(())
}
