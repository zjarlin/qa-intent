//! 标签体系（taxonomy）。
//!
//! 题库里每道题引用一个 qid；qid 的定义（类型、问句、候选、阈值）集中在这里，
//! 而不是散落在各处手写 JSON。这是运行时植入与训练的公共契约。

use crate::model::{self, Envelope, QuestionKind};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionDef {
    #[serde(rename = "type")]
    pub kind: QuestionKind,
    pub instructions: String,
    /// choice 必填；noul 忽略。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub criteria: Option<BTreeMap<String, String>>,
    /// 判定阈值，仅 noul 有意义。默认 0.5。
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    /// 命中后要触发的业务动作，供应用层读取。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

fn default_threshold() -> f64 {
    0.5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Taxonomy {
    pub version: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub questions: BTreeMap<String, QuestionDef>,
}

fn default_model() -> String {
    "laya".to_string()
}

impl Taxonomy {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("读取标签体系失败：{}", path.display()))?;
        let taxonomy: Taxonomy = serde_json::from_str(&raw)
            .with_context(|| format!("解析标签体系失败：{}", path.display()))?;
        taxonomy.validate()?;
        Ok(taxonomy)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw + "\n")
            .with_context(|| format!("写入标签体系失败：{}", path.display()))?;
        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.questions.is_empty() {
            anyhow::bail!("标签体系里没有任何 qid");
        }
        for (qid, def) in &self.questions {
            crate::validate::validate_qid(qid)?;
            crate::validate::validate_threshold(def.threshold)?;
            if def.instructions.trim().is_empty() {
                anyhow::bail!("qid {qid} 的 instructions 为空");
            }
            match def.kind {
                QuestionKind::Noul => {
                    if def.criteria.is_some() {
                        anyhow::bail!("qid {qid} 是 noul，不应有 criteria");
                    }
                }
                QuestionKind::Choice => {
                    let criteria = def
                        .criteria
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("qid {qid} 是 choice，必须提供 criteria"))?;
                    if criteria.is_empty() {
                        anyhow::bail!("qid {qid} 的 criteria 为空");
                    }
                }
            }
        }
        Ok(())
    }

    /// 取出某个 qid 的定义。
    pub fn get(&self, qid: &str) -> Result<&QuestionDef> {
        self.questions
            .get(qid)
            .ok_or_else(|| anyhow::anyhow!("标签体系里不存在 qid：{qid}"))
    }

    /// 按标签体系构造某道题的信封；criteria 来自体系，保证训练/推理对齐。
    pub fn envelope_for(&self, qid: &str, state: serde_json::Value) -> Result<Envelope> {
        let def = self.get(qid)?;
        let question = match def.kind {
            QuestionKind::Noul => model::noul(qid, &def.instructions)?.1,
            QuestionKind::Choice => {
                model::choice(
                    qid,
                    &def.instructions,
                    def.criteria.clone().unwrap_or_default(),
                )?
                .1
            }
        };
        let mut questions = BTreeMap::new();
        questions.insert(qid.to_string(), question);
        let envelope = Envelope {
            model: self.model.clone(),
            state,
            questions,
        };
        crate::validate::validate_envelope(&envelope)?;
        Ok(envelope)
    }
}
