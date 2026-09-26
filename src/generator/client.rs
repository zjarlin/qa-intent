//! 通用生成模型的 HTTP 协议适配，复用连接并限制响应大小。
use anyhow::{bail, ensure, Context, Result};
use serde_json::{json, Value};
use std::{io::Read, time::Duration};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum Api {
    Responses,
    Chat,
}

impl Api {
    pub fn name(self) -> &'static str {
        match self {
            Self::Responses => "responses",
            Self::Chat => "chat",
        }
    }
}

pub struct GeneratorClient {
    http: reqwest::blocking::Client,
    url: reqwest::Url,
    key: Option<String>,
    pub model: String,
    pub api: Api,
    json_mode: bool,
}

impl GeneratorClient {
    pub fn new(
        base_url: &str,
        key: Option<String>,
        model: String,
        api: Api,
        json_mode: bool,
        timeout: u64,
    ) -> Result<Self> {
        ensure!(
            !model.trim().is_empty(),
            "请通过 --model 或 QAI_GENERATOR_MODEL 指定生成模型"
        );
        ensure!(timeout > 0, "timeout 必须大于 0");
        let suffix = match api {
            Api::Responses => "responses",
            Api::Chat => "chat/completions",
        };
        let url = reqwest::Url::parse(&format!("{}/{suffix}", base_url.trim_end_matches('/')))
            .context("无效的生成模型 base-url")?;
        ensure!(
            matches!(url.scheme(), "http" | "https")
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "base-url 必须是无凭据、无查询参数的 HTTP(S) 地址"
        );
        let http = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(timeout))
            .build()?;
        Ok(Self {
            http,
            url,
            key,
            model,
            api,
            json_mode,
        })
    }

    pub fn complete(&self, task: &Value, schema: &Value) -> Result<String> {
        let instructions = include_str!("../../prompts/generate.md");
        let user = serde_json::to_string(&json!({"task":task,"output_schema":schema}))?;
        let format = if self.json_mode {
            json!({"type":"json_object"})
        } else {
            json!({"type":"json_schema","name":"qa_generation","strict":true,"schema":schema})
        };
        let body = match self.api {
            Api::Responses => {
                json!({"model":self.model,"store":false,"instructions":instructions,"input":user,"text":{"format":format}})
            }
            Api::Chat => {
                let response_format = if self.json_mode {
                    format
                } else {
                    json!({"type":"json_schema","json_schema":{"name":"qa_generation","strict":true,"schema":schema}})
                };
                json!({"model":self.model,"messages":[{"role":"system","content":instructions},{"role":"user","content":user}],"response_format":response_format})
            }
        };
        for attempt in 0..3 {
            let mut request = self.http.post(self.url.clone()).json(&body);
            if let Some(key) = self.key.as_ref().filter(|key| !key.is_empty()) {
                request = request.bearer_auth(key);
            }
            let mut response = request
                .send()
                .map_err(|_| anyhow::anyhow!("生成模型连接失败或超时"))?;
            let status = response.status();
            if (status.as_u16() == 429 || status.is_server_error()) && attempt < 2 {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .min(15);
                std::thread::sleep(Duration::from_secs(delay));
                continue;
            }
            ensure!(
                status.is_success(),
                "生成模型返回 HTTP {status}；检查端点、模型和认证配置"
            );
            let mut bytes = Vec::new();
            response
                .by_ref()
                .take(8 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .context("读取生成响应失败")?;
            ensure!(
                bytes.len() <= 8 * 1024 * 1024,
                "生成响应超过 8 MiB；请减小 batch-size"
            );
            let value: Value = serde_json::from_slice(&bytes).context("生成端点未返回 JSON")?;
            return extract_text(&value, self.api);
        }
        bail!("生成模型重试耗尽")
    }
}

pub fn extract_text(value: &Value, api: Api) -> Result<String> {
    match api {
        Api::Responses => {
            ensure!(
                value["status"] == "completed",
                "模型生成未完成；请减小 batch-size"
            );
            let outputs = value["output"]
                .as_array()
                .context("缺少 Responses output")?;
            let mut text = String::new();
            for output in outputs {
                if let Some(contents) = output["content"].as_array() {
                    for content in contents {
                        ensure!(content["type"] != "refusal", "模型拒绝生成该内容");
                        if content["type"] == "output_text" {
                            text.push_str(
                                content["text"].as_str().context("缺少 output_text.text")?,
                            );
                        }
                    }
                }
            }
            ensure!(!text.is_empty(), "模型没有返回文本");
            Ok(text)
        }
        Api::Chat => {
            let choice = &value["choices"][0];
            ensure!(choice["message"]["refusal"].is_null(), "模型拒绝生成该内容");
            ensure!(
                choice["finish_reason"] == "stop",
                "模型生成未正常结束；请减小 batch-size"
            );
            Ok(choice["message"]["content"]
                .as_str()
                .context("缺少 chat message.content")?
                .to_owned())
        }
    }
}
