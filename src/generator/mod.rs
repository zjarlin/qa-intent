//! 先固定意图体系，再按批次并发生成；全部校验通过后才返回完整题库。
pub mod client;
use crate::{
    bank::{normalized_question, BankItem, Generation, QuestionBank, SCHEMA_VERSION},
    model::QuestionKind,
    taxonomy::{QuestionDef, Taxonomy},
};
use anyhow::{ensure, Context, Result};
use client::GeneratorClient;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

pub struct Settings {
    pub scenario: String,
    pub context: String,
    pub count: usize,
    pub batch_size: usize,
    pub concurrency: usize,
    pub retries: usize,
    pub decision_model: String,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.scenario.trim().is_empty(), "场景不能为空");
        ensure!((1..=10000).contains(&self.count), "count 必须在 1..10000");
        ensure!(
            (1..=100).contains(&self.batch_size),
            "batch-size 必须在 1..100"
        );
        ensure!(
            (1..=16).contains(&self.concurrency),
            "concurrency 必须在 1..16"
        );
        ensure!(self.retries <= 3, "retries 必须在 0..3");
        ensure!(
            !self.decision_model.trim().is_empty(),
            "decision-model 不能为空"
        );
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    instructions: String,
    labels: Vec<Label>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Label {
    key: String,
    description: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Batch {
    items: Vec<Draft>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Draft {
    question: String,
    answer: String,
    intent: String,
    tags: Vec<String>,
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","additionalProperties":false,"properties":properties,"required":required})
}

fn plan_schema() -> Value {
    object(
        json!({"instructions":{"type":"string"},"labels":{"type":"array","items":object(json!({"key":{"type":"string"},"description":{"type":"string"}}), &["key","description"])}}),
        &["instructions", "labels"],
    )
}

fn batch_schema(taxonomy: &Taxonomy) -> Value {
    let labels = taxonomy.questions["intent_type"]
        .criteria
        .as_ref()
        .map(|c| c.keys().collect::<Vec<_>>())
        .unwrap_or_default();
    object(
        json!({"items":{"type":"array","items":object(json!({
        "question":{"type":"string"},"answer":{"type":"string"},
        "intent":{"type":"string","enum":labels},"tags":{"type":"array","items":{"type":"string"}}
    }), &["question","answer","intent","tags"])}}),
        &["items"],
    )
}

fn make_taxonomy(raw: &str, model: &str) -> Result<Taxonomy> {
    let plan: Plan = serde_json::from_str(raw).context("意图体系不是约定的 JSON 对象")?;
    ensure!(
        (2..=64).contains(&plan.labels.len()),
        "意图类别必须为 2..64 个"
    );
    let mut criteria = BTreeMap::new();
    for label in plan.labels {
        ensure!(
            !label.key.is_empty()
                && label
                    .key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "标签 key 必须是英文 snake_case"
        );
        ensure!(
            criteria.insert(label.key, label.description).is_none(),
            "意图体系有重复标签"
        );
    }
    let taxonomy = Taxonomy {
        version: "1.0.0".into(),
        model: model.into(),
        questions: BTreeMap::from([(
            "intent_type".into(),
            QuestionDef {
                kind: QuestionKind::Choice,
                instructions: plan.instructions,
                criteria: Some(criteria),
                threshold: 0.7,
                min_confidence: 0.7,
                action: None,
            },
        )]),
    };
    taxonomy.validate()?;
    Ok(taxonomy)
}

fn parse_batch(
    raw: &str,
    count: usize,
    offset: usize,
    taxonomy: &Taxonomy,
    previous: &[BankItem],
) -> Result<Vec<BankItem>> {
    let batch: Batch = serde_json::from_str(raw).context("问答不是约定的 JSON 对象")?;
    ensure!(
        batch.items.len() == count,
        "本批次必须正好包含 {count} 个问答"
    );
    let mut seen: HashSet<String> = previous
        .iter()
        .map(|i| normalized_question(&i.question))
        .collect();
    let criteria = taxonomy
        .get("intent_type")?
        .criteria
        .as_ref()
        .context("缺少意图候选")?;
    let mut items = Vec::with_capacity(count);
    for (index, draft) in batch.items.into_iter().enumerate() {
        ensure!(
            !draft.question.trim().is_empty() && !draft.answer.trim().is_empty(),
            "问题和答案不能为空"
        );
        ensure!(criteria.contains_key(&draft.intent), "生成了未知意图标签");
        ensure!(
            seen.insert(normalized_question(&draft.question)),
            "生成了重复问题，请改写成新的问题"
        );
        items.push(BankItem {
            id: format!("qa-{:06}", offset + index + 1),
            qid: "intent_type".into(),
            question: draft.question,
            answer: draft.answer,
            intent: draft.intent,
            tags: draft.tags,
            needs_review: true,
        });
    }
    Ok(items)
}

fn generate_batch(
    client: &GeneratorClient,
    settings: &Settings,
    taxonomy: &Taxonomy,
    offset: usize,
    previous: &[BankItem],
) -> Result<Vec<BankItem>> {
    let count = settings.batch_size.min(settings.count - offset);
    let avoid: Vec<_> = previous
        .iter()
        .rev()
        .take(100)
        .map(|i| &i.question)
        .collect();
    let mut task = json!({"stage":"items","scenario":settings.scenario,"context":settings.context,"count":count,"offset":offset,"taxonomy":taxonomy,"avoid_questions":avoid});
    for attempt in 0..=settings.retries {
        let text = client.complete(&task, &batch_schema(taxonomy))?;
        match parse_batch(&text, count, offset, taxonomy, previous) {
            Ok(items) => return Ok(items),
            Err(error) if attempt < settings.retries => {
                task["validation_error"] = json!(error.to_string())
            }
            Err(error) => {
                return Err(error).context(format!(
                    "第 {} 批生成失败",
                    offset / settings.batch_size + 1
                ))
            }
        }
    }
    anyhow::bail!("生成重试耗尽")
}

pub fn generate(client: &GeneratorClient, settings: &Settings) -> Result<QuestionBank> {
    settings.validate()?;
    let mut task =
        json!({"stage":"taxonomy","scenario":settings.scenario,"context":settings.context});
    let mut plan = None;
    for attempt in 0..=settings.retries {
        let text = client.complete(&task, &plan_schema())?;
        match make_taxonomy(&text, &settings.decision_model) {
            Ok(taxonomy) => {
                plan = Some(taxonomy);
                break;
            }
            Err(error) if attempt < settings.retries => {
                task["validation_error"] = json!(error.to_string())
            }
            Err(error) => return Err(error).context("生成意图体系失败"),
        }
    }
    let taxonomy = plan.context("缺少意图体系")?;
    let mut bank = QuestionBank {
        schema_version: SCHEMA_VERSION.into(),
        scenario: settings.scenario.clone(),
        generation: Generation {
            model: client.model.clone(),
            api: client.api.name().into(),
        },
        taxonomy,
        items: Vec::with_capacity(settings.count),
    };
    let offsets: Vec<_> = (0..settings.count).step_by(settings.batch_size).collect();
    for group in offsets.chunks(settings.concurrency) {
        // 共享只读体系与连接池；按题号收集结果，完成顺序不影响导出顺序。
        let batches = std::thread::scope(|scope| -> Result<Vec<_>> {
            let mut handles = Vec::new();
            for offset in group {
                let bank_ref = &bank;
                handles.push(scope.spawn(move || {
                    generate_batch(
                        client,
                        settings,
                        &bank_ref.taxonomy,
                        *offset,
                        &bank_ref.items,
                    )
                }));
            }
            handles
                .into_iter()
                .map(|h| h.join().map_err(|_| anyhow::anyhow!("生成线程异常"))?)
                .collect()
        })?;
        for (offset, mut items) in group.iter().zip(batches) {
            let seen: HashSet<_> = bank
                .items
                .iter()
                .map(|i| normalized_question(&i.question))
                .collect();
            if items
                .iter()
                .any(|i| seen.contains(&normalized_question(&i.question)))
            {
                ensure!(settings.retries > 0, "并发批次之间出现重复问题");
                items = generate_batch(client, settings, &bank.taxonomy, *offset, &bank.items)?;
            }
            bank.items.extend(items);
            eprintln!("已校验 {}/{} 条问答", bank.items.len(), settings.count);
        }
    }
    bank.validate()?;
    Ok(bank)
}

/// 离线提示词可交给任意文本模型，返回结果仍使用同一题库校验器。
pub fn prompt(settings: &Settings) -> Result<String> {
    settings.validate()?;
    let instructions = include_str!("../../prompts/generate.md");
    let count = settings.count;
    let scenario = serde_json::to_string_pretty(
        &json!({"scenario":settings.scenario,"context":settings.context}),
    )?;
    let mut example: Value =
        serde_json::from_str(include_str!("../../examples/customer-service-bank.json"))?;
    example["scenario"] = json!(settings.scenario);
    example["taxonomy"]["model"] = json!(settings.decision_model);
    let example = serde_json::to_string_pretty(&example)?;
    Ok(format!(
        r#"{instructions}
请生成完整题库 JSON，items 必须正好包含 {count} 条。
下面是结构示例，内容需替换为给定场景。
所有条目 needs_review 必须为 true；id 唯一；intent 必须属于 taxonomy 的 criteria；qid 固定 intent_type。
场景和业务资料：
{scenario}
完整题库结构示例：
{example}
"#
    ))
}
