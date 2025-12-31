//! 路由解析步骤
//!
//! 解析模型别名并选择 Provider

#![allow(dead_code)]

use super::traits::{PipelineStep, StepError};
use crate::processor::RequestContext;
use crate::router::{ModelMapper, Router};
use crate::ProviderType;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 路由解析步骤
///
/// 解析模型别名并根据路由规则选择 Provider
pub struct RoutingStep {
    /// 路由器
    router: Arc<RwLock<Router>>,
    /// 模型映射器
    mapper: Arc<RwLock<ModelMapper>>,
    /// 默认 Provider
    default_provider: Arc<RwLock<String>>,
}

impl RoutingStep {
    /// 创建新的路由步骤
    pub fn new(
        router: Arc<RwLock<Router>>,
        mapper: Arc<RwLock<ModelMapper>>,
        default_provider: Arc<RwLock<String>>,
    ) -> Self {
        Self {
            router,
            mapper,
            default_provider,
        }
    }

    /// 解析模型别名
    pub async fn resolve_model(&self, model: &str) -> String {
        let mapper = self.mapper.read().await;
        mapper.resolve(model)
    }

    /// 根据模型选择 Provider
    pub async fn select_provider(&self, model: &str) -> Result<ProviderType, StepError> {
        let router = self.router.read().await;

        // 使用路由规则（如果没有匹配的规则，会返回默认 Provider）
        let result = router.route(model);
        Ok(result.provider)
    }
}

#[async_trait]
impl PipelineStep for RoutingStep {
    async fn execute(
        &self,
        ctx: &mut RequestContext,
        payload: &mut serde_json::Value,
    ) -> Result<(), StepError> {
        // 解析模型别名
        let resolved_model = self.resolve_model(&ctx.original_model).await;
        ctx.set_resolved_model(resolved_model.clone());

        // 更新 payload 中的模型名
        if let Some(obj) = payload.as_object_mut() {
            obj.insert("model".to_string(), serde_json::json!(resolved_model));
        }

        // 选择 Provider
        let provider = self.select_provider(&ctx.resolved_model).await?;
        ctx.set_provider(provider);

        tracing::info!(
            "[ROUTE] request_id={} original_model={} resolved_model={} provider={}",
            ctx.request_id,
            ctx.original_model,
            ctx.resolved_model,
            provider
        );

        Ok(())
    }

    fn name(&self) -> &str {
        "routing"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::RoutingRule;

    #[tokio::test]
    async fn test_routing_step_resolve_model() {
        let mut mapper = ModelMapper::new();
        mapper.add_alias("gpt-4", "claude-sonnet-4-5");

        let step = RoutingStep::new(
            Arc::new(RwLock::new(Router::new(ProviderType::Kiro))),
            Arc::new(RwLock::new(mapper)),
            Arc::new(RwLock::new("kiro".to_string())),
        );

        // 别名应解析为实际模型
        let resolved = step.resolve_model("gpt-4").await;
        assert_eq!(resolved, "claude-sonnet-4-5");

        // 非别名应返回原值
        let resolved = step.resolve_model("unknown-model").await;
        assert_eq!(resolved, "unknown-model");
    }

    #[tokio::test]
    async fn test_routing_step_select_provider() {
        let mut router = Router::new(ProviderType::Kiro);
        router.add_rule(RoutingRule::new("gemini-*", ProviderType::Gemini, 10));

        let step = RoutingStep::new(
            Arc::new(RwLock::new(router)),
            Arc::new(RwLock::new(ModelMapper::new())),
            Arc::new(RwLock::new("kiro".to_string())),
        );

        // 匹配路由规则
        let provider = step.select_provider("gemini-2.5-flash").await;
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap(), ProviderType::Gemini);

        // 使用默认 Provider
        let provider = step.select_provider("claude-sonnet-4-5").await;
        assert!(provider.is_ok());
        assert_eq!(provider.unwrap(), ProviderType::Kiro);
    }

    #[tokio::test]
    async fn test_routing_step_execute() {
        let mut mapper = ModelMapper::new();
        mapper.add_alias("gpt-4", "claude-sonnet-4-5");

        let step = RoutingStep::new(
            Arc::new(RwLock::new(Router::new(ProviderType::Kiro))),
            Arc::new(RwLock::new(mapper)),
            Arc::new(RwLock::new("kiro".to_string())),
        );

        let mut ctx = RequestContext::new("gpt-4".to_string());
        let mut payload = serde_json::json!({"model": "gpt-4"});

        let result = step.execute(&mut ctx, &mut payload).await;
        assert!(result.is_ok());
        assert_eq!(ctx.resolved_model, "claude-sonnet-4-5");
        assert_eq!(ctx.provider, Some(ProviderType::Kiro));
        assert_eq!(payload["model"], "claude-sonnet-4-5");
    }
}
