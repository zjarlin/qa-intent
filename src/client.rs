//! System One 决策端点客户端。
//!
//! Laya 与 JEV 共用同一份 wire protocol，唯一区别是 model 字段。

use crate::model::DecisionResponse;
use anyhow::{bail, Context, Result};
use std::time::Duration;

pub const DEFAULT_ENDPOINT: &str = "https://company-ai.addzero.site/v1/systemone";

pub struct Client {
    http: reqwest::blocking::Client,
    endpoint: String,
    api_key: Option<String>,
}

impl Client {
    pub fn new(
        endpoint: Option<String>,
        api_key: Option<String>,
        timeout_secs: u64,
    ) -> Result<Self> {
        let http = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("构建 HTTP 客户端失败")?;
        Ok(Self {
            http,
            endpoint: endpoint.unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()),
            api_key,
        })
    }

    /// 发送信封，返回解析后的决策结果。
    pub fn predict(&self, envelope: &crate::model::Envelope) -> Result<DecisionResponse> {
        let body = envelope.to_json()?;
        let mut request = self
            .http
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .body(body);

        if let Some(key) = &self.api_key {
            if !key.trim().is_empty() {
                request = request.header("Authorization", format!("Bearer {key}"));
            }
        }

        let response = request
            .send()
            .with_context(|| format!("调用 System One 失败：{}", self.endpoint))?;
        let status = response.status();
        let text = response.text().context("读取 System One 响应失败")?;

        if !status.is_success() {
            bail!("System One 返回 HTTP {status}");
        }
        let parsed: DecisionResponse =
            serde_json::from_str(&text).context("解析 System One 返回失败")?;
        Ok(parsed)
    }
}
