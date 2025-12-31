//! AI Agent 集成模块
//!
//! 提供 Agent 功能：
//! - Goose Agent: 基于 Goose 框架的完整 Agent 实现
//! - Native Agent: 基于 OpenAI 兼容 API 的简单实现

pub mod goose_agent;
pub mod native_agent;
pub mod types;

// Goose Agent (推荐)
pub use goose_agent::{GooseAgentManager, GooseAgentState};

// Native Agent (简单实现)
pub use native_agent::{NativeAgent, NativeAgentState};

// 公共类型
pub use types::*;
