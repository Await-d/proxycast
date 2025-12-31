//! 流式传输核心模块
//!
//! 该模块提供真正的端到端流式传输支持，将当前的"伪流式"架构改造为
//! 逐 chunk 处理和实时 Flow 监控的真正流式传输。
//!
//! # 主要组件
//!
//! - `error`: 流式错误类型定义
//! - `metrics`: 流式指标类型定义
//! - `aws_parser`: AWS Event Stream 解析器（用于 Kiro/CodeWhisperer）
//! - `anthropic_sse`: Anthropic SSE 事件生成器（将 AWS 事件转换为 Anthropic SSE 格式）
//! - `converter`: 流式格式转换器
//! - `traits`: StreamingProvider trait 定义
//! - `manager`: 流式管理器

pub mod anthropic_sse;
pub mod aws_parser;
pub mod converter;
pub mod error;
pub mod manager;
pub mod metrics;
pub mod traits;

// 重新导出核心类型
pub use anthropic_sse::{AnthropicSseGenerator, ToolCallState};
pub use aws_parser::{
    extract_content, extract_tool_calls, serialize_event, AwsEvent, AwsEventStreamParser,
    ParserState,
};
pub use converter::{
    extract_content_from_sse, extract_tool_calls_from_sse, ConverterState, PartialJsonAccumulator,
    StreamConverter, StreamFormat,
};
pub use error::StreamError;
pub use manager::{
    collect_stream_content, create_flow_monitor_callback, with_timeout, FlowMonitorCallback,
    ManagedStream, ManagedStreamWithCallback, StreamConfig, StreamContext, StreamEvent,
    StreamManager, TimeoutStream,
};
pub use metrics::StreamMetrics;
pub use traits::{
    reqwest_stream_to_stream_response, StreamFormat as TraitsStreamFormat, StreamResponse,
    StreamingProvider,
};
