//! Token 追踪模块
//!
//! 提供 Token 计数记录、估算和统计功能

#![allow(dead_code)]

use crate::ProviderType;
use chrono::{DateTime, Duration, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};

/// Token 使用记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsageRecord {
    /// 唯一记录 ID
    pub id: String,
    /// 记录时间戳
    pub timestamp: DateTime<Utc>,
    /// Provider 类型
    pub provider: ProviderType,
    /// 模型名称
    pub model: String,
    /// 输入 Token 数
    pub input_tokens: u32,
    /// 输出 Token 数
    pub output_tokens: u32,
    /// 总 Token 数
    pub total_tokens: u32,
    /// Token 来源（实际值或估算值）
    pub source: TokenSource,
    /// 关联的请求 ID
    pub request_id: Option<String>,
}

impl TokenUsageRecord {
    /// 创建新的 Token 使用记录
    pub fn new(
        id: String,
        provider: ProviderType,
        model: String,
        input_tokens: u32,
        output_tokens: u32,
        source: TokenSource,
    ) -> Self {
        Self {
            id,
            timestamp: Utc::now(),
            provider,
            model,
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
            source,
            request_id: None,
        }
    }

    /// 设置关联的请求 ID
    pub fn with_request_id(mut self, request_id: String) -> Self {
        self.request_id = Some(request_id);
        self
    }
}

/// Token 来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenSource {
    /// Provider 返回的实际值
    Actual,
    /// 使用 tiktoken 估算的值
    Estimated,
}

impl std::fmt::Display for TokenSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenSource::Actual => write!(f, "actual"),
            TokenSource::Estimated => write!(f, "estimated"),
        }
    }
}

/// Token 统计摘要
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenStatsSummary {
    /// 总输入 Token 数
    pub total_input_tokens: u64,
    /// 总输出 Token 数
    pub total_output_tokens: u64,
    /// 总 Token 数
    pub total_tokens: u64,
    /// 记录数量
    pub record_count: u64,
    /// 实际值记录数
    pub actual_count: u64,
    /// 估算值记录数
    pub estimated_count: u64,
    /// 平均输入 Token 数
    pub avg_input_tokens: f64,
    /// 平均输出 Token 数
    pub avg_output_tokens: f64,
}

impl TokenStatsSummary {
    /// 从记录列表计算统计摘要
    pub fn from_records(records: &[TokenUsageRecord]) -> Self {
        if records.is_empty() {
            return Self::default();
        }

        let record_count = records.len() as u64;
        let total_input_tokens: u64 = records.iter().map(|r| r.input_tokens as u64).sum();
        let total_output_tokens: u64 = records.iter().map(|r| r.output_tokens as u64).sum();
        let total_tokens = total_input_tokens + total_output_tokens;
        let actual_count = records
            .iter()
            .filter(|r| r.source == TokenSource::Actual)
            .count() as u64;
        let estimated_count = records
            .iter()
            .filter(|r| r.source == TokenSource::Estimated)
            .count() as u64;

        Self {
            total_input_tokens,
            total_output_tokens,
            total_tokens,
            record_count,
            actual_count,
            estimated_count,
            avg_input_tokens: total_input_tokens as f64 / record_count as f64,
            avg_output_tokens: total_output_tokens as f64 / record_count as f64,
        }
    }
}

/// Provider Token 统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderTokenStats {
    /// Provider 类型
    pub provider: Option<ProviderType>,
    /// 统计摘要
    #[serde(flatten)]
    pub summary: TokenStatsSummary,
}

impl ProviderTokenStats {
    /// 从记录列表计算 Provider Token 统计
    pub fn from_records(provider: ProviderType, records: &[TokenUsageRecord]) -> Self {
        Self {
            provider: Some(provider),
            summary: TokenStatsSummary::from_records(records),
        }
    }
}

/// 模型 Token 统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelTokenStats {
    /// 模型名称
    pub model: String,
    /// 统计摘要
    #[serde(flatten)]
    pub summary: TokenStatsSummary,
}

impl ModelTokenStats {
    /// 从记录列表计算模型 Token 统计
    pub fn from_records(model: String, records: &[TokenUsageRecord]) -> Self {
        Self {
            model,
            summary: TokenStatsSummary::from_records(records),
        }
    }
}

/// 时间段 Token 统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeriodTokenStats {
    /// 时间段开始
    pub period_start: Option<DateTime<Utc>>,
    /// 时间段结束
    pub period_end: Option<DateTime<Utc>>,
    /// 统计摘要
    #[serde(flatten)]
    pub summary: TokenStatsSummary,
}

/// Token 追踪器
///
/// 管理 Token 使用记录的存储、查询和统计
pub struct TokenTracker {
    /// Token 使用记录队列
    records: RwLock<VecDeque<TokenUsageRecord>>,
    /// 记录保留时长
    retention: Duration,
    /// 最大记录条数
    max_records: usize,
}

impl TokenTracker {
    /// 创建新的 Token 追踪器
    ///
    /// # Arguments
    /// * `retention` - 记录保留时长
    /// * `max_records` - 最大记录条数
    pub fn new(retention: Duration, max_records: usize) -> Self {
        Self {
            records: RwLock::new(VecDeque::with_capacity(max_records)),
            retention,
            max_records,
        }
    }

    /// 使用默认配置创建 Token 追踪器（保留 30 天，最多 50000 条）
    pub fn with_defaults() -> Self {
        Self::new(Duration::days(30), 50000)
    }

    /// 记录 Token 使用
    pub fn record(&self, record: TokenUsageRecord) {
        let mut records = self.records.write();
        records.push_back(record);

        // 清理超出数量限制的记录
        while records.len() > self.max_records {
            records.pop_front();
        }

        // 清理过期记录
        let cutoff = Utc::now() - self.retention;
        while let Some(front) = records.front() {
            if front.timestamp < cutoff {
                records.pop_front();
            } else {
                break;
            }
        }
    }

    /// 从响应中提取并记录 Token 使用
    ///
    /// 从 Provider 响应中提取 Token 计数信息
    pub fn record_from_response(
        &self,
        request_id: String,
        provider: ProviderType,
        model: String,
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
    ) {
        // 只有当至少有一个 Token 值时才记录
        if input_tokens.is_some() || output_tokens.is_some() {
            let record = TokenUsageRecord::new(
                uuid::Uuid::new_v4().to_string(),
                provider,
                model,
                input_tokens.unwrap_or(0),
                output_tokens.unwrap_or(0),
                TokenSource::Actual,
            )
            .with_request_id(request_id);

            self.record(record);
        }
    }

    /// 获取所有记录
    pub fn get_all(&self) -> Vec<TokenUsageRecord> {
        self.records.read().iter().cloned().collect()
    }

    /// 获取指定时间范围内的记录
    pub fn get_by_time_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Vec<TokenUsageRecord> {
        self.records
            .read()
            .iter()
            .filter(|r| r.timestamp >= start && r.timestamp <= end)
            .cloned()
            .collect()
    }

    /// 按 Provider 过滤记录
    pub fn get_by_provider(&self, provider: ProviderType) -> Vec<TokenUsageRecord> {
        self.records
            .read()
            .iter()
            .filter(|r| r.provider == provider)
            .cloned()
            .collect()
    }

    /// 按模型过滤记录
    pub fn get_by_model(&self, model: &str) -> Vec<TokenUsageRecord> {
        self.records
            .read()
            .iter()
            .filter(|r| r.model == model)
            .cloned()
            .collect()
    }

    /// 获取记录数量
    pub fn len(&self) -> usize {
        self.records.read().len()
    }

    /// 检查是否为空
    pub fn is_empty(&self) -> bool {
        self.records.read().is_empty()
    }

    /// 清空所有记录
    pub fn clear(&self) {
        self.records.write().clear();
    }

    /// 获取统计摘要
    pub fn summary(
        &self,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> TokenStatsSummary {
        let records = match (start, end) {
            (Some(s), Some(e)) => self.get_by_time_range(s, e),
            _ => self.get_all(),
        };
        TokenStatsSummary::from_records(&records)
    }

    /// 按 Provider 分组统计
    pub fn by_provider(
        &self,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> HashMap<ProviderType, ProviderTokenStats> {
        let records = match (start, end) {
            (Some(s), Some(e)) => self.get_by_time_range(s, e),
            _ => self.get_all(),
        };

        let mut grouped: HashMap<ProviderType, Vec<TokenUsageRecord>> = HashMap::new();
        for record in records {
            grouped.entry(record.provider).or_default().push(record);
        }

        grouped
            .into_iter()
            .map(|(provider, records)| {
                (
                    provider,
                    ProviderTokenStats::from_records(provider, &records),
                )
            })
            .collect()
    }

    /// 按模型分组统计
    pub fn by_model(
        &self,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> HashMap<String, ModelTokenStats> {
        let records = match (start, end) {
            (Some(s), Some(e)) => self.get_by_time_range(s, e),
            _ => self.get_all(),
        };

        let mut grouped: HashMap<String, Vec<TokenUsageRecord>> = HashMap::new();
        for record in records {
            grouped
                .entry(record.model.clone())
                .or_default()
                .push(record);
        }

        grouped
            .into_iter()
            .map(|(model, records)| {
                let stats = ModelTokenStats::from_records(model.clone(), &records);
                (model, stats)
            })
            .collect()
    }

    /// 按时间段汇总（按天）
    pub fn by_day(&self, days: i64) -> Vec<PeriodTokenStats> {
        let now = Utc::now();
        let mut result = Vec::new();

        for day_offset in 0..days {
            let period_end = now - Duration::days(day_offset);
            let period_start = period_end - Duration::days(1);

            let records = self.get_by_time_range(period_start, period_end);
            let summary = TokenStatsSummary::from_records(&records);

            result.push(PeriodTokenStats {
                period_start: Some(period_start),
                period_end: Some(period_end),
                summary,
            });
        }

        result
    }

    /// 清理过期记录
    ///
    /// 返回清理的记录数量
    pub fn cleanup_expired(&self) -> usize {
        let mut records = self.records.write();
        let cutoff = Utc::now() - self.retention;
        let initial_len = records.len();

        while let Some(front) = records.front() {
            if front.timestamp < cutoff {
                records.pop_front();
            } else {
                break;
            }
        }

        initial_len - records.len()
    }
}

impl Default for TokenTracker {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// Token 估算器
///
/// 使用 tiktoken 库估算文本的 Token 数量
pub struct TokenEstimator {
    /// 默认使用的 BPE 编码器（cl100k_base，适用于 GPT-4/Claude 等）
    default_bpe: tiktoken_rs::CoreBPE,
    /// o200k_base 编码器（适用于 GPT-4o 等新模型）
    o200k_bpe: tiktoken_rs::CoreBPE,
}

impl TokenEstimator {
    /// 创建新的 Token 估算器
    pub fn new() -> Result<Self, TokenEstimatorError> {
        let default_bpe = tiktoken_rs::cl100k_base()
            .map_err(|e| TokenEstimatorError::InitializationError(e.to_string()))?;
        let o200k_bpe = tiktoken_rs::o200k_base()
            .map_err(|e| TokenEstimatorError::InitializationError(e.to_string()))?;

        Ok(Self {
            default_bpe,
            o200k_bpe,
        })
    }

    /// 估算文本的 Token 数量
    ///
    /// # Arguments
    /// * `text` - 要估算的文本
    /// * `model` - 可选的模型名称，用于选择合适的编码器
    pub fn estimate(&self, text: &str, model: Option<&str>) -> u32 {
        let bpe = self.select_bpe(model);
        bpe.encode_with_special_tokens(text).len() as u32
    }

    /// 估算消息列表的 Token 数量（用于聊天完成请求）
    ///
    /// 包含消息格式化的额外 Token 开销
    pub fn estimate_messages(&self, messages: &[ChatMessage], model: Option<&str>) -> u32 {
        let bpe = self.select_bpe(model);
        let mut total_tokens = 0u32;

        // 每条消息的格式化开销（role + content 分隔符等）
        let tokens_per_message = 4; // <|im_start|>role\ncontent<|im_end|>
        let tokens_per_name = 1; // 如果有 name 字段

        for message in messages {
            total_tokens += tokens_per_message;
            total_tokens += bpe.encode_with_special_tokens(&message.role).len() as u32;
            total_tokens += bpe.encode_with_special_tokens(&message.content).len() as u32;
            if message.name.is_some() {
                total_tokens += tokens_per_name;
            }
        }

        // 每个回复的前缀开销
        total_tokens += 3; // <|im_start|>assistant

        total_tokens
    }

    /// 根据模型名称选择合适的 BPE 编码器
    fn select_bpe(&self, model: Option<&str>) -> &tiktoken_rs::CoreBPE {
        match model {
            Some(m) if m.contains("gpt-4o") || m.contains("o1") || m.contains("o3") => {
                &self.o200k_bpe
            }
            _ => &self.default_bpe,
        }
    }
}

impl Default for TokenEstimator {
    fn default() -> Self {
        Self::new().expect("Failed to create TokenEstimator")
    }
}

/// Token 估算器错误
#[derive(Debug, Clone)]
pub enum TokenEstimatorError {
    /// 初始化错误
    InitializationError(String),
}

impl std::fmt::Display for TokenEstimatorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenEstimatorError::InitializationError(msg) => {
                write!(f, "Token 估算器初始化失败: {}", msg)
            }
        }
    }
}

impl std::error::Error for TokenEstimatorError {}

/// 聊天消息（用于 Token 估算）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    /// 角色（system, user, assistant）
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 可选的名称
    pub name: Option<String>,
}

impl ChatMessage {
    /// 创建新的聊天消息
    pub fn new(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            name: None,
        }
    }

    /// 设置名称
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;

    #[test]
    fn test_token_usage_record_new() {
        let record = TokenUsageRecord::new(
            "test-id".to_string(),
            ProviderType::Kiro,
            "claude-sonnet".to_string(),
            100,
            50,
            TokenSource::Actual,
        );

        assert_eq!(record.id, "test-id");
        assert_eq!(record.provider, ProviderType::Kiro);
        assert_eq!(record.model, "claude-sonnet");
        assert_eq!(record.input_tokens, 100);
        assert_eq!(record.output_tokens, 50);
        assert_eq!(record.total_tokens, 150);
        assert_eq!(record.source, TokenSource::Actual);
        assert!(record.request_id.is_none());
    }

    #[test]
    fn test_token_usage_record_with_request_id() {
        let record = TokenUsageRecord::new(
            "test-id".to_string(),
            ProviderType::Gemini,
            "gemini-pro".to_string(),
            200,
            100,
            TokenSource::Estimated,
        )
        .with_request_id("req-123".to_string());

        assert_eq!(record.request_id, Some("req-123".to_string()));
    }

    #[test]
    fn test_token_stats_summary_from_records() {
        let records = vec![
            TokenUsageRecord::new(
                "1".to_string(),
                ProviderType::Kiro,
                "model".to_string(),
                100,
                50,
                TokenSource::Actual,
            ),
            TokenUsageRecord::new(
                "2".to_string(),
                ProviderType::Kiro,
                "model".to_string(),
                200,
                100,
                TokenSource::Actual,
            ),
            TokenUsageRecord::new(
                "3".to_string(),
                ProviderType::Kiro,
                "model".to_string(),
                150,
                75,
                TokenSource::Estimated,
            ),
        ];

        let summary = TokenStatsSummary::from_records(&records);

        assert_eq!(summary.total_input_tokens, 450);
        assert_eq!(summary.total_output_tokens, 225);
        assert_eq!(summary.total_tokens, 675);
        assert_eq!(summary.record_count, 3);
        assert_eq!(summary.actual_count, 2);
        assert_eq!(summary.estimated_count, 1);
        assert!((summary.avg_input_tokens - 150.0).abs() < 0.001);
        assert!((summary.avg_output_tokens - 75.0).abs() < 0.001);
    }

    #[test]
    fn test_token_tracker_basic_operations() {
        let tracker = TokenTracker::with_defaults();

        let record = TokenUsageRecord::new(
            "test-1".to_string(),
            ProviderType::Kiro,
            "claude-sonnet".to_string(),
            100,
            50,
            TokenSource::Actual,
        );

        tracker.record(record);

        assert_eq!(tracker.len(), 1);
        assert!(!tracker.is_empty());
    }

    #[test]
    fn test_token_tracker_record_from_response() {
        let tracker = TokenTracker::with_defaults();

        tracker.record_from_response(
            "req-1".to_string(),
            ProviderType::Kiro,
            "claude-sonnet".to_string(),
            Some(100),
            Some(50),
        );

        assert_eq!(tracker.len(), 1);

        let records = tracker.get_all();
        assert_eq!(records[0].input_tokens, 100);
        assert_eq!(records[0].output_tokens, 50);
        assert_eq!(records[0].source, TokenSource::Actual);
        assert_eq!(records[0].request_id, Some("req-1".to_string()));
    }

    #[test]
    fn test_token_tracker_by_provider() {
        let tracker = TokenTracker::with_defaults();

        tracker.record(TokenUsageRecord::new(
            "1".to_string(),
            ProviderType::Kiro,
            "model".to_string(),
            100,
            50,
            TokenSource::Actual,
        ));
        tracker.record(TokenUsageRecord::new(
            "2".to_string(),
            ProviderType::Gemini,
            "model".to_string(),
            200,
            100,
            TokenSource::Actual,
        ));
        tracker.record(TokenUsageRecord::new(
            "3".to_string(),
            ProviderType::Kiro,
            "model".to_string(),
            150,
            75,
            TokenSource::Actual,
        ));

        let stats = tracker.by_provider(None, None);

        assert!(stats.contains_key(&ProviderType::Kiro));
        assert!(stats.contains_key(&ProviderType::Gemini));
        assert_eq!(stats[&ProviderType::Kiro].summary.record_count, 2);
        assert_eq!(stats[&ProviderType::Gemini].summary.record_count, 1);
    }

    #[test]
    fn test_token_tracker_by_model() {
        let tracker = TokenTracker::with_defaults();

        tracker.record(TokenUsageRecord::new(
            "1".to_string(),
            ProviderType::Kiro,
            "model-a".to_string(),
            100,
            50,
            TokenSource::Actual,
        ));
        tracker.record(TokenUsageRecord::new(
            "2".to_string(),
            ProviderType::Kiro,
            "model-b".to_string(),
            200,
            100,
            TokenSource::Actual,
        ));
        tracker.record(TokenUsageRecord::new(
            "3".to_string(),
            ProviderType::Kiro,
            "model-a".to_string(),
            150,
            75,
            TokenSource::Actual,
        ));

        let stats = tracker.by_model(None, None);

        assert!(stats.contains_key("model-a"));
        assert!(stats.contains_key("model-b"));
        assert_eq!(stats["model-a"].summary.record_count, 2);
        assert_eq!(stats["model-b"].summary.record_count, 1);
    }

    #[test]
    fn test_token_tracker_clear() {
        let tracker = TokenTracker::with_defaults();

        for i in 0..5 {
            tracker.record(TokenUsageRecord::new(
                format!("test-{}", i),
                ProviderType::Kiro,
                "model".to_string(),
                100,
                50,
                TokenSource::Actual,
            ));
        }

        assert_eq!(tracker.len(), 5);

        tracker.clear();

        assert_eq!(tracker.len(), 0);
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_token_tracker_max_records_limit() {
        let tracker = TokenTracker::new(Duration::days(7), 10);

        for i in 0..20 {
            tracker.record(TokenUsageRecord::new(
                format!("test-{}", i),
                ProviderType::Kiro,
                "model".to_string(),
                100,
                50,
                TokenSource::Actual,
            ));
        }

        assert_eq!(tracker.len(), 10);
    }

    // ========== TokenEstimator 测试 ==========

    #[test]
    fn test_token_estimator_new() {
        let estimator = TokenEstimator::new();
        assert!(estimator.is_ok());
    }

    #[test]
    fn test_token_estimator_estimate_simple_text() {
        let estimator = TokenEstimator::new().unwrap();

        // 简单文本应该返回合理的 Token 数
        let tokens = estimator.estimate("Hello, world!", None);
        assert!(tokens > 0);
        assert!(tokens < 10); // "Hello, world!" 应该是几个 Token
    }

    #[test]
    fn test_token_estimator_estimate_empty_text() {
        let estimator = TokenEstimator::new().unwrap();

        let tokens = estimator.estimate("", None);
        assert_eq!(tokens, 0);
    }

    #[test]
    fn test_token_estimator_estimate_chinese_text() {
        let estimator = TokenEstimator::new().unwrap();

        // 中文文本
        let tokens = estimator.estimate("你好，世界！", None);
        assert!(tokens > 0);
    }

    #[test]
    fn test_token_estimator_estimate_with_model() {
        let estimator = TokenEstimator::new().unwrap();

        let text = "This is a test sentence for token estimation.";

        // 使用不同模型应该返回 Token 数（可能相同或不同）
        let tokens_default = estimator.estimate(text, None);
        let tokens_gpt4o = estimator.estimate(text, Some("gpt-4o"));
        let tokens_claude = estimator.estimate(text, Some("claude-sonnet"));

        assert!(tokens_default > 0);
        assert!(tokens_gpt4o > 0);
        assert!(tokens_claude > 0);
    }

    #[test]
    fn test_token_estimator_estimate_messages() {
        let estimator = TokenEstimator::new().unwrap();

        let messages = vec![
            ChatMessage::new("system", "You are a helpful assistant."),
            ChatMessage::new("user", "Hello!"),
        ];

        let tokens = estimator.estimate_messages(&messages, None);
        assert!(tokens > 0);

        // 消息 Token 应该大于单独文本的 Token（因为有格式化开销）
        let text_only_tokens = estimator.estimate("You are a helpful assistant.", None)
            + estimator.estimate("Hello!", None);
        assert!(tokens > text_only_tokens);
    }

    #[test]
    fn test_token_estimator_estimate_messages_with_name() {
        let estimator = TokenEstimator::new().unwrap();

        let messages_without_name = vec![ChatMessage::new("user", "Hello!")];

        let messages_with_name = vec![ChatMessage::new("user", "Hello!").with_name("Alice")];

        let tokens_without = estimator.estimate_messages(&messages_without_name, None);
        let tokens_with = estimator.estimate_messages(&messages_with_name, None);

        // 有名称的消息应该有更多 Token
        assert!(tokens_with > tokens_without);
    }

    #[test]
    fn test_chat_message_new() {
        let msg = ChatMessage::new("user", "Hello!");

        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello!");
        assert!(msg.name.is_none());
    }

    #[test]
    fn test_chat_message_with_name() {
        let msg = ChatMessage::new("user", "Hello!").with_name("Alice");

        assert_eq!(msg.role, "user");
        assert_eq!(msg.content, "Hello!");
        assert_eq!(msg.name, Some("Alice".to_string()));
    }
}
