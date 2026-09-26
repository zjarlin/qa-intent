//! 编译契约的集成测试：qid 校验、criteria 生成、信封合法性。

use serde_json::json;
use std::collections::BTreeMap;
use std::process::Command;

fn bin() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_qai"))
}

fn tmpdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("qai-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn init_then_validate_roundtrip() {
    let dir = tmpdir("init");
    let taxonomy = dir.join("taxonomy.json");
    let out = Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("init")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "init 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("validate")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "validate 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("intent_type"));
    assert!(stdout.contains("equivalent"));
}

#[test]
fn init_refuses_overwrite_without_force() {
    let dir = tmpdir("force");
    let taxonomy = dir.join("taxonomy.json");
    Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("init")
        .output()
        .unwrap();
    let out = Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("init")
        .output()
        .unwrap();
    assert!(!out.status.success(), "重复 init 应失败");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--force"));
}

#[test]
fn compile_noul_uses_taxonomy_question() {
    let dir = tmpdir("noul");
    let taxonomy = dir.join("taxonomy.json");
    Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("init")
        .output()
        .unwrap();

    // 写一份自定义标签体系：只留 noul 的 equivalent。
    let mut questions = BTreeMap::new();
    questions.insert(
        "equivalent".to_string(),
        json!({
            "type": "noul",
            "instructions": "用户的回答与参考答案在语义上是否等价？",
            "threshold": 0.7
        }),
    );
    std::fs::write(
        &taxonomy,
        serde_json::to_string_pretty(&json!({
            "version": "1.0.0", "model": "laya", "questions": questions
        }))
        .unwrap(),
    )
    .unwrap();

    let item = dir.join("item.json");
    std::fs::write(
        &item,
        serde_json::to_string(&json!({
            "id": "bird-01",
            "question": "树上7个鸟，开了一枪还剩几个鸟？",
            "qid": "equivalent",
            "reference_answer": "0个，枪声吓飞其他鸟",
            "user_answer": "都飞走了所以是0"
        }))
        .unwrap(),
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .args(["--feedback"])
        .arg(dir.join("feedback.jsonl"))
        .args(["compile", "-i"])
        .arg(&item)
        .arg("--raw")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(envelope["model"], "laya");
    assert_eq!(envelope["questions"]["equivalent"]["type"], "noul");
    assert!(envelope["questions"]["equivalent"]
        .get("criteria")
        .is_none());
    assert_eq!(
        envelope["state"]["question"],
        "树上7个鸟，开了一枪还剩几个鸟？"
    );
    assert_eq!(envelope["state"]["user_answer"], "都飞走了所以是0");
}

#[test]
fn compile_choice_generates_criteria_from_options() {
    let dir = tmpdir("choice");
    let taxonomy = dir.join("taxonomy.json");
    Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("init")
        .output()
        .unwrap();

    let item = dir.join("item.json");
    std::fs::write(
        &item,
        serde_json::to_string(&json!({
            "id": "bird-01",
            "question": "树上7个鸟，开了一枪还剩几个鸟？",
            "qid": "intent_type",
            "options": [
                {"key": "common_sense", "description": "需要常识推理"},
                {"key": "calculation", "description": "纯数值计算"}
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .args(["compile", "-i"])
        .arg(&item)
        .arg("--raw")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "compile 失败：{}",
        String::from_utf8_lossy(&out.stderr)
    );

    let envelope: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let q = &envelope["questions"]["intent_type"];
    assert_eq!(q["type"], "choice");
    // 题库选项覆盖了标签体系里的静态 criteria。
    assert_eq!(q["criteria"]["common_sense"], "需要常识推理");
    assert_eq!(q["criteria"]["calculation"], "纯数值计算");
}

#[test]
fn compile_rejects_unknown_qid() {
    let dir = tmpdir("badqid");
    let taxonomy = dir.join("taxonomy.json");
    Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .arg("init")
        .output()
        .unwrap();

    let item = dir.join("item.json");
    std::fs::write(
        &item,
        serde_json::to_string(&json!({
            "id": "x", "question": "?", "qid": "does_not_exist"
        }))
        .unwrap(),
    )
    .unwrap();

    let out = Command::new(bin())
        .args(["-t"])
        .arg(&taxonomy)
        .args(["compile", "-i"])
        .arg(&item)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does_not_exist"));
}
