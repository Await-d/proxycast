//! 原生 Rust Agent 实现
//!
//! 支持连续对话（Conversation History）和工具调用（Tools）
//! 参考 goose 项目的 Agent 设计

use crate::agent::types::*;
use crate::models::openai::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ContentPart as OpenAIContentPart,
    MessageContent as OpenAIMessageContent,
};
use futures::StreamExt;
use parking_lot::RwLock;
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// 原生 Agent 实现
pub struct NativeAgent {
    client: Client,
    base_url: String,
    api_key: String,
    sessions: Arc<RwLock<HashMap<String, AgentSession>>>,
    config: AgentConfig,
}

impl NativeAgent {
    pub fn new(base_url: String, api_key: String) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .connect_timeout(Duration::from_secs(30))
            .no_proxy()
            .build()
            .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?;

        Ok(Self {
            client,
            base_url,
            api_key,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config: AgentConfig::default(),
        })
    }

    pub fn with_model(mut self, model: String) -> Self {
        self.config.model = model;
        self
    }

    pub fn with_system_prompt(mut self, prompt: String) -> Self {
        self.config.system_prompt = Some(prompt);
        self
    }

    /// 将 AgentMessage 转换为 OpenAI ChatMessage
    fn convert_to_chat_message(&self, msg: &AgentMessage) -> ChatMessage {
        let content = match &msg.content {
            MessageContent::Text(text) => Some(OpenAIMessageContent::Text(text.clone())),
            MessageContent::Parts(parts) => {
                let openai_parts: Vec<OpenAIContentPart> = parts
                    .iter()
                    .map(|p| match p {
                        ContentPart::Text { text } => {
                            OpenAIContentPart::Text { text: text.clone() }
                        }
                        ContentPart::ImageUrl { image_url } => OpenAIContentPart::ImageUrl {
                            image_url: crate::models::openai::ImageUrl {
                                url: image_url.url.clone(),
                                detail: image_url.detail.clone(),
                            },
                        },
                    })
                    .collect();
                Some(OpenAIMessageContent::Parts(openai_parts))
            }
        };

        ChatMessage {
            role: msg.role.clone(),
            content,
            tool_calls: msg.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|tc| crate::models::openai::ToolCall {
                        id: tc.id.clone(),
                        call_type: tc.call_type.clone(),
                        function: crate::models::openai::FunctionCall {
                            name: tc.function.name.clone(),
                            arguments: tc.function.arguments.clone(),
                        },
                    })
                    .collect()
            }),
            tool_call_id: msg.tool_call_id.clone(),
        }
    }

    /// 构建完整的消息列表（包含历史）
    fn build_messages_with_history(
        &self,
        session: &AgentSession,
        user_message: &str,
        images: Option<&[ImageData]>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // 1. 添加系统提示词
        let system_prompt = session
            .system_prompt
            .as_ref()
            .or(self.config.system_prompt.as_ref());
        if let Some(prompt) = system_prompt {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: Some(OpenAIMessageContent::Text(prompt.clone())),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 2. 添加历史消息
        for msg in &session.messages {
            messages.push(self.convert_to_chat_message(msg));
        }

        // 3. 添加当前用户消息
        let user_msg = if let Some(imgs) = images {
            let mut parts = vec![OpenAIContentPart::Text {
                text: user_message.to_string(),
            }];

            for img in imgs {
                parts.push(OpenAIContentPart::ImageUrl {
                    image_url: crate::models::openai::ImageUrl {
                        url: format!("data:{};base64,{}", img.media_type, img.data),
                        detail: None,
                    },
                });
            }

            ChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Parts(parts)),
                tool_calls: None,
                tool_call_id: None,
            }
        } else {
            ChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Text(user_message.to_string())),
                tool_calls: None,
                tool_call_id: None,
            }
        };

        messages.push(user_msg);
        messages
    }

    /// 发送聊天请求（支持连续对话）
    pub async fn chat(&self, request: NativeChatRequest) -> Result<NativeChatResponse, String> {
        let model = request.model.unwrap_or_else(|| self.config.model.clone());
        let session_id = request.session_id.clone();
        let has_images = request.images.as_ref().map(|i| i.len()).unwrap_or(0);

        info!(
            "[NativeAgent] 发送聊天请求: model={}, session={:?}, images={}",
            model, session_id, has_images
        );

        // 获取或创建会话
        let session = if let Some(sid) = &session_id {
            self.sessions.read().get(sid).cloned()
        } else {
            None
        };

        let messages = if let Some(ref sess) = session {
            // 使用会话历史构建消息
            self.build_messages_with_history(sess, &request.message, request.images.as_deref())
        } else {
            // 无会话，单次对话
            self.build_single_messages(&request.message, request.images.as_deref())
        };

        // 打印消息结构用于调试
        for (i, msg) in messages.iter().enumerate() {
            let content_type = match &msg.content {
                Some(OpenAIMessageContent::Text(_)) => "text",
                Some(OpenAIMessageContent::Parts(parts)) => {
                    let has_image = parts
                        .iter()
                        .any(|p| matches!(p, OpenAIContentPart::ImageUrl { .. }));
                    if has_image {
                        "parts_with_image"
                    } else {
                        "parts_text_only"
                    }
                }
                None => "none",
            };
            debug!(
                "[NativeAgent] 消息[{}]: role={}, content_type={}",
                i, msg.role, content_type
            );
        }

        let chat_request = ChatCompletionRequest {
            model: model.clone(),
            messages,
            stream: false,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            top_p: None,
            tools: None, // TODO: 添加工具支持
            tool_choice: None,
            reasoning_effort: None,
        };

        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!("[NativeAgent] 请求失败: {} - {}", status, body);
            return Ok(NativeChatResponse {
                content: String::new(),
                model,
                usage: None,
                success: false,
                error: Some(format!("API 错误 ({}): {}", status, body)),
            });
        }

        let body: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        let content = body
            .choices
            .first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = Some(TokenUsage {
            input_tokens: body.usage.prompt_tokens,
            output_tokens: body.usage.completion_tokens,
        });

        // 更新会话历史
        if let Some(sid) = session_id {
            self.add_message_to_session(
                &sid,
                "user",
                MessageContent::Text(request.message.clone()),
                request.images.as_deref(),
            );
            self.add_message_to_session(
                &sid,
                "assistant",
                MessageContent::Text(content.clone()),
                None,
            );
        }

        info!("[NativeAgent] 聊天完成: content_len={}", content.len());

        Ok(NativeChatResponse {
            content,
            model: body.model,
            usage,
            success: true,
            error: None,
        })
    }

    /// 构建单次对话消息（无历史）
    fn build_single_messages(
        &self,
        user_message: &str,
        images: Option<&[ImageData]>,
    ) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        if let Some(system_prompt) = &self.config.system_prompt {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: Some(OpenAIMessageContent::Text(system_prompt.clone())),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        let user_msg = if let Some(imgs) = images {
            let mut parts = vec![OpenAIContentPart::Text {
                text: user_message.to_string(),
            }];

            for img in imgs {
                parts.push(OpenAIContentPart::ImageUrl {
                    image_url: crate::models::openai::ImageUrl {
                        url: format!("data:{};base64,{}", img.media_type, img.data),
                        detail: None,
                    },
                });
            }

            ChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Parts(parts)),
                tool_calls: None,
                tool_call_id: None,
            }
        } else {
            ChatMessage {
                role: "user".to_string(),
                content: Some(OpenAIMessageContent::Text(user_message.to_string())),
                tool_calls: None,
                tool_call_id: None,
            }
        };

        messages.push(user_msg);
        messages
    }

    /// 添加消息到会话
    fn add_message_to_session(
        &self,
        session_id: &str,
        role: &str,
        content: MessageContent,
        images: Option<&[ImageData]>,
    ) {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(session_id) {
            let final_content = if let Some(imgs) = images {
                // 如果有图片，转换为 Parts
                let mut parts = vec![ContentPart::Text {
                    text: content.as_text(),
                }];
                for img in imgs {
                    parts.push(ContentPart::ImageUrl {
                        image_url: ImageUrl {
                            url: format!("data:{};base64,{}", img.media_type, img.data),
                            detail: None,
                        },
                    });
                }
                MessageContent::Parts(parts)
            } else {
                content
            };

            session.messages.push(AgentMessage {
                role: role.to_string(),
                content: final_content,
                timestamp: chrono::Utc::now().to_rfc3339(),
                tool_calls: None,
                tool_call_id: None,
            });
            session.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }

    /// 流式聊天（支持连续对话）
    pub async fn chat_stream(
        &self,
        request: NativeChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), String> {
        let model = request.model.unwrap_or_else(|| self.config.model.clone());
        let session_id = request.session_id.clone();

        debug!(
            "[NativeAgent] 发送流式聊天请求: model={}, session={:?}",
            model, session_id
        );

        // 获取会话
        let session = if let Some(sid) = &session_id {
            self.sessions.read().get(sid).cloned()
        } else {
            None
        };

        let messages = if let Some(ref sess) = session {
            self.build_messages_with_history(sess, &request.message, request.images.as_deref())
        } else {
            self.build_single_messages(&request.message, request.images.as_deref())
        };

        let chat_request = ChatCompletionRequest {
            model: model.clone(),
            messages,
            stream: true,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
            top_p: None,
            tools: None,
            tool_choice: None,
            reasoning_effort: None,
        };

        let url = format!("{}/v1/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&chat_request)
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            error!("[NativeAgent] 流式请求失败: {} - {}", status, body);
            let _ = tx
                .send(StreamEvent::Error {
                    message: format!("API 错误 ({}): {}", status, body),
                })
                .await;
            return Err(format!("API 错误: {}", status));
        }

        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        let mut full_content = String::new();

        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    buffer.push_str(&text);

                    while let Some(pos) = buffer.find("\n\n") {
                        let event = buffer[..pos].to_string();
                        buffer = buffer[pos + 2..].to_string();

                        for line in event.lines() {
                            if let Some(data) = line.strip_prefix("data: ") {
                                if data.trim() == "[DONE]" {
                                    // 更新会话历史
                                    if let Some(sid) = &session_id {
                                        self.add_message_to_session(
                                            sid,
                                            "user",
                                            MessageContent::Text(request.message.clone()),
                                            request.images.as_deref(),
                                        );
                                        self.add_message_to_session(
                                            sid,
                                            "assistant",
                                            MessageContent::Text(full_content.clone()),
                                            None,
                                        );
                                    }
                                    let _ = tx.send(StreamEvent::Done { usage: None }).await;
                                    return Ok(());
                                }

                                if let Ok(json) = serde_json::from_str::<Value>(data) {
                                    if let Some(delta) = json
                                        .get("choices")
                                        .and_then(|c| c.get(0))
                                        .and_then(|c| c.get("delta"))
                                        .and_then(|d| d.get("content"))
                                        .and_then(|c| c.as_str())
                                    {
                                        if !delta.is_empty() {
                                            full_content.push_str(delta);
                                            let _ = tx
                                                .send(StreamEvent::TextDelta {
                                                    text: delta.to_string(),
                                                })
                                                .await;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("[NativeAgent] 流读取错误: {}", e);
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: format!("流读取错误: {}", e),
                        })
                        .await;
                    return Err(format!("流读取错误: {}", e));
                }
            }
        }

        // 更新会话历史
        if let Some(sid) = &session_id {
            self.add_message_to_session(
                sid,
                "user",
                MessageContent::Text(request.message.clone()),
                request.images.as_deref(),
            );
            self.add_message_to_session(sid, "assistant", MessageContent::Text(full_content), None);
        }

        let _ = tx.send(StreamEvent::Done { usage: None }).await;
        Ok(())
    }

    pub fn create_session(&self, model: Option<String>, system_prompt: Option<String>) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let session = AgentSession {
            id: session_id.clone(),
            model: model.unwrap_or_else(|| self.config.model.clone()),
            messages: Vec::new(),
            system_prompt,
            created_at: now.clone(),
            updated_at: now,
        };

        self.sessions.write().insert(session_id.clone(), session);
        info!("[NativeAgent] 创建会话: {}", session_id);

        session_id
    }

    pub fn get_session(&self, session_id: &str) -> Option<AgentSession> {
        self.sessions.read().get(session_id).cloned()
    }

    pub fn delete_session(&self, session_id: &str) -> bool {
        self.sessions.write().remove(session_id).is_some()
    }

    pub fn list_sessions(&self) -> Vec<AgentSession> {
        self.sessions.read().values().cloned().collect()
    }

    pub fn clear_session_messages(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write();
        if let Some(session) = sessions.get_mut(session_id) {
            session.messages.clear();
            session.updated_at = chrono::Utc::now().to_rfc3339();
            true
        } else {
            false
        }
    }

    pub fn get_session_messages(&self, session_id: &str) -> Option<Vec<AgentMessage>> {
        self.sessions
            .read()
            .get(session_id)
            .map(|s| s.messages.clone())
    }
}

/// Tauri 状态：原生 Agent 管理器
#[derive(Clone, Default)]
pub struct NativeAgentState {
    agent: Arc<RwLock<Option<NativeAgent>>>,
}

impl NativeAgentState {
    pub fn new() -> Self {
        Self {
            agent: Arc::new(RwLock::new(None)),
        }
    }

    pub fn init(&self, base_url: String, api_key: String) -> Result<(), String> {
        let agent = NativeAgent::new(base_url, api_key)?;
        *self.agent.write() = Some(agent);
        Ok(())
    }

    pub fn is_initialized(&self) -> bool {
        self.agent.read().is_some()
    }

    pub fn reset(&self) {
        *self.agent.write() = None;
    }

    pub async fn chat(&self, request: NativeChatRequest) -> Result<NativeChatResponse, String> {
        let (base_url, api_key, config, sessions) = {
            let guard = self.agent.read();
            let agent = guard.as_ref().ok_or_else(|| "Agent 未初始化".to_string())?;
            (
                agent.base_url.clone(),
                agent.api_key.clone(),
                agent.config.clone(),
                agent.sessions.clone(),
            )
        };

        // 创建临时 Agent，共享 sessions
        let temp_agent = NativeAgent {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .connect_timeout(Duration::from_secs(30))
                .no_proxy()
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?,
            base_url,
            api_key,
            sessions,
            config,
        };

        temp_agent.chat(request).await
    }

    pub async fn chat_stream(
        &self,
        request: NativeChatRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), String> {
        let (base_url, api_key, config, sessions) = {
            let guard = self.agent.read();
            let agent = guard.as_ref().ok_or_else(|| "Agent 未初始化".to_string())?;
            (
                agent.base_url.clone(),
                agent.api_key.clone(),
                agent.config.clone(),
                agent.sessions.clone(),
            )
        };

        let temp_agent = NativeAgent {
            client: Client::builder()
                .timeout(Duration::from_secs(300))
                .connect_timeout(Duration::from_secs(30))
                .no_proxy()
                .build()
                .map_err(|e| format!("创建 HTTP 客户端失败: {}", e))?,
            base_url,
            api_key,
            sessions,
            config,
        };

        temp_agent.chat_stream(request, tx).await
    }

    pub fn create_session(
        &self,
        model: Option<String>,
        system_prompt: Option<String>,
    ) -> Result<String, String> {
        let guard = self.agent.read();
        let agent = guard.as_ref().ok_or_else(|| "Agent 未初始化".to_string())?;
        Ok(agent.create_session(model, system_prompt))
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<AgentSession>, String> {
        let guard = self.agent.read();
        let agent = guard.as_ref().ok_or_else(|| "Agent 未初始化".to_string())?;
        Ok(agent.get_session(session_id))
    }

    pub fn delete_session(&self, session_id: &str) -> bool {
        let guard = self.agent.read();
        if let Some(agent) = guard.as_ref() {
            agent.delete_session(session_id)
        } else {
            false
        }
    }

    pub fn list_sessions(&self) -> Vec<AgentSession> {
        let guard = self.agent.read();
        if let Some(agent) = guard.as_ref() {
            agent.list_sessions()
        } else {
            Vec::new()
        }
    }

    pub fn clear_session_messages(&self, session_id: &str) -> bool {
        let guard = self.agent.read();
        if let Some(agent) = guard.as_ref() {
            agent.clear_session_messages(session_id)
        } else {
            false
        }
    }

    pub fn get_session_messages(&self, session_id: &str) -> Option<Vec<AgentMessage>> {
        let guard = self.agent.read();
        guard
            .as_ref()
            .and_then(|a| a.get_session_messages(session_id))
    }
}
