//! 生成题库的交换格式、校验和导出；客服答案与分类输入始终分开。
use crate::{
    compiler::{Compiler, QaItem},
    taxonomy::Taxonomy,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::Path,
};

pub const SCHEMA_VERSION: &str = "qa-intent.bank.v1";

/// 完整题库可作为一个 JSON 文档传给任意语言的应用。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionBank {
    pub schema_version: String,
    pub scenario: String,
    pub generation: Generation,
    pub taxonomy: Taxonomy,
    pub items: Vec<BankItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generation {
    pub model: String,
    pub api: String,
}

/// intent 是生成的建议标签；needs_review=false 表示调用方已经人工复核。
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BankItem {
    pub id: String,
    pub qid: String,
    pub question: String,
    pub answer: String,
    pub intent: String,
    pub tags: Vec<String>,
    pub needs_review: bool,
}

impl BankItem {
    pub fn as_qa_item(&self) -> QaItem {
        QaItem {
            id: self.id.clone(),
            question: self.question.clone(),
            qid: self.qid.clone(),
            reference_answer: None,
            user_answer: None,
            options: None,
            tags: self.tags.clone(),
            // 只有人工复核过的标签才能进入原有评测流程。
            expected: (!self.needs_review).then(|| serde_json::json!(self.intent)),
        }
    }
}

pub fn normalized_question(question: &str) -> String {
    question
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

impl QuestionBank {
    pub fn load(path: &Path) -> Result<Self> {
        let input: Box<dyn Read> = if path == Path::new("-") {
            Box::new(std::io::stdin())
        } else {
            Box::new(std::fs::File::open(path).context("读取题库失败")?)
        };
        let bank: Self = serde_json::from_reader(input).context("解析完整题库 JSON 失败")?;
        bank.validate()?;
        Ok(bank)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == SCHEMA_VERSION,
            "不支持的题库 schema_version"
        );
        ensure!(!self.scenario.trim().is_empty(), "scenario 不能为空");
        ensure!(
            !self.generation.model.trim().is_empty(),
            "generation.model 不能为空"
        );
        self.taxonomy.validate()?;
        ensure!(!self.items.is_empty(), "题库不能为空");
        let mut ids = HashSet::new();
        let mut questions = HashSet::new();
        let compiler = Compiler::new(&self.taxonomy);
        for item in &self.items {
            ensure!(ids.insert(&item.id), "重复题目 id：{}", item.id);
            ensure!(
                questions.insert(normalized_question(&item.question)),
                "题库有重复问题"
            );
            ensure!(!item.answer.trim().is_empty(), "客服 answer 不能为空");
            let def = self.taxonomy.get(&item.qid)?;
            ensure!(
                def.criteria
                    .as_ref()
                    .is_some_and(|c| c.contains_key(&item.intent)),
                "intent 不在标签体系中"
            );
            compiler.compile(&item.as_qa_item())?;
        }
        Ok(())
    }
}

/// 客服导出和应用导出使用同一份已校验的题库。
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ExportFormat {
    Supervised,
    FaqJson,
    FaqCsv,
    Items,
    Taxonomy,
    Systemone,
    Training,
}

pub fn export(bank: &QuestionBank, format: ExportFormat, mut output: impl Write) -> Result<()> {
    bank.validate()?;
    match format {
        ExportFormat::FaqJson => {
            serde_json::to_writer_pretty(&mut output, &bank.items)?;
            writeln!(output)?;
        }
        ExportFormat::FaqCsv => {
            // BOM 方便 Excel 识别中文；使用 CSV 库处理引号、逗号和换行。
            output.write_all(b"\xef\xbb\xbf")?;
            let mut writer = csv::Writer::from_writer(output);
            writer.write_record(["id", "question", "answer", "intent", "tags", "needs_review"])?;
            for item in &bank.items {
                let tags = serde_json::to_string(&item.tags)?;
                let review = item.needs_review.to_string();
                let cells = [
                    &item.id,
                    &item.question,
                    &item.answer,
                    &item.intent,
                    &tags,
                    &review,
                ];
                writer.write_record(cells.map(|value| spreadsheet_cell(value)))?;
            }
            writer.flush()?;
        }
        ExportFormat::Taxonomy => {
            serde_json::to_writer_pretty(&mut output, &bank.taxonomy)?;
            writeln!(output)?;
        }
        ExportFormat::Items | ExportFormat::Systemone | ExportFormat::Training => {
            let compiler = Compiler::new(&bank.taxonomy);
            for item in &bank.items {
                let qa = item.as_qa_item();
                let row = match format {
                    ExportFormat::Items => serde_json::to_value(&qa)?,
                    ExportFormat::Systemone => {
                        serde_json::to_value(compiler.compile(&qa)?.envelope)?
                    }
                    _ => serde_json::json!({
                        "format":"qa-intent.supervised.v1", "id":item.id, "qid":item.qid,
                        "taxonomy_version":bank.taxonomy.version,
                        "request":compiler.compile(&qa)?.envelope, "target":item.intent,
                        "label_source":if item.needs_review {"model_generated"} else {"human_reviewed"},
                        "needs_review":item.needs_review
                    }),
                };
                serde_json::to_writer(&mut output, &row)?;
                writeln!(output)?;
            }
        }
        ExportFormat::Supervised => {
            anyhow::bail!("supervised 用于原有 QaItem；完整题库请用 --format training")
        }
    }
    Ok(())
}

fn spreadsheet_cell(value: &str) -> String {
    if value.trim_start().starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r', '\n'])
    {
        format!("'{value}")
    } else {
        value.to_owned()
    }
}

/// 只有写入成功才落盘；禁止悄悄覆盖已有文件。
pub fn write_output(
    path: Option<&Path>,
    write: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    if let Some(path) = path {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut file = tempfile::NamedTempFile::new_in(parent).context("输出目录不存在或不可写")?;
        write(&mut file)?;
        file.flush()?;
        file.persist_noclobber(path)
            .context("保存失败；不会覆盖已有文件")?;
    } else {
        let mut output = std::io::BufWriter::new(std::io::stdout().lock());
        write(&mut output)?;
        output.flush()?;
    }
    Ok(())
}
