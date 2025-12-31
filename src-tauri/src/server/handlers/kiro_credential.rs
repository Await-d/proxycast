//! Kiro凭证管理API处理器
//!
//! 为kiro凭证池管理提供REST API端点，支持：
//! - 获取可用凭证列表
//! - 智能选择凭证
//! - 手动刷新凭证
//! - 凭证状态查询

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::database::dao::provider_pool::ProviderPoolDao;
use crate::models::provider_pool_model::{CachedTokenInfo, PoolProviderType, ProviderCredential};
use crate::server::AppState;

/// 可用凭证信息
#[derive(Debug, Clone, Serialize)]
pub struct AvailableCredential {
    /// 凭证UUID
    pub uuid: String,
    /// 凭证名称
    pub name: String,
    /// 是否可用
    pub available: bool,
    /// Token过期时间
    pub expires_at: Option<DateTime<Utc>>,
    /// 最后使用时间
    pub last_used: Option<DateTime<Utc>>,
    /// 健康状态分数 (0-100)
    pub health_score: f64,
    /// 错误计数
    pub error_count: u32,
    /// 最后错误信息
    pub last_error: Option<String>,
}

/// 获取可用凭证列表的响应
#[derive(Debug, Serialize)]
pub struct AvailableCredentialsResponse {
    /// 可用凭证列表
    pub credentials: Vec<AvailableCredential>,
    /// 总凭证数
    pub total: usize,
    /// 可用凭证数
    pub available: usize,
    /// 系统状态
    pub status: String,
}

/// 选择凭证请求参数
#[derive(Debug, Deserialize)]
pub struct SelectCredentialRequest {
    /// 指定模型（可选）
    pub model: Option<String>,
    /// 强制选择特定UUID（可选）
    pub force_uuid: Option<String>,
}

/// 选择凭证响应
#[derive(Debug, Serialize)]
pub struct SelectCredentialResponse {
    /// 选中的凭证UUID
    pub uuid: String,
    /// 凭证名称
    pub name: String,
    /// Access Token（脱敏显示）
    pub access_token_preview: String,
    /// Token过期时间
    pub expires_at: Option<DateTime<Utc>>,
    /// 选择原因
    pub selection_reason: String,
}

/// 刷新凭证响应
#[derive(Debug, Serialize)]
pub struct RefreshCredentialResponse {
    /// 凭证UUID
    pub uuid: String,
    /// 刷新是否成功
    pub success: bool,
    /// 新的过期时间
    pub new_expires_at: Option<DateTime<Utc>>,
    /// 刷新结果信息
    pub message: String,
    /// 错误信息（如果有）
    pub error: Option<String>,
}

/// API错误响应
#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
    pub message: String,
    pub status_code: u16,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self)).into_response()
    }
}

/// GET /api/kiro/credentials/available - 获取可用凭证列表
pub async fn get_available_credentials(
    State(state): State<AppState>,
    _headers: HeaderMap,
) -> Result<Json<AvailableCredentialsResponse>, ApiError> {
    tracing::info!("[KIRO_API] 获取可用凭证列表请求");

    let db = &state.db.as_ref().ok_or_else(|| ApiError {
        error: "database_unavailable".to_string(),
        message: "数据库连接不可用".to_string(),
        status_code: 503,
    })?;

    let _pool_service = &state.pool_service;
    let token_cache = &state.token_cache;

    // 获取所有kiro凭证
    let credentials = {
        let conn = db.lock().map_err(|e| ApiError {
            error: "database_lock_error".to_string(),
            message: format!("数据库锁定失败: {}", e),
            status_code: 500,
        })?;

        ProviderPoolDao::get_all(&conn)
            .map_err(|e| ApiError {
                error: "database_query_error".to_string(),
                message: format!("查询凭证失败: {}", e),
                status_code: 500,
            })?
            .into_iter()
            .filter(|cred| cred.provider_type == PoolProviderType::Kiro)
            .collect::<Vec<_>>()
    };

    let mut available_credentials = Vec::new();
    let mut available_count = 0;

    for credential in &credentials {
        // 获取凭证缓存状态
        let cache_status = token_cache
            .get_cache_status(db, &credential.uuid)
            .map_err(|e| ApiError {
                error: "cache_query_error".to_string(),
                message: format!("获取缓存状态失败: {}", e),
                status_code: 500,
            })?;

        // 计算健康状态分数
        let health_score = calculate_health_score(&credential, cache_status.as_ref());

        let is_available = health_score > 50.0; // 健康分数大于50认为可用
        if is_available {
            available_count += 1;
        }

        let available_cred = AvailableCredential {
            uuid: credential.uuid.clone(),
            name: credential
                .name
                .clone()
                .unwrap_or_else(|| "未命名".to_string()),
            available: is_available,
            expires_at: cache_status.as_ref().and_then(|c| c.expiry_time),
            last_used: credential.last_used,
            health_score,
            error_count: cache_status
                .as_ref()
                .map(|c| c.refresh_error_count)
                .unwrap_or(0),
            last_error: cache_status.and_then(|c| c.last_refresh_error),
        };

        available_credentials.push(available_cred);
    }

    // 按健康分数降序排列
    available_credentials.sort_by(|a, b| {
        b.health_score
            .partial_cmp(&a.health_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let response = AvailableCredentialsResponse {
        credentials: available_credentials,
        total: credentials.len(),
        available: available_count,
        status: if available_count > 0 {
            "healthy".to_string()
        } else {
            "degraded".to_string()
        },
    };

    tracing::info!(
        "[KIRO_API] 返回{}个凭证，其中{}个可用",
        response.total,
        response.available
    );
    Ok(Json(response))
}

/// POST /api/kiro/credentials/select - 智能选择凭证
pub async fn select_credential(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Json(request): Json<SelectCredentialRequest>,
) -> Result<Json<SelectCredentialResponse>, ApiError> {
    tracing::info!(
        "[KIRO_API] 选择凭证请求，模型: {:?}, 强制UUID: {:?}",
        request.model,
        request.force_uuid
    );

    let db = &state.db.as_ref().ok_or_else(|| ApiError {
        error: "database_unavailable".to_string(),
        message: "数据库连接不可用".to_string(),
        status_code: 503,
    })?;

    let selected_credential = if let Some(ref force_uuid) = request.force_uuid {
        // 强制选择指定UUID
        let conn = db.lock().map_err(|e| ApiError {
            error: "database_lock_error".to_string(),
            message: format!("数据库锁定失败: {}", e),
            status_code: 500,
        })?;

        ProviderPoolDao::get_by_uuid(&conn, &force_uuid)
            .map_err(|e| ApiError {
                error: "database_query_error".to_string(),
                message: format!("查询凭证失败: {}", e),
                status_code: 500,
            })?
            .ok_or_else(|| ApiError {
                error: "credential_not_found".to_string(),
                message: format!("未找到UUID为{}的凭证", force_uuid),
                status_code: 404,
            })?
    } else {
        // 智能选择最优凭证
        state
            .pool_service
            .select_credential(db, "kiro", request.model.as_deref())
            .map_err(|e| ApiError {
                error: "selection_error".to_string(),
                message: format!("凭证选择失败: {}", e),
                status_code: 500,
            })?
            .ok_or_else(|| ApiError {
                error: "no_available_credentials".to_string(),
                message: "没有可用的kiro凭证".to_string(),
                status_code: 503,
            })?
    };

    // 脱敏显示token
    let token_preview = if let Some(cached_token_info) = &selected_credential.cached_token {
        if let Some(token) = &cached_token_info.access_token {
            if token.len() > 20 {
                format!("{}...{}", &token[..10], &token[token.len() - 10..])
            } else {
                "***".to_string()
            }
        } else {
            "无token".to_string()
        }
    } else {
        "未缓存".to_string()
    };

    let selection_reason = if request.force_uuid.is_some() {
        "手动指定".to_string()
    } else {
        "智能算法选择".to_string()
    };

    let expires_at = selected_credential
        .cached_token
        .as_ref()
        .and_then(|cache| cache.expiry_time);

    let response = SelectCredentialResponse {
        uuid: selected_credential.uuid.clone(),
        name: selected_credential
            .name
            .clone()
            .unwrap_or_else(|| "未命名".to_string()),
        access_token_preview: token_preview,
        expires_at,
        selection_reason,
    };

    tracing::info!(
        "[KIRO_API] 选择凭证成功: {} ({})",
        response.name,
        response.uuid
    );
    Ok(Json(response))
}

/// PUT /api/kiro/credentials/{uuid}/refresh - 手动刷新指定凭证
pub async fn refresh_credential(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    _headers: HeaderMap,
) -> Result<Json<RefreshCredentialResponse>, ApiError> {
    tracing::info!("[KIRO_API] 刷新凭证请求: {}", uuid);

    let db = &state.db.as_ref().ok_or_else(|| ApiError {
        error: "database_unavailable".to_string(),
        message: "数据库连接不可用".to_string(),
        status_code: 503,
    })?;

    let token_cache = &state.token_cache;

    // 验证凭证存在且为kiro类型
    let credential = {
        let conn = db.lock().map_err(|e| ApiError {
            error: "database_lock_error".to_string(),
            message: format!("数据库锁定失败: {}", e),
            status_code: 500,
        })?;

        let cred = ProviderPoolDao::get_by_uuid(&conn, &uuid)
            .map_err(|e| ApiError {
                error: "database_query_error".to_string(),
                message: format!("查询凭证失败: {}", e),
                status_code: 500,
            })?
            .ok_or_else(|| ApiError {
                error: "credential_not_found".to_string(),
                message: format!("未找到UUID为{}的凭证", uuid),
                status_code: 404,
            })?;

        if cred.provider_type.to_string() != PoolProviderType::Kiro.to_string() {
            return Err(ApiError {
                error: "invalid_credential_type".to_string(),
                message: format!("凭证类型不是kiro: {}", cred.provider_type),
                status_code: 400,
            });
        }

        cred
    };

    // 执行强制刷新
    match token_cache
        .refresh_and_cache_with_events(db, &uuid, true, Some(state.kiro_event_service.clone()))
        .await
    {
        Ok(_new_token) => {
            // 获取刷新后的缓存状态
            let cache_status = token_cache
                .get_cache_status(db, &uuid)
                .map_err(|e| ApiError {
                    error: "cache_query_error".to_string(),
                    message: format!("获取刷新后缓存状态失败: {}", e),
                    status_code: 500,
                })?;

            let response = RefreshCredentialResponse {
                uuid: uuid.clone(),
                success: true,
                new_expires_at: cache_status.as_ref().and_then(|c| c.expiry_time),
                message: format!(
                    "凭证 {} 刷新成功",
                    credential
                        .name
                        .clone()
                        .unwrap_or_else(|| "未命名".to_string())
                ),
                error: None,
            };

            tracing::info!(
                "[KIRO_API] 凭证刷新成功: {} ({})",
                credential
                    .name
                    .clone()
                    .unwrap_or_else(|| "未命名".to_string()),
                uuid
            );
            Ok(Json(response))
        }
        Err(refresh_error) => {
            let response = RefreshCredentialResponse {
                uuid: uuid.clone(),
                success: false,
                new_expires_at: None,
                message: format!(
                    "凭证 {} 刷新失败",
                    credential
                        .name
                        .clone()
                        .unwrap_or_else(|| "未命名".to_string())
                ),
                error: Some(refresh_error.clone()),
            };

            tracing::warn!(
                "[KIRO_API] 凭证刷新失败: {} ({}): {}",
                credential
                    .name
                    .clone()
                    .unwrap_or_else(|| "未命名".to_string()),
                uuid,
                refresh_error
            );
            Ok(Json(response))
        }
    }
}

/// GET /api/kiro/credentials/{uuid}/status - 获取凭证详细状态
pub async fn get_credential_status(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    _headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    tracing::info!("[KIRO_API] 获取凭证状态: {}", uuid);

    let db = &state.db.as_ref().ok_or_else(|| ApiError {
        error: "database_unavailable".to_string(),
        message: "数据库连接不可用".to_string(),
        status_code: 503,
    })?;

    let token_cache = &state.token_cache;

    // 验证凭证存在
    let credential = {
        let conn = db.lock().map_err(|e| ApiError {
            error: "database_lock_error".to_string(),
            message: format!("数据库锁定失败: {}", e),
            status_code: 500,
        })?;

        ProviderPoolDao::get_by_uuid(&conn, &uuid)
            .map_err(|e| ApiError {
                error: "database_query_error".to_string(),
                message: format!("查询凭证失败: {}", e),
                status_code: 500,
            })?
            .ok_or_else(|| ApiError {
                error: "credential_not_found".to_string(),
                message: format!("未找到UUID为{}的凭证", uuid),
                status_code: 404,
            })?
    };

    // 获取缓存状态
    let cache_status = token_cache
        .get_cache_status(db, &uuid)
        .map_err(|e| ApiError {
            error: "cache_query_error".to_string(),
            message: format!("获取缓存状态失败: {}", e),
            status_code: 500,
        })?;

    // 计算健康分数
    let health_score = calculate_health_score(&credential, cache_status.as_ref());

    let mut status = serde_json::Map::new();
    status.insert(
        "uuid".to_string(),
        serde_json::Value::String(credential.uuid.clone()),
    );
    status.insert(
        "name".to_string(),
        serde_json::Value::String(
            credential
                .name
                .clone()
                .unwrap_or_else(|| "未命名".to_string()),
        ),
    );
    status.insert(
        "provider_type".to_string(),
        serde_json::Value::String(credential.provider_type.to_string()),
    );
    status.insert(
        "created_at".to_string(),
        serde_json::Value::String(credential.created_at.to_rfc3339()),
    );
    status.insert(
        "last_used".to_string(),
        credential
            .last_used
            .map(|dt| serde_json::Value::String(dt.to_rfc3339()))
            .unwrap_or(serde_json::Value::Null),
    );
    status.insert(
        "health_score".to_string(),
        serde_json::Value::Number(
            serde_json::Number::from_f64(health_score).unwrap_or(serde_json::Number::from(0)),
        ),
    );
    status.insert(
        "is_available".to_string(),
        serde_json::Value::Bool(health_score > 50.0),
    );

    if let Some(cache) = cache_status {
        status.insert(
            "has_cached_token".to_string(),
            serde_json::Value::Bool(cache.access_token.is_some()),
        );
        status.insert(
            "expires_at".to_string(),
            cache
                .expiry_time
                .map(|dt| serde_json::Value::String(dt.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null),
        );
        status.insert(
            "last_refresh".to_string(),
            cache
                .last_refresh
                .map(|dt| serde_json::Value::String(dt.to_rfc3339()))
                .unwrap_or(serde_json::Value::Null),
        );
        status.insert(
            "refresh_error_count".to_string(),
            serde_json::Value::Number(serde_json::Number::from(cache.refresh_error_count)),
        );
        status.insert(
            "last_refresh_error".to_string(),
            cache
                .last_refresh_error
                .map(|err| serde_json::Value::String(err))
                .unwrap_or(serde_json::Value::Null),
        );
    } else {
        status.insert(
            "has_cached_token".to_string(),
            serde_json::Value::Bool(false),
        );
        status.insert("expires_at".to_string(), serde_json::Value::Null);
        status.insert("last_refresh".to_string(), serde_json::Value::Null);
        status.insert(
            "refresh_error_count".to_string(),
            serde_json::Value::Number(serde_json::Number::from(0)),
        );
        status.insert("last_refresh_error".to_string(), serde_json::Value::Null);
    }

    tracing::info!(
        "[KIRO_API] 返回凭证状态: {} (健康分数: {:.1})",
        credential
            .name
            .clone()
            .unwrap_or_else(|| "未命名".to_string()),
        health_score
    );
    Ok(Json(serde_json::Value::Object(status)))
}

/// 计算凭证健康分数
///
/// 基于凭证的基本状态、缓存状态、错误计数等因素综合计算健康分数
/// 分数范围: 0-100，分数越高表示凭证越健康
fn calculate_health_score(
    credential: &ProviderCredential,
    cache_status: Option<&CachedTokenInfo>,
) -> f64 {
    let mut score = 0.0;

    // 1. 基础健康状态 (40分)
    if credential.is_healthy {
        score += 40.0;
    } else {
        score -= 20.0; // 不健康严重扣分
    }

    // 2. 错误计数影响 (20分)
    let error_count = credential.error_count;
    if error_count == 0 {
        score += 20.0;
    } else if error_count <= 2 {
        score += 10.0; // 少量错误，轻微扣分
    } else {
        score -= error_count as f64 * 5.0; // 错误越多扣分越多
    }

    // 3. Token缓存状态 (25分)
    if let Some(cache) = cache_status {
        if cache.access_token.is_some() {
            score += 15.0; // 有缓存token

            // 检查过期时间
            if let Some(expiry_time) = cache.expiry_time {
                let now = chrono::Utc::now();
                let time_until_expiry = expiry_time - now;

                if time_until_expiry > chrono::Duration::hours(1) {
                    score += 10.0; // 距离过期还有较长时间
                } else if time_until_expiry > chrono::Duration::minutes(30) {
                    score += 5.0; // 距离过期还有一些时间
                } else if time_until_expiry <= chrono::Duration::zero() {
                    score -= 10.0; // 已过期
                }
            }
        } else {
            score -= 5.0; // 没有缓存token
        }

        // 刷新错误计数影响
        if cache.refresh_error_count == 0 {
            // 无刷新错误，不加分不减分
        } else if cache.refresh_error_count <= 2 {
            score -= cache.refresh_error_count as f64 * 2.0; // 少量刷新错误
        } else {
            score -= cache.refresh_error_count as f64 * 5.0; // 大量刷新错误严重扣分
        }
    } else {
        score -= 10.0; // 完全没有缓存状态
    }

    // 4. 使用活跃度 (15分)
    if let Some(last_used) = credential.last_used {
        let now = chrono::Utc::now();
        let time_since_used = now - last_used;

        if time_since_used <= chrono::Duration::hours(1) {
            score += 15.0; // 最近1小时内使用过
        } else if time_since_used <= chrono::Duration::hours(24) {
            score += 10.0; // 最近24小时内使用过
        } else if time_since_used <= chrono::Duration::days(7) {
            score += 5.0; // 最近一周内使用过
        } else {
            score += 0.0; // 很久未使用，不扣分也不加分
        }
    } else {
        score -= 5.0; // 从未使用过
    }

    // 确保分数在0-100范围内
    score.max(0.0).min(100.0)
}
