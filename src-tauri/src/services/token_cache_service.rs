//! Token 缓存管理服务
//!
//! 负责管理凭证池中 OAuth Token 的生命周期：
//! - 从源文件加载初始 Token
//! - 缓存刷新后的 Token 到数据库

#![allow(dead_code)]
//! - 按需刷新即将过期的 Token
//! - 处理 401/403 错误时的强制刷新

use crate::database::dao::provider_pool::ProviderPoolDao;
use crate::database::DbConnection;
use crate::models::provider_pool_model::{
    CachedTokenInfo, CredentialData, PoolProviderType, ProviderCredential,
};
use crate::providers::gemini::GeminiProvider;
use crate::providers::kiro::KiroProvider;
use crate::providers::qwen::QwenProvider;
use crate::services::kiro_event_service::KiroEventService;
use chrono::Utc;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Token 刷新错误类型
#[derive(Debug, Clone, PartialEq)]
pub enum RefreshErrorType {
    /// Token被截断或格式问题
    TokenTruncated,
    /// Token格式异常（长度过短等）
    TokenFormat,
    /// 网络连接问题
    Network,
    /// 服务不可用
    ServiceUnavailable,
    /// 认证失败（401, 403等）
    AuthenticationFailed,
    /// 未知错误
    Unknown,
}

/// Token 刷新错误分类结果
#[derive(Debug, Clone)]
pub struct RefreshErrorClassification {
    /// 错误类型
    pub error_type: RefreshErrorType,
    /// 错误描述
    pub error_description: String,
    /// 建议重试次数
    pub retry_count: u32,
    /// 是否支持降级策略
    pub supports_fallback: bool,
    /// 是否应该自动禁用凭证（永久性错误）
    pub should_disable_credential: bool,
}

/// Token 缓存服务
pub struct TokenCacheService {
    /// 每凭证一把锁，防止并发刷新
    locks: DashMap<String, Arc<Mutex<()>>>,
}

impl Default for TokenCacheService {
    fn default() -> Self {
        Self::new()
    }
}

impl TokenCacheService {
    pub fn new() -> Self {
        Self {
            locks: DashMap::new(),
        }
    }

    /// 获取有效的 Token（核心方法）
    ///
    /// 1. 检查数据库缓存是否有效
    /// 2. 如果缓存有效且未过期，直接返回
    /// 3. 如果缓存无效或即将过期，执行刷新
    /// 4. 如果刷新失败（如 refreshToken 被截断），尝试使用源文件中的 accessToken
    pub async fn get_valid_token(&self, db: &DbConnection, uuid: &str) -> Result<String, String> {
        // 首先检查缓存
        let cached = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            ProviderPoolDao::get_token_cache(&conn, uuid).map_err(|e| e.to_string())?
        };

        // 缓存有效且未即将过期，直接返回
        if let Some(ref cache) = cached {
            if cache.is_valid() && !cache.is_expiring_soon() {
                if let Some(token) = &cache.access_token {
                    tracing::debug!(
                        "[TOKEN_CACHE] Using cached token for {}, expires at {:?}",
                        &uuid[..8],
                        cache.expiry_time
                    );
                    return Ok(token.clone());
                }
            }
        }

        // 需要刷新（无缓存、已过期或即将过期）
        match self.refresh_and_cache(db, uuid, false).await {
            Ok(token) => Ok(token),
            Err(refresh_error) => {
                // 增强的错误处理机制 - 智能检测各种token问题
                let error_classification = self.classify_refresh_error(&refresh_error);

                tracing::warn!(
                    "[TOKEN_CACHE] Token 刷新失败，错误类型: {:?}, 详情: {}",
                    error_classification.error_type,
                    &refresh_error
                );

                match error_classification.error_type {
                    RefreshErrorType::TokenTruncated | RefreshErrorType::TokenFormat => {
                        tracing::warn!(
                            "[TOKEN_CACHE] 检测到 token 问题，尝试使用源文件中的 accessToken: {}",
                            &uuid[..8]
                        );

                        // 获取凭证信息
                        let credential = {
                            let conn = db.lock().map_err(|e| e.to_string())?;
                            ProviderPoolDao::get_by_uuid(&conn, uuid)
                                .map_err(|e| e.to_string())?
                                .ok_or_else(|| format!("Credential not found: {}", uuid))?
                        };

                        // 尝试从源文件读取 accessToken
                        match self.read_token_from_source(&credential).await {
                            Ok(token_info) => {
                                if let Some(token) = token_info.access_token {
                                    tracing::info!(
                                        "[TOKEN_CACHE] 使用源文件中的 accessToken 作为降级方案: {}",
                                        &uuid[..8]
                                    );

                                    // 缓存这个 token 但标记为降级状态
                                    let cache_info = CachedTokenInfo {
                                        access_token: Some(token.clone()),
                                        refresh_token: token_info.refresh_token,
                                        expiry_time: None, // 无法确定过期时间
                                        last_refresh: Some(Utc::now()),
                                        refresh_error_count: error_classification.retry_count,
                                        last_refresh_error: Some(format!(
                                            "{}(降级使用源文件 accessToken): {}",
                                            error_classification.error_description, refresh_error
                                        )),
                                    };

                                    // 缓存到数据库
                                    if let Ok(conn) = db.lock() {
                                        let _ = ProviderPoolDao::update_token_cache(
                                            &conn,
                                            uuid,
                                            &cache_info,
                                        );
                                    }

                                    return Ok(token);
                                }
                            }
                            Err(e) => {
                                tracing::error!(
                                    "[TOKEN_CACHE] 降级策略失败，无法从源文件读取 accessToken: {}",
                                    e
                                );
                            }
                        }
                    }
                    RefreshErrorType::Network | RefreshErrorType::ServiceUnavailable => {
                        tracing::warn!("[TOKEN_CACHE] 网络/服务问题，建议稍后重试: {}", &uuid[..8]);
                        // 可以考虑使用缓存中的过期 token 作为临时方案
                        if let Some(cache) = cached {
                            if let Some(token) = cache.access_token {
                                tracing::info!(
                                    "[TOKEN_CACHE] 网络问题时使用过期缓存 token: {}",
                                    &uuid[..8]
                                );
                                return Ok(token);
                            }
                        }
                    }
                    RefreshErrorType::AuthenticationFailed => {
                        tracing::error!("[TOKEN_CACHE] 认证失败，凭证可能已被撤销: {}", &uuid[..8]);
                        // 认证失败通常需要用户重新授权，不进行降级
                    }
                    RefreshErrorType::Unknown => {
                        tracing::warn!("[TOKEN_CACHE] 未知错误类型，使用默认处理: {}", &uuid[..8]);
                    }
                }

                // 更新错误计数
                if let Ok(conn) = db.lock() {
                    let _ = ProviderPoolDao::record_token_refresh_error(
                        &conn,
                        uuid,
                        &format!(
                            "{}(分类: {:?}): {}",
                            error_classification.error_description,
                            error_classification.error_type,
                            refresh_error
                        ),
                    );
                }

                // 返回分类后的错误信息
                Err(format!(
                    "{}: {}",
                    error_classification.error_description, refresh_error
                ))
            }
        }
    }

    /// 刷新 Token 并缓存到数据库（带事件发送）
    ///
    /// - force: 是否强制刷新（忽略缓存状态）
    /// - kiro_event_service: 可选的事件服务，用于发送 Kiro 凭证刷新事件
    ///
    /// 优化说明：添加了随机延迟机制，避免多个凭证同时刷新造成请求过于集中
    pub async fn refresh_and_cache_with_events(
        &self,
        db: &DbConnection,
        uuid: &str,
        force: bool,
        kiro_event_service: Option<Arc<KiroEventService>>,
    ) -> Result<String, String> {
        // 添加随机延迟，避免多个凭证同时刷新
        // 基于凭证UUID生成0-30秒的随机延迟，确保同一凭证的延迟时间一致但不同凭证间分散
        if !force {
            let delay_ms = self.calculate_refresh_delay(uuid);
            if delay_ms > 0 {
                tracing::debug!(
                    "[TOKEN_CACHE] Adding {}ms delay before refreshing token for {}",
                    delay_ms,
                    &uuid[..8]
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            }
        }

        // 获取该凭证的锁
        let lock = self
            .locks
            .entry(uuid.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();

        let _guard = lock.lock().await;

        // 双重检查：可能其他线程已完成刷新
        if !force {
            let cached = {
                let conn = db.lock().map_err(|e| e.to_string())?;
                ProviderPoolDao::get_token_cache(&conn, uuid).map_err(|e| e.to_string())?
            };

            if let Some(cache) = cached {
                if cache.is_valid() && !cache.is_expiring_soon() {
                    if let Some(token) = cache.access_token {
                        tracing::debug!(
                            "[TOKEN_CACHE] Double-check: another thread refreshed for {}",
                            &uuid[..8]
                        );
                        return Ok(token);
                    }
                }
            }
        }

        // 获取凭证信息
        let credential = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            ProviderPoolDao::get_by_uuid(&conn, uuid)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Credential not found: {}", uuid))?
        };

        tracing::info!(
            "[TOKEN_CACHE] Refreshing token for {} ({})",
            &uuid[..8],
            credential.provider_type
        );

        // 发送刷新开始事件（仅针对 Kiro 凭证）
        if let Some(event_service) = &kiro_event_service {
            if credential.provider_type == PoolProviderType::Kiro {
                event_service
                    .emit_refresh_started(uuid.to_string(), credential.name.clone())
                    .await;
            }
        }

        // 执行刷新
        match self.do_refresh(&credential).await {
            Ok(token_info) => {
                // 缓存到数据库
                {
                    let conn = db.lock().map_err(|e| e.to_string())?;
                    ProviderPoolDao::update_token_cache(&conn, uuid, &token_info)
                        .map_err(|e| e.to_string())?;
                }

                let token = token_info
                    .access_token
                    .ok_or_else(|| "Refresh succeeded but no access_token".to_string())?;

                tracing::info!(
                    "[TOKEN_CACHE] Token refreshed and cached for {}, expires at {:?}",
                    &uuid[..8],
                    token_info.expiry_time
                );

                // 发送刷新成功事件（仅针对 Kiro 凭证）
                if let Some(event_service) = &kiro_event_service {
                    if credential.provider_type == PoolProviderType::Kiro {
                        event_service
                            .emit_refresh_success(
                                uuid.to_string(),
                                credential.name.clone(),
                                token_info
                                    .expiry_time
                                    .unwrap_or_else(|| Utc::now() + chrono::Duration::hours(1)),
                                "IdC".to_string(), // 默认为IdC认证
                                "BuilderId".to_string(),
                                "us-east-1".to_string(),
                            )
                            .await;
                    }
                }

                Ok(token)
            }
            Err(e) => {
                // 记录刷新错误
                {
                    let conn = db.lock().map_err(|e| e.to_string())?;
                    let _ = ProviderPoolDao::record_token_refresh_error(&conn, uuid, &e);
                }

                tracing::error!(
                    "[TOKEN_CACHE] Token refresh failed for {}: {}",
                    &uuid[..8],
                    e
                );

                // 分析错误并决定是否自动禁用凭证
                let error_classification = self.classify_refresh_error(&e);

                // 如果是永久性错误，自动禁用凭证
                if error_classification.should_disable_credential {
                    let disable_result = {
                        let conn = db.lock().map_err(|e| e.to_string())?;
                        // 简化禁用逻辑：直接在数据库中标记为禁用
                        let sql = "UPDATE credentials SET is_disabled = true WHERE uuid = ?";
                        conn.execute(sql, &[&uuid]).map_err(|e| e.to_string())
                    };

                    match disable_result {
                        Ok(_) => {
                            tracing::warn!(
                                "[TOKEN_CACHE] Auto-disabled credential {} due to permanent failure: {:?}",
                                &uuid[..8],
                                error_classification.error_type
                            );

                            // 发送凭证禁用事件
                            if let Some(event_service) = &kiro_event_service {
                                if credential.provider_type == PoolProviderType::Kiro {
                                    // 发送状态更新事件
                                    event_service
                                        .emit_credential_status_update(
                                            uuid.to_string(),
                                            false, // is_healthy
                                            true,  // is_disabled
                                            credential.error_count + 1,
                                            Some(0.0), // health_score降为0
                                            None,
                                        )
                                        .await;

                                    // 发送自动禁用事件
                                    event_service
                                        .emit_credential_auto_disabled(
                                            uuid.to_string(),
                                            credential.name.clone(),
                                            error_classification.error_description.clone(),
                                            format!("{:?}", error_classification.error_type),
                                        )
                                        .await;
                                }
                            }
                        }
                        Err(disable_err) => {
                            tracing::error!(
                                "[TOKEN_CACHE] Failed to auto-disable credential {}: {}",
                                &uuid[..8],
                                disable_err
                            );
                        }
                    }
                }

                // 发送刷新失败事件（仅针对 Kiro 凭证）
                if let Some(event_service) = &kiro_event_service {
                    if credential.provider_type == PoolProviderType::Kiro {
                        event_service
                            .emit_refresh_failed(
                                uuid.to_string(),
                                credential.name.clone(),
                                e.clone(),
                                Some(format!("{:?}", error_classification.error_type)),
                            )
                            .await;
                    }
                }

                Err(e)
            }
        }
    }

    /// 执行实际的 Token 刷新
    async fn do_refresh(&self, credential: &ProviderCredential) -> Result<CachedTokenInfo, String> {
        match &credential.credential {
            CredentialData::KiroOAuth { creds_file_path } => {
                self.refresh_kiro(creds_file_path).await
            }
            CredentialData::GeminiOAuth {
                creds_file_path, ..
            } => self.refresh_gemini(creds_file_path).await,
            CredentialData::QwenOAuth { creds_file_path } => {
                self.refresh_qwen(creds_file_path).await
            }
            CredentialData::AntigravityOAuth {
                creds_file_path, ..
            } => self.refresh_antigravity(creds_file_path).await,
            CredentialData::OpenAIKey { api_key, .. } => {
                // API Key 不需要刷新，直接返回
                Ok(CachedTokenInfo {
                    access_token: Some(api_key.clone()),
                    refresh_token: None,
                    expiry_time: None, // 永不过期
                    last_refresh: Some(Utc::now()),
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::ClaudeKey { api_key, .. } => {
                // API Key 不需要刷新，直接返回
                Ok(CachedTokenInfo {
                    access_token: Some(api_key.clone()),
                    refresh_token: None,
                    expiry_time: None, // 永不过期
                    last_refresh: Some(Utc::now()),
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::VertexKey { api_key, .. } => {
                // API Key 不需要刷新，直接返回
                Ok(CachedTokenInfo {
                    access_token: Some(api_key.clone()),
                    refresh_token: None,
                    expiry_time: None, // 永不过期
                    last_refresh: Some(Utc::now()),
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::GeminiApiKey { api_key, .. } => {
                // API Key 不需要刷新，直接返回
                Ok(CachedTokenInfo {
                    access_token: Some(api_key.clone()),
                    refresh_token: None,
                    expiry_time: None, // 永不过期
                    last_refresh: Some(Utc::now()),
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::CodexOAuth {
                creds_file_path, ..
            } => self.refresh_codex(creds_file_path).await,
            CredentialData::ClaudeOAuth { creds_file_path } => {
                self.refresh_claude_oauth(creds_file_path).await
            }
            CredentialData::IFlowOAuth { creds_file_path } => {
                self.refresh_iflow_oauth(creds_file_path).await
            }
            CredentialData::IFlowCookie { creds_file_path } => {
                self.refresh_iflow_cookie(creds_file_path).await
            }
        }
    }

    /// 刷新 Kiro Token
    async fn refresh_kiro(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        let mut provider = KiroProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 Kiro 凭证失败: {}", e))?;

        let token = provider
            .refresh_token()
            .await
            .map_err(|e| format!("刷新 Kiro Token 失败: {}", e))?;

        // Kiro token 通常 1 小时过期，我们假设 50 分钟
        let expiry_time = Utc::now() + chrono::Duration::minutes(50);

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 Gemini Token
    async fn refresh_gemini(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        let mut provider = GeminiProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 Gemini 凭证失败: {}", e))?;

        let token = provider
            .refresh_token()
            .await
            .map_err(|e| format!("刷新 Gemini Token 失败: {}", e))?;

        // Gemini token 通常 1 小时过期
        let expiry_time = provider
            .credentials
            .expiry_date
            .map(|ts| chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default())
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(50));

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 Qwen Token
    async fn refresh_qwen(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        let mut provider = QwenProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 Qwen 凭证失败: {}", e))?;

        let token = provider
            .refresh_token()
            .await
            .map_err(|e| format!("刷新 Qwen Token 失败: {}", e))?;

        // Qwen token 通常 1 小时过期
        let expiry_time = provider
            .credentials
            .expiry_date
            .map(|ts| chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default())
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(50));

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 Antigravity Token
    async fn refresh_antigravity(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        use crate::providers::antigravity::AntigravityProvider;

        let mut provider = AntigravityProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 Antigravity 凭证失败: {}", e))?;

        let token = provider
            .refresh_token()
            .await
            .map_err(|e| format!("刷新 Antigravity Token 失败: {}", e))?;

        // Antigravity token 通常 1 小时过期
        let expiry_time = provider
            .credentials
            .expiry_date
            .map(|ts| chrono::DateTime::from_timestamp_millis(ts).unwrap_or_default())
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(50));

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 Codex Token
    async fn refresh_codex(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        use crate::providers::codex::CodexProvider;

        let mut provider = CodexProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 Codex 凭证失败: {}", e))?;

        let token = provider
            .refresh_token_with_retry(3)
            .await
            .map_err(|e| format!("刷新 Codex Token 失败: {}", e))?;

        // 解析过期时间
        let expiry_time = provider
            .credentials
            .expires_at
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(50));

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 Claude OAuth Token
    async fn refresh_claude_oauth(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        use crate::providers::claude_oauth::ClaudeOAuthProvider;

        let mut provider = ClaudeOAuthProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 Claude OAuth 凭证失败: {}", e))?;

        let token = provider
            .refresh_token_with_retry(3)
            .await
            .map_err(|e| format!("刷新 Claude OAuth Token 失败: {}", e))?;

        // 解析过期时间
        let expiry_time = provider
            .credentials
            .expire
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(50));

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 iFlow OAuth Token
    async fn refresh_iflow_oauth(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        use crate::providers::iflow::IFlowProvider;

        let mut provider = IFlowProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 iFlow OAuth 凭证失败: {}", e))?;

        let token = provider
            .refresh_token_with_retry(3)
            .await
            .map_err(|e| format!("刷新 iFlow OAuth Token 失败: {}", e))?;

        // 解析过期时间
        let expiry_time = provider
            .credentials
            .expire
            .as_ref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|| Utc::now() + chrono::Duration::minutes(50));

        Ok(CachedTokenInfo {
            access_token: Some(token),
            refresh_token: provider.credentials.refresh_token.clone(),
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 刷新 iFlow Cookie Token
    /// 与 CLIProxyAPI 的 refreshCookieBased 对齐
    async fn refresh_iflow_cookie(&self, creds_path: &str) -> Result<CachedTokenInfo, String> {
        use crate::providers::iflow::IFlowProvider;

        let mut provider = IFlowProvider::new();
        provider
            .load_credentials_from_path(creds_path)
            .await
            .map_err(|e| format!("加载 iFlow Cookie 凭证失败: {}", e))?;

        // 检查是否需要刷新 API Key（距离过期 2 天内）
        if provider.should_refresh_api_key() {
            tracing::info!("[IFLOW] Cookie API Key 需要刷新");

            // 通过 Cookie 刷新 API Key
            let api_key = provider
                .refresh_api_key_with_cookie()
                .await
                .map_err(|e| format!("刷新 iFlow Cookie API Key 失败: {}", e))?;

            // 解析新的过期时间
            let expiry_time = provider
                .credentials
                .expire
                .as_ref()
                .and_then(|s| {
                    // 尝试解析 "2006-01-02 15:04" 格式
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
                        .ok()
                        .map(|dt| dt.and_utc())
                        .or_else(|| {
                            // 尝试解析 RFC3339 格式
                            chrono::DateTime::parse_from_rfc3339(s)
                                .ok()
                                .map(|dt| dt.with_timezone(&Utc))
                        })
                })
                .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

            return Ok(CachedTokenInfo {
                access_token: Some(api_key),
                refresh_token: None,
                expiry_time: Some(expiry_time),
                last_refresh: Some(Utc::now()),
                refresh_error_count: 0,
                last_refresh_error: None,
            });
        }

        // 不需要刷新，直接返回现有的 API Key
        let api_key = provider
            .credentials
            .api_key
            .clone()
            .ok_or_else(|| "iFlow Cookie 凭证中没有 API Key".to_string())?;

        // 解析过期时间
        let expiry_time = provider
            .credentials
            .expire
            .as_ref()
            .and_then(|s| {
                // 尝试解析 "2006-01-02 15:04" 格式
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M")
                    .ok()
                    .map(|dt| dt.and_utc())
                    .or_else(|| {
                        // 尝试解析 RFC3339 格式
                        chrono::DateTime::parse_from_rfc3339(s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    })
            })
            .unwrap_or_else(|| Utc::now() + chrono::Duration::days(30));

        Ok(CachedTokenInfo {
            access_token: Some(api_key),
            refresh_token: None,
            expiry_time: Some(expiry_time),
            last_refresh: Some(Utc::now()),
            refresh_error_count: 0,
            last_refresh_error: None,
        })
    }

    /// 从源文件加载初始 Token（首次使用时）
    pub async fn load_initial_token(
        &self,
        db: &DbConnection,
        uuid: &str,
    ) -> Result<String, String> {
        let credential = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            ProviderPoolDao::get_by_uuid(&conn, uuid)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Credential not found: {}", uuid))?
        };

        // 尝试从源文件读取 token
        let token_info = self.read_token_from_source(&credential).await?;

        // 缓存到数据库
        {
            let conn = db.lock().map_err(|e| e.to_string())?;
            ProviderPoolDao::update_token_cache(&conn, uuid, &token_info)
                .map_err(|e| e.to_string())?;
        }

        token_info
            .access_token
            .ok_or_else(|| "源文件中没有 access_token".to_string())
    }

    /// 从源文件读取 Token（不刷新）
    async fn read_token_from_source(
        &self,
        credential: &ProviderCredential,
    ) -> Result<CachedTokenInfo, String> {
        match &credential.credential {
            CredentialData::KiroOAuth { creds_file_path } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 Kiro 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["accessToken"]
                    .as_str()
                    .or_else(|| creds["access_token"].as_str())
                    .map(|s| s.to_string());
                let refresh_token = creds["refreshToken"]
                    .as_str()
                    .or_else(|| creds["refresh_token"].as_str())
                    .map(|s| s.to_string());

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time: None, // Kiro 源文件通常没有过期时间
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::GeminiOAuth {
                creds_file_path, ..
            } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 Gemini 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["access_token"].as_str().map(|s| s.to_string());
                let refresh_token = creds["refresh_token"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expiry_date"]
                    .as_i64()
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::QwenOAuth { creds_file_path } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 Qwen 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["access_token"].as_str().map(|s| s.to_string());
                let refresh_token = creds["refresh_token"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expiry_date"]
                    .as_i64()
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::AntigravityOAuth {
                creds_file_path, ..
            } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 Antigravity 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["access_token"].as_str().map(|s| s.to_string());
                let refresh_token = creds["refresh_token"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expiry_date"]
                    .as_i64()
                    .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::OpenAIKey { api_key, .. } => Ok(CachedTokenInfo {
                access_token: Some(api_key.clone()),
                refresh_token: None,
                expiry_time: None,
                last_refresh: None,
                refresh_error_count: 0,
                last_refresh_error: None,
            }),
            CredentialData::ClaudeKey { api_key, .. } => Ok(CachedTokenInfo {
                access_token: Some(api_key.clone()),
                refresh_token: None,
                expiry_time: None,
                last_refresh: None,
                refresh_error_count: 0,
                last_refresh_error: None,
            }),
            CredentialData::VertexKey { api_key, .. } => Ok(CachedTokenInfo {
                access_token: Some(api_key.clone()),
                refresh_token: None,
                expiry_time: None,
                last_refresh: None,
                refresh_error_count: 0,
                last_refresh_error: None,
            }),
            CredentialData::GeminiApiKey { api_key, .. } => Ok(CachedTokenInfo {
                access_token: Some(api_key.clone()),
                refresh_token: None,
                expiry_time: None,
                last_refresh: None,
                refresh_error_count: 0,
                last_refresh_error: None,
            }),
            CredentialData::CodexOAuth {
                creds_file_path, ..
            } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 Codex 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["access_token"].as_str().map(|s| s.to_string());
                let refresh_token = creds["refresh_token"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expired"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::ClaudeOAuth { creds_file_path } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 Claude OAuth 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["access_token"].as_str().map(|s| s.to_string());
                let refresh_token = creds["refresh_token"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expire"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::IFlowOAuth { creds_file_path } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 iFlow OAuth 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let access_token = creds["access_token"].as_str().map(|s| s.to_string());
                let refresh_token = creds["refresh_token"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expire"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                Ok(CachedTokenInfo {
                    access_token,
                    refresh_token,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
            CredentialData::IFlowCookie { creds_file_path } => {
                let content = tokio::fs::read_to_string(creds_file_path)
                    .await
                    .map_err(|e| format!("读取 iFlow Cookie 凭证文件失败: {}", e))?;
                let creds: serde_json::Value =
                    serde_json::from_str(&content).map_err(|e| format!("解析凭证失败: {}", e))?;

                let api_key = creds["api_key"].as_str().map(|s| s.to_string());
                let expiry_time = creds["expire"]
                    .as_str()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                Ok(CachedTokenInfo {
                    access_token: api_key,
                    refresh_token: None,
                    expiry_time,
                    last_refresh: None,
                    refresh_error_count: 0,
                    last_refresh_error: None,
                })
            }
        }
    }

    /// 清除凭证的 Token 缓存
    pub fn clear_cache(&self, db: &DbConnection, uuid: &str) -> Result<(), String> {
        let conn = db.lock().map_err(|e| e.to_string())?;
        ProviderPoolDao::clear_token_cache(&conn, uuid).map_err(|e| e.to_string())
    }

    /// 检查凭证类型是否支持 Token 刷新
    pub fn supports_refresh(provider_type: PoolProviderType) -> bool {
        matches!(
            provider_type,
            PoolProviderType::Kiro | PoolProviderType::Gemini | PoolProviderType::Qwen
        )
    }

    /// 获取凭证的缓存状态
    pub fn get_cache_status(
        &self,
        db: &DbConnection,
        uuid: &str,
    ) -> Result<Option<CachedTokenInfo>, String> {
        let conn = db.lock().map_err(|e| e.to_string())?;
        ProviderPoolDao::get_token_cache(&conn, uuid).map_err(|e| e.to_string())
    }

    /// 计算刷新延迟时间（毫秒）
    ///
    /// 基于凭证UUID生成确定性但分散的延迟时间，避免多个凭证同时刷新
    /// 延迟范围：0-30秒，确保同一凭证每次的延迟一致
    fn calculate_refresh_delay(&self, uuid: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // 使用凭证UUID作为种子生成确定性的延迟
        let mut hasher = DefaultHasher::new();
        uuid.hash(&mut hasher);
        let hash_value = hasher.finish();

        // 生成0-30秒的延迟（转换为毫秒）
        (hash_value % 30000) as u64
    }

    /// 智能错误分类方法
    ///
    /// 基于错误信息智能识别错误类型，提供针对性的处理建议
    fn classify_refresh_error(&self, error_message: &str) -> RefreshErrorClassification {
        let error_lower = error_message.to_lowercase();

        // Token 被截断问题检测（最严重的问题，优先检查）
        if error_lower.contains("截断") || error_lower.contains("truncated") {
            return RefreshErrorClassification {
                error_type: RefreshErrorType::TokenTruncated,
                error_description: "Token 被截断，需检查配置文件".to_string(),
                retry_count: 1,
                supports_fallback: true,
                should_disable_credential: true, // 永久性问题，自动禁用
            };
        }

        // Token 格式问题检测
        if error_lower.contains("格式异常")
            || error_lower.contains("长度过短")
            || error_lower.contains("format")
            || error_lower.contains("invalid")
            || error_lower.contains("malformed")
        {
            return RefreshErrorClassification {
                error_type: RefreshErrorType::TokenFormat,
                error_description: "Token 格式异常，需重新配置".to_string(),
                retry_count: 1,
                supports_fallback: true,
                should_disable_credential: true, // 配置问题，自动禁用
            };
        }

        // 认证失败检测
        if error_lower.contains("unauthorized")
            || error_lower.contains("forbidden")
            || error_lower.contains("401")
            || error_lower.contains("403")
            || error_lower.contains("认证失败")
            || error_lower.contains("invalid_grant")
            || error_lower.contains("access_denied")
            || error_lower.contains("refresh_token")
            || error_lower.contains("expired")
        {
            return RefreshErrorClassification {
                error_type: RefreshErrorType::AuthenticationFailed,
                error_description: "认证失败，凭证已过期或无效".to_string(),
                retry_count: 0, // 不建议重试
                supports_fallback: false,
                should_disable_credential: true, // 认证失效，自动禁用
            };
        }

        // 网络问题检测
        if error_lower.contains("network")
            || error_lower.contains("connection")
            || error_lower.contains("timeout")
            || error_lower.contains("dns")
            || error_lower.contains("connect")
            || error_lower.contains("网络")
            || error_lower.contains("连接")
        {
            return RefreshErrorClassification {
                error_type: RefreshErrorType::Network,
                error_description: "网络连接问题".to_string(),
                retry_count: 3,
                supports_fallback: true,
                should_disable_credential: false, // 临时问题，不禁用
            };
        }

        // 服务不可用检测
        if error_lower.contains("service unavailable")
            || error_lower.contains("502")
            || error_lower.contains("503")
            || error_lower.contains("504")
            || error_lower.contains("internal server error")
            || error_lower.contains("服务不可用")
        {
            return RefreshErrorClassification {
                error_type: RefreshErrorType::ServiceUnavailable,
                error_description: "服务暂时不可用".to_string(),
                retry_count: 2,
                supports_fallback: true,
                should_disable_credential: false, // 临时问题，不禁用
            };
        }

        // 未知错误（默认分类）
        RefreshErrorClassification {
            error_type: RefreshErrorType::Unknown,
            error_description: "未知错误".to_string(),
            retry_count: 1,
            supports_fallback: false,
            should_disable_credential: false, // 未知错误暂不自动禁用
        }
    }

    /// 刷新 Token 并缓存到数据库（兼容版本）
    ///
    /// - force: 是否强制刷新（忽略缓存状态）
    ///
    /// 此方法保持与旧版本的兼容性，不发送任何事件。
    /// 如需事件支持，请使用 refresh_and_cache_with_events 方法。
    pub async fn refresh_and_cache(
        &self,
        db: &DbConnection,
        uuid: &str,
        force: bool,
    ) -> Result<String, String> {
        self.refresh_and_cache_with_events(db, uuid, force, None)
            .await
    }

    /// 检查 Token 是否即将过期并提前刷新（需求 4.4）
    ///
    /// 在流式请求前调用此方法，检查 Token 是否在指定分钟数内过期。
    /// 如果即将过期，则提前刷新 Token。
    ///
    /// # 参数
    /// - `db`: 数据库连接
    /// - `uuid`: 凭证 UUID
    /// - `minutes`: 检查的时间阈值（分钟），默认 10 分钟
    ///
    /// # 返回
    /// - `Ok(token)`: 有效的 Token（可能是刷新后的新 Token）
    /// - `Err(error)`: 获取或刷新 Token 失败
    pub async fn ensure_token_valid_for_streaming(
        &self,
        db: &DbConnection,
        uuid: &str,
        minutes: i64,
    ) -> Result<String, String> {
        // 首先检查缓存
        let cached = {
            let conn = db.lock().map_err(|e| e.to_string())?;
            crate::database::dao::provider_pool::ProviderPoolDao::get_token_cache(&conn, uuid)
                .map_err(|e| e.to_string())?
        };

        // 检查是否需要提前刷新（使用指定的分钟数阈值）
        if let Some(ref cache) = cached {
            if cache.is_valid() && !cache.is_expiring_within_minutes(minutes) {
                if let Some(token) = &cache.access_token {
                    tracing::debug!(
                        "[TOKEN_CACHE] Token valid for streaming ({}min threshold) for {}, expires at {:?}",
                        minutes,
                        &uuid[..8],
                        cache.expiry_time
                    );
                    return Ok(token.clone());
                }
            }

            // Token 即将过期（在指定分钟数内），提前刷新
            if cache.is_expiring_within_minutes(minutes) {
                tracing::info!(
                    "[TOKEN_CACHE] Token expiring within {}min for {}, proactively refreshing",
                    minutes,
                    &uuid[..8]
                );
            }
        }

        // 需要刷新（无缓存、已过期或即将过期）
        self.refresh_and_cache(db, uuid, false).await
    }
}
