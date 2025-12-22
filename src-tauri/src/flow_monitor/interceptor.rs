//! Flow 拦截器
//!
//! 该模块实现 LLM Flow 的拦截功能，允许用户暂停、查看和修改请求/响应。
//!
//! # 功能
//!
//! - 根据过滤表达式拦截匹配的 Flow
//! - 支持拦截请求、响应或两者
//! - 支持超时自动处理
//! - 实时事件广播

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, RwLock};
use tokio::time::{timeout, Duration};

use super::filter_parser::{FilterExpr, FilterParser};
use super::models::{LLMFlow, LLMRequest, LLMResponse};

// ============================================================================
// 配置结构
// ============================================================================

/// 超时动作
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeoutAction {
    /// 超时后继续处理
    Continue,
    /// 超时后取消请求
    Cancel,
}

impl Default for TimeoutAction {
    fn default() -> Self {
        TimeoutAction::Continue
    }
}

/// 拦截配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptConfig {
    /// 是否启用拦截
    #[serde(default)]
    pub enabled: bool,
    /// 过滤表达式（可选，为空时拦截所有）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_expr: Option<String>,
    /// 是否拦截请求
    #[serde(default = "default_intercept_request")]
    pub intercept_request: bool,
    /// 是否拦截响应
    #[serde(default)]
    pub intercept_response: bool,
    /// 超时时间（毫秒）
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// 超时动作
    #[serde(default)]
    pub timeout_action: TimeoutAction,
}

fn default_intercept_request() -> bool {
    true
}

fn default_timeout_ms() -> u64 {
    30000 // 30 秒
}

impl Default for InterceptConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            filter_expr: None,
            intercept_request: default_intercept_request(),
            intercept_response: false,
            timeout_ms: default_timeout_ms(),
            timeout_action: TimeoutAction::default(),
        }
    }
}

// ============================================================================
// 拦截状态和类型
// ============================================================================

/// 拦截类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterceptType {
    /// 拦截请求
    Request,
    /// 拦截响应
    Response,
}

/// 拦截状态
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterceptState {
    /// 等待用户操作
    Pending,
    /// 用户正在编辑
    Editing,
    /// 已继续处理
    Continued,
    /// 已取消
    Cancelled,
    /// 已超时
    TimedOut,
}

impl Default for InterceptState {
    fn default() -> Self {
        InterceptState::Pending
    }
}

// ============================================================================
// 被拦截的 Flow
// ============================================================================

/// 被拦截的 Flow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptedFlow {
    /// Flow ID
    pub flow_id: String,
    /// 拦截状态
    pub state: InterceptState,
    /// 拦截类型
    pub intercept_type: InterceptType,
    /// 原始请求（如果拦截请求）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_request: Option<LLMRequest>,
    /// 修改后的请求
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_request: Option<LLMRequest>,
    /// 原始响应（如果拦截响应）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_response: Option<LLMResponse>,
    /// 修改后的响应
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_response: Option<LLMResponse>,
    /// 拦截时间
    pub intercepted_at: DateTime<Utc>,
}

impl InterceptedFlow {
    /// 创建新的拦截请求
    pub fn new_request(flow_id: String, request: LLMRequest) -> Self {
        Self {
            flow_id,
            state: InterceptState::Pending,
            intercept_type: InterceptType::Request,
            original_request: Some(request),
            modified_request: None,
            original_response: None,
            modified_response: None,
            intercepted_at: Utc::now(),
        }
    }

    /// 创建新的拦截响应
    pub fn new_response(flow_id: String, response: LLMResponse) -> Self {
        Self {
            flow_id,
            state: InterceptState::Pending,
            intercept_type: InterceptType::Response,
            original_request: None,
            modified_request: None,
            original_response: Some(response),
            modified_response: None,
            intercepted_at: Utc::now(),
        }
    }
}

// ============================================================================
// 修改数据
// ============================================================================

/// 修改后的数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModifiedData {
    /// 修改后的请求
    Request(LLMRequest),
    /// 修改后的响应
    Response(LLMResponse),
}

// ============================================================================
// 拦截事件
// ============================================================================

/// 拦截事件
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum InterceptEvent {
    /// Flow 被拦截
    FlowIntercepted {
        /// 被拦截的 Flow 信息
        flow: InterceptedFlow,
    },
    /// Flow 继续处理
    FlowContinued {
        /// Flow ID
        flow_id: String,
        /// 是否有修改
        modified: bool,
    },
    /// Flow 被取消
    FlowCancelled {
        /// Flow ID
        flow_id: String,
    },
    /// Flow 超时
    FlowTimedOut {
        /// Flow ID
        flow_id: String,
        /// 超时动作
        action: TimeoutAction,
    },
    /// 配置已更新
    ConfigUpdated {
        /// 新配置
        config: InterceptConfig,
    },
}

// ============================================================================
// 拦截动作
// ============================================================================

/// 用户拦截动作
#[derive(Debug, Clone)]
pub enum InterceptAction {
    /// 继续处理（可能带有修改）
    Continue(Option<ModifiedData>),
    /// 取消请求
    Cancel,
    /// 超时
    Timeout(TimeoutAction),
}

// ============================================================================
// 等待中的拦截
// ============================================================================

/// 等待中的拦截
struct PendingIntercept {
    /// 被拦截的 Flow 信息
    flow: InterceptedFlow,
    /// 动作发送器
    action_sender: Option<oneshot::Sender<InterceptAction>>,
}

// ============================================================================
// 拦截器错误
// ============================================================================

/// 拦截器错误
#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
pub enum InterceptorError {
    /// Flow 不存在
    #[error("Flow '{0}' 不存在或未被拦截")]
    FlowNotFound(String),
    /// 无效的过滤表达式
    #[error("无效的过滤表达式: {0}")]
    InvalidFilterExpr(String),
    /// 操作已完成
    #[error("Flow '{0}' 的拦截操作已完成")]
    AlreadyCompleted(String),
    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

// ============================================================================
// Flow 拦截器
// ============================================================================

/// Flow 拦截器
///
/// 负责拦截和管理 LLM Flow 的核心服务。
pub struct FlowInterceptor {
    /// 拦截配置
    config: RwLock<InterceptConfig>,
    /// 编译后的过滤器
    filter: RwLock<Option<Arc<dyn Fn(&LLMFlow) -> bool + Send + Sync>>>,
    /// 等待中的拦截
    pending_intercepts: RwLock<HashMap<String, PendingIntercept>>,
    /// 事件发送器
    event_sender: broadcast::Sender<InterceptEvent>,
}

impl FlowInterceptor {
    /// 创建新的拦截器
    pub fn new(config: InterceptConfig) -> Self {
        let (event_sender, _) = broadcast::channel(100);
        let filter = Self::compile_filter(&config.filter_expr);

        Self {
            config: RwLock::new(config),
            filter: RwLock::new(filter),
            pending_intercepts: RwLock::new(HashMap::new()),
            event_sender,
        }
    }

    /// 编译过滤表达式
    fn compile_filter(
        filter_expr: &Option<String>,
    ) -> Option<Arc<dyn Fn(&LLMFlow) -> bool + Send + Sync>> {
        filter_expr.as_ref().and_then(|expr| {
            FilterParser::parse(expr).ok().map(|parsed| {
                let filter = FilterParser::compile(&parsed);
                Arc::new(move |flow: &LLMFlow| filter(flow))
                    as Arc<dyn Fn(&LLMFlow) -> bool + Send + Sync>
            })
        })
    }

    /// 获取当前配置
    pub async fn config(&self) -> InterceptConfig {
        self.config.read().await.clone()
    }

    /// 更新配置
    pub async fn update_config(&self, config: InterceptConfig) -> Result<(), InterceptorError> {
        // 验证过滤表达式
        if let Some(ref expr) = config.filter_expr {
            FilterParser::parse(expr)
                .map_err(|e| InterceptorError::InvalidFilterExpr(e.to_string()))?;
        }

        // 编译新的过滤器
        let new_filter = Self::compile_filter(&config.filter_expr);

        // 更新配置和过滤器
        {
            let mut current_config = self.config.write().await;
            *current_config = config.clone();
        }
        {
            let mut current_filter = self.filter.write().await;
            *current_filter = new_filter;
        }

        // 发送配置更新事件
        let _ = self
            .event_sender
            .send(InterceptEvent::ConfigUpdated { config });

        Ok(())
    }

    /// 订阅拦截事件
    pub fn subscribe(&self) -> broadcast::Receiver<InterceptEvent> {
        self.event_sender.subscribe()
    }

    /// 检查是否应该拦截
    pub async fn should_intercept(&self, flow: &LLMFlow, intercept_type: &InterceptType) -> bool {
        let config = self.config.read().await;

        // 检查是否启用
        if !config.enabled {
            return false;
        }

        // 检查拦截类型
        match intercept_type {
            InterceptType::Request => {
                if !config.intercept_request {
                    return false;
                }
            }
            InterceptType::Response => {
                if !config.intercept_response {
                    return false;
                }
            }
        }

        // 检查过滤器
        let filter = self.filter.read().await;
        if let Some(ref f) = *filter {
            f(flow)
        } else {
            // 没有过滤器时，拦截所有
            true
        }
    }

    /// 拦截请求
    pub async fn intercept_request(&self, flow_id: &str, request: LLMRequest) -> InterceptedFlow {
        let intercepted = InterceptedFlow::new_request(flow_id.to_string(), request);
        self.add_pending_intercept(intercepted.clone()).await;

        // 发送拦截事件
        let _ = self.event_sender.send(InterceptEvent::FlowIntercepted {
            flow: intercepted.clone(),
        });

        intercepted
    }

    /// 拦截响应
    pub async fn intercept_response(
        &self,
        flow_id: &str,
        response: LLMResponse,
    ) -> InterceptedFlow {
        let intercepted = InterceptedFlow::new_response(flow_id.to_string(), response);
        self.add_pending_intercept(intercepted.clone()).await;

        // 发送拦截事件
        let _ = self.event_sender.send(InterceptEvent::FlowIntercepted {
            flow: intercepted.clone(),
        });

        intercepted
    }

    /// 添加等待中的拦截
    async fn add_pending_intercept(&self, flow: InterceptedFlow) {
        let mut pending = self.pending_intercepts.write().await;
        pending.insert(
            flow.flow_id.clone(),
            PendingIntercept {
                flow,
                action_sender: None,
            },
        );
    }

    /// 继续处理 Flow
    pub async fn continue_flow(
        &self,
        flow_id: &str,
        modified: Option<ModifiedData>,
    ) -> Result<(), InterceptorError> {
        let mut pending = self.pending_intercepts.write().await;

        if let Some(mut intercept) = pending.remove(flow_id) {
            // 更新状态
            intercept.flow.state = InterceptState::Continued;

            // 更新修改后的数据
            if let Some(ref data) = modified {
                match data {
                    ModifiedData::Request(req) => {
                        intercept.flow.modified_request = Some(req.clone());
                    }
                    ModifiedData::Response(resp) => {
                        intercept.flow.modified_response = Some(resp.clone());
                    }
                }
            }

            // 发送动作
            if let Some(sender) = intercept.action_sender {
                let _ = sender.send(InterceptAction::Continue(modified.clone()));
            }

            // 发送事件
            let _ = self.event_sender.send(InterceptEvent::FlowContinued {
                flow_id: flow_id.to_string(),
                modified: modified.is_some(),
            });

            Ok(())
        } else {
            Err(InterceptorError::FlowNotFound(flow_id.to_string()))
        }
    }

    /// 取消 Flow
    pub async fn cancel_flow(&self, flow_id: &str) -> Result<(), InterceptorError> {
        let mut pending = self.pending_intercepts.write().await;

        if let Some(mut intercept) = pending.remove(flow_id) {
            // 更新状态
            intercept.flow.state = InterceptState::Cancelled;

            // 发送动作
            if let Some(sender) = intercept.action_sender {
                let _ = sender.send(InterceptAction::Cancel);
            }

            // 发送事件
            let _ = self.event_sender.send(InterceptEvent::FlowCancelled {
                flow_id: flow_id.to_string(),
            });

            Ok(())
        } else {
            Err(InterceptorError::FlowNotFound(flow_id.to_string()))
        }
    }

    /// 等待用户操作
    ///
    /// 此方法会阻塞直到用户执行操作或超时。
    pub async fn wait_for_action(&self, flow_id: &str) -> InterceptAction {
        let config = self.config.read().await.clone();
        let timeout_ms = config.timeout_ms;
        let timeout_action = config.timeout_action.clone();
        drop(config);

        // 创建 oneshot channel
        let (tx, rx) = oneshot::channel();

        // 设置 action_sender
        {
            let mut pending = self.pending_intercepts.write().await;
            if let Some(intercept) = pending.get_mut(flow_id) {
                intercept.action_sender = Some(tx);
            } else {
                // Flow 不存在，返回取消
                return InterceptAction::Cancel;
            }
        }

        // 等待动作或超时
        let result = timeout(Duration::from_millis(timeout_ms), rx).await;

        match result {
            Ok(Ok(action)) => action,
            Ok(Err(_)) => {
                // Channel 被关闭，视为取消
                InterceptAction::Cancel
            }
            Err(_) => {
                // 超时
                self.handle_timeout(flow_id, &timeout_action).await;
                InterceptAction::Timeout(timeout_action)
            }
        }
    }

    /// 处理超时
    async fn handle_timeout(&self, flow_id: &str, timeout_action: &TimeoutAction) {
        let mut pending = self.pending_intercepts.write().await;

        if let Some(mut intercept) = pending.remove(flow_id) {
            intercept.flow.state = InterceptState::TimedOut;

            // 发送超时事件
            let _ = self.event_sender.send(InterceptEvent::FlowTimedOut {
                flow_id: flow_id.to_string(),
                action: timeout_action.clone(),
            });
        }
    }

    /// 获取被拦截的 Flow
    pub async fn get_intercepted_flow(&self, flow_id: &str) -> Option<InterceptedFlow> {
        let pending = self.pending_intercepts.read().await;
        pending.get(flow_id).map(|p| p.flow.clone())
    }

    /// 获取所有被拦截的 Flow
    pub async fn list_intercepted_flows(&self) -> Vec<InterceptedFlow> {
        let pending = self.pending_intercepts.read().await;
        pending.values().map(|p| p.flow.clone()).collect()
    }

    /// 获取被拦截的 Flow 数量
    pub async fn intercepted_count(&self) -> usize {
        self.pending_intercepts.read().await.len()
    }

    /// 检查拦截是否启用
    pub async fn is_enabled(&self) -> bool {
        self.config.read().await.enabled
    }

    /// 启用拦截
    pub async fn enable(&self) {
        let mut config = self.config.write().await;
        config.enabled = true;
    }

    /// 禁用拦截
    pub async fn disable(&self) {
        let mut config = self.config.write().await;
        config.enabled = false;
    }

    /// 设置编辑状态
    pub async fn set_editing(&self, flow_id: &str) -> Result<(), InterceptorError> {
        let mut pending = self.pending_intercepts.write().await;

        if let Some(intercept) = pending.get_mut(flow_id) {
            intercept.flow.state = InterceptState::Editing;
            Ok(())
        } else {
            Err(InterceptorError::FlowNotFound(flow_id.to_string()))
        }
    }
}

impl Default for FlowInterceptor {
    fn default() -> Self {
        Self::new(InterceptConfig::default())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_monitor::models::{
        FlowMetadata, FlowType, LLMRequest, Message, MessageContent, MessageRole,
        RequestParameters, TokenUsage,
    };
    use crate::ProviderType;
    use std::collections::HashMap;

    /// 创建测试用的 LLMRequest
    fn create_test_request(model: &str) -> LLMRequest {
        LLMRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: HashMap::new(),
            body: serde_json::Value::Null,
            messages: vec![Message {
                role: MessageRole::User,
                content: MessageContent::Text("Hello".to_string()),
                tool_calls: None,
                tool_result: None,
                name: None,
            }],
            system_prompt: None,
            tools: None,
            model: model.to_string(),
            original_model: None,
            parameters: RequestParameters::default(),
            size_bytes: 0,
            timestamp: Utc::now(),
        }
    }

    /// 创建测试用的 LLMResponse
    fn create_test_response() -> LLMResponse {
        LLMResponse {
            status_code: 200,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body: serde_json::Value::Null,
            content: "Hello, world!".to_string(),
            thinking: None,
            tool_calls: Vec::new(),
            usage: TokenUsage::default(),
            stop_reason: None,
            size_bytes: 0,
            timestamp_start: Utc::now(),
            timestamp_end: Utc::now(),
            stream_info: None,
        }
    }

    /// 创建测试用的 LLMFlow
    fn create_test_flow(model: &str, provider: ProviderType) -> LLMFlow {
        let request = create_test_request(model);
        let metadata = FlowMetadata {
            provider,
            ..Default::default()
        };
        LLMFlow::new(
            "test-flow-id".to_string(),
            FlowType::ChatCompletions,
            request,
            metadata,
        )
    }

    #[tokio::test]
    async fn test_interceptor_creation() {
        let config = InterceptConfig::default();
        let interceptor = FlowInterceptor::new(config);

        assert!(!interceptor.is_enabled().await);
        assert_eq!(interceptor.intercepted_count().await, 0);
    }

    #[tokio::test]
    async fn test_interceptor_enable_disable() {
        let interceptor = FlowInterceptor::default();

        assert!(!interceptor.is_enabled().await);

        interceptor.enable().await;
        assert!(interceptor.is_enabled().await);

        interceptor.disable().await;
        assert!(!interceptor.is_enabled().await);
    }

    #[tokio::test]
    async fn test_should_intercept_disabled() {
        let interceptor = FlowInterceptor::default();
        let flow = create_test_flow("gpt-4", ProviderType::OpenAI);

        // 禁用时不应该拦截
        assert!(
            !interceptor
                .should_intercept(&flow, &InterceptType::Request)
                .await
        );
    }

    #[tokio::test]
    async fn test_should_intercept_enabled_no_filter() {
        let config = InterceptConfig {
            enabled: true,
            intercept_request: true,
            ..Default::default()
        };
        let interceptor = FlowInterceptor::new(config);
        let flow = create_test_flow("gpt-4", ProviderType::OpenAI);

        // 启用且无过滤器时应该拦截所有
        assert!(
            interceptor
                .should_intercept(&flow, &InterceptType::Request)
                .await
        );
    }

    #[tokio::test]
    async fn test_should_intercept_with_filter() {
        let config = InterceptConfig {
            enabled: true,
            filter_expr: Some("~m claude".to_string()),
            intercept_request: true,
            ..Default::default()
        };
        let interceptor = FlowInterceptor::new(config);

        let flow_claude = create_test_flow("claude-3-opus", ProviderType::Claude);
        let flow_gpt = create_test_flow("gpt-4", ProviderType::OpenAI);

        // 应该拦截 claude 模型
        assert!(
            interceptor
                .should_intercept(&flow_claude, &InterceptType::Request)
                .await
        );
        // 不应该拦截 gpt 模型
        assert!(
            !interceptor
                .should_intercept(&flow_gpt, &InterceptType::Request)
                .await
        );
    }

    #[tokio::test]
    async fn test_should_intercept_request_only() {
        let config = InterceptConfig {
            enabled: true,
            intercept_request: true,
            intercept_response: false,
            ..Default::default()
        };
        let interceptor = FlowInterceptor::new(config);
        let flow = create_test_flow("gpt-4", ProviderType::OpenAI);

        assert!(
            interceptor
                .should_intercept(&flow, &InterceptType::Request)
                .await
        );
        assert!(
            !interceptor
                .should_intercept(&flow, &InterceptType::Response)
                .await
        );
    }

    #[tokio::test]
    async fn test_should_intercept_response_only() {
        let config = InterceptConfig {
            enabled: true,
            intercept_request: false,
            intercept_response: true,
            ..Default::default()
        };
        let interceptor = FlowInterceptor::new(config);
        let flow = create_test_flow("gpt-4", ProviderType::OpenAI);

        assert!(
            !interceptor
                .should_intercept(&flow, &InterceptType::Request)
                .await
        );
        assert!(
            interceptor
                .should_intercept(&flow, &InterceptType::Response)
                .await
        );
    }

    #[tokio::test]
    async fn test_intercept_request() {
        let interceptor = FlowInterceptor::default();
        let request = create_test_request("gpt-4");

        let intercepted = interceptor
            .intercept_request("flow-1", request.clone())
            .await;

        assert_eq!(intercepted.flow_id, "flow-1");
        assert_eq!(intercepted.state, InterceptState::Pending);
        assert_eq!(intercepted.intercept_type, InterceptType::Request);
        assert!(intercepted.original_request.is_some());
        assert!(intercepted.modified_request.is_none());
        assert_eq!(interceptor.intercepted_count().await, 1);
    }

    #[tokio::test]
    async fn test_intercept_response() {
        let interceptor = FlowInterceptor::default();
        let response = create_test_response();

        let intercepted = interceptor
            .intercept_response("flow-1", response.clone())
            .await;

        assert_eq!(intercepted.flow_id, "flow-1");
        assert_eq!(intercepted.state, InterceptState::Pending);
        assert_eq!(intercepted.intercept_type, InterceptType::Response);
        assert!(intercepted.original_response.is_some());
        assert!(intercepted.modified_response.is_none());
        assert_eq!(interceptor.intercepted_count().await, 1);
    }

    #[tokio::test]
    async fn test_continue_flow() {
        let interceptor = FlowInterceptor::default();
        let request = create_test_request("gpt-4");

        interceptor.intercept_request("flow-1", request).await;

        // 继续处理
        let result = interceptor.continue_flow("flow-1", None).await;
        assert!(result.is_ok());
        assert_eq!(interceptor.intercepted_count().await, 0);
    }

    #[tokio::test]
    async fn test_continue_flow_with_modification() {
        let interceptor = FlowInterceptor::default();
        let request = create_test_request("gpt-4");

        interceptor.intercept_request("flow-1", request).await;

        // 修改请求并继续
        let modified_request = create_test_request("gpt-4-turbo");
        let result = interceptor
            .continue_flow("flow-1", Some(ModifiedData::Request(modified_request)))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cancel_flow() {
        let interceptor = FlowInterceptor::default();
        let request = create_test_request("gpt-4");

        interceptor.intercept_request("flow-1", request).await;

        // 取消
        let result = interceptor.cancel_flow("flow-1").await;
        assert!(result.is_ok());
        assert_eq!(interceptor.intercepted_count().await, 0);
    }

    #[tokio::test]
    async fn test_continue_nonexistent_flow() {
        let interceptor = FlowInterceptor::default();

        let result = interceptor.continue_flow("nonexistent", None).await;
        assert!(matches!(result, Err(InterceptorError::FlowNotFound(_))));
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_flow() {
        let interceptor = FlowInterceptor::default();

        let result = interceptor.cancel_flow("nonexistent").await;
        assert!(matches!(result, Err(InterceptorError::FlowNotFound(_))));
    }

    #[tokio::test]
    async fn test_update_config() {
        let interceptor = FlowInterceptor::default();

        let new_config = InterceptConfig {
            enabled: true,
            filter_expr: Some("~m claude".to_string()),
            intercept_request: true,
            intercept_response: true,
            timeout_ms: 60000,
            timeout_action: TimeoutAction::Cancel,
        };

        let result = interceptor.update_config(new_config.clone()).await;
        assert!(result.is_ok());

        let config = interceptor.config().await;
        assert!(config.enabled);
        assert_eq!(config.filter_expr, Some("~m claude".to_string()));
        assert_eq!(config.timeout_ms, 60000);
        assert_eq!(config.timeout_action, TimeoutAction::Cancel);
    }

    #[tokio::test]
    async fn test_update_config_invalid_filter() {
        let interceptor = FlowInterceptor::default();

        let new_config = InterceptConfig {
            enabled: true,
            filter_expr: Some("~invalid".to_string()),
            ..Default::default()
        };

        let result = interceptor.update_config(new_config).await;
        assert!(matches!(
            result,
            Err(InterceptorError::InvalidFilterExpr(_))
        ));
    }

    #[tokio::test]
    async fn test_get_intercepted_flow() {
        let interceptor = FlowInterceptor::default();
        let request = create_test_request("gpt-4");

        interceptor.intercept_request("flow-1", request).await;

        let flow = interceptor.get_intercepted_flow("flow-1").await;
        assert!(flow.is_some());
        assert_eq!(flow.unwrap().flow_id, "flow-1");

        let nonexistent = interceptor.get_intercepted_flow("nonexistent").await;
        assert!(nonexistent.is_none());
    }

    #[tokio::test]
    async fn test_list_intercepted_flows() {
        let interceptor = FlowInterceptor::default();

        interceptor
            .intercept_request("flow-1", create_test_request("gpt-4"))
            .await;
        interceptor
            .intercept_request("flow-2", create_test_request("claude-3"))
            .await;

        let flows = interceptor.list_intercepted_flows().await;
        assert_eq!(flows.len(), 2);
    }

    #[tokio::test]
    async fn test_set_editing() {
        let interceptor = FlowInterceptor::default();
        let request = create_test_request("gpt-4");

        interceptor.intercept_request("flow-1", request).await;

        let result = interceptor.set_editing("flow-1").await;
        assert!(result.is_ok());

        let flow = interceptor.get_intercepted_flow("flow-1").await.unwrap();
        assert_eq!(flow.state, InterceptState::Editing);
    }

    #[tokio::test]
    async fn test_event_subscription() {
        let interceptor = FlowInterceptor::default();
        let mut receiver = interceptor.subscribe();

        let request = create_test_request("gpt-4");
        interceptor.intercept_request("flow-1", request).await;

        // 应该收到 FlowIntercepted 事件
        let event = receiver.try_recv();
        assert!(event.is_ok());
        if let InterceptEvent::FlowIntercepted { flow } = event.unwrap() {
            assert_eq!(flow.flow_id, "flow-1");
        } else {
            panic!("Expected FlowIntercepted event");
        }
    }
}

// ============================================================================
// 属性测试
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::flow_monitor::models::{
        FlowAnnotations, FlowError, FlowErrorType, FlowMetadata, FlowTimestamps, FlowType,
        FunctionCall, LLMRequest, LLMResponse, Message, MessageContent, MessageRole,
        RequestParameters, ThinkingContent, TokenUsage, ToolCall,
    };
    use crate::ProviderType;
    use proptest::prelude::*;
    use tokio::runtime::Runtime;

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
            Just(ProviderType::Antigravity),
        ]
    }

    /// 生成随机的 FlowState
    fn arb_flow_state() -> impl Strategy<Value = crate::flow_monitor::models::FlowState> {
        prop_oneof![
            Just(crate::flow_monitor::models::FlowState::Pending),
            Just(crate::flow_monitor::models::FlowState::Streaming),
            Just(crate::flow_monitor::models::FlowState::Completed),
            Just(crate::flow_monitor::models::FlowState::Failed),
            Just(crate::flow_monitor::models::FlowState::Cancelled),
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
            Just("qwen-max".to_string()),
        ]
    }

    /// 生成随机的标签
    fn arb_tags() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-z]{3,10}", 0..5)
    }

    /// 生成随机的 LLMFlow
    fn arb_llm_flow() -> impl Strategy<Value = LLMFlow> {
        (
            "[a-f0-9]{8}",
            arb_model_name(),
            arb_provider_type(),
            arb_flow_state(),
            any::<bool>(),  // starred
            arb_tags(),     // tags
            any::<bool>(),  // has_error
            any::<bool>(),  // has_tool_calls
            any::<bool>(),  // has_thinking
            0u32..50000u32, // total_tokens
            0u64..30000u64, // duration_ms
        )
            .prop_map(
                |(
                    id,
                    model,
                    provider,
                    state,
                    starred,
                    tags,
                    has_error,
                    has_tool_calls,
                    has_thinking,
                    total_tokens,
                    duration_ms,
                )| {
                    let request = LLMRequest {
                        method: "POST".to_string(),
                        path: "/v1/chat/completions".to_string(),
                        model,
                        parameters: RequestParameters::default(),
                        ..Default::default()
                    };

                    let metadata = FlowMetadata {
                        provider,
                        ..Default::default()
                    };

                    let mut flow = LLMFlow::new(id, FlowType::ChatCompletions, request, metadata);
                    flow.state = state;
                    flow.annotations.starred = starred;
                    flow.annotations.tags = tags;
                    flow.timestamps.duration_ms = duration_ms;

                    if has_error {
                        flow.error = Some(FlowError::new(FlowErrorType::ServerError, "Test error"));
                    }

                    let mut response = LLMResponse {
                        usage: TokenUsage {
                            input_tokens: total_tokens / 2,
                            output_tokens: total_tokens / 2,
                            total_tokens,
                            ..Default::default()
                        },
                        ..Default::default()
                    };

                    if has_tool_calls {
                        response.tool_calls = vec![ToolCall {
                            id: "call_1".to_string(),
                            tool_type: "function".to_string(),
                            function: FunctionCall {
                                name: "test_function".to_string(),
                                arguments: "{}".to_string(),
                            },
                        }];
                    }

                    if has_thinking {
                        response.thinking = Some(ThinkingContent {
                            text: "Thinking...".to_string(),
                            tokens: Some(100),
                            signature: None,
                        });
                    }

                    flow.response = Some(response);
                    flow
                },
            )
    }

    /// 生成随机的 InterceptType
    fn arb_intercept_type() -> impl Strategy<Value = InterceptType> {
        prop_oneof![Just(InterceptType::Request), Just(InterceptType::Response),]
    }

    /// 生成随机的过滤表达式
    fn arb_filter_expr() -> impl Strategy<Value = String> {
        prop_oneof![
            arb_model_name().prop_map(|m| format!("~m {}", m)),
            prop_oneof![
                Just("kiro".to_string()),
                Just("openai".to_string()),
                Just("claude".to_string()),
                Just("gemini".to_string()),
            ]
            .prop_map(|p| format!("~p {}", p)),
            Just("~e".to_string()),
            Just("~t".to_string()),
            Just("~k".to_string()),
            Just("~starred".to_string()),
            (0i64..50000i64).prop_map(|n| format!("~tokens >{}", n)),
            (0i64..30000i64).prop_map(|n| format!("~latency >{}ms", n)),
        ]
    }

    /// 生成随机的 InterceptConfig
    fn arb_intercept_config() -> impl Strategy<Value = InterceptConfig> {
        (
            any::<bool>(),                       // enabled
            prop::option::of(arb_filter_expr()), // filter_expr
            any::<bool>(),                       // intercept_request
            any::<bool>(),                       // intercept_response
            1000u64..60000u64,                   // timeout_ms
            prop_oneof![Just(TimeoutAction::Continue), Just(TimeoutAction::Cancel),],
        )
            .prop_map(
                |(
                    enabled,
                    filter_expr,
                    intercept_request,
                    intercept_response,
                    timeout_ms,
                    timeout_action,
                )| {
                    InterceptConfig {
                        enabled,
                        filter_expr,
                        intercept_request,
                        intercept_response,
                        timeout_ms,
                        timeout_action,
                    }
                },
            )
    }

    // ========================================================================
    // Property 4: 拦截规则匹配正确性
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 4: 拦截规则匹配正确性**
        /// **Validates: Requirements 2.1, 2.7**
        ///
        /// *对于任意* 拦截配置和 Flow，拦截器的 should_intercept 方法应该正确判断是否需要拦截。
        #[test]
        fn prop_intercept_disabled_never_intercepts(
            flow in arb_llm_flow(),
            intercept_type in arb_intercept_type(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                // 禁用拦截时，永远不应该拦截
                let config = InterceptConfig {
                    enabled: false,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &intercept_type).await;
                prop_assert!(
                    !should_intercept,
                    "禁用拦截时不应该拦截任何 Flow"
                );
                Ok(())
            })?;
        }

        #[test]
        fn prop_intercept_type_respected(
            flow in arb_llm_flow(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                // 只拦截请求
                let config_request_only = InterceptConfig {
                    enabled: true,
                    intercept_request: true,
                    intercept_response: false,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config_request_only);

                let should_intercept_request = interceptor.should_intercept(&flow, &InterceptType::Request).await;
                let should_intercept_response = interceptor.should_intercept(&flow, &InterceptType::Response).await;

                prop_assert!(
                    should_intercept_request,
                    "配置为拦截请求时应该拦截请求"
                );
                prop_assert!(
                    !should_intercept_response,
                    "配置为不拦截响应时不应该拦截响应"
                );

                // 只拦截响应
                let config_response_only = InterceptConfig {
                    enabled: true,
                    intercept_request: false,
                    intercept_response: true,
                    ..Default::default()
                };
                let interceptor2 = FlowInterceptor::new(config_response_only);

                let should_intercept_request2 = interceptor2.should_intercept(&flow, &InterceptType::Request).await;
                let should_intercept_response2 = interceptor2.should_intercept(&flow, &InterceptType::Response).await;

                prop_assert!(
                    !should_intercept_request2,
                    "配置为不拦截请求时不应该拦截请求"
                );
                prop_assert!(
                    should_intercept_response2,
                    "配置为拦截响应时应该拦截响应"
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_filter_model_intercept_correctness(
            flow in arb_llm_flow(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let model = flow.request.model.clone();

                // 使用模型过滤器
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: Some(format!("~m {}", model)),
                    intercept_request: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &InterceptType::Request).await;

                prop_assert!(
                    should_intercept,
                    "使用模型 '{}' 的过滤器应该拦截模型为 '{}' 的 Flow",
                    model,
                    flow.request.model
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_filter_provider_intercept_correctness(
            flow in arb_llm_flow(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let provider_str = format!("{:?}", flow.metadata.provider).to_lowercase();

                // 使用提供商过滤器
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: Some(format!("~p {}", provider_str)),
                    intercept_request: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &InterceptType::Request).await;

                prop_assert!(
                    should_intercept,
                    "使用提供商 '{}' 的过滤器应该拦截提供商为 '{:?}' 的 Flow",
                    provider_str,
                    flow.metadata.provider
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_filter_error_intercept_correctness(
            flow in arb_llm_flow(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                // 使用错误过滤器
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: Some("~e".to_string()),
                    intercept_request: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &InterceptType::Request).await;
                let has_error = flow.error.is_some();

                prop_assert_eq!(
                    should_intercept,
                    has_error,
                    "错误过滤器的拦截结果应该与 flow.error.is_some() 一致"
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_filter_starred_intercept_correctness(
            flow in arb_llm_flow(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                // 使用收藏过滤器
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: Some("~starred".to_string()),
                    intercept_request: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &InterceptType::Request).await;

                prop_assert_eq!(
                    should_intercept,
                    flow.annotations.starred,
                    "收藏过滤器的拦截结果应该与 flow.annotations.starred 一致"
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_filter_tokens_intercept_correctness(
            flow in arb_llm_flow(),
            threshold in 0i64..50000i64,
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let total_tokens = flow
                    .response
                    .as_ref()
                    .map_or(0, |r| r.usage.total_tokens as i64);

                // 使用 Token 过滤器
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: Some(format!("~tokens >{}", threshold)),
                    intercept_request: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &InterceptType::Request).await;

                prop_assert_eq!(
                    should_intercept,
                    total_tokens > threshold,
                    "Token 过滤器的拦截结果应该正确 (actual: {}, threshold: {})",
                    total_tokens,
                    threshold
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_filter_latency_intercept_correctness(
            flow in arb_llm_flow(),
            threshold in 0i64..30000i64,
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                let duration_ms = flow.timestamps.duration_ms as i64;

                // 使用延迟过滤器
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: Some(format!("~latency >{}ms", threshold)),
                    intercept_request: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &InterceptType::Request).await;

                prop_assert_eq!(
                    should_intercept,
                    duration_ms > threshold,
                    "延迟过滤器的拦截结果应该正确 (actual: {}, threshold: {})",
                    duration_ms,
                    threshold
                );

                Ok(())
            })?;
        }

        #[test]
        fn prop_no_filter_intercepts_all(
            flow in arb_llm_flow(),
            intercept_type in arb_intercept_type(),
        ) {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                // 启用但无过滤器时应该拦截所有
                let config = InterceptConfig {
                    enabled: true,
                    filter_expr: None,
                    intercept_request: true,
                    intercept_response: true,
                    ..Default::default()
                };
                let interceptor = FlowInterceptor::new(config);

                let should_intercept = interceptor.should_intercept(&flow, &intercept_type).await;

                prop_assert!(
                    should_intercept,
                    "无过滤器时应该拦截所有 Flow"
                );

                Ok(())
            })?;
        }
    }
}
