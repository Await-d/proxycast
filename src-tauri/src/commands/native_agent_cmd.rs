//! 原生 Agent 命令模块
//!
//! 提供原生 Rust Agent 的 Tauri 命令，替代 aster sidecar 方案

use crate::agent::{
    AgentSession, ImageData, NativeAgent, NativeAgentState, NativeChatRequest, NativeChatResponse,
    StreamEvent,
};
use crate::AppState;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, State};
use tokio::sync::mpsc;

#[derive(Debug, Serialize)]
pub struct NativeAgentStatus {
    pub initialized: bool,
    pub base_url: Option<String>,
}

#[tauri::command]
pub async fn native_agent_init(
    agent_state: State<'_, NativeAgentState>,
    app_state: State<'_, AppState>,
) -> Result<NativeAgentStatus, String> {
    tracing::info!("[NativeAgent] 初始化 Agent");

    let (port, api_key, running) = {
        let state = app_state.read().await;
        (
            state.config.server.port,
            state.running_api_key.clone(),
            state.running,
        )
    };

    if !running {
        return Err("ProxyCast API Server 未运行，请先启动服务器".to_string());
    }

    let api_key = api_key.ok_or_else(|| "ProxyCast API Server 未配置 API Key".to_string())?;

    let base_url = format!("http://127.0.0.1:{}", port);

    agent_state.init(base_url.clone(), api_key)?;

    tracing::info!("[NativeAgent] Agent 初始化成功: {}", base_url);

    Ok(NativeAgentStatus {
        initialized: true,
        base_url: Some(base_url),
    })
}

#[tauri::command]
pub async fn native_agent_status(
    agent_state: State<'_, NativeAgentState>,
) -> Result<NativeAgentStatus, String> {
    Ok(NativeAgentStatus {
        initialized: agent_state.is_initialized(),
        base_url: None,
    })
}

#[tauri::command]
pub async fn native_agent_reset(agent_state: State<'_, NativeAgentState>) -> Result<(), String> {
    agent_state.reset();
    tracing::info!("[NativeAgent] Agent 已重置");
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ImageInputParam {
    pub data: String,
    pub media_type: String,
}

#[tauri::command]
pub async fn native_agent_chat(
    agent_state: State<'_, NativeAgentState>,
    app_state: State<'_, AppState>,
    message: String,
    model: Option<String>,
    images: Option<Vec<ImageInputParam>>,
) -> Result<NativeChatResponse, String> {
    tracing::info!(
        "[NativeAgent] 发送消息: message_len={}, model={:?}",
        message.len(),
        model
    );

    // 如果 Agent 未初始化，自动初始化
    if !agent_state.is_initialized() {
        let (port, api_key, running) = {
            let state = app_state.read().await;
            (
                state.config.server.port,
                state.running_api_key.clone(),
                state.running,
            )
        };

        if !running {
            return Err("ProxyCast API Server 未运行".to_string());
        }

        let api_key = api_key.ok_or_else(|| "未配置 API Key".to_string())?;
        let base_url = format!("http://127.0.0.1:{}", port);
        agent_state.init(base_url, api_key)?;
    }

    let request = NativeChatRequest {
        session_id: None,
        message,
        model,
        images: images.map(|imgs| {
            imgs.into_iter()
                .map(|img| ImageData {
                    data: img.data,
                    media_type: img.media_type,
                })
                .collect()
        }),
        stream: false,
    };

    // 使用 chat_sync 方法避免跨 await 持有锁
    agent_state.chat(request).await
}

#[tauri::command]
pub async fn native_agent_chat_stream(
    app_handle: tauri::AppHandle,
    agent_state: State<'_, NativeAgentState>,
    app_state: State<'_, AppState>,
    message: String,
    model: Option<String>,
    images: Option<Vec<ImageInputParam>>,
    event_name: String,
) -> Result<(), String> {
    tracing::info!(
        "[NativeAgent] 发送流式消息: message_len={}, model={:?}, event={}",
        message.len(),
        model,
        event_name
    );

    // 如果 Agent 未初始化，自动初始化
    if !agent_state.is_initialized() {
        let (port, api_key, running) = {
            let state = app_state.read().await;
            (
                state.config.server.port,
                state.running_api_key.clone(),
                state.running,
            )
        };

        if !running {
            return Err("ProxyCast API Server 未运行".to_string());
        }

        let api_key = api_key.ok_or_else(|| "未配置 API Key".to_string())?;
        let base_url = format!("http://127.0.0.1:{}", port);
        agent_state.init(base_url, api_key)?;
    }

    // 获取配置用于创建独立的 Agent
    let (base_url, api_key) = {
        let state = app_state.read().await;
        let base_url = format!("http://127.0.0.1:{}", state.config.server.port);
        let api_key = state
            .running_api_key
            .clone()
            .ok_or_else(|| "未配置 API Key".to_string())?;
        (base_url, api_key)
    };

    let request = NativeChatRequest {
        session_id: None,
        message,
        model,
        images: images.map(|imgs| {
            imgs.into_iter()
                .map(|img| ImageData {
                    data: img.data,
                    media_type: img.media_type,
                })
                .collect()
        }),
        stream: true,
    };

    // 在后台任务中处理流式响应
    let event_name_clone = event_name.clone();
    tauri::async_runtime::spawn(async move {
        let agent = match NativeAgent::new(base_url, api_key) {
            Ok(a) => a,
            Err(e) => {
                let _ = app_handle.emit(
                    &event_name_clone,
                    StreamEvent::Error {
                        message: e.to_string(),
                    },
                );
                return;
            }
        };

        let (tx, mut rx) = mpsc::channel::<StreamEvent>(100);

        let stream_task = tokio::spawn(async move { agent.chat_stream(request, tx).await });

        while let Some(event) = rx.recv().await {
            if let Err(e) = app_handle.emit(&event_name_clone, &event) {
                tracing::error!("[NativeAgent] 发送事件失败: {}", e);
                break;
            }

            if matches!(event, StreamEvent::Done { .. } | StreamEvent::Error { .. }) {
                break;
            }
        }

        let _ = stream_task.await;
    });

    Ok(())
}

#[tauri::command]
pub async fn native_agent_create_session(
    agent_state: State<'_, NativeAgentState>,
    model: Option<String>,
    system_prompt: Option<String>,
) -> Result<String, String> {
    agent_state.create_session(model, system_prompt)
}

#[tauri::command]
pub async fn native_agent_get_session(
    agent_state: State<'_, NativeAgentState>,
    session_id: String,
) -> Result<Option<AgentSession>, String> {
    agent_state.get_session(&session_id)
}

#[tauri::command]
pub async fn native_agent_delete_session(
    agent_state: State<'_, NativeAgentState>,
    session_id: String,
) -> Result<bool, String> {
    Ok(agent_state.delete_session(&session_id))
}

#[tauri::command]
pub async fn native_agent_list_sessions(
    agent_state: State<'_, NativeAgentState>,
) -> Result<Vec<AgentSession>, String> {
    Ok(agent_state.list_sessions())
}
