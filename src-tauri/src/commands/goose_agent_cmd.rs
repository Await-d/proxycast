//! Goose Agent 命令模块
//!
//! 提供基于 Goose 框架的 Agent Tauri 命令

use crate::agent::{GooseAgentState, StreamEvent};
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tokio::sync::mpsc;
use tracing::{error, info};

/// Goose Agent 状态响应
#[derive(Debug, Serialize)]
pub struct GooseAgentStatus {
    pub initialized: bool,
    pub provider: Option<String>,
    pub model: Option<String>,
}

/// 初始化 Goose Agent
///
/// # Arguments
/// * `provider_name` - Provider 名称 (如 "anthropic", "openai", "ollama")
/// * `model_name` - 模型名称 (如 "claude-sonnet-4-20250514", "gpt-4o")
#[tauri::command]
pub async fn goose_agent_init(
    agent_state: State<'_, GooseAgentState>,
    provider_name: String,
    model_name: String,
) -> Result<GooseAgentStatus, String> {
    info!(
        "[GooseAgent] 初始化: provider={}, model={}",
        provider_name, model_name
    );

    agent_state.init(&provider_name, &model_name).await?;

    Ok(GooseAgentStatus {
        initialized: true,
        provider: Some(provider_name),
        model: Some(model_name),
    })
}

/// 获取 Goose Agent 状态
#[tauri::command]
pub async fn goose_agent_status(
    agent_state: State<'_, GooseAgentState>,
) -> Result<GooseAgentStatus, String> {
    let initialized = agent_state.is_initialized();
    let info = agent_state.get_provider_info();

    Ok(GooseAgentStatus {
        initialized,
        provider: info.as_ref().map(|(p, _)| p.clone()),
        model: info.map(|(_, m)| m),
    })
}

/// 重置 Goose Agent
#[tauri::command]
pub async fn goose_agent_reset(agent_state: State<'_, GooseAgentState>) -> Result<(), String> {
    agent_state.reset();
    info!("[GooseAgent] Agent 已重置");
    Ok(())
}

/// 创建会话响应
#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub session_id: String,
}

/// 创建 Goose Agent 会话
#[tauri::command]
pub async fn goose_agent_create_session(
    agent_state: State<'_, GooseAgentState>,
    name: Option<String>,
) -> Result<CreateSessionResponse, String> {
    let session_id = agent_state.create_session(name).await?;

    info!("[GooseAgent] 创建会话: {}", session_id);

    Ok(CreateSessionResponse { session_id })
}

/// 发送消息请求参数
#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub session_id: String,
    pub message: String,
    pub event_name: String,
}

/// 发送消息到 Goose Agent (流式响应)
///
/// 通过 Tauri 事件发送响应流
#[tauri::command]
pub async fn goose_agent_send_message(
    app_handle: tauri::AppHandle,
    agent_state: State<'_, GooseAgentState>,
    request: SendMessageRequest,
) -> Result<(), String> {
    info!(
        "[GooseAgent] 发送消息: session_id={}, message_len={}",
        request.session_id,
        request.message.len()
    );

    if !agent_state.is_initialized() {
        return Err("Goose Agent 未初始化，请先调用 goose_agent_init".to_string());
    }

    let session_id = request.session_id.clone();
    let message = request.message.clone();
    let event_name = request.event_name.clone();

    // 克隆 agent 信息用于后台任务
    let agent_guard = agent_state.inner().clone();

    // 在后台任务中处理流式响应
    tauri::async_runtime::spawn(async move {
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(100);

        // 启动消息发送任务
        let send_task = {
            let agent_guard = agent_guard.clone();
            let session_id = session_id.clone();
            let message = message.clone();
            tokio::spawn(async move { agent_guard.send_message(&message, &session_id, tx).await })
        };

        // 接收并转发事件
        while let Some(event) = rx.recv().await {
            if let Err(e) = app_handle.emit(&event_name, &event) {
                error!("[GooseAgent] 发送事件失败: {}", e);
                break;
            }

            if matches!(event, StreamEvent::Done { .. } | StreamEvent::Error { .. }) {
                break;
            }
        }

        // 等待发送任务完成
        if let Err(e) = send_task.await {
            error!("[GooseAgent] 发送任务失败: {}", e);
        }
    });

    Ok(())
}

/// 扩展系统提示词
#[tauri::command]
pub async fn goose_agent_extend_system_prompt(
    agent_state: State<'_, GooseAgentState>,
    instruction: String,
) -> Result<(), String> {
    info!("[GooseAgent] 扩展系统提示词: len={}", instruction.len());

    agent_state.extend_system_prompt(&instruction).await
}

/// 获取可用的 Provider 列表
#[derive(Debug, Serialize)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
}

/// 获取 Goose 支持的 Provider 列表
#[tauri::command]
pub async fn goose_agent_list_providers() -> Result<Vec<ProviderInfo>, String> {
    // Goose 支持的主要 Provider
    let providers = vec![
        ProviderInfo {
            name: "anthropic".to_string(),
            display_name: "Anthropic (Claude)".to_string(),
        },
        ProviderInfo {
            name: "openai".to_string(),
            display_name: "OpenAI (GPT)".to_string(),
        },
        ProviderInfo {
            name: "google".to_string(),
            display_name: "Google (Gemini)".to_string(),
        },
        ProviderInfo {
            name: "ollama".to_string(),
            display_name: "Ollama (Local)".to_string(),
        },
        ProviderInfo {
            name: "openrouter".to_string(),
            display_name: "OpenRouter".to_string(),
        },
        ProviderInfo {
            name: "bedrock".to_string(),
            display_name: "AWS Bedrock".to_string(),
        },
        ProviderInfo {
            name: "azure".to_string(),
            display_name: "Azure OpenAI".to_string(),
        },
        ProviderInfo {
            name: "databricks".to_string(),
            display_name: "Databricks".to_string(),
        },
    ];

    Ok(providers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_info_serialize() {
        let info = ProviderInfo {
            name: "anthropic".to_string(),
            display_name: "Anthropic".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("anthropic"));
    }
}
