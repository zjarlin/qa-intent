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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionSpec {
    pub key: String,
    pub description: String,
}

/// 编译结果：信封 + 阈值 + 动作，供应用层直接消费。
#[derive(Debug, Clone, Serialize)]
pub struct Compiled {
    pub qid: String,
    pub envelope: Envelope,
    pub threshold: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl Compiled {
    /// 判断模型返回的 answers[qid] 是否命中。
    pub fn is_hit(&self, answer: &crate::model::Answer) -> bool {
        match self.envelope.questions.get(&self.qid).map(|q| q.kind) {
            Some(crate::model::QuestionKind::Noul) => {
                answer.noul.map(|p| p >= self.threshold).unwrap_or(false)
            }
            Some(crate::model::QuestionKind::Choice) => answer.choice.is_some(),
            None => false,
        }
    }
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
    /// - 题型标签一并带上，供题型分类
    pub fn compile(&self, item: &QaItem) -> Result<Compiled> {
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
        if !item.tags.is_empty() {
            state.insert("tags".into(), json!(item.tags));
        }
        if let Some(options) = &item.options {
            state.insert("options".into(), json!(options));
        }
        let state = Value::Object(state);

        let envelope = if let Some(options) = &item.options {
            // 题库里带选项 → 动态生成 criteria，覆盖标签体系里的静态定义。
            let criteria: std::collections::BTreeMap<String, String> = options
                .iter()
                .map(|o| (o.key.clone(), o.description.clone()))
                .collect();
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
        Ok(Compiled {
            qid: item.qid.clone(),
            envelope,
            threshold: def.threshold,
            action: def.action.clone(),
        })
    }

    /// 批量编译，用于训练数据导出。
    pub fn compile_all(&self, items: &[QaItem]) -> Result<Vec<Compiled>> {
        items.iter().map(|i| self.compile(i)).collect()
    }
}
