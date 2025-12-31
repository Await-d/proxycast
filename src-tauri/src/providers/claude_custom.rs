//! Claude Custom Provider (自定义 Claude API)
use crate::models::anthropic::AnthropicMessagesRequest;
use crate::models::openai::{ChatCompletionRequest, ContentPart, MessageContent};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaudeCustomConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub enabled: bool,
}

pub struct ClaudeCustomProvider {
    pub config: ClaudeCustomConfig,
    pub client: Client,
}

impl Default for ClaudeCustomProvider {
    fn default() -> Self {
        Self {
            config: ClaudeCustomConfig::default(),
            client: Client::new(),
        }
    }
}

impl ClaudeCustomProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用 API key 和 base_url 创建 Provider
    pub fn with_config(api_key: String, base_url: Option<String>) -> Self {
        Self {
            config: ClaudeCustomConfig {
                api_key: Some(api_key),
                base_url,
                enabled: true,
            },
            client: Client::new(),
        }
    }

    pub fn get_base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com".to_string())
    }

    pub fn is_configured(&self) -> bool {
        self.config.api_key.is_some() && self.config.enabled
    }

    /// 构建完整的 API URL
    /// 智能处理用户输入的 base_url，无论是否带 /v1 都能正确工作
    fn build_url(&self, endpoint: &str) -> String {
        let base = self.get_base_url();
        let base = base.trim_end_matches('/');

        // 如果用户输入了带 /v1 的 URL，直接拼接 endpoint
        // 否则拼接 /v1/endpoint
        if base.ends_with("/v1") {
            format!("{}/{}", base, endpoint)
        } else {
            format!("{}/v1/{}", base, endpoint)
        }
    }

    /// 将 OpenAI 图片 URL 格式转换为 Claude 图片格式
    ///
    /// 支持两种格式：
    /// 1. data URL: `data:image/jpeg;base64,xxxxx` -> Claude base64 格式
    /// 2. HTTP URL: `https://...` -> 作为文本提示（Claude 不直接支持 URL）
    fn convert_image_url_to_claude(url: &str) -> Option<serde_json::Value> {
        if url.starts_with("data:") {
            // 解析 data URL: data:image/jpeg;base64,xxxxx
            let parts: Vec<&str> = url.splitn(2, ',').collect();
            if parts.len() == 2 {
                let header = parts[0]; // data:image/jpeg;base64
                let data = parts[1]; // base64 数据

                // 提取 media_type: image/jpeg, image/png, image/gif, image/webp
                let media_type = header
                    .strip_prefix("data:")
                    .and_then(|s| s.split(';').next())
                    .unwrap_or("image/jpeg");

                tracing::debug!("[CLAUDE_IMAGE] 转换 base64 图片: media_type={}", media_type);

                return Some(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data
                    }
                }));
            }
        } else if url.starts_with("http://") || url.starts_with("https://") {
            // Claude 不直接支持 URL 图片，转为文本提示
            tracing::warn!("[CLAUDE_IMAGE] Claude 不支持 URL 图片，转为文本: {}", url);
            return Some(serde_json::json!({
                "type": "text",
                "text": format!("[Image: {}]", url)
            }));
        }

        tracing::warn!("[CLAUDE_IMAGE] 无法解析图片 URL: {}", url);
        None
    }

    /// 调用 Anthropic API（原生格式）
    pub async fn call_api(
        &self,
        request: &AnthropicMessagesRequest,
    ) -> Result<reqwest::Response, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Claude API key not configured")?;

        let url = self.build_url("messages");

        // 打印请求 URL 和模型用于调试
        tracing::info!(
            "[CLAUDE_API] 发送请求: url={} model={} stream={}",
            url,
            request.model,
            request.stream
        );

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        // 打印响应状态
        tracing::info!(
            "[CLAUDE_API] 响应状态: status={} model={}",
            resp.status(),
            request.model
        );

        Ok(resp)
    }

    /// 调用 OpenAI 格式的 API（内部转换为 Anthropic 格式）
    pub async fn call_openai_api(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
        // 手动转换 OpenAI 请求为 Anthropic 格式
        let mut anthropic_messages = Vec::new();
        let mut system_content = None;

        for msg in &request.messages {
            let role = &msg.role;

            // 提取消息内容，转换为 Anthropic 格式的 content 数组
            let content_blocks: Vec<serde_json::Value> = match &msg.content {
                Some(MessageContent::Text(text)) => {
                    if text.is_empty() {
                        vec![]
                    } else {
                        vec![serde_json::json!({"type": "text", "text": text})]
                    }
                }
                Some(MessageContent::Parts(parts)) => {
                    parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Text { text } => {
                                if text.is_empty() {
                                    None
                                } else {
                                    Some(serde_json::json!({"type": "text", "text": text}))
                                }
                            }
                            ContentPart::ImageUrl { image_url } => {
                                // 转换 OpenAI 图片格式为 Claude 图片格式
                                Self::convert_image_url_to_claude(&image_url.url)
                            }
                        })
                        .collect()
                }
                None => vec![],
            };

            if role == "system" {
                // system 消息只提取文本
                let text = content_blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                system_content = Some(text);
            } else if !content_blocks.is_empty() {
                let anthropic_role = if role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                anthropic_messages.push(serde_json::json!({
                    "role": anthropic_role,
                    "content": content_blocks
                }));
            }
        }

        let mut anthropic_body = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "messages": anthropic_messages
        });

        if let Some(sys) = system_content {
            anthropic_body["system"] = serde_json::json!(sys);
        }

        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Claude API key not configured")?;

        let url = self.build_url("messages");

        // 打印请求 URL 和模型用于调试
        tracing::info!(
            "[CLAUDE_API] 发送请求 (OpenAI 格式转换): url={} model={} stream={}",
            url,
            request.model,
            request.stream
        );

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&anthropic_body)
            .send()
            .await?;

        // 打印响应状态
        let status = resp.status();
        tracing::info!(
            "[CLAUDE_API] 响应状态: status={} model={}",
            status,
            request.model
        );

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Claude API error: {status} - {body}").into());
        }

        let anthropic_resp: serde_json::Value = resp.json().await?;

        // 转换回 OpenAI 格式
        let content = anthropic_resp["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|block| block["text"].as_str())
            .unwrap_or("");

        Ok(serde_json::json!({
            "id": format!("chatcmpl-{}", uuid::Uuid::new_v4()),
            "object": "chat.completion",
            "created": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            "model": request.model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": anthropic_resp["usage"]["input_tokens"].as_u64().unwrap_or(0),
                "completion_tokens": anthropic_resp["usage"]["output_tokens"].as_u64().unwrap_or(0),
                "total_tokens": 0
            }
        }))
    }

    pub async fn messages(
        &self,
        request: &serde_json::Value,
    ) -> Result<reqwest::Response, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Claude API key not configured")?;

        let url = self.build_url("messages");

        // 打印请求 URL 用于调试
        let model = request
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown");
        let stream = request
            .get("stream")
            .and_then(|s| s.as_bool())
            .unwrap_or(false);
        tracing::info!(
            "[CLAUDE_API] 发送请求 (原始 JSON): url={} model={} stream={}",
            url,
            model,
            stream
        );

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        // 打印响应状态
        tracing::info!(
            "[CLAUDE_API] 响应状态: status={} model={}",
            resp.status(),
            model
        );

        Ok(resp)
    }

    pub async fn count_tokens(
        &self,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Claude API key not configured")?;

        let url = self.build_url("messages/count_tokens");

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Failed to count tokens: {status} - {body}").into());
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }
}

// ============================================================================
// StreamingProvider Trait 实现
// ============================================================================

use crate::providers::ProviderError;
use crate::streaming::traits::{
    reqwest_stream_to_stream_response, StreamFormat, StreamResponse, StreamingProvider,
};
use async_trait::async_trait;

#[async_trait]
impl StreamingProvider for ClaudeCustomProvider {
    /// 发起流式 API 调用
    ///
    /// 使用 reqwest 的 bytes_stream 返回字节流，支持真正的端到端流式传输。
    /// Claude 使用 Anthropic SSE 格式。
    ///
    /// # 需求覆盖
    /// - 需求 1.2: ClaudeCustomProvider 流式支持
    async fn call_api_stream(
        &self,
        request: &ChatCompletionRequest,
    ) -> Result<StreamResponse, ProviderError> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| {
            ProviderError::ConfigurationError("Claude API key not configured".to_string())
        })?;

        // 转换 OpenAI 请求为 Anthropic 格式
        let mut anthropic_messages = Vec::new();
        let mut system_content = None;

        for msg in &request.messages {
            let role = &msg.role;

            // 提取消息内容，转换为 Anthropic 格式的 content 数组
            let content_blocks: Vec<serde_json::Value> = match &msg.content {
                Some(MessageContent::Text(text)) => {
                    if text.is_empty() {
                        vec![]
                    } else {
                        vec![serde_json::json!({"type": "text", "text": text})]
                    }
                }
                Some(MessageContent::Parts(parts)) => {
                    parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Text { text } => {
                                if text.is_empty() {
                                    None
                                } else {
                                    Some(serde_json::json!({"type": "text", "text": text}))
                                }
                            }
                            ContentPart::ImageUrl { image_url } => {
                                // 转换 OpenAI 图片格式为 Claude 图片格式
                                Self::convert_image_url_to_claude(&image_url.url)
                            }
                        })
                        .collect()
                }
                None => vec![],
            };

            if role == "system" {
                // system 消息只提取文本
                let text = content_blocks
                    .iter()
                    .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("");
                system_content = Some(text);
            } else if !content_blocks.is_empty() {
                let anthropic_role = if role == "assistant" {
                    "assistant"
                } else {
                    "user"
                };
                anthropic_messages.push(serde_json::json!({
                    "role": anthropic_role,
                    "content": content_blocks
                }));
            }
        }

        let mut anthropic_body = serde_json::json!({
            "model": request.model,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "messages": anthropic_messages,
            "stream": true
        });

        if let Some(sys) = system_content {
            anthropic_body["system"] = serde_json::json!(sys);
        }

        let url = self.build_url("messages");

        tracing::info!(
            "[CLAUDE_STREAM] 发起流式请求: url={} model={}",
            url,
            request.model
        );

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .header("Accept", "text/event-stream")
            .json(&anthropic_body)
            .send()
            .await
            .map_err(|e| ProviderError::from_reqwest_error(&e))?;

        // 检查响应状态
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("[CLAUDE_STREAM] 请求失败: {} - {}", status, body);
            return Err(ProviderError::from_http_status(status.as_u16(), &body));
        }

        tracing::info!("[CLAUDE_STREAM] 流式响应开始: status={}", status);

        // 将 reqwest 响应转换为 StreamResponse
        Ok(reqwest_stream_to_stream_response(resp))
    }

    fn supports_streaming(&self) -> bool {
        self.is_configured()
    }

    fn provider_name(&self) -> &'static str {
        "ClaudeCustomProvider"
    }

    fn stream_format(&self) -> StreamFormat {
        StreamFormat::AnthropicSse
    }
}
