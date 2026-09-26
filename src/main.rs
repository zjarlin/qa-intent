//! qai 命令入口：参数解析、文件读写、调用库模块。
mod cli;
use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command};
use qa_intent::{
    bank::{self, ExportFormat, QuestionBank},
    client,
    compiler::Compiler,
    dataset, feedback, generator,
    model::Answer,
    taxonomy::Taxonomy,
};
use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
};

fn main() {
    if let Err(error) = run(Cli::parse()) {
        eprintln!("错误：{error:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match &cli.command {
        Command::Generate(args) => {
            if let Some(output) = &args.output {
                anyhow::ensure!(!output.exists(), "输出文件已存在，不会覆盖");
            }
            let scenario = match &args.scenario_file {
                Some(path) if path == Path::new("-") => {
                    let mut text = String::new();
                    std::io::stdin().read_to_string(&mut text)?;
                    text
                }
                Some(path) => std::fs::read_to_string(path).context("读取场景文件失败")?,
                None => args.scenario.clone().unwrap_or_default(),
            };
            let context = args
                .context
                .as_ref()
                .map(std::fs::read_to_string)
                .transpose()
                .context("读取业务资料失败")?
                .unwrap_or_default();
            let settings = generator::Settings {
                scenario,
                context,
                count: args.count,
                batch_size: args.batch_size,
                concurrency: args.concurrency,
                retries: args.retries,
                decision_model: args.decision_model.clone(),
            };
            settings.validate()?;
            if args.prompt_only {
                let prompt = generator::prompt(&settings)?;
                return bank::write_output(args.output.as_deref(), |out| {
                    out.write_all(prompt.as_bytes())?;
                    Ok(())
                });
            }
            let model = args.model.clone().context(
                "请配置 --model 或 QAI_GENERATOR_MODEL；离线生成提示词可用 --prompt-only",
            )?;
            let client = generator::client::GeneratorClient::new(
                &args.base_url,
                args.api_key.clone(),
                model,
                args.api,
                args.json_mode,
                args.timeout,
            )?;
            let generated = generator::generate(&client, &settings)?;
            bank::write_output(args.output.as_deref(), |out| {
                serde_json::to_writer_pretty(&mut *out, &generated)?;
                writeln!(out)?;
                Ok(())
            })?;
        }
        Command::Init { force } => {
            let taxonomy: Taxonomy =
                serde_json::from_str(include_str!("../examples/taxonomy.json"))?;
            taxonomy.validate()?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(*force)
                .truncate(*force)
                .create_new(!*force)
                .open(&cli.taxonomy)
                .context("创建标签体系失败；已存在时可使用 --force")?;
            serde_json::to_writer_pretty(&mut file, &taxonomy)?;
            writeln!(file)?;
            eprintln!("已生成标签体系：{}", cli.taxonomy.display());
        }
        Command::Validate { bank: Some(path) } => {
            let bank = QuestionBank::load(path)?;
            println!(
                "{}",
                serde_json::json!({"valid":true,"schema_version":bank.schema_version,"items":bank.items.len()})
            );
        }
        Command::Validate { bank: None } => {
            let taxonomy = Taxonomy::load(&cli.taxonomy)?;
            println!(
                "{}",
                serde_json::json!({"valid": true, "version": taxonomy.version,
                "model": taxonomy.model, "qids": taxonomy.questions.keys().collect::<Vec<_>>() })
            );
        }
        Command::Compile { input, raw } => {
            let taxonomy = Taxonomy::load(&cli.taxonomy)?;
            let compiler = Compiler::new(&taxonomy);
            let mut output = BufWriter::new(std::io::stdout().lock());
            dataset::each_item(input, |item| {
                let compiled = compiler.compile(&item)?;
                if *raw {
                    serde_json::to_writer(&mut output, &compiled.envelope)?;
                } else {
                    serde_json::to_writer(&mut output, &compiled)?;
                }
                writeln!(output)?;
                Ok(())
            })?;
            output.flush()?;
        }
        Command::Export {
            input,
            format,
            output,
        } => match format {
            ExportFormat::Supervised => {
                let taxonomy = Taxonomy::load(&cli.taxonomy)?;
                bank::write_output(output.as_deref(), |out| {
                    dataset::export(&taxonomy, input, out)
                })?;
            }
            _ => {
                let bank = QuestionBank::load(input)?;
                bank::write_output(output.as_deref(), |out| bank::export(&bank, *format, out))?;
            }
        },
        Command::Evaluate => dataset::evaluate(&cli.feedback, std::io::stdout().lock())?,
        Command::Ask {
            input,
            model,
            endpoint,
            api_key,
            timeout,
            no_feedback,
        } => {
            let mut taxonomy = Taxonomy::load(&cli.taxonomy)?;
            if let Some(model) = model {
                taxonomy.model = model.clone();
            }
            taxonomy.validate()?;
            let client = client::Client::new(endpoint.clone(), api_key.clone(), *timeout)?;
            ask(
                &taxonomy,
                &client,
                input,
                if *no_feedback {
                    None
                } else {
                    Some(&cli.feedback)
                },
            )?;
        }
        Command::Review { pending_only } => {
            let reader = BufReader::new(File::open(&cli.feedback)?);
            let mut output = BufWriter::new(std::io::stdout().lock());
            dataset::each_json_line(reader, |record: feedback::FeedbackRecord| {
                if !pending_only || record.needs_review {
                    serde_json::to_writer(&mut output, &record)?;
                    writeln!(output)?;
                }
                Ok(())
            })?;
            output.flush()?;
        }
    }
    Ok(())
}

/// 复用 HTTP 连接，每次处理一条记录；stdout 只输出可供应用消费的 JSONL。
fn ask(
    taxonomy: &Taxonomy,
    client: &client::Client,
    input: &Path,
    feedback_path: Option<&Path>,
) -> Result<()> {
    let compiler = Compiler::new(taxonomy);
    let mut output = std::io::stdout().lock();
    dataset::each_item(input, |item| {
        let compiled = compiler.compile(&item)?;
        let response = client.predict(&compiled.envelope)?;
        let raw = response
            .answers
            .get(&compiled.qid)
            .ok_or_else(|| anyhow::anyhow!("缺少 answers.{}", compiled.qid))?;
        let answer: Answer = serde_json::from_value(raw.clone()).context("解析决策结果失败")?;
        let decision = compiled.decide(&answer)?;
        let actual_model = response
            .model
            .as_deref()
            .unwrap_or(&compiled.envelope.model);
        let record = feedback::FeedbackRecord::from_answer(
            &item.id,
            &compiled,
            raw.clone(),
            &decision,
            actual_model,
            item.expected.clone(),
        );
        if let Some(path) = feedback_path {
            feedback::append(path, &record)?;
        }
        let result = serde_json::json!({
            "item_id": item.id, "qid": compiled.qid, "model": actual_model,
            "taxonomy_version": taxonomy.version, "decision": decision,
            "needs_review": record.needs_review, "answer": raw,
            "usage": {"input_tokens": response.usage.input_tokens, "output_tokens": response.usage.output_tokens}
        });
        serde_json::to_writer(&mut output, &result)?;
        writeln!(output)?;
        Ok(())
    })
}
