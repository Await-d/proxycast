//! 增强统计服务
//!
//! 该模块实现 LLM Flow 的增强统计功能，包括时间序列趋势、分布分析、直方图等。
//!
//! **Validates: Requirements 9.1-9.7**

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use super::memory_store::{FlowFilter, FlowMemoryStore, TimeRange};
use super::models::{FlowState, LLMFlow};
use tokio::sync::RwLock;

// ============================================================================
// 数据结构
// ============================================================================

/// 时间序列数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeSeriesPoint {
    /// 时间戳
    pub timestamp: DateTime<Utc>,
    /// 数值
    pub value: f64,
}

/// 分布数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Distribution {
    /// 分布桶 (标签, 数量)
    pub buckets: Vec<(String, u64)>,
    /// 总数
    pub total: u64,
}

impl Default for Distribution {
    fn default() -> Self {
        Self {
            buckets: Vec::new(),
            total: 0,
        }
    }
}

/// 趋势数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendData {
    /// 数据点列表
    pub points: Vec<TimeSeriesPoint>,
    /// 时间间隔
    pub interval: String,
}

impl Default for TrendData {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            interval: "1h".to_string(),
        }
    }
}

/// 增强统计结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnhancedStats {
    /// 请求趋势
    pub request_trend: TrendData,
    /// 按模型的 Token 分布
    pub token_by_model: Distribution,
    /// 按提供商的成功率
    pub success_by_provider: Vec<(String, f64)>,
    /// 延迟直方图
    pub latency_histogram: Distribution,
    /// 错误分布
    pub error_distribution: Distribution,
    /// 请求速率（每秒）
    pub request_rate: f64,
    /// 时间范围
    pub time_range: StatsTimeRange,
}

impl Default for EnhancedStats {
    fn default() -> Self {
        Self {
            request_trend: TrendData::default(),
            token_by_model: Distribution::default(),
            success_by_provider: Vec::new(),
            latency_histogram: Distribution::default(),
            error_distribution: Distribution::default(),
            request_rate: 0.0,
            time_range: StatsTimeRange::default(),
        }
    }
}

/// 统计时间范围
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsTimeRange {
    /// 开始时间
    pub start: DateTime<Utc>,
    /// 结束时间
    pub end: DateTime<Utc>,
}

impl Default for StatsTimeRange {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            start: now - Duration::hours(24),
            end: now,
        }
    }
}

/// 统计报告格式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    /// JSON 格式
    Json,
    /// Markdown 格式
    Markdown,
    /// CSV 格式
    Csv,
}

impl Default for ReportFormat {
    fn default() -> Self {
        ReportFormat::Json
    }
}

// ============================================================================
// 增强统计服务
// ============================================================================

/// 增强统计服务
///
/// 提供更详细的统计分析功能，包括时间序列趋势、分布分析等。
pub struct EnhancedStatsService {
    /// 内存存储
    memory_store: Arc<RwLock<FlowMemoryStore>>,
}

impl EnhancedStatsService {
    /// 创建新的增强统计服务
    pub fn new(memory_store: Arc<RwLock<FlowMemoryStore>>) -> Self {
        Self { memory_store }
    }

    /// 获取增强统计
    ///
    /// **Validates: Requirements 9.1-9.5**
    ///
    /// # Arguments
    /// * `filter` - 过滤条件
    /// * `time_range` - 时间范围
    ///
    /// # Returns
    /// 增强统计结果
    pub async fn get_stats(
        &self,
        filter: &FlowFilter,
        time_range: &StatsTimeRange,
    ) -> EnhancedStats {
        // 获取 Flow 数据
        let flows = self.get_flows_in_range(filter, time_range).await;

        if flows.is_empty() {
            return EnhancedStats {
                time_range: time_range.clone(),
                ..Default::default()
            };
        }

        // 计算各项统计
        let request_trend = self.calculate_request_trend(&flows, "1h");
        let token_by_model = self.calculate_token_distribution(&flows);
        let success_by_provider = self.calculate_success_by_provider(&flows);
        let latency_histogram =
            self.calculate_latency_histogram(&flows, &default_latency_buckets());
        let error_distribution = self.calculate_error_distribution(&flows);
        let request_rate = self.calculate_request_rate(&flows, time_range);

        EnhancedStats {
            request_trend,
            token_by_model,
            success_by_provider,
            latency_histogram,
            error_distribution,
            request_rate,
            time_range: time_range.clone(),
        }
    }

    /// 获取请求趋势
    ///
    /// **Validates: Requirements 9.1**
    ///
    /// # Arguments
    /// * `filter` - 过滤条件
    /// * `time_range` - 时间范围
    /// * `interval` - 时间间隔（如 "1h", "30m", "1d"）
    ///
    /// # Returns
    /// 趋势数据
    pub async fn get_request_trend(
        &self,
        filter: &FlowFilter,
        time_range: &StatsTimeRange,
        interval: &str,
    ) -> TrendData {
        let flows = self.get_flows_in_range(filter, time_range).await;
        self.calculate_request_trend(&flows, interval)
    }

    /// 获取 Token 分布
    ///
    /// **Validates: Requirements 9.2**
    ///
    /// # Arguments
    /// * `filter` - 过滤条件
    /// * `time_range` - 时间范围
    ///
    /// # Returns
    /// Token 分布数据
    pub async fn get_token_distribution(
        &self,
        filter: &FlowFilter,
        time_range: &StatsTimeRange,
    ) -> Distribution {
        let flows = self.get_flows_in_range(filter, time_range).await;
        self.calculate_token_distribution(&flows)
    }

    /// 获取延迟直方图
    ///
    /// **Validates: Requirements 9.4**
    ///
    /// # Arguments
    /// * `filter` - 过滤条件
    /// * `time_range` - 时间范围
    /// * `buckets` - 直方图桶边界（毫秒）
    ///
    /// # Returns
    /// 延迟直方图数据
    pub async fn get_latency_histogram(
        &self,
        filter: &FlowFilter,
        time_range: &StatsTimeRange,
        buckets: &[u64],
    ) -> Distribution {
        let flows = self.get_flows_in_range(filter, time_range).await;
        self.calculate_latency_histogram(&flows, buckets)
    }

    /// 导出统计报告
    ///
    /// **Validates: Requirements 9.7**
    ///
    /// # Arguments
    /// * `filter` - 过滤条件
    /// * `time_range` - 时间范围
    /// * `format` - 报告格式
    ///
    /// # Returns
    /// 格式化的报告字符串
    pub async fn export_report(
        &self,
        filter: &FlowFilter,
        time_range: &StatsTimeRange,
        format: &ReportFormat,
    ) -> String {
        let stats = self.get_stats(filter, time_range).await;

        match format {
            ReportFormat::Json => self.export_json(&stats),
            ReportFormat::Markdown => self.export_markdown(&stats),
            ReportFormat::Csv => self.export_csv(&stats),
        }
    }

    // ========================================================================
    // 内部方法
    // ========================================================================

    /// 获取时间范围内的 Flow
    async fn get_flows_in_range(
        &self,
        filter: &FlowFilter,
        time_range: &StatsTimeRange,
    ) -> Vec<LLMFlow> {
        let store = self.memory_store.read().await;

        // 创建带时间范围的过滤器
        let mut filter_with_time = filter.clone();
        filter_with_time.time_range = Some(TimeRange {
            start: Some(time_range.start),
            end: Some(time_range.end),
        });

        store.query(&filter_with_time)
    }

    /// 计算请求趋势
    fn calculate_request_trend(&self, flows: &[LLMFlow], interval: &str) -> TrendData {
        if flows.is_empty() {
            return TrendData {
                points: Vec::new(),
                interval: interval.to_string(),
            };
        }

        // 解析时间间隔
        let interval_duration = parse_interval(interval);

        // 找到时间范围
        let min_time = flows
            .iter()
            .map(|f| f.timestamps.created)
            .min()
            .unwrap_or_else(Utc::now);
        let max_time = flows
            .iter()
            .map(|f| f.timestamps.created)
            .max()
            .unwrap_or_else(Utc::now);

        // 按时间间隔分组计数
        let mut counts: HashMap<i64, u64> = HashMap::new();

        for flow in flows {
            let bucket = (flow.timestamps.created.timestamp() / interval_duration.num_seconds())
                * interval_duration.num_seconds();
            *counts.entry(bucket).or_insert(0) += 1;
        }

        // 生成完整的时间序列（包括零值点）
        let mut points = Vec::new();
        let mut current = (min_time.timestamp() / interval_duration.num_seconds())
            * interval_duration.num_seconds();
        let end = max_time.timestamp();

        while current <= end {
            let count = counts.get(&current).copied().unwrap_or(0);
            if let Some(timestamp) = DateTime::from_timestamp(current, 0) {
                points.push(TimeSeriesPoint {
                    timestamp: timestamp.with_timezone(&Utc),
                    value: count as f64,
                });
            }
            current += interval_duration.num_seconds();
        }

        TrendData {
            points,
            interval: interval.to_string(),
        }
    }

    /// 计算 Token 分布（按模型）
    fn calculate_token_distribution(&self, flows: &[LLMFlow]) -> Distribution {
        let mut model_tokens: HashMap<String, u64> = HashMap::new();
        let mut total: u64 = 0;

        for flow in flows {
            if let Some(ref response) = flow.response {
                let tokens = response.usage.total_tokens as u64;
                *model_tokens.entry(flow.request.model.clone()).or_insert(0) += tokens;
                total += tokens;
            }
        }

        // 按 Token 数量降序排序
        let mut buckets: Vec<(String, u64)> = model_tokens.into_iter().collect();
        buckets.sort_by(|a, b| b.1.cmp(&a.1));

        Distribution { buckets, total }
    }

    /// 计算按提供商的成功率
    fn calculate_success_by_provider(&self, flows: &[LLMFlow]) -> Vec<(String, f64)> {
        let mut provider_stats: HashMap<String, (usize, usize)> = HashMap::new();

        for flow in flows {
            let provider = format!("{:?}", flow.metadata.provider);
            let entry = provider_stats.entry(provider).or_insert((0, 0));
            entry.0 += 1; // 总数
            if flow.state == FlowState::Completed {
                entry.1 += 1; // 成功数
            }
        }

        let mut result: Vec<(String, f64)> = provider_stats
            .into_iter()
            .map(|(provider, (total, success))| {
                let rate = if total > 0 {
                    success as f64 / total as f64
                } else {
                    0.0
                };
                (provider, rate)
            })
            .collect();

        // 按成功率降序排序
        result.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        result
    }

    /// 计算延迟直方图
    fn calculate_latency_histogram(&self, flows: &[LLMFlow], buckets: &[u64]) -> Distribution {
        let mut bucket_counts: Vec<u64> = vec![0; buckets.len() + 1];
        let mut total: u64 = 0;

        for flow in flows {
            let latency = flow.timestamps.duration_ms;
            total += 1;

            // 找到对应的桶
            let bucket_idx = buckets
                .iter()
                .position(|&b| latency < b)
                .unwrap_or(buckets.len());
            bucket_counts[bucket_idx] += 1;
        }

        // 生成桶标签
        let mut result_buckets = Vec::new();
        for (i, count) in bucket_counts.iter().enumerate() {
            let label = if i == 0 {
                format!("<{}ms", buckets.first().unwrap_or(&0))
            } else if i == buckets.len() {
                format!(">={}ms", buckets.last().unwrap_or(&0))
            } else {
                format!("{}-{}ms", buckets[i - 1], buckets[i])
            };
            result_buckets.push((label, *count));
        }

        Distribution {
            buckets: result_buckets,
            total,
        }
    }

    /// 计算错误分布
    fn calculate_error_distribution(&self, flows: &[LLMFlow]) -> Distribution {
        let mut error_counts: HashMap<String, u64> = HashMap::new();
        let mut total: u64 = 0;

        for flow in flows {
            if let Some(ref error) = flow.error {
                let error_type = format!("{:?}", error.error_type);
                *error_counts.entry(error_type).or_insert(0) += 1;
                total += 1;
            }
        }

        // 按数量降序排序
        let mut buckets: Vec<(String, u64)> = error_counts.into_iter().collect();
        buckets.sort_by(|a, b| b.1.cmp(&a.1));

        Distribution { buckets, total }
    }

    /// 计算请求速率（每秒）
    fn calculate_request_rate(&self, flows: &[LLMFlow], time_range: &StatsTimeRange) -> f64 {
        if flows.is_empty() {
            return 0.0;
        }

        let duration_secs = (time_range.end - time_range.start).num_seconds() as f64;
        if duration_secs <= 0.0 {
            return 0.0;
        }

        flows.len() as f64 / duration_secs
    }

    /// 导出为 JSON 格式
    fn export_json(&self, stats: &EnhancedStats) -> String {
        serde_json::to_string_pretty(stats).unwrap_or_else(|_| "{}".to_string())
    }

    /// 导出为 Markdown 格式
    fn export_markdown(&self, stats: &EnhancedStats) -> String {
        let mut md = String::new();

        md.push_str("# Flow 统计报告\n\n");
        md.push_str(&format!(
            "**时间范围**: {} - {}\n\n",
            stats.time_range.start.format("%Y-%m-%d %H:%M:%S"),
            stats.time_range.end.format("%Y-%m-%d %H:%M:%S")
        ));
        md.push_str(&format!(
            "**请求速率**: {:.2} 请求/秒\n\n",
            stats.request_rate
        ));

        // Token 分布
        md.push_str("## Token 分布（按模型）\n\n");
        md.push_str("| 模型 | Token 数 |\n");
        md.push_str("|------|----------|\n");
        for (model, tokens) in &stats.token_by_model.buckets {
            md.push_str(&format!("| {} | {} |\n", model, tokens));
        }
        md.push_str(&format!(
            "| **总计** | **{}** |\n\n",
            stats.token_by_model.total
        ));

        // 成功率
        md.push_str("## 成功率（按提供商）\n\n");
        md.push_str("| 提供商 | 成功率 |\n");
        md.push_str("|--------|--------|\n");
        for (provider, rate) in &stats.success_by_provider {
            md.push_str(&format!("| {} | {:.1}% |\n", provider, rate * 100.0));
        }
        md.push('\n');

        // 延迟直方图
        md.push_str("## 延迟分布\n\n");
        md.push_str("| 延迟范围 | 请求数 |\n");
        md.push_str("|----------|--------|\n");
        for (range, count) in &stats.latency_histogram.buckets {
            md.push_str(&format!("| {} | {} |\n", range, count));
        }
        md.push('\n');

        // 错误分布
        if !stats.error_distribution.buckets.is_empty() {
            md.push_str("## 错误分布\n\n");
            md.push_str("| 错误类型 | 数量 |\n");
            md.push_str("|----------|------|\n");
            for (error_type, count) in &stats.error_distribution.buckets {
                md.push_str(&format!("| {} | {} |\n", error_type, count));
            }
            md.push('\n');
        }

        md
    }

    /// 导出为 CSV 格式
    fn export_csv(&self, stats: &EnhancedStats) -> String {
        let mut csv = String::new();

        // Token 分布
        csv.push_str("# Token Distribution by Model\n");
        csv.push_str("Model,Tokens\n");
        for (model, tokens) in &stats.token_by_model.buckets {
            csv.push_str(&format!("{},{}\n", model, tokens));
        }
        csv.push('\n');

        // 成功率
        csv.push_str("# Success Rate by Provider\n");
        csv.push_str("Provider,SuccessRate\n");
        for (provider, rate) in &stats.success_by_provider {
            csv.push_str(&format!("{},{:.4}\n", provider, rate));
        }
        csv.push('\n');

        // 延迟直方图
        csv.push_str("# Latency Histogram\n");
        csv.push_str("Range,Count\n");
        for (range, count) in &stats.latency_histogram.buckets {
            csv.push_str(&format!("{},{}\n", range, count));
        }
        csv.push('\n');

        // 错误分布
        csv.push_str("# Error Distribution\n");
        csv.push_str("ErrorType,Count\n");
        for (error_type, count) in &stats.error_distribution.buckets {
            csv.push_str(&format!("{},{}\n", error_type, count));
        }

        csv
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 解析时间间隔字符串
fn parse_interval(interval: &str) -> Duration {
    let interval = interval.trim().to_lowercase();

    if let Some(num_str) = interval.strip_suffix('h') {
        if let Ok(hours) = num_str.parse::<i64>() {
            return Duration::hours(hours);
        }
    } else if let Some(num_str) = interval.strip_suffix('m') {
        if let Ok(minutes) = num_str.parse::<i64>() {
            return Duration::minutes(minutes);
        }
    } else if let Some(num_str) = interval.strip_suffix('d') {
        if let Ok(days) = num_str.parse::<i64>() {
            return Duration::days(days);
        }
    } else if let Some(num_str) = interval.strip_suffix('s') {
        if let Ok(seconds) = num_str.parse::<i64>() {
            return Duration::seconds(seconds);
        }
    }

    // 默认 1 小时
    Duration::hours(1)
}

/// 默认延迟桶边界（毫秒）
fn default_latency_buckets() -> Vec<u64> {
    vec![100, 500, 1000, 2000, 5000, 10000]
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_interval() {
        assert_eq!(parse_interval("1h"), Duration::hours(1));
        assert_eq!(parse_interval("30m"), Duration::minutes(30));
        assert_eq!(parse_interval("1d"), Duration::days(1));
        assert_eq!(parse_interval("60s"), Duration::seconds(60));
        assert_eq!(parse_interval("invalid"), Duration::hours(1)); // 默认值
    }

    #[test]
    fn test_default_latency_buckets() {
        let buckets = default_latency_buckets();
        assert_eq!(buckets, vec![100, 500, 1000, 2000, 5000, 10000]);
    }

    #[test]
    fn test_distribution_default() {
        let dist = Distribution::default();
        assert!(dist.buckets.is_empty());
        assert_eq!(dist.total, 0);
    }

    #[test]
    fn test_trend_data_default() {
        let trend = TrendData::default();
        assert!(trend.points.is_empty());
        assert_eq!(trend.interval, "1h");
    }

    #[test]
    fn test_enhanced_stats_default() {
        let stats = EnhancedStats::default();
        assert!(stats.request_trend.points.is_empty());
        assert!(stats.token_by_model.buckets.is_empty());
        assert!(stats.success_by_provider.is_empty());
        assert_eq!(stats.request_rate, 0.0);
    }

    #[test]
    fn test_report_format_default() {
        let format = ReportFormat::default();
        assert_eq!(format, ReportFormat::Json);
    }

    #[test]
    fn test_stats_time_range_default() {
        let range = StatsTimeRange::default();
        assert!(range.start < range.end);
        // 默认应该是 24 小时范围
        let diff = range.end - range.start;
        assert_eq!(diff.num_hours(), 24);
    }
}

// ============================================================================
// 属性测试模块
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::flow_monitor::models::{
        FlowMetadata, FlowState, FlowType, LLMRequest, LLMResponse, Message, MessageContent,
        MessageRole, RequestParameters, TokenUsage,
    };
    use crate::ProviderType;
    use proptest::prelude::*;

    // ========================================================================
    // 生成器
    // ========================================================================

    /// 生成随机的 ProviderType
    fn arb_provider_type() -> impl Strategy<Value = ProviderType> {
        prop_oneof![
            Just(ProviderType::Kiro),
            Just(ProviderType::Gemini),
            Just(ProviderType::Qwen),
            Just(ProviderType::OpenAI),
            Just(ProviderType::Claude),
        ]
    }

    /// 生成随机的 FlowState
    fn arb_flow_state() -> impl Strategy<Value = FlowState> {
        prop_oneof![
            Just(FlowState::Pending),
            Just(FlowState::Streaming),
            Just(FlowState::Completed),
            Just(FlowState::Failed),
            Just(FlowState::Cancelled),
        ]
    }

    /// 生成随机的模型名称
    fn arb_model_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4".to_string()),
            Just("gpt-4-turbo".to_string()),
            Just("gpt-3.5-turbo".to_string()),
            Just("claude-3-opus".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("gemini-pro".to_string()),
        ]
    }

    /// 生成随机的 TokenUsage
    fn arb_token_usage() -> impl Strategy<Value = TokenUsage> {
        (0u32..10000u32, 0u32..5000u32).prop_map(|(input, output)| TokenUsage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
            ..Default::default()
        })
    }

    /// 生成随机的 LLMFlow
    fn arb_llm_flow() -> impl Strategy<Value = LLMFlow> {
        (
            "[a-f0-9]{8}",
            arb_model_name(),
            arb_provider_type(),
            arb_flow_state(),
            0u64..10000u64, // duration_ms
            arb_token_usage(),
            any::<bool>(), // has_response
        )
            .prop_map(
                |(id, model, provider, state, duration, usage, has_response)| {
                    let request = LLMRequest {
                        method: "POST".to_string(),
                        path: "/v1/chat/completions".to_string(),
                        model,
                        messages: vec![Message {
                            role: MessageRole::User,
                            content: MessageContent::Text("test".to_string()),
                            ..Default::default()
                        }],
                        parameters: RequestParameters::default(),
                        ..Default::default()
                    };

                    let metadata = FlowMetadata {
                        provider,
                        ..Default::default()
                    };

                    let mut flow = LLMFlow::new(id, FlowType::ChatCompletions, request, metadata);
                    flow.state = state;
                    flow.timestamps.duration_ms = duration;

                    if has_response {
                        flow.response = Some(LLMResponse {
                            usage,
                            ..Default::default()
                        });
                    }

                    flow
                },
            )
    }

    /// 生成随机的 Flow 列表
    fn arb_flow_list() -> impl Strategy<Value = Vec<LLMFlow>> {
        prop::collection::vec(arb_llm_flow(), 0..50)
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 17: 统计计算正确性**
        /// **Validates: Requirements 9.1-9.6**
        ///
        /// *对于任意* Flow 集合和时间范围，统计计算应该正确反映该范围内的数据。
        #[test]
        fn prop_stats_calculation_correctness(flows in arb_flow_list()) {
            // 创建一个临时的 EnhancedStatsService 实例来测试内部计算方法
            let service = EnhancedStatsService::new(
                Arc::new(RwLock::new(FlowMemoryStore::new(1000)))
            );

            // 测试 Token 分布计算
            let token_dist = service.calculate_token_distribution(&flows);

            // 验证: Token 分布的总数应该等于所有 Flow 的 Token 总和
            let expected_total: u64 = flows
                .iter()
                .filter_map(|f| f.response.as_ref())
                .map(|r| r.usage.total_tokens as u64)
                .sum();
            prop_assert_eq!(
                token_dist.total,
                expected_total,
                "Token 分布总数应该等于所有 Flow 的 Token 总和"
            );

            // 验证: 每个模型的 Token 数应该正确
            let bucket_total: u64 = token_dist.buckets.iter().map(|(_, count)| *count).sum();
            prop_assert_eq!(
                bucket_total,
                expected_total,
                "所有桶的 Token 数之和应该等于总数"
            );

            // 测试成功率计算
            let success_by_provider = service.calculate_success_by_provider(&flows);

            // 验证: 成功率应该在 0.0 到 1.0 之间
            for (_, rate) in &success_by_provider {
                prop_assert!(
                    *rate >= 0.0 && *rate <= 1.0,
                    "成功率应该在 0.0 到 1.0 之间，实际值: {}",
                    rate
                );
            }

            // 测试延迟直方图计算
            let buckets = vec![100, 500, 1000, 2000, 5000, 10000];
            let latency_hist = service.calculate_latency_histogram(&flows, &buckets);

            // 验证: 直方图总数应该等于 Flow 数量
            prop_assert_eq!(
                latency_hist.total,
                flows.len() as u64,
                "延迟直方图总数应该等于 Flow 数量"
            );

            // 验证: 所有桶的数量之和应该等于总数
            let hist_bucket_total: u64 = latency_hist.buckets.iter().map(|(_, count)| *count).sum();
            prop_assert_eq!(
                hist_bucket_total,
                latency_hist.total,
                "所有直方图桶的数量之和应该等于总数"
            );

            // 测试错误分布计算
            let error_dist = service.calculate_error_distribution(&flows);

            // 验证: 错误分布总数应该等于有错误的 Flow 数量
            let expected_error_count = flows.iter().filter(|f| f.error.is_some()).count() as u64;
            prop_assert_eq!(
                error_dist.total,
                expected_error_count,
                "错误分布总数应该等于有错误的 Flow 数量"
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 17b: 请求趋势计算正确性**
        /// **Validates: Requirements 9.1**
        ///
        /// *对于任意* Flow 集合，请求趋势的数据点值之和应该等于 Flow 总数。
        #[test]
        fn prop_request_trend_correctness(flows in arb_flow_list()) {
            let service = EnhancedStatsService::new(
                Arc::new(RwLock::new(FlowMemoryStore::new(1000)))
            );

            // 测试请求趋势计算
            let trend = service.calculate_request_trend(&flows, "1h");

            // 验证: 趋势数据点的值之和应该等于 Flow 数量
            let trend_total: f64 = trend.points.iter().map(|p| p.value).sum();
            prop_assert_eq!(
                trend_total as usize,
                flows.len(),
                "趋势数据点的值之和应该等于 Flow 数量"
            );

            // 验证: 间隔应该正确设置
            prop_assert_eq!(
                trend.interval,
                "1h",
                "趋势间隔应该正确设置"
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 17c: 请求速率计算正确性**
        /// **Validates: Requirements 9.1**
        ///
        /// *对于任意* Flow 集合和时间范围，请求速率应该正确计算。
        #[test]
        fn prop_request_rate_correctness(flows in arb_flow_list()) {
            let service = EnhancedStatsService::new(
                Arc::new(RwLock::new(FlowMemoryStore::new(1000)))
            );

            let now = Utc::now();
            let time_range = StatsTimeRange {
                start: now - Duration::hours(1),
                end: now,
            };

            let rate = service.calculate_request_rate(&flows, &time_range);

            // 验证: 请求速率应该非负
            prop_assert!(
                rate >= 0.0,
                "请求速率应该非负，实际值: {}",
                rate
            );

            // 验证: 如果有 Flow，速率应该大于 0
            if !flows.is_empty() {
                prop_assert!(
                    rate > 0.0,
                    "如果有 Flow，请求速率应该大于 0"
                );
            }

            // 验证: 速率计算正确（Flow 数量 / 时间范围秒数）
            let duration_secs = (time_range.end - time_range.start).num_seconds() as f64;
            let expected_rate = flows.len() as f64 / duration_secs;
            prop_assert!(
                (rate - expected_rate).abs() < 0.0001,
                "请求速率计算应该正确，期望: {}, 实际: {}",
                expected_rate,
                rate
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 17d: 报告导出正确性**
        /// **Validates: Requirements 9.7**
        ///
        /// *对于任意* 统计数据，导出的报告应该包含所有必要信息。
        #[test]
        fn prop_report_export_correctness(flows in arb_flow_list()) {
            let service = EnhancedStatsService::new(
                Arc::new(RwLock::new(FlowMemoryStore::new(1000)))
            );

            let now = Utc::now();
            let time_range = StatsTimeRange {
                start: now - Duration::hours(24),
                end: now,
            };

            // 计算统计数据
            let token_dist = service.calculate_token_distribution(&flows);
            let success_by_provider = service.calculate_success_by_provider(&flows);
            let latency_hist = service.calculate_latency_histogram(&flows, &default_latency_buckets());
            let error_dist = service.calculate_error_distribution(&flows);
            let request_rate = service.calculate_request_rate(&flows, &time_range);

            let stats = EnhancedStats {
                request_trend: TrendData::default(),
                token_by_model: token_dist,
                success_by_provider,
                latency_histogram: latency_hist,
                error_distribution: error_dist,
                request_rate,
                time_range: time_range.clone(),
            };

            // 测试 JSON 导出
            let json_report = service.export_json(&stats);
            prop_assert!(
                !json_report.is_empty(),
                "JSON 报告不应该为空"
            );
            // 验证 JSON 可以解析
            let parsed: Result<EnhancedStats, _> = serde_json::from_str(&json_report);
            prop_assert!(
                parsed.is_ok(),
                "JSON 报告应该可以解析回 EnhancedStats"
            );

            // 测试 Markdown 导出
            let md_report = service.export_markdown(&stats);
            prop_assert!(
                !md_report.is_empty(),
                "Markdown 报告不应该为空"
            );
            prop_assert!(
                md_report.contains("# Flow 统计报告"),
                "Markdown 报告应该包含标题"
            );

            // 测试 CSV 导出
            let csv_report = service.export_csv(&stats);
            prop_assert!(
                !csv_report.is_empty(),
                "CSV 报告不应该为空"
            );
            prop_assert!(
                csv_report.contains("Model,Tokens"),
                "CSV 报告应该包含 Token 分布表头"
            );
        }
    }
}
