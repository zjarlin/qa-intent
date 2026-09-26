use anyhow::Result;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    process::Command,
    thread,
};

#[test]
fn ask_records_actual_model_prediction_and_correct_choice_feedback() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}/v1/systemone", listener.local_addr()?);
    let server = thread::spawn(move || -> Result<Value> {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut len = 0usize;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                len = value.trim().parse()?;
            }
        }
        let mut body = vec![0; len];
        reader.read_exact(&mut body)?;
        let payload = json!({
            "model":"actual-laya", "answers":{"intent_type":{
                "type":"choice","choice":"lookup","answer_confidence":0.92,"confidence":0.3
            }}, "usage":{"input_tokens":12,"output_tokens":0}
        })
        .to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}", payload.len())?;
        Ok(serde_json::from_slice(&body)?)
    });
    let dir = std::env::temp_dir().join(format!("qai-http-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let item = dir.join("item.json");
    let feedback = dir.join("feedback.jsonl");
    std::fs::write(
        &item,
        json!({"id":"ticket","qid":"intent_type","question":"查询账单","expected":"lookup"})
            .to_string(),
    )?;
    let taxonomy = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/taxonomy.json");
    let out = Command::new(env!("CARGO_BIN_EXE_qai"))
        .arg("-t")
        .arg(taxonomy)
        .arg("--feedback")
        .arg(&feedback)
        .args(["ask", "--endpoint", &endpoint, "--timeout", "5", "-i"])
        .arg(item)
        .output()?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sent = server
        .join()
        .map_err(|_| anyhow::anyhow!("mock server failed"))??;
    assert!(sent["state"].get("expected").is_none());
    let result: Value = serde_json::from_slice(&out.stdout)?;
    assert_eq!(result["model"], "actual-laya");
    assert_eq!(result["decision"]["prediction"], "lookup");
    assert_eq!(result["needs_review"], false);
    let record: Value = serde_json::from_str(
        std::fs::read_to_string(&feedback)?
            .lines()
            .last()
            .ok_or_else(|| anyhow::anyhow!("missing feedback"))?,
    )?;
    assert_eq!(record["expected"], "lookup");
    assert_eq!(record["model"], "actual-laya");
    let report = Command::new(env!("CARGO_BIN_EXE_qai"))
        .arg("--feedback")
        .arg(feedback)
        .arg("evaluate")
        .output()?;
    assert!(report.status.success());
    let report: Value = serde_json::from_slice(&report.stdout)?;
    assert_eq!(report["actual-laya:1.0.0:intent_type"]["accuracy"], 1.0);
    Ok(())
}

#[test]
fn help_never_prints_api_key_env() -> Result<()> {
    let out = Command::new(env!("CARGO_BIN_EXE_qai"))
        .env("CODEX_GROUP_KEY", "secret-sentinel")
        .args(["ask", "--help"])
        .output()?;
    assert!(out.status.success());
    assert!(!String::from_utf8_lossy(&out.stdout).contains("secret-sentinel"));
    Ok(())
}
