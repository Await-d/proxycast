//! Goose Agent 模块测试
//!
//! 测试 GooseAgentManager 和 GooseAgentState 的基本功能

use proxycast_lib::agent::{GooseAgentState, StreamEvent};

#[test]
fn test_goose_agent_state_creation() {
    let state = GooseAgentState::new();
    assert!(!state.is_initialized());
}

#[test]
fn test_goose_agent_state_not_initialized() {
    let state = GooseAgentState::new();
    let info = state.get_provider_info();
    assert!(info.is_none());
}

#[test]
fn test_stream_event_serialization() {
    let event = StreamEvent::TextDelta {
        text: "Hello".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("text_delta"));
    assert!(json.contains("Hello"));
}

#[test]
fn test_stream_event_done() {
    let event = StreamEvent::Done { usage: None };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("done"));
}

#[test]
fn test_stream_event_error() {
    let event = StreamEvent::Error {
        message: "Test error".to_string(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("error"));
    assert!(json.contains("Test error"));
}
