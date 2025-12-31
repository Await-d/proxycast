//! Agent 类型定义
//!
//! 定义 Agent 模块使用的核心类型
//! 参考 goose 项目的 Conversation 设计，支持连续对话和工具调用

use serde::{Deserialize, Serialize};

/// Agent 会话状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    /// 会话 ID
    pub id: String,
    /// 使用的模型
    pub model: String,
    /// 会话消息历史（支持连续对话）
    pub messages: Vec<AgentMessage>,
    /// 系统提示词
    pub system_prompt: Option<String>,
    /// 创建时间
    pub created_at: String,
    /// 最后活动时间
    pub updated_at: String,
}

/// Agent 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// 角色: user, assistant, system, tool
    pub role: String,
    /// 消息内容（文本或结构化内容）
    pub content: MessageContent,
    /// 时间戳
    pub timestamp: String,
    /// 工具调用（assistant 消息可能包含）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// 工具调用 ID（tool 角色消息需要）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 消息内容类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本
    Text(String),
    /// 多部分内容（文本 + 图片）
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    /// 获取文本内容
    pub fn as_text(&self) -> String {
        match self {
            MessageContent::Text(s) => s.clone(),
            MessageContent::Parts(parts) => parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// 内容部分
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// 文本
    Text { text: String },
    /// 图片 URL
    ImageUrl { image_url: ImageUrl },
}

/// 图片 URL
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// 工具调用 ID
    pub id: String,
    /// 工具类型
    #[serde(rename = "type")]
    pub call_type: String,
    /// 函数调用详情
    pub function: FunctionCall,
}

/// 函数调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// 函数名
    pub name: String,
    /// 参数（JSON 字符串）
    pub arguments: String,
}

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// 工具类型
    #[serde(rename = "type")]
    pub tool_type: String,
    /// 函数定义
    pub function: FunctionDefinition,
}

/// 函数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinition {
    /// 函数名
    pub name: String,
    /// 函数描述
    pub description: String,
    /// 参数 schema
    pub parameters: serde_json::Value,
}

/// Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 模型名称
    pub model: String,
    /// 系统提示词
    pub system_prompt: Option<String>,
    /// 温度参数
    pub temperature: Option<f32>,
    /// 最大 token 数
    pub max_tokens: Option<u32>,
    /// 可用工具
    pub tools: Vec<ToolDefinition>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            system_prompt: None,
            temperature: Some(0.7),
            max_tokens: Some(4096),
            tools: Vec::new(),
        }
    }
}

/// 聊天请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeChatRequest {
    /// 会话 ID（用于连续对话）
    pub session_id: Option<String>,
    /// 用户消息
    pub message: String,
    /// 模型名称（可选）
    pub model: Option<String>,
    /// 图片列表（可选）
    pub images: Option<Vec<ImageData>>,
    /// 是否流式响应
    pub stream: bool,
}

/// 图片数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
    /// base64 编码的图片数据
    pub data: String,
    /// MIME 类型
    pub media_type: String,
}

/// 聊天响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NativeChatResponse {
    /// 响应内容
    pub content: String,
    /// 使用的模型
    pub model: String,
    /// Token 使用量
    pub usage: Option<TokenUsage>,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
}

/// Token 使用量
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    /// 输入 token 数
    pub input_tokens: u32,
    /// 输出 token 数
    pub output_tokens: u32,
}

/// 流式响应事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    /// 文本增量
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    /// 完成
    #[serde(rename = "done")]
    Done { usage: Option<TokenUsage> },
    /// 错误
    #[serde(rename = "error")]
    Error { message: String },
}
