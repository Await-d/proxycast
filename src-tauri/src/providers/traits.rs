//! Provider Trait 定义
//!
//! 统一的 Provider 接口，用于凭证管理和 Token 生命周期管理。

#![allow(dead_code)]

use async_trait::async_trait;
use std::error::Error;

/// Provider 结果类型别名（与现有方法签名兼容）
pub type ProviderResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// 凭证管理 Trait
///
/// 定义所有 OAuth Provider 必须实现的凭证管理接口
#[async_trait]
pub trait CredentialProvider: Send + Sync {
    /// 从指定路径加载凭证
    async fn load_credentials_from_path(&mut self, path: &str) -> ProviderResult<()>;

    /// 保存凭证到文件
    async fn save_credentials(&self) -> ProviderResult<()>;

    /// 检查 Token 是否有效（未过期）
    fn is_token_valid(&self) -> bool;

    /// 检查 Token 是否即将过期（通常提前 5 分钟）
    fn is_token_expiring_soon(&self) -> bool;

    /// 刷新 Token
    ///
    /// 返回新的 access_token
    async fn refresh_token(&mut self) -> ProviderResult<String>;

    /// 获取当前 access_token
    fn get_access_token(&self) -> Option<&str>;

    /// 获取 Provider 类型名称
    fn provider_type(&self) -> &'static str;
}

/// Token 管理辅助 Trait
///
/// 提供带重试的 Token 刷新功能
#[async_trait]
pub trait TokenManager: CredentialProvider {
    /// 带重试的 Token 刷新
    ///
    /// # Arguments
    /// * `max_retries` - 最大重试次数
    /// * `retry_delay_ms` - 重试间隔（毫秒）
    async fn refresh_token_with_retry(
        &mut self,
        max_retries: u32,
        retry_delay_ms: u64,
    ) -> ProviderResult<String> {
        let mut last_error = None;

        for attempt in 0..=max_retries {
            match self.refresh_token().await {
                Ok(token) => return Ok(token),
                Err(e) => {
                    tracing::warn!(
                        "[{}] Token refresh attempt {} failed: {}",
                        self.provider_type(),
                        attempt + 1,
                        e
                    );
                    last_error = Some(e);
                    if attempt < max_retries {
                        tokio::time::sleep(tokio::time::Duration::from_millis(retry_delay_ms))
                            .await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| "Token refresh failed".into()))
    }

    /// 确保 Token 有效（如需要则刷新）
    async fn ensure_valid_token(&mut self) -> ProviderResult<String> {
        if !self.is_token_valid() || self.is_token_expiring_soon() {
            self.refresh_token().await
        } else {
            self.get_access_token()
                .map(|s| s.to_string())
                .ok_or_else(|| "No access token available".into())
        }
    }
}

// 为所有实现了 CredentialProvider 的类型自动实现 TokenManager
impl<T: CredentialProvider> TokenManager for T {}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock Provider for testing
    struct MockProvider {
        token: Option<String>,
        valid: bool,
        expiring_soon: bool,
        refresh_count: u32,
    }

    #[async_trait]
    impl CredentialProvider for MockProvider {
        async fn load_credentials_from_path(&mut self, _path: &str) -> ProviderResult<()> {
            Ok(())
        }

        async fn save_credentials(&self) -> ProviderResult<()> {
            Ok(())
        }

        fn is_token_valid(&self) -> bool {
            self.valid
        }

        fn is_token_expiring_soon(&self) -> bool {
            self.expiring_soon
        }

        async fn refresh_token(&mut self) -> ProviderResult<String> {
            self.refresh_count += 1;
            self.token = Some(format!("new_token_{}", self.refresh_count));
            self.valid = true;
            self.expiring_soon = false;
            Ok(self.token.clone().unwrap())
        }

        fn get_access_token(&self) -> Option<&str> {
            self.token.as_deref()
        }

        fn provider_type(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_ensure_valid_token_when_valid() {
        let mut provider = MockProvider {
            token: Some("existing_token".to_string()),
            valid: true,
            expiring_soon: false,
            refresh_count: 0,
        };

        let token = provider.ensure_valid_token().await.unwrap();
        assert_eq!(token, "existing_token");
        assert_eq!(provider.refresh_count, 0);
    }

    #[tokio::test]
    async fn test_ensure_valid_token_when_expiring() {
        let mut provider = MockProvider {
            token: Some("old_token".to_string()),
            valid: true,
            expiring_soon: true,
            refresh_count: 0,
        };

        let token = provider.ensure_valid_token().await.unwrap();
        assert_eq!(token, "new_token_1");
        assert_eq!(provider.refresh_count, 1);
    }

    #[tokio::test]
    async fn test_ensure_valid_token_when_invalid() {
        let mut provider = MockProvider {
            token: Some("invalid_token".to_string()),
            valid: false,
            expiring_soon: false,
            refresh_count: 0,
        };

        let token = provider.ensure_valid_token().await.unwrap();
        assert_eq!(token, "new_token_1");
        assert_eq!(provider.refresh_count, 1);
    }
}
