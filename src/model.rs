//! Laya/JEV System One 协议的数据模型。
//!
//! 这里刻意保持「协议骨架」最小：请求体只有 state + questions，
//! questions 的每个 qid 只允许 noul / choice 两种类型。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// 问题类型。Laya 只支持这两种，不允许扩展。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QuestionKind {
    /// 是否类：模型给出「是」的概率。
    Noul,
    /// 多选一：必须在 criteria 里穷举候选。
    Choice,
}

/// 单个问题。criteria 只在 choice 时出现。
#[derive(Debug, Clone, Serialize)]
pub struct Question {
    #[serde(rename = "type")]
    pub kind: QuestionKind,
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub criteria: Option<BTreeMap<String, String>>,
}

/// 完整的 System One 请求体。
#[derive(Debug, Clone, Serialize)]
pub struct Envelope {
    pub model: String,
    pub state: Value,
    pub questions: BTreeMap<String, Question>,
}

impl Envelope {
    pub fn to_json_pretty(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

/// 返回体里 answers[qid] 的形状。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Answer {
    #[serde(rename = "type")]
    pub kind: QuestionKind,
    #[serde(default)]
    pub noul: Option<f64>,
    #[serde(default)]
    pub choice: Option<String>,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub answer_confidence: Option<f64>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: Option<u64>,
    #[serde(default)]
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionResponse {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub answers: Map<String, Value>,
    #[serde(default)]
    pub usage: Usage,
}

/// 编译一个 noul 问题。
pub fn noul(qid: &str, instructions: &str) -> anyhow::Result<(String, Question)> {
    crate::validate::validate_qid(qid)?;
    if instructions.trim().is_empty() {
        anyhow::bail!("instructions 不能为空（qid = {qid}）");
    }
    Ok((
        qid.to_string(),
        Question {
            kind: QuestionKind::Noul,
            instructions: instructions.trim().to_string(),
            criteria: None,
        },
    ))
}

/// 编译一个 choice 问题，并校验候选集合非空。
pub fn choice(
    qid: &str,
    instructions: &str,
    criteria: BTreeMap<String, String>,
) -> anyhow::Result<(String, Question)> {
    crate::validate::validate_qid(qid)?;
    if instructions.trim().is_empty() {
        anyhow::bail!("instructions 不能为空（qid = {qid}）");
    }
    if criteria.is_empty() {
        anyhow::bail!("choice 类型必须提供非空 criteria（qid = {qid}）");
    }
    Ok((
        qid.to_string(),
        Question {
            kind: QuestionKind::Choice,
            instructions: instructions.trim().to_string(),
            criteria: Some(criteria),
        },
    ))
}
