//! Anthropic SSE 事件生成器
//!
//! 将 AWS Event Stream 事件转换为 Anthropic SSE 格式。
//!
//! # 需求覆盖
//!
//! - 需求 1.3: 收到 content chunk 时立即发送 `content_block_delta` 事件
//! - 需求 1.4: 收到 toolUse 事件时累积 input 数据直到收到 stop 事件
//! - 需求 1.5: 流式传输开始时发送 `message_start` 和 `content_block_start` 事件
//! - 需求 1.6: 流式传输完成时发送 `content_block_stop`、`message_delta` 和 `message_stop` 事件

use crate::streaming::aws_parser::AwsEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// 工具调用状态
///
/// 用于累积多个事件的工具调用数据
/// 对应需求 1.4
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallState {
    /// 工具调用 ID
    pub tool_use_id: String,
    /// 工具名称
    pub name: String,
    /// 累积的输入 JSON
    pub input: String,
    /// 内容块索引
    pub index: usize,
}

/// Anthropic SSE 事件生成器
///
/// 将 AWS Event Stream 事件转换为 Anthropic SSE 格式的事件字符串。
///
/// # 示例
///
/// ```ignore
/// use proxycast::streaming::anthropic_sse::AnthropicSseGenerator;
/// use proxycast::streaming::aws_parser::AwsEvent;
///
/// let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");
///
/// // 处理内容事件
/// let events = generator.process_event(AwsEvent::Content {
///     text: "Hello".to_string()
/// });
///
/// // 完成流
/// let final_events = generator.finalize();
/// ```
#[derive(Debug)]
pub struct AnthropicSseGenerator {
    /// 消息 ID
    message_id: String,
    /// 模型名称
    model: String,
    /// 当前内容块索引
    block_index: usize,
    /// 是否已发送 message_start 事件
    has_sent_message_start: bool,
    /// 是否已发送第一个 content_block_start 事件（文本类型）
    has_sent_text_block_start: bool,
    /// 工具调用状态映射 (tool_use_id -> ToolCallState)
    tool_calls: HashMap<String, ToolCallState>,
    /// 累积的文本内容
    total_content: String,
    /// 输入 token 数量
    input_tokens: u32,
    /// 输出 token 数量
    output_tokens: u32,
}

impl Default for AnthropicSseGenerator {
    fn default() -> Self {
        Self::new("")
    }
}

impl AnthropicSseGenerator {
    /// 创建新的生成器
    ///
    /// # 参数
    ///
    /// * `model` - 模型名称
    pub fn new(model: &str) -> Self {
        Self {
            message_id: format!("msg_{}", Uuid::new_v4().to_string().replace("-", "")),
            model: model.to_string(),
            block_index: 0,
            has_sent_message_start: false,
            has_sent_text_block_start: false,
            tool_calls: HashMap::new(),
            total_content: String::new(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    /// 获取消息 ID
    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    /// 获取累积的文本内容
    pub fn total_content(&self) -> &str {
        &self.total_content
    }

    /// 获取所有工具调用
    pub fn tool_calls(&self) -> &HashMap<String, ToolCallState> {
        &self.tool_calls
    }

    /// 获取当前内容块索引
    pub fn block_index(&self) -> usize {
        self.block_index
    }

    /// 处理 AWS 事件，生成 SSE 事件字符串
    ///
    /// 对应需求 1.3, 1.4, 1.5
    ///
    /// # 参数
    ///
    /// * `event` - AWS Event Stream 事件
    ///
    /// # 返回
    ///
    /// SSE 事件字符串列表
    pub fn process_event(&mut self, event: AwsEvent) -> Vec<String> {
        let mut sse_events = Vec::new();

        // 确保发送 message_start（需求 1.5）
        if !self.has_sent_message_start {
            sse_events.push(self.create_message_start());
            self.has_sent_message_start = true;
        }

        match event {
            AwsEvent::Content { text } => {
                // 需求 1.3: 立即发送 content_block_delta 事件
                sse_events.extend(self.handle_content(&text));
            }
            AwsEvent::ToolUseStart { id, name } => {
                // 需求 1.4: 工具调用开始
                sse_events.extend(self.handle_tool_use_start(&id, &name));
            }
            AwsEvent::ToolUseInput { id, input } => {
                // 需求 1.4: 累积工具调用输入
                sse_events.extend(self.handle_tool_use_input(&id, &input));
            }
            AwsEvent::ToolUseStop { id } => {
                // 需求 1.4: 工具调用结束
                sse_events.extend(self.handle_tool_use_stop(&id));
            }
            AwsEvent::Stop => {
                // 流结束信号，在 finalize() 中处理
            }
            AwsEvent::Usage {
                credits: _,
                context_percentage: _,
            } => {
                // Usage 信息在 finalize() 中处理
            }
            AwsEvent::FollowupPrompt { .. } | AwsEvent::ParseError { .. } => {
                // 忽略这些事件
            }
        }

        sse_events
    }

    /// 生成流结束事件
    ///
    /// 对应需求 1.6
    ///
    /// # 返回
    ///
    /// 流结束的 SSE 事件字符串列表
    pub fn finalize(&mut self) -> Vec<String> {
        let mut sse_events = Vec::new();

        // 如果还没发送 message_start，先发送
        if !self.has_sent_message_start {
            sse_events.push(self.create_message_start());
            self.has_sent_message_start = true;
        }

        // 关闭文本内容块（如果有）
        if self.has_sent_text_block_start && !self.total_content.is_empty() {
            sse_events.push(self.create_content_block_stop(0));
        }

        // 关闭所有未关闭的工具调用
        let tool_ids: Vec<String> = self.tool_calls.keys().cloned().collect();
        for id in tool_ids {
            if let Some(state) = self.tool_calls.get(&id) {
                sse_events.push(self.create_content_block_stop(state.index));
            }
        }

        // 发送 message_delta（需求 1.6）
        sse_events.push(self.create_message_delta());

        // 发送 message_stop（需求 1.6）
        sse_events.push(self.create_message_stop());

        sse_events
    }

    /// 重置生成器状态
    pub fn reset(&mut self) {
        self.message_id = format!("msg_{}", Uuid::new_v4().to_string().replace("-", ""));
        self.block_index = 0;
        self.has_sent_message_start = false;
        self.has_sent_text_block_start = false;
        self.tool_calls.clear();
        self.total_content.clear();
        self.input_tokens = 0;
        self.output_tokens = 0;
    }

    // ========================================================================
    // 内部处理方法
    // ========================================================================

    /// 处理内容事件
    fn handle_content(&mut self, text: &str) -> Vec<String> {
        let mut events = Vec::new();

        // 如果是第一个内容，发送 content_block_start（需求 1.5）
        if !self.has_sent_text_block_start {
            events.push(self.create_content_block_start_text(0));
            self.has_sent_text_block_start = true;
            self.block_index = 1; // 下一个块从索引 1 开始
        }

        // 累积内容
        self.total_content.push_str(text);

        // 发送 content_block_delta（需求 1.3）
        events.push(self.create_text_delta(0, text));

        events
    }

    /// 处理工具调用开始
    fn handle_tool_use_start(&mut self, id: &str, name: &str) -> Vec<String> {
        let mut events = Vec::new();

        // 如果有文本内容块且未关闭，先关闭它
        if self.has_sent_text_block_start && !self.total_content.is_empty() {
            events.push(self.create_content_block_stop(0));
        }

        let index = self.block_index;
        self.block_index += 1;

        // 创建工具调用状态
        self.tool_calls.insert(
            id.to_string(),
            ToolCallState {
                tool_use_id: id.to_string(),
                name: name.to_string(),
                input: String::new(),
                index,
            },
        );

        // 发送 content_block_start (tool_use)
        events.push(self.create_content_block_start_tool(index, id, name));

        events
    }

    /// 处理工具调用输入
    fn handle_tool_use_input(&mut self, id: &str, input: &str) -> Vec<String> {
        let mut events = Vec::new();

        if let Some(state) = self.tool_calls.get_mut(id) {
            // 累积输入（需求 1.4）
            state.input.push_str(input);
            let index = state.index;

            // 发送 input_json_delta
            events.push(self.create_input_json_delta(index, input));
        }

        events
    }

    /// 处理工具调用结束
    fn handle_tool_use_stop(&mut self, id: &str) -> Vec<String> {
        let mut events = Vec::new();

        if let Some(state) = self.tool_calls.remove(id) {
            // 发送 content_block_stop
            events.push(self.create_content_block_stop(state.index));
        }

        events
    }

    // ========================================================================
    // SSE 事件创建方法
    // ========================================================================

    /// 创建 message_start 事件
    fn create_message_start(&self) -> String {
        let event = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": self.message_id,
                "type": "message",
                "role": "assistant",
                "model": self.model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": self.input_tokens,
                    "output_tokens": self.output_tokens
                }
            }
        });
        format!("event: message_start\ndata: {}\n\n", event)
    }

    /// 创建文本类型的 content_block_start 事件
    fn create_content_block_start_text(&self, index: usize) -> String {
        let event = serde_json::json!({
            "type": "content_block_start",
            "index": index,
            "content_block": {
                "type": "text",
                "text": ""
            }
        });
        format!("event: content_block_start\ndata: {}\n\n", event)
    }

    /// 创建工具调用类型的 content_block_start 事件
    fn create_content_block_start_tool(&self, index: usize, id: &str, name: &str) -> String {
        let event = serde_json::json!({
            "type": "content_block_start",
            "index": index,
            "content_block": {
                "type": "tool_use",
                "id": id,
                "name": name,
                "input": {}
            }
        });
        format!("event: content_block_start\ndata: {}\n\n", event)
    }

    /// 创建文本增量事件
    fn create_text_delta(&self, index: usize, text: &str) -> String {
        let event = serde_json::json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {
                "type": "text_delta",
                "text": text
            }
        });
        format!("event: content_block_delta\ndata: {}\n\n", event)
    }

    /// 创建工具调用输入增量事件
    fn create_input_json_delta(&self, index: usize, partial_json: &str) -> String {
        let event = serde_json::json!({
            "type": "content_block_delta",
            "index": index,
            "delta": {
                "type": "input_json_delta",
                "partial_json": partial_json
            }
        });
        format!("event: content_block_delta\ndata: {}\n\n", event)
    }

    /// 创建 content_block_stop 事件
    fn create_content_block_stop(&self, index: usize) -> String {
        let event = serde_json::json!({
            "type": "content_block_stop",
            "index": index
        });
        format!("event: content_block_stop\ndata: {}\n\n", event)
    }

    /// 创建 message_delta 事件
    fn create_message_delta(&self) -> String {
        let stop_reason = if self.tool_calls.is_empty() {
            "end_turn"
        } else {
            "tool_use"
        };

        let event = serde_json::json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": stop_reason,
                "stop_sequence": null
            },
            "usage": {
                "output_tokens": self.output_tokens
            }
        });
        format!("event: message_delta\ndata: {}\n\n", event)
    }

    /// 创建 message_stop 事件
    fn create_message_stop(&self) -> String {
        let event = serde_json::json!({
            "type": "message_stop"
        });
        format!("event: message_stop\ndata: {}\n\n", event)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generator_new() {
        let generator = AnthropicSseGenerator::new("claude-3-sonnet");
        assert_eq!(generator.model, "claude-3-sonnet");
        assert!(generator.message_id.starts_with("msg_"));
        assert_eq!(generator.block_index, 0);
        assert!(!generator.has_sent_message_start);
        assert!(!generator.has_sent_text_block_start);
        assert!(generator.tool_calls.is_empty());
        assert!(generator.total_content.is_empty());
    }

    #[test]
    fn test_process_content_event() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        let events = generator.process_event(AwsEvent::Content {
            text: "Hello".to_string(),
        });

        // 应该生成 3 个事件: message_start, content_block_start, content_block_delta
        assert_eq!(events.len(), 3);
        assert!(events[0].contains("message_start"));
        assert!(events[1].contains("content_block_start"));
        assert!(events[2].contains("content_block_delta"));
        assert!(events[2].contains("Hello"));

        // 验证状态
        assert!(generator.has_sent_message_start);
        assert!(generator.has_sent_text_block_start);
        assert_eq!(generator.total_content, "Hello");
    }

    #[test]
    fn test_process_multiple_content_events() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 第一个内容
        let events1 = generator.process_event(AwsEvent::Content {
            text: "Hello".to_string(),
        });
        assert_eq!(events1.len(), 3);

        // 第二个内容（不应该再发送 message_start 和 content_block_start）
        let events2 = generator.process_event(AwsEvent::Content {
            text: ", world!".to_string(),
        });
        assert_eq!(events2.len(), 1);
        assert!(events2[0].contains("content_block_delta"));
        assert!(events2[0].contains(", world!"));

        // 验证累积内容
        assert_eq!(generator.total_content, "Hello, world!");
    }

    #[test]
    fn test_process_tool_use_start() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        let events = generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_123".to_string(),
            name: "read_file".to_string(),
        });

        // 应该生成 2 个事件: message_start, content_block_start (tool_use)
        assert_eq!(events.len(), 2);
        assert!(events[0].contains("message_start"));
        assert!(events[1].contains("content_block_start"));
        assert!(events[1].contains("tool_use"));
        assert!(events[1].contains("tool_123"));
        assert!(events[1].contains("read_file"));

        // 验证工具调用状态
        assert!(generator.tool_calls.contains_key("tool_123"));
        let state = generator.tool_calls.get("tool_123").unwrap();
        assert_eq!(state.name, "read_file");
        assert_eq!(state.index, 0);
    }

    #[test]
    fn test_process_tool_use_input() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 先开始工具调用
        generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_123".to_string(),
            name: "read_file".to_string(),
        });

        // 发送输入
        let events = generator.process_event(AwsEvent::ToolUseInput {
            id: "tool_123".to_string(),
            input: "{\"path\":".to_string(),
        });

        assert_eq!(events.len(), 1);
        assert!(events[0].contains("input_json_delta"));
        assert!(events[0].contains("{\\\"path\\\":"));

        // 验证累积的输入
        let state = generator.tool_calls.get("tool_123").unwrap();
        assert_eq!(state.input, "{\"path\":");
    }

    #[test]
    fn test_process_tool_use_stop() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 开始工具调用
        generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_123".to_string(),
            name: "read_file".to_string(),
        });

        // 发送输入
        generator.process_event(AwsEvent::ToolUseInput {
            id: "tool_123".to_string(),
            input: "{\"path\":\"/tmp\"}".to_string(),
        });

        // 结束工具调用
        let events = generator.process_event(AwsEvent::ToolUseStop {
            id: "tool_123".to_string(),
        });

        assert_eq!(events.len(), 1);
        assert!(events[0].contains("content_block_stop"));

        // 工具调用应该被移除
        assert!(!generator.tool_calls.contains_key("tool_123"));
    }

    #[test]
    fn test_finalize() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 发送一些内容
        generator.process_event(AwsEvent::Content {
            text: "Hello".to_string(),
        });

        // 完成流
        let events = generator.finalize();

        // 应该生成 3 个事件: content_block_stop, message_delta, message_stop
        assert_eq!(events.len(), 3);
        assert!(events[0].contains("content_block_stop"));
        assert!(events[1].contains("message_delta"));
        assert!(events[1].contains("end_turn"));
        assert!(events[2].contains("message_stop"));
    }

    #[test]
    fn test_finalize_with_tool_calls() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 开始工具调用但不结束
        generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_123".to_string(),
            name: "read_file".to_string(),
        });

        // 完成流
        let events = generator.finalize();

        // 应该关闭未完成的工具调用
        assert!(events.iter().any(|e| e.contains("content_block_stop")));
        assert!(events.iter().any(|e| e.contains("message_delta")));
        assert!(events.iter().any(|e| e.contains("message_stop")));
    }

    #[test]
    fn test_content_then_tool_use() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 先发送内容
        generator.process_event(AwsEvent::Content {
            text: "Let me read the file.".to_string(),
        });

        // 然后开始工具调用
        let events = generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_123".to_string(),
            name: "read_file".to_string(),
        });

        // 应该先关闭文本块，然后开始工具调用块
        assert_eq!(events.len(), 2);
        assert!(events[0].contains("content_block_stop"));
        assert!(events[1].contains("content_block_start"));
        assert!(events[1].contains("tool_use"));

        // 工具调用的索引应该是 1
        let state = generator.tool_calls.get("tool_123").unwrap();
        assert_eq!(state.index, 1);
    }

    #[test]
    fn test_reset() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 处理一些事件
        generator.process_event(AwsEvent::Content {
            text: "Hello".to_string(),
        });

        let old_message_id = generator.message_id.clone();

        // 重置
        generator.reset();

        // 验证状态被重置
        assert_ne!(generator.message_id, old_message_id);
        assert_eq!(generator.block_index, 0);
        assert!(!generator.has_sent_message_start);
        assert!(!generator.has_sent_text_block_start);
        assert!(generator.tool_calls.is_empty());
        assert!(generator.total_content.is_empty());
    }

    #[test]
    fn test_multiple_tool_calls() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 开始第一个工具调用
        generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_1".to_string(),
            name: "read_file".to_string(),
        });

        // 开始第二个工具调用
        generator.process_event(AwsEvent::ToolUseStart {
            id: "tool_2".to_string(),
            name: "write_file".to_string(),
        });

        // 验证两个工具调用都被跟踪
        assert_eq!(generator.tool_calls.len(), 2);
        assert_eq!(generator.tool_calls.get("tool_1").unwrap().index, 0);
        assert_eq!(generator.tool_calls.get("tool_2").unwrap().index, 1);
    }

    #[test]
    fn test_sse_event_format() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        let events = generator.process_event(AwsEvent::Content {
            text: "Test".to_string(),
        });

        // 验证 SSE 格式
        for event in &events {
            assert!(event.contains("event: "));
            assert!(event.contains("data: "));
            assert!(event.ends_with("\n\n"));
        }
    }

    #[test]
    fn test_ignore_followup_prompt() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 先发送一些内容以触发 message_start
        generator.process_event(AwsEvent::Content {
            text: "Hello".to_string(),
        });

        // FollowupPrompt 应该被忽略
        let events = generator.process_event(AwsEvent::FollowupPrompt {
            content: "suggestion".to_string(),
        });

        assert!(events.is_empty());
    }

    #[test]
    fn test_ignore_parse_error() {
        let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

        // 先发送一些内容以触发 message_start
        generator.process_event(AwsEvent::Content {
            text: "Hello".to_string(),
        });

        // ParseError 应该被忽略
        let events = generator.process_event(AwsEvent::ParseError {
            message: "error".to_string(),
            raw_data: None,
        });

        assert!(events.is_empty());
    }
}

// ============================================================================
// 属性测试
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // 生成随机文本内容
    fn arb_text() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?]{1,100}".prop_map(|s| s)
    }

    // 生成随机工具 ID
    fn arb_tool_id() -> impl Strategy<Value = String> {
        "tool_[a-z0-9]{8}".prop_map(|s| s)
    }

    // 生成随机工具名称
    fn arb_tool_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("read_file".to_string()),
            Just("write_file".to_string()),
            Just("execute_command".to_string()),
            Just("search".to_string()),
        ]
    }

    // 生成随机 JSON 输入片段
    fn arb_json_input() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("{\"path\":".to_string()),
            Just("\"/tmp/test\"".to_string()),
            Just(",\"content\":".to_string()),
            Just("\"hello\"".to_string()),
            Just("}".to_string()),
        ]
    }

    // ========================================================================
    // Property 4: 工具调用累积
    // **Validates: Requirements 1.4**
    // ========================================================================

    proptest! {
        /// Property 4: 工具调用累积
        ///
        /// *对于任意* 工具调用事件序列（start → input* → stop），
        /// 系统应该正确累积所有 input 数据并生成完整的 tool_use block。
        ///
        /// **Validates: Requirements 1.4**
        #[test]
        fn prop_tool_call_accumulation(
            tool_id in arb_tool_id(),
            tool_name in arb_tool_name(),
            inputs in prop::collection::vec(arb_json_input(), 1..5)
        ) {
            let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

            // 开始工具调用
            let start_events = generator.process_event(AwsEvent::ToolUseStart {
                id: tool_id.clone(),
                name: tool_name.clone(),
            });

            // 验证 content_block_start 包含正确的工具信息
            prop_assert!(start_events.iter().any(|e|
                e.contains("content_block_start") &&
                e.contains(&tool_id) &&
                e.contains(&tool_name)
            ));

            // 发送所有输入
            let mut accumulated_input = String::new();
            for input in &inputs {
                let input_events = generator.process_event(AwsEvent::ToolUseInput {
                    id: tool_id.clone(),
                    input: input.clone(),
                });

                // 每个输入都应该生成 input_json_delta 事件
                prop_assert!(input_events.iter().any(|e|
                    e.contains("input_json_delta")
                ));

                accumulated_input.push_str(input);
            }

            // 验证累积的输入
            if let Some(state) = generator.tool_calls.get(&tool_id) {
                prop_assert_eq!(&state.input, &accumulated_input);
            }

            // 结束工具调用
            let stop_events = generator.process_event(AwsEvent::ToolUseStop {
                id: tool_id.clone(),
            });

            // 验证 content_block_stop 事件
            prop_assert!(stop_events.iter().any(|e| e.contains("content_block_stop")));

            // 工具调用应该被移除
            prop_assert!(!generator.tool_calls.contains_key(&tool_id));
        }
    }

    // ========================================================================
    // Property 5: SSE 事件顺序
    // **Validates: Requirements 1.5, 1.6**
    // ========================================================================

    proptest! {
        /// Property 5: SSE 事件顺序
        ///
        /// *对于任意* 流式响应，生成的 SSE 事件应该按正确顺序：
        /// message_start → content_block_start → content_block_delta* →
        /// content_block_stop → message_delta → message_stop
        ///
        /// **Validates: Requirements 1.5, 1.6**
        #[test]
        fn prop_sse_event_order(
            contents in prop::collection::vec(arb_text(), 1..5)
        ) {
            let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");
            let mut all_events = Vec::new();

            // 处理所有内容事件
            for content in &contents {
                let events = generator.process_event(AwsEvent::Content {
                    text: content.clone(),
                });
                all_events.extend(events);
            }

            // 完成流
            let final_events = generator.finalize();
            all_events.extend(final_events);

            // 验证事件顺序
            let mut seen_message_start = false;
            let mut seen_content_block_start = false;
            let mut seen_content_block_delta = false;
            let mut seen_content_block_stop = false;
            let mut seen_message_delta = false;
            let mut seen_message_stop = false;

            for event in &all_events {
                if event.contains("message_start") {
                    // message_start 应该是第一个
                    prop_assert!(!seen_content_block_start);
                    prop_assert!(!seen_content_block_delta);
                    prop_assert!(!seen_content_block_stop);
                    prop_assert!(!seen_message_delta);
                    prop_assert!(!seen_message_stop);
                    seen_message_start = true;
                } else if event.contains("content_block_start") {
                    // content_block_start 应该在 message_start 之后
                    prop_assert!(seen_message_start);
                    prop_assert!(!seen_message_delta);
                    prop_assert!(!seen_message_stop);
                    seen_content_block_start = true;
                } else if event.contains("content_block_delta") {
                    // content_block_delta 应该在 content_block_start 之后
                    prop_assert!(seen_content_block_start);
                    prop_assert!(!seen_message_delta);
                    prop_assert!(!seen_message_stop);
                    seen_content_block_delta = true;
                } else if event.contains("content_block_stop") {
                    // content_block_stop 应该在 content_block_delta 之后
                    prop_assert!(seen_content_block_delta);
                    prop_assert!(!seen_message_stop);
                    seen_content_block_stop = true;
                } else if event.contains("message_delta") {
                    // message_delta 应该在 content_block_stop 之后
                    prop_assert!(seen_content_block_stop);
                    prop_assert!(!seen_message_stop);
                    seen_message_delta = true;
                } else if event.contains("message_stop") {
                    // message_stop 应该是最后一个
                    prop_assert!(seen_message_delta);
                    seen_message_stop = true;
                }
            }

            // 验证所有必要的事件都存在
            prop_assert!(seen_message_start);
            prop_assert!(seen_content_block_start);
            prop_assert!(seen_content_block_delta);
            prop_assert!(seen_content_block_stop);
            prop_assert!(seen_message_delta);
            prop_assert!(seen_message_stop);
        }
    }

    // ========================================================================
    // Property 6: 内容完整性
    // **Validates: Requirements 1.3, 3.3**
    // ========================================================================

    proptest! {
        /// Property 6: 内容完整性
        ///
        /// *对于任意* 流式响应，所有 content_block_delta 事件的文本拼接
        /// 应该等于原始响应的完整内容。
        ///
        /// **Validates: Requirements 1.3, 3.3**
        #[test]
        fn prop_content_integrity(
            contents in prop::collection::vec(arb_text(), 1..10)
        ) {
            let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");
            let mut all_events = Vec::new();

            // 处理所有内容事件
            for content in &contents {
                let events = generator.process_event(AwsEvent::Content {
                    text: content.clone(),
                });
                all_events.extend(events);
            }

            // 从 SSE 事件中提取所有文本
            let mut extracted_content = String::new();
            for event in &all_events {
                if event.contains("content_block_delta") && event.contains("text_delta") {
                    // 解析 JSON 提取文本
                    if let Some(data_start) = event.find("data: ") {
                        let json_str = &event[data_start + 6..event.len() - 2]; // 去掉 "data: " 和 "\n\n"
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                            if let Some(text) = json
                                .get("delta")
                                .and_then(|d| d.get("text"))
                                .and_then(|t| t.as_str())
                            {
                                extracted_content.push_str(text);
                            }
                        }
                    }
                }
            }

            // 验证提取的内容等于原始内容
            let original_content: String = contents.join("");
            prop_assert_eq!(extracted_content, original_content.clone());

            // 验证生成器累积的内容也正确
            prop_assert_eq!(generator.total_content(), original_content);
        }
    }

    // ========================================================================
    // 额外的属性测试
    // ========================================================================

    proptest! {
        /// 验证 message_id 格式正确
        #[test]
        fn prop_message_id_format(_seed in 0u64..1000) {
            let generator = AnthropicSseGenerator::new("claude-3-sonnet");
            prop_assert!(generator.message_id().starts_with("msg_"));
            prop_assert!(generator.message_id().len() > 4);
        }

        /// 验证重置后状态正确
        #[test]
        fn prop_reset_clears_state(
            contents in prop::collection::vec(arb_text(), 1..5)
        ) {
            let mut generator = AnthropicSseGenerator::new("claude-3-sonnet");

            // 处理一些事件
            for content in &contents {
                generator.process_event(AwsEvent::Content {
                    text: content.clone(),
                });
            }

            let old_message_id = generator.message_id().to_string();

            // 重置
            generator.reset();

            // 验证状态被清除
            prop_assert_ne!(generator.message_id(), old_message_id);
            prop_assert_eq!(generator.block_index(), 0);
            prop_assert!(generator.total_content().is_empty());
            prop_assert!(generator.tool_calls().is_empty());
        }
    }
}
