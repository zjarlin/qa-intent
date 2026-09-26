//! JSONL 流式读取、监督数据导出与离线评测，供 CLI 和应用层共同使用。

use crate::{
    compiler::{Compiler, QaItem},
    feedback::FeedbackRecord,
    taxonomy::Taxonomy,
};
use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, BufRead, BufReader, Write},
    path::Path,
};

/// JSON 支持单题/数组；JSONL 和 stdin 逐行处理，内存随单条记录大小增长。
pub fn each_item(path: &Path, mut consume: impl FnMut(QaItem) -> Result<()>) -> Result<()> {
    if path == Path::new("-") {
        return each_json_line(io::stdin().lock(), consume);
    }
    let file = File::open(path).with_context(|| format!("读取题库失败：{}", path.display()))?;
    if path.extension().is_some_and(|ext| ext == "jsonl") {
        return each_json_line(BufReader::new(file), consume);
    }
    let value: serde_json::Value = serde_json::from_reader(BufReader::new(file))?;
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                consume(serde_json::from_value(value)?)?;
            }
        }
        value @ serde_json::Value::Object(_) => consume(serde_json::from_value(value)?)?,
        _ => anyhow::bail!("题库必须是对象、数组或 JSONL"),
    }
    Ok(())
}

/// 逐行解析时保留行号，坏数据必须报告而不是静默跳过。
pub fn each_json_line<T: serde::de::DeserializeOwned>(
    reader: impl BufRead,
    mut consume: impl FnMut(T) -> Result<()>,
) -> Result<()> {
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str(&line)
            .with_context(|| format!("第 {} 行 JSON 无效", index + 1))?;
        consume(value).with_context(|| format!("处理第 {} 行失败", index + 1))?;
    }
    Ok(())
}

pub fn export(taxonomy: &Taxonomy, input: &Path, mut output: impl Write) -> Result<()> {
    let compiler = Compiler::new(taxonomy);
    each_item(input, |item| {
        let compiled = compiler.compile(&item)?;
        let target = item
            .expected
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("{} 缺少人工 expected", item.id))?;
        let row = serde_json::json!({
            "format": "qa-intent.supervised.v1", "id": item.id,
            "taxonomy_version": taxonomy.version, "qid": item.qid,
            "request": compiled.envelope, "target": target
        });
        serde_json::to_writer(&mut output, &row)?;
        writeln!(output)?;
        Ok(())
    })
}

#[derive(Default, Serialize)]
struct Metrics {
    total: u64,
    labeled: u64,
    correct: u64,
    accepted: u64,
    accepted_labeled: u64,
    accepted_correct: u64,
}

pub fn evaluate(path: &Path, mut output: impl Write) -> Result<()> {
    let mut groups: BTreeMap<String, Metrics> = BTreeMap::new();
    each_json_line(
        BufReader::new(File::open(path)?),
        |record: FeedbackRecord| {
            // 不混合不同模型和 taxonomy 版本，避免掩盖回归。
            let key = format!(
                "{}:{}:{}",
                record.model, record.taxonomy_version, record.qid
            );
            let metric = groups.entry(key).or_default();
            metric.total += 1;
            metric.accepted += u64::from(record.accepted);
            if let Some(expected) = record.expected {
                let correct = expected == record.prediction;
                metric.labeled += 1;
                metric.correct += u64::from(correct);
                metric.accepted_labeled += u64::from(record.accepted);
                metric.accepted_correct += u64::from(record.accepted && correct);
            }
            Ok(())
        },
    )?;
    let ratio = |a: u64, b: u64| (b > 0).then(|| a as f64 / b as f64);
    let report: BTreeMap<_, _> = groups
        .into_iter()
        .map(|(key, m)| {
            let value = serde_json::json!({
                "counts": m, "accuracy": ratio(m.correct, m.labeled),
                "coverage": ratio(m.accepted, m.total),
                "accepted_accuracy": ratio(m.accepted_correct, m.accepted_labeled)
            });
            (key, value)
        })
        .collect();
    serde_json::to_writer_pretty(&mut output, &report)?;
    writeln!(output)?;
    Ok(())
}
