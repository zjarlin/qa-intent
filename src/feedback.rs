//! 回流机制：记录真实调用结果与低置信/错判样本，作为下一轮训练种子。
//!
//! 契约固定为 JSONL：每行一条记录，append-only，便于后续离线分析。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackRecord {
    /// 题目 id，对应题库主键。
    pub item_id: String,
    pub qid: String,
    pub model: String,
    pub taxonomy_version: String,
    pub state: serde_json::Value,
    pub prediction: serde_json::Value,
    pub accepted: bool,
    /// 模型给出的原始 answers[qid]。
    pub answer: serde_json::Value,
    /// 应用层最终判定结果（业务真值）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<serde_json::Value>,
    /// 应用层按阈值得到的结果。
    pub hit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// 是否属于需要回流训练的低置信/错判样本。
    pub needs_review: bool,
    pub recorded_at: String,
}

impl FeedbackRecord {
    pub fn from_answer(
        item_id: &str,
        compiled: &crate::compiler::Compiled,
        answer: serde_json::Value,
        decision: &crate::compiler::Decision,
        actual_model: &str,
        expected: Option<serde_json::Value>,
    ) -> Self {
        let hit = decision.hit;
        let confidence = decision.confidence;
        // 低置信、或与业务真值不一致，都进回流队列。
        let mismatched = expected
            .as_ref()
            .is_some_and(|exp| *exp != decision.prediction);
        Self {
            item_id: item_id.to_string(),
            qid: compiled.qid.clone(),
            model: actual_model.to_string(),
            taxonomy_version: compiled.taxonomy_version.clone(),
            state: compiled.envelope.state.clone(),
            prediction: decision.prediction.clone(),
            accepted: decision.accepted,
            answer,
            expected,
            hit,
            confidence,
            needs_review: mismatched || !decision.accepted,
            recorded_at: now_rfc3339(),
        }
    }
}

/// 追加一条回流记录到 JSONL 文件。
pub fn append(path: &Path, record: &FeedbackRecord) -> Result<()> {
    let mut line = serde_json::to_string(record)?;
    line.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("打开回流文件失败：{}", path.display()))?;
    file.write_all(line.as_bytes())
        .with_context(|| format!("写入回流文件失败：{}", path.display()))?;
    Ok(())
}

/// 不引入额外依赖的 RFC3339 时间戳（UTC）。
fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (y, mo, d) = civil_from_days(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Howard Hinnant 的 civil_from_days 算法。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
