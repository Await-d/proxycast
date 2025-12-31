//! Kiro 凭证事件服务
//!
//! 负责管理 Kiro 凭证相关的实时事件推送，包括：
//! - 凭证状态更新
//! - Token 刷新事件
//! - 健康检查结果
//! - 凭证池统计

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::{broadcast, RwLock};

use crate::websocket::{KiroTokenInfo, WsKiroEvent};

/// Kiro 事件服务
#[derive(Debug)]
pub struct KiroEventService {
    /// 事件发送器
    event_sender: broadcast::Sender<WsKiroEvent>,
    /// 凭证状态缓存
    credential_states: RwLock<HashMap<String, CachedCredentialState>>,
}

/// 缓存的凭证状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedCredentialState {
    uuid: String,
    is_healthy: bool,
    is_disabled: bool,
    error_count: u32,
    health_score: Option<f64>,
    last_used: Option<DateTime<Utc>>,
    last_updated: DateTime<Utc>,
}

impl KiroEventService {
    /// 创建新的 Kiro 事件服务
    pub fn new() -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        Self {
            event_sender,
            credential_states: RwLock::new(HashMap::new()),
        }
    }

    /// 订阅 Kiro 事件
    pub fn subscribe(&self) -> broadcast::Receiver<WsKiroEvent> {
        self.event_sender.subscribe()
    }

    /// 发送凭证状态更新事件
    pub async fn emit_credential_status_update(
        &self,
        uuid: String,
        is_healthy: bool,
        is_disabled: bool,
        error_count: u32,
        health_score: Option<f64>,
        last_used: Option<DateTime<Utc>>,
    ) {
        let now = Utc::now();

        // 更新缓存
        {
            let mut states = self.credential_states.write().await;
            states.insert(
                uuid.clone(),
                CachedCredentialState {
                    uuid: uuid.clone(),
                    is_healthy,
                    is_disabled,
                    error_count,
                    health_score,
                    last_used,
                    last_updated: now,
                },
            );
        }

        // 发送事件
        let event = WsKiroEvent::CredentialStatusUpdate {
            uuid,
            is_healthy,
            is_disabled,
            error_count,
            health_score,
            last_used,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send credential status update event: {}", e);
        }
    }

    /// 发送凭证刷新开始事件
    pub async fn emit_refresh_started(&self, uuid: String, credential_name: Option<String>) {
        let event = WsKiroEvent::RefreshStarted {
            uuid,
            credential_name,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send refresh started event: {}", e);
        }
    }

    /// 发送凭证刷新成功事件
    pub async fn emit_refresh_success(
        &self,
        uuid: String,
        credential_name: Option<String>,
        expires_at: DateTime<Utc>,
        auth_method: String,
        provider: String,
        region: String,
    ) {
        let new_token_info = KiroTokenInfo {
            expires_at,
            auth_method,
            provider,
            region,
        };

        let event = WsKiroEvent::RefreshSuccess {
            uuid,
            credential_name,
            new_token_info,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send refresh success event: {}", e);
        }
    }

    /// 发送凭证刷新失败事件
    pub async fn emit_refresh_failed(
        &self,
        uuid: String,
        credential_name: Option<String>,
        error: String,
        error_code: Option<String>,
    ) {
        let event = WsKiroEvent::RefreshFailed {
            uuid,
            credential_name,
            error,
            error_code,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send refresh failed event: {}", e);
        }
    }

    /// 发送健康检查完成事件
    pub async fn emit_health_check_completed(
        &self,
        uuid: String,
        credential_name: Option<String>,
        is_healthy: bool,
        health_score: Option<f64>,
    ) {
        let last_check = Utc::now();
        let event = WsKiroEvent::HealthCheckCompleted {
            uuid,
            credential_name,
            is_healthy,
            health_score,
            last_check,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send health check completed event: {}", e);
        }
    }

    /// 发送凭证池统计更新事件
    pub async fn emit_pool_stats_update(
        &self,
        total_credentials: u32,
        healthy_credentials: u32,
        available_credentials: u32,
        average_health_score: Option<f64>,
        last_rotation: Option<DateTime<Utc>>,
    ) {
        let event = WsKiroEvent::PoolStatsUpdate {
            total_credentials,
            healthy_credentials,
            available_credentials,
            average_health_score,
            last_rotation,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send pool stats update event: {}", e);
        }
    }

    /// 发送凭证轮换事件
    pub async fn emit_credential_rotated(
        &self,
        from_uuid: Option<String>,
        to_uuid: String,
        reason: String,
    ) {
        let rotation_time = Utc::now();
        let event = WsKiroEvent::CredentialRotated {
            from_uuid,
            to_uuid,
            reason,
            rotation_time,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send credential rotated event: {}", e);
        }
    }

    /// 发送凭证自动禁用事件
    pub async fn emit_credential_auto_disabled(
        &self,
        uuid: String,
        credential_name: Option<String>,
        reason: String,
        error_type: String,
    ) {
        let disable_time = Utc::now();
        let event = WsKiroEvent::CredentialAutoDisabled {
            uuid,
            credential_name,
            reason,
            error_type,
            disable_time,
        };

        if let Err(e) = self.event_sender.send(event) {
            tracing::debug!("Failed to send credential auto disabled event: {}", e);
        }
    }

    /// 获取当前活跃订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.event_sender.receiver_count()
    }

    /// 获取缓存的凭证状态
    pub async fn get_credential_state(&self, uuid: &str) -> Option<CachedCredentialState> {
        let states = self.credential_states.read().await;
        states.get(uuid).cloned()
    }

    /// 获取所有凭证状态
    pub async fn get_all_credential_states(&self) -> Vec<CachedCredentialState> {
        let states = self.credential_states.read().await;
        states.values().cloned().collect()
    }

    /// 清理过期的凭证状态缓存
    pub async fn cleanup_expired_states(&self, retention_hours: u64) {
        let cutoff = Utc::now() - chrono::Duration::hours(retention_hours as i64);
        let mut states = self.credential_states.write().await;
        states.retain(|_, state| state.last_updated > cutoff);
    }
}

impl Default for KiroEventService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_credential_status_update_event() {
        let service = KiroEventService::new();
        let mut receiver = service.subscribe();

        // 发送凭证状态更新事件
        let uuid = "test-uuid".to_string();
        service
            .emit_credential_status_update(uuid.clone(), true, false, 0, Some(85.5), None)
            .await;

        // 验证事件被正确接收
        let event = receiver.recv().await.unwrap();
        match event {
            WsKiroEvent::CredentialStatusUpdate {
                uuid: event_uuid,
                is_healthy,
                health_score,
                ..
            } => {
                assert_eq!(event_uuid, uuid);
                assert!(is_healthy);
                assert_eq!(health_score, Some(85.5));
            }
            _ => panic!("Expected CredentialStatusUpdate event"),
        }

        // 验证状态被缓存
        let cached_state = service.get_credential_state(&uuid).await.unwrap();
        assert_eq!(cached_state.uuid, uuid);
        assert!(cached_state.is_healthy);
        assert_eq!(cached_state.health_score, Some(85.5));
    }

    #[tokio::test]
    async fn test_refresh_events() {
        let service = KiroEventService::new();
        let mut receiver = service.subscribe();

        let uuid = "test-uuid".to_string();
        let credential_name = Some("test-credential".to_string());

        // 测试刷新开始事件
        service
            .emit_refresh_started(uuid.clone(), credential_name.clone())
            .await;

        let event = receiver.recv().await.unwrap();
        match event {
            WsKiroEvent::RefreshStarted {
                uuid: event_uuid,
                credential_name: event_name,
            } => {
                assert_eq!(event_uuid, uuid);
                assert_eq!(event_name, credential_name);
            }
            _ => panic!("Expected RefreshStarted event"),
        }

        // 测试刷新成功事件
        let expires_at = Utc::now() + chrono::Duration::hours(1);
        service
            .emit_refresh_success(
                uuid.clone(),
                credential_name.clone(),
                expires_at,
                "IdC".to_string(),
                "BuilderId".to_string(),
                "us-east-1".to_string(),
            )
            .await;

        let event = receiver.recv().await.unwrap();
        match event {
            WsKiroEvent::RefreshSuccess {
                uuid: event_uuid,
                new_token_info,
                ..
            } => {
                assert_eq!(event_uuid, uuid);
                assert_eq!(new_token_info.auth_method, "IdC");
                assert_eq!(new_token_info.provider, "BuilderId");
            }
            _ => panic!("Expected RefreshSuccess event"),
        }
    }

    #[tokio::test]
    async fn test_multiple_subscribers() {
        let service = KiroEventService::new();
        let mut receiver1 = service.subscribe();
        let mut receiver2 = service.subscribe();

        assert_eq!(service.subscriber_count(), 2);

        // 发送事件
        service
            .emit_refresh_started("test-uuid".to_string(), None)
            .await;

        // 两个订阅者都应该收到事件
        let event1 = receiver1.recv().await.unwrap();
        let event2 = receiver2.recv().await.unwrap();

        matches!(event1, WsKiroEvent::RefreshStarted { .. });
        matches!(event2, WsKiroEvent::RefreshStarted { .. });
    }

    #[tokio::test]
    async fn test_credential_state_caching() {
        let service = KiroEventService::new();

        // 添加多个凭证状态
        service
            .emit_credential_status_update("uuid1".to_string(), true, false, 0, Some(90.0), None)
            .await;

        service
            .emit_credential_status_update("uuid2".to_string(), false, true, 5, Some(30.0), None)
            .await;

        // 验证所有状态都被缓存
        let all_states = service.get_all_credential_states().await;
        assert_eq!(all_states.len(), 2);

        // 验证可以按 UUID 获取特定状态
        let state1 = service.get_credential_state("uuid1").await.unwrap();
        assert_eq!(state1.health_score, Some(90.0));

        let state2 = service.get_credential_state("uuid2").await.unwrap();
        assert_eq!(state2.error_count, 5);
    }
}
