//! qai — Laya/JEV 决策模型的题库编译器。

mod cli;
mod client;
mod compiler;
mod feedback;
mod model;
mod taxonomy;
mod validate;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command};
use compiler::{Compiler, QaItem};
use model::Answer;
use std::path::Path;
use taxonomy::Taxonomy;

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("错误：{err:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Command::Init { force } => cmd_init(&cli.taxonomy, *force),
        Command::Validate => cmd_validate(&cli.taxonomy),
        Command::Compile { input, raw } => cmd_compile(&cli.taxonomy, input, *raw),
        Command::Ask {
            input,
            model,
            endpoint,
            api_key,
            timeout,
            no_feedback,
        } => cmd_ask(
            &cli,
            input,
            model.as_deref(),
            endpoint.clone(),
            api_key.clone(),
            *timeout,
            *no_feedback,
        ),
        Command::Review { pending_only } => cmd_review(&cli.feedback, *pending_only),
    }
}

fn cmd_init(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        anyhow::bail!("{} 已存在，使用 --force 覆盖", path.display());
    }
    let taxonomy = Taxonomy {
        version: "1.0.0".into(),
        model: "laya".into(),
        questions: [
            (
                "intent_type".to_string(),
                taxonomy::QuestionDef {
                    kind: model::QuestionKind::Choice,
                    instructions: "这道题属于哪一类意图？".into(),
                    criteria: Some(
                        [
                            ("common_sense".to_string(), "常识推理题".to_string()),
                            ("calculation".to_string(), "数值计算题".to_string()),
                            ("lookup".to_string(), "事实查询题".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    threshold: 0.5,
                    action: Some("route_to_solver".into()),
                },
            ),
            (
                "equivalent".to_string(),
                taxonomy::QuestionDef {
                    kind: model::QuestionKind::Noul,
                    instructions: "用户的回答与参考答案在语义上是否等价？".into(),
                    criteria: None,
                    threshold: 0.7,
                    action: Some("mark_correct".into()),
                },
            ),
        ]
        .into_iter()
        .collect(),
    };
    taxonomy.validate()?;
    taxonomy.save(path)?;
    println!("已生成标签体系：{}", path.display());
    Ok(())
}

fn cmd_validate(path: &Path) -> Result<()> {
    let taxonomy = Taxonomy::load(path)?;
    println!(
        "标签体系合法：version={} model={} qid 数量={}",
        taxonomy.version,
        taxonomy.model,
        taxonomy.questions.len()
    );
    for (qid, def) in &taxonomy.questions {
        let kind = match def.kind {
            model::QuestionKind::Noul => "noul",
            model::QuestionKind::Choice => "choice",
        };
        let extra = def
            .criteria
            .as_ref()
            .map(|c| format!("，候选 {} 个", c.len()))
            .unwrap_or_default();
        println!("  - {qid} [{kind}] 阈值={}{extra}", def.threshold);
    }
    Ok(())
}

fn load_items(path: &Path) -> Result<Vec<QaItem>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("读取题库失败：{}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&raw).with_context(|| format!("解析题库失败：{}", path.display()))?;
    match value {
        serde_json::Value::Array(_) => Ok(serde_json::from_value(value)?),
        serde_json::Value::Object(_) => Ok(vec![serde_json::from_value(value)?]),
        _ => anyhow::bail!("题库必须是对象或对象数组：{}", path.display()),
    }
}

fn cmd_compile(taxonomy_path: &Path, input: &Path, raw: bool) -> Result<()> {
    let taxonomy = Taxonomy::load(taxonomy_path)?;
    let compiler = Compiler::new(&taxonomy);
    let items = load_items(input)?;
    let compiled = compiler.compile_all(&items)?;

    if raw {
        for c in &compiled {
            println!("{}", c.envelope.to_json()?);
        }
        return Ok(());
    }

    for c in &compiled {
        println!("qid: {}", c.qid);
        println!(
            "阈值: {}  动作: {}",
            c.threshold,
            c.action.as_deref().unwrap_or("-")
        );
        println!("{}", c.envelope.to_json_pretty()?);
        println!();
    }
    Ok(())
}

fn cmd_ask(
    cli: &Cli,
    input: &Path,
    model: Option<&str>,
    endpoint: Option<String>,
    api_key: Option<String>,
    timeout: u64,
    no_feedback: bool,
) -> Result<()> {
    let mut taxonomy = Taxonomy::load(&cli.taxonomy)?;
    if let Some(m) = model {
        taxonomy.model = m.to_string();
    }
    let compiler = Compiler::new(&taxonomy);
    let items = load_items(input)?;
    let compiled = compiler.compile_all(&items)?;
    let client = client::Client::new(endpoint, api_key, timeout)?;

    for (item, c) in items.iter().zip(compiled.iter()) {
        let response = client.predict(&c.envelope)?;
        let raw_answer = response
            .answers
            .get(&c.qid)
            .ok_or_else(|| anyhow::anyhow!("返回体缺少 answers.{}", c.qid))?;
        let answer: Answer = serde_json::from_value(raw_answer.clone())
            .with_context(|| format!("解析 answers.{} 失败", c.qid))?;

        let hit = c.is_hit(&answer);
        println!("题目: {}  qid: {}", item.id, c.qid);
        if let Some(p) = answer.noul {
            println!(
                "  概率: {p:.4}  阈值: {}  命中: {}",
                c.threshold,
                yes_no(hit)
            );
        }
        if let Some(choice) = &answer.choice {
            println!("  选择: {choice}  命中: {}", yes_no(hit));
        }
        if let Some(conf) = answer.answer_confidence.or(answer.confidence) {
            println!("  置信度: {conf:.4}");
        }
        if c.action.is_some() {
            println!("  动作: {}", c.action.as_deref().unwrap());
        }
        if let Some(m) = &response.model {
            println!("  实际模型: {m}");
        }
        if let Some(tokens) = response.usage.input_tokens {
            // Laya 不生成 token，output_tokens 恒为 0，这是正常现象。
            let out = response.usage.output_tokens.unwrap_or(0);
            println!("  输入 token: {tokens}  输出 token: {out}");
        }

        if !no_feedback {
            let record = feedback::FeedbackRecord::from_answer(&item.id, c, &answer, None);
            feedback::append(&cli.feedback, &record)?;
        }
    }
    Ok(())
}

fn cmd_review(path: &Path, pending_only: bool) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("回流文件不存在：{}", path.display());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("读取回流文件失败：{}", path.display()))?;
    let mut total = 0usize;
    let mut shown = 0usize;
    for (idx, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        total += 1;
        let record: feedback::FeedbackRecord =
            serde_json::from_str(line).with_context(|| format!("解析第 {} 行失败", idx + 1))?;
        if pending_only && !record.needs_review {
            continue;
        }
        shown += 1;
        println!(
            "[{}] item={} qid={} hit={} 置信度={} 时间={}",
            idx + 1,
            record.item_id,
            record.qid,
            record.hit,
            record
                .confidence
                .map(|c| format!("{c:.4}"))
                .unwrap_or_else(|| "-".into()),
            record.recorded_at
        );
    }
    println!("共 {total} 条，显示 {shown} 条");
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "是"
    } else {
        "否"
    }
}
