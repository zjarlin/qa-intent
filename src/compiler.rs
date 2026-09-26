//! QA 题目 → Laya 信封 的编译器。
//!
//! 这是「应用层植入题库」的唯一入口：题目、选项、用户作答都从题库读出，
//! 组装成 {state, questions}，再交给决策模型。业务侧不应手写 JSON。

use crate::model::Envelope;
use crate::taxonomy::Taxonomy;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 题库里的一道题。字段刻意贴近常见 QA 表结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaItem {
    pub id: String,
    pub question: String,
    /// 该题要触发的 qid，必须在标签体系里存在。
    pub qid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<OptionSpec>>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// 人工标签，仅供评测和回流，不注入推理 state。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionSpec {
    pub key: String,
    pub description: String,
}

/// 编译结果：信封 + 阈值 + 动作，供应用层直接消费。
#[derive(Debug, Clone, Serialize)]
pub struct Compiled {
    pub taxonomy_version: String,
    pub qid: String,
    pub envelope: Envelope,
    pub threshold: f64,
    pub min_confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl Compiled {
    /// 判断模型返回的 answers[qid] 是否命中。
    pub fn decide(&self, answer: &crate::model::Answer) -> Result<Decision> {
        use crate::model::QuestionKind;
        let question = self
            .envelope
            .questions
            .get(&self.qid)
            .ok_or_else(|| anyhow::anyhow!("信封缺少 qid：{}", self.qid))?;
        anyhow::ensure!(question.kind == answer.kind, "返回题型与请求不一致");
        let check = |p: f64| -> Result<f64> {
            anyhow::ensure!(p.is_finite() && (0.0..=1.0).contains(&p), "非法概率：{p}");
            Ok(p)
        };
        if let Some(p) = answer.answer_confidence {
            check(p)?;
        }
        if let Some(p) = answer.confidence {
            check(p)?;
        }
        let (prediction, confidence) = match question.kind {
            QuestionKind::Noul => {
                let p = check(
                    answer
                        .noul
                        .ok_or_else(|| anyhow::anyhow!("缺少 noul 概率"))?,
                )?;
                let yes = p >= self.threshold;
                // 非对称阈值下，所判标签的概率不能直接用 max(p, 1-p)。
                (Value::Bool(yes), Some(if yes { p } else { 1.0 - p }))
            }
            QuestionKind::Choice => {
                let label = answer
                    .choice
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("缺少 choice"))?;
                let criteria = question
                    .criteria
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("缺少候选"))?;
                anyhow::ensure!(criteria.contains_key(label), "模型返回未知候选：{label}");
                for (key, probability) in &answer.probabilities {
                    anyhow::ensure!(criteria.contains_key(key), "概率表包含未知候选：{key}");
                    check(*probability)?;
                }
                // choice.confidence 是熵指标，不能替代所选答案的概率。
                let probability = answer
                    .probabilities
                    .get(label)
                    .copied()
                    .or(answer.answer_confidence);
                (Value::String(label.clone()), probability)
            }
        };
        let gate = if question.kind == QuestionKind::Choice {
            self.threshold.max(self.min_confidence)
        } else {
            self.min_confidence
        };
        let accepted = confidence.is_some_and(|p| p >= gate);
        let hit = accepted && prediction != Value::Bool(false);
        Ok(Decision {
            prediction,
            confidence,
            accepted,
            hit,
            action: if hit { self.action.clone() } else { None },
        })
    }
}

/// 应用只在 accepted 时消费 prediction；action 是输出建议，不直接执行。
#[derive(Debug, Clone, Serialize)]
pub struct Decision {
    pub prediction: Value,
    pub confidence: Option<f64>,
    pub accepted: bool,
    pub hit: bool,
    pub action: Option<String>,
}

pub struct Compiler<'a> {
    taxonomy: &'a Taxonomy,
}

impl<'a> Compiler<'a> {
    pub fn new(taxonomy: &'a Taxonomy) -> Self {
        Self { taxonomy }
    }

    /// 把一道 QA 题编译成 Laya 信封。
    ///
    /// state 的组装规则：
    /// - 始终带上题目原文
    /// - 有参考答案 / 用户作答时一并带上，供等价判定
    /// - 人工标签只用于评测，不注入输入材料
    pub fn compile(&self, item: &QaItem) -> Result<Compiled> {
        anyhow::ensure!(
            !item.id.trim().is_empty() && !item.question.trim().is_empty(),
            "id 与 question 不能为空"
        );
        let def = self.taxonomy.get(&item.qid)?;

        // 若标签体系是 choice，但题库自带选项，则以题库选项为准覆盖 criteria，
        // 这是运行时植入题库的关键：选项变化不需要重新训练。
        let mut state = serde_json::Map::new();
        state.insert("question".into(), Value::String(item.question.clone()));
        if let Some(reference) = &item.reference_answer {
            state.insert("reference_answer".into(), Value::String(reference.clone()));
        }
        if let Some(user) = &item.user_answer {
            state.insert("user_answer".into(), Value::String(user.clone()));
        }
        if let Some(options) = &item.options {
            state.insert("options".into(), json!(options));
        }
        let state = Value::Object(state);

        let envelope = if let Some(options) = &item.options {
            anyhow::ensure!(
                def.kind == crate::model::QuestionKind::Choice,
                "noul 不能由 options 隐式变成 choice"
            );
            // 题库里带选项 → 动态生成 criteria，覆盖标签体系里的静态定义。
            let criteria: std::collections::BTreeMap<String, String> = options
                .iter()
                .map(|o| (o.key.clone(), o.description.clone()))
                .collect();
            anyhow::ensure!(criteria.len() == options.len(), "options 存在重复 key");
            crate::validate::validate_criteria(&criteria)?;
            let (qid, question) = crate::model::choice(&item.qid, &def.instructions, criteria)?;
            let mut questions = std::collections::BTreeMap::new();
            questions.insert(qid, question);
            Envelope {
                model: self.taxonomy.model.clone(),
                state,
                questions,
            }
        } else {
            self.taxonomy.envelope_for(&item.qid, state)?
        };

        crate::validate::validate_envelope(&envelope)?;
        if let Some(expected) = &item.expected {
            match def.kind {
                crate::model::QuestionKind::Noul => {
                    anyhow::ensure!(expected.is_boolean(), "noul 的 expected 必须是布尔值")
                }
                crate::model::QuestionKind::Choice => {
                    let key = expected
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("choice 的 expected 必须是标签字符串"))?;
                    anyhow::ensure!(
                        envelope.questions[&item.qid]
                            .criteria
                            .as_ref()
                            .is_some_and(|c| c.contains_key(key)),
                        "expected 不是有效候选"
                    );
                }
            }
        }
        Ok(Compiled {
            taxonomy_version: self.taxonomy.version.clone(),
            qid: item.qid.clone(),
            envelope,
            threshold: def.threshold,
            min_confidence: def.min_confidence,
            action: def.action.clone(),
        })
    }

    /// 应用层小批量调用；大题库使用 dataset 的流式入口。
    pub fn compile_all(&self, items: &[QaItem]) -> Result<Vec<Compiled>> {
        items.iter().map(|i| self.compile(i)).collect()
    }
}
