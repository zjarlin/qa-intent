//! 命令行定义。子命令覆盖：标签体系管理、题目编译、调用、回流。

use clap::{Args, Parser, Subcommand};
use qa_intent::{bank::ExportFormat, generator::client::Api};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "qai",
    version,
    about = "自然语言生成客服题库，导出 JSON/CSV 并编译 Laya/JEV 决策请求"
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

#[derive(Subcommand)]
pub enum Command {
    /// 根据自然语言场景批量生成题库（通用文本模型）
    Generate(GenerateArgs),
    /// 初始化一份示例标签体系
    Init {
        /// 覆盖已存在文件
        #[arg(long)]
        force: bool,
    },

    /// 校验标签体系是否合法
    Validate {
        /// 校验完整生成题库；- 从 stdin 读取
        #[arg(long)]
        bank: Option<PathBuf>,
    },

    /// 导出客服 JSON/CSV、应用请求或训练样本
    Export {
        #[arg(short, long)]
        input: PathBuf,
        /// 默认保留原有人工监督样本导出；其他格式读取完整题库
        #[arg(long, value_enum, default_value_t = ExportFormat::Supervised)]
        format: ExportFormat,
        /// 写入新文件；省略时输出到 stdout
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// 统计带人工 expected 的回流数据准确率与覆盖率
    Evaluate,

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
        #[arg(long, env = "CODEX_GROUP_KEY", hide_env_values = true)]
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
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        pending_only: bool,
    },
}

#[derive(Args)]
pub struct GenerateArgs {
    /// 自然语言场景（或通过 --scenario-file 提供）
    #[arg(
        required_unless_present = "scenario_file",
        conflicts_with = "scenario_file"
    )]
    pub scenario: Option<String>,
    /// UTF-8 场景文件；- 表示 stdin
    #[arg(long)]
    pub scenario_file: Option<PathBuf>,
    /// 业务规则或客服资料文件，作为答案依据
    #[arg(long)]
    pub context: Option<PathBuf>,
    #[arg(long, default_value_t = 20)]
    pub count: usize,
    #[arg(long, default_value_t = 20)]
    pub batch_size: usize,
    #[arg(long, default_value_t = 2)]
    pub concurrency: usize,
    /// 结构或内容校验失败后的重新生成次数（0..3）
    #[arg(long, default_value_t = 2)]
    pub retries: usize,
    /// 生成模型 ID，与 Laya/JEV 决策模型不同
    #[arg(long, env = "QAI_GENERATOR_MODEL")]
    pub model: Option<String>,
    /// API 基地址，包含 /v1，不包含 /responses 或 /chat/completions
    #[arg(
        long,
        env = "QAI_GENERATOR_BASE_URL",
        default_value = "https://api.openai.com/v1"
    )]
    pub base_url: String,
    #[arg(long, env = "QAI_GENERATOR_API_KEY", hide_env_values = true)]
    pub api_key: Option<String>,
    #[arg(long, value_enum, default_value_t = Api::Responses)]
    pub api: Api,
    /// 兼容仅支持 json_object 的服务；本地校验仍然执行
    #[arg(long)]
    pub json_mode: bool,
    #[arg(long, default_value_t = 120)]
    pub timeout: u64,
    #[arg(long, default_value = "laya")]
    pub decision_model: String,
    /// 只导出离线提示词，不调用 API，不需要模型或密钥
    #[arg(long)]
    pub prompt_only: bool,
    /// 完整题库写入新文件；省略时 stdout 只输出 JSON
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}
