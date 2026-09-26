//! 命令行定义。子命令覆盖：标签体系管理、题目编译、调用、回流。

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "qai",
    version,
    about = "把垂直领域 QA 题库编译成 Laya/JEV 决策信封",
    long_about = "qai 是 Laya/JEV System One 决策模型的工程化工具：\n\
                  用一份标签体系(taxonomy)作为契约，把题库编译成 {state, questions}，\n\
                  调用决策端点，并把低置信/错判样本回流为训练种子。"
)]
pub struct Cli {
    /// 标签体系文件路径
    #[arg(long, short = 't', default_value = "taxonomy.json", global = true)]
    pub taxonomy: PathBuf,

    /// 回流记录输出路径
    #[arg(long, default_value = "feedback.jsonl", global = true)]
    pub feedback: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 初始化一份示例标签体系
    Init {
        /// 覆盖已存在文件
        #[arg(long)]
        force: bool,
    },

    /// 校验标签体系是否合法
    Validate,

    /// 把一道 QA 题编译成 Laya 信封
    Compile {
        /// 题库文件（单题或数组）
        #[arg(long, short = 'i')]
        input: PathBuf,

        /// 仅输出信封 JSON（便于管道）
        #[arg(long)]
        raw: bool,
    },

    /// 编译并调用决策端点，输出判定结果
    Ask {
        /// 题库文件（单题或数组）
        #[arg(long, short = 'i')]
        input: PathBuf,

        /// 覆盖标签体系里的 model
        #[arg(long)]
        model: Option<String>,

        /// 覆盖端点地址
        #[arg(long, env = "SYSTEMONE_ENDPOINT")]
        endpoint: Option<String>,

        /// API Key（默认读 CODEX_GROUP_KEY）
        #[arg(long, env = "CODEX_GROUP_KEY")]
        api_key: Option<String>,

        /// 请求超时（秒）
        #[arg(long, default_value_t = 60)]
        timeout: u64,

        /// 不写回流记录
        #[arg(long)]
        no_feedback: bool,
    },

    /// 查看回流文件里的待复核样本
    Review {
        /// 只显示 needs_review = true 的记录
        #[arg(long, default_value_t = true)]
        pending_only: bool,
    },
}
