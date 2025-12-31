//! Goose Agent 集成模块
//!
//! 封装 Goose 框架，提供简化的 Agent API
//! 参考: https://github.com/block/goose

use anyhow::Result;
use futures::StreamExt;
use goose::agents::{Agent, AgentEvent, SessionConfig};
use goose::conversation::message::Message;
use goose::providers::create_with_named_model;
use goose::session::session_manager::SessionType;
use goose::session::SessionManager;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

use crate::agent::types::*;

/// Goose Agent 管理器
///
/// 封装 Goose 框架的 Agent，提供简化的 API
pub struct GooseAgentManager {
    /// 底层 Goose Agent
    agent: Arc<Agent>,
    /// Provider 名称
    provider_name: String,
    /// 模型名称
    model_name: String,
    /// Session ID 映射
    sessions: Arc<RwLock<HashMap<String, String>>>,
}

impl GooseAgentManager {
    /// 创建新的 Goose Agent 管理器
    ///
    /// # Arguments
    /// * `provider_name` - Provider 名称 (如 "anthropic", "openai", "ollama")
    /// * `model_name` - 模型名称 (如 "claude-sonnet-4-20250514", "gpt-4o")
    pub async fn new(provider_name: &str, model_name: &str) -> Result<Self> {
        info!(
            "[GooseAgent] 创建 Agent: provider={}, model={}",
            provider_name, model_name
        );

        // 创建 Provider
        let provider = create_with_named_model(provider_name, model_name).await?;

        // 创建 Agent
        let agent = Agent::new();

        // 创建初始 Session
        let session = SessionManager::create_session(
            PathBuf::default(),
            "proxycast-session".to_string(),
            SessionType::Hidden,
        )
        .await?;

        // 设置 Provider
        agent.update_provider(provider, &session.id).await?;

        // 自动加载 ProxyCast Skills
        if let Some(skills_prompt) = Self::generate_skills_prompt() {
            agent.extend_system_prompt(skills_prompt).await;
            info!("[GooseAgent] 已注入 ProxyCast Skills 到 System Prompt");
        }

        info!("[GooseAgent] Agent 创建成功: session_id={}", session.id);

        Ok(Self {
            agent: Arc::new(agent),
            provider_name: provider_name.to_string(),
            model_name: model_name.to_string(),
            sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 获取 Skills 目录列表
    fn get_skills_directories() -> Vec<PathBuf> {
        let mut dirs = Vec::new();

        if let Some(home) = dirs::home_dir() {
            // ProxyCast Skills 目录
            dirs.push(home.join(".proxycast").join("skills"));
            // Claude Code 兼容目录
            dirs.push(home.join(".claude").join("skills"));
        }

        dirs
    }

    /// 解析 SKILL.md 文件的 frontmatter
    fn parse_skill_frontmatter(content: &str) -> Option<(String, String)> {
        // 解析 YAML frontmatter
        if !content.starts_with("---") {
            return None;
        }

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return None;
        }

        let yaml_content = parts[1].trim();

        // 简单解析 name 和 description
        let mut name = None;
        let mut description = None;

        for line in yaml_content.lines() {
            let line = line.trim();
            if let Some(value) = line.strip_prefix("name:") {
                name = Some(value.trim().trim_matches('"').to_string());
            } else if let Some(value) = line.strip_prefix("description:") {
                description = Some(value.trim().trim_matches('"').to_string());
            }
        }

        match (name, description) {
            (Some(n), Some(d)) => Some((n, d)),
            _ => None,
        }
    }

    /// 扫描目录中的 Skills
    fn discover_skills(directories: &[PathBuf]) -> Vec<(String, String)> {
        let mut skills = Vec::new();

        for dir in directories {
            if !dir.exists() {
                continue;
            }

            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let skill_file = path.join("SKILL.md");
                        if skill_file.exists() {
                            if let Ok(content) = fs::read_to_string(&skill_file) {
                                if let Some((name, desc)) = Self::parse_skill_frontmatter(&content)
                                {
                                    skills.push((name, desc));
                                }
                            }
                        }
                    }
                }
            }
        }

        // 按名称排序
        skills.sort_by(|a, b| a.0.cmp(&b.0));
        skills
    }

    /// 生成 Skills 提示词
    fn generate_skills_prompt() -> Option<String> {
        let directories = Self::get_skills_directories();
        let skills = Self::discover_skills(&directories);

        if skills.is_empty() {
            debug!("[GooseAgent] 未发现已安装的 Skills");
            return None;
        }

        let mut prompt =
            String::from("\n\n<available_skills>\nYou have these skills at your disposal:\n\n");

        for (name, description) in &skills {
            prompt.push_str(&format!("- {}: {}\n", name, description));
        }

        prompt.push_str("</available_skills>");

        info!("[GooseAgent] 发现 {} 个 Skills", skills.len());

        Some(prompt)
    }

    /// 发送消息并获取流式响应
    pub async fn send_message(
        &self,
        message: &str,
        session_id: &str,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        debug!(
            "[GooseAgent] 发送消息: session_id={}, message_len={}",
            session_id,
            message.len()
        );

        // 创建用户消息
        let user_message = Message::user().with_text(message);

        // 创建 SessionConfig
        let session_config = SessionConfig {
            id: session_id.to_string(),
            schedule_id: None,
            max_turns: Some(100),
            retry_config: None,
        };

        // 发送消息并获取响应流
        let mut stream = self.agent.reply(user_message, session_config, None).await?;

        let mut full_content = String::new();

        // 处理响应流
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    // 提取文本内容
                    for content in &msg.content {
                        if let Some(text) = content.as_text() {
                            full_content.push_str(&text);
                            let _ = tx
                                .send(StreamEvent::TextDelta {
                                    text: text.to_string(),
                                })
                                .await;
                        }
                    }
                }
                Ok(AgentEvent::McpNotification(_)) => {
                    // MCP 通知，可以忽略或记录
                    debug!("[GooseAgent] MCP 通知");
                }
                Ok(AgentEvent::ModelChange { model, mode }) => {
                    debug!("[GooseAgent] 模型切换: model={}, mode={}", model, mode);
                }
                Ok(AgentEvent::HistoryReplaced(_)) => {
                    debug!("[GooseAgent] 历史替换");
                }
                Err(e) => {
                    error!("[GooseAgent] 流错误: {}", e);
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: format!("流错误: {}", e),
                        })
                        .await;
                    return Err(e);
                }
            }
        }

        // 发送完成事件
        let _ = tx.send(StreamEvent::Done { usage: None }).await;

        info!(
            "[GooseAgent] 消息处理完成: content_len={}",
            full_content.len()
        );

        Ok(())
    }

    /// 创建新会话
    pub async fn create_session(&self, name: Option<String>) -> Result<String> {
        let session_name = name.unwrap_or_else(|| format!("proxycast-{}", uuid::Uuid::new_v4()));

        let session = SessionManager::create_session(
            PathBuf::default(),
            session_name.clone(),
            SessionType::Hidden,
        )
        .await?;

        // 存储会话映射
        self.sessions
            .write()
            .insert(session_name.clone(), session.id.clone());

        info!(
            "[GooseAgent] 创建会话: name={}, id={}",
            session_name, session.id
        );

        Ok(session.id)
    }

    /// 获取 Provider 名称
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// 获取模型名称
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// 扩展系统提示词
    pub async fn extend_system_prompt(&self, instruction: &str) {
        self.agent
            .extend_system_prompt(instruction.to_string())
            .await;
        debug!("[GooseAgent] 扩展系统提示词: len={}", instruction.len());
    }
}

/// Goose Agent 状态 (Tauri State)
#[derive(Clone, Default)]
pub struct GooseAgentState {
    agent: Arc<RwLock<Option<Arc<GooseAgentManager>>>>,
}

impl GooseAgentState {
    pub fn new() -> Self {
        Self {
            agent: Arc::new(RwLock::new(None)),
        }
    }

    /// 初始化 Goose Agent
    pub async fn init(&self, provider_name: &str, model_name: &str) -> Result<(), String> {
        let manager = GooseAgentManager::new(provider_name, model_name)
            .await
            .map_err(|e| format!("初始化 Goose Agent 失败: {}", e))?;

        *self.agent.write() = Some(Arc::new(manager));
        info!("[GooseAgentState] Goose Agent 初始化成功");
        Ok(())
    }

    /// 检查是否已初始化
    pub fn is_initialized(&self) -> bool {
        self.agent.read().is_some()
    }

    /// 重置 Agent
    pub fn reset(&self) {
        *self.agent.write() = None;
        info!("[GooseAgentState] Goose Agent 已重置");
    }

    /// 发送消息（流式）
    pub async fn send_message(
        &self,
        message: &str,
        session_id: &str,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), String> {
        // 先获取 manager 的克隆，然后释放锁
        let manager = {
            let guard = self.agent.read();
            guard
                .as_ref()
                .ok_or_else(|| "Goose Agent 未初始化".to_string())?
                .clone()
        };

        // 创建用户消息
        let user_message = Message::user().with_text(message);

        // 创建 SessionConfig
        let session_config = SessionConfig {
            id: session_id.to_string(),
            schedule_id: None,
            max_turns: Some(100),
            retry_config: None,
        };

        // 发送消息并获取响应流
        let mut stream = manager
            .agent
            .reply(user_message, session_config, None)
            .await
            .map_err(|e| format!("发送消息失败: {}", e))?;

        // 处理响应流
        while let Some(event) = stream.next().await {
            match event {
                Ok(AgentEvent::Message(msg)) => {
                    for content in &msg.content {
                        if let Some(text) = content.as_text() {
                            let _ = tx
                                .send(StreamEvent::TextDelta {
                                    text: text.to_string(),
                                })
                                .await;
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: format!("流错误: {}", e),
                        })
                        .await;
                    return Err(format!("流错误: {}", e));
                }
            }
        }

        let _ = tx.send(StreamEvent::Done { usage: None }).await;
        Ok(())
    }

    /// 创建新会话
    pub async fn create_session(&self, name: Option<String>) -> Result<String, String> {
        // 先获取 manager 的克隆，然后释放锁
        let manager = {
            let guard = self.agent.read();
            guard
                .as_ref()
                .ok_or_else(|| "Goose Agent 未初始化".to_string())?
                .clone()
        };

        manager
            .create_session(name)
            .await
            .map_err(|e| format!("创建会话失败: {}", e))
    }

    /// 扩展系统提示词
    pub async fn extend_system_prompt(&self, instruction: &str) -> Result<(), String> {
        // 先获取 manager 的克隆，然后释放锁
        let manager = {
            let guard = self.agent.read();
            guard
                .as_ref()
                .ok_or_else(|| "Goose Agent 未初始化".to_string())?
                .clone()
        };

        manager.extend_system_prompt(instruction).await;
        Ok(())
    }

    /// 获取 Provider 信息
    pub fn get_provider_info(&self) -> Option<(String, String)> {
        let guard = self.agent.read();
        guard
            .as_ref()
            .map(|m| (m.provider_name.clone(), m.model_name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_goose_state_default() {
        let state = GooseAgentState::new();
        assert!(!state.is_initialized());
    }
}
