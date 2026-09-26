//! 契约校验：qid 命名、标签体系、阈值范围。
//!
//! 这些规则是整个工程化的「硬约束」，训练与应用必须共用同一份校验。

use anyhow::{bail, Result};

/// qid 必须是语义化短名，禁止 q1/问题1 这类不可读编号。
pub fn validate_qid(qid: &str) -> Result<()> {
    let qid = qid.trim();
    if qid.is_empty() {
        bail!("qid 不能为空");
    }
    if qid.len() > 64 {
        bail!("qid 过长（{qid}），请控制在 64 字符内");
    }
    if !qid.chars().next().unwrap().is_ascii_lowercase() {
        bail!("qid 必须以小写字母开头：{qid}");
    }
    let ok = qid
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !ok {
        bail!("qid 只允许小写字母、数字、下划线和连字符：{qid}");
    }
    // 拒绝纯编号形式（q1、q_2、问题1）。
    let bare_digits = qid
        .trim_start_matches(['q', '_'])
        .chars()
        .all(|c| c.is_ascii_digit());
    if bare_digits && qid.chars().any(|c| c.is_ascii_digit()) {
        bail!("qid 不能是纯编号（{qid}），请用语义化名称，如 urgent / department");
    }
    Ok(())
}

/// 阈值必须在 (0, 1) 区间内。
pub fn validate_threshold(threshold: f64) -> Result<()> {
    if !(0.0..1.0).contains(&threshold) {
        bail!("阈值必须在 (0, 1) 区间内，当前为 {threshold}");
    }
    Ok(())
}

/// 协议级硬校验：choice 必须有 criteria，noul 不能有 criteria。
pub fn validate_envelope(envelope: &crate::model::Envelope) -> Result<()> {
    use crate::model::QuestionKind;
    if envelope.questions.is_empty() {
        bail!("questions 不能为空");
    }
    if envelope.model.trim().is_empty() {
        bail!("model 不能为空");
    }
    for (qid, question) in &envelope.questions {
        validate_qid(qid)?;
        match question.kind {
            QuestionKind::Noul => {
                if question.criteria.is_some() {
                    bail!("noul 类型不应出现 criteria（qid = {qid}）");
                }
            }
            QuestionKind::Choice => match &question.criteria {
                None => bail!("choice 类型必须提供 criteria（qid = {qid}）"),
                Some(c) if c.is_empty() => bail!("choice 的 criteria 不能为空（qid = {qid}）"),
                Some(_) => {}
            },
        }
    }
    Ok(())
}
