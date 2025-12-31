//! 托盘状态同步模块
//!
//! 提供托盘状态与应用状态的同步功能
//!
//! # Requirements

#![allow(dead_code)]
//! - 7.1: API 服务器状态变化时在 1 秒内更新托盘图标
//! - 7.2: 凭证健康状态变化时在 1 秒内更新托盘图标

use super::state::{calculate_icon_status, CredentialHealth, TrayIconStatus, TrayStateSnapshot};
use super::TrayManager;
use std::sync::Arc;
use tauri::{AppHandle, Runtime};
use tokio::sync::RwLock;
use tracing::{debug, info};

/// 托盘状态同步器
///
/// 负责监听应用状态变化并更新托盘
pub struct TraySynchronizer<R: Runtime> {
    /// AppHandle 引用
    app: AppHandle<R>,
    /// 托盘管理器引用
    tray_manager: Arc<RwLock<Option<TrayManager<R>>>>,
}

impl<R: Runtime> TraySynchronizer<R> {
    /// 创建托盘状态同步器
    pub fn new(app: AppHandle<R>, tray_manager: Arc<RwLock<Option<TrayManager<R>>>>) -> Self {
        Self { app, tray_manager }
    }

    /// 同步托盘状态
    ///
    /// 从应用状态获取最新数据并更新托盘
    ///
    /// # Requirements
    /// - 7.1: API 服务器状态变化时更新托盘图标
    /// - 7.2: 凭证健康状态变化时更新托盘图标
    pub async fn sync_state(
        &self,
        server_running: bool,
        server_host: &str,
        server_port: u16,
        credentials: &[CredentialHealth],
        today_requests: u64,
        auto_start_enabled: bool,
    ) -> Result<(), String> {
        let tray_guard = self.tray_manager.read().await;
        let tray_manager = tray_guard
            .as_ref()
            .ok_or_else(|| "托盘管理器未初始化".to_string())?;

        // 计算图标状态
        let icon_status = calculate_icon_status(server_running, credentials);

        // 计算可用凭证数
        let available_credentials = credentials.iter().filter(|c| c.is_valid).count();
        let total_credentials = credentials.len();

        // 构建状态快照
        let snapshot = TrayStateSnapshot {
            icon_status,
            server_running,
            server_address: if server_running {
                format!("{}:{}", server_host, server_port)
            } else {
                String::new()
            },
            available_credentials,
            total_credentials,
            today_requests,
            auto_start_enabled,
        };

        // 更新托盘状态
        tray_manager
            .update_state(snapshot)
            .await
            .map_err(|e| e.to_string())?;

        debug!(
            "托盘状态已同步: server_running={}, icon_status={:?}, credentials={}/{}",
            server_running, icon_status, available_credentials, total_credentials
        );

        Ok(())
    }

    /// 仅更新服务器状态
    ///
    /// # Requirements
    /// - 7.1: API 服务器状态变化时在 1 秒内更新托盘图标
    pub async fn update_server_status(
        &self,
        server_running: bool,
        server_host: &str,
        server_port: u16,
    ) -> Result<(), String> {
        let tray_guard = self.tray_manager.read().await;
        let tray_manager = tray_guard
            .as_ref()
            .ok_or_else(|| "托盘管理器未初始化".to_string())?;

        // 获取当前状态
        let mut current_state = tray_manager.get_state().await;

        // 更新服务器相关字段
        current_state.server_running = server_running;
        current_state.server_address = if server_running {
            format!("{}:{}", server_host, server_port)
        } else {
            String::new()
        };

        // 重新计算图标状态
        // 如果服务器停止，图标状态为 Stopped
        // 否则保持当前状态（凭证状态未变）
        if !server_running {
            current_state.icon_status = TrayIconStatus::Stopped;
        } else if current_state.icon_status == TrayIconStatus::Stopped {
            // 服务器启动，但之前是停止状态，设为 Running
            current_state.icon_status = TrayIconStatus::Running;
        }

        // 更新托盘状态
        tray_manager
            .update_state(current_state)
            .await
            .map_err(|e| e.to_string())?;

        info!(
            "托盘服务器状态已更新: running={}, address={}:{}",
            server_running, server_host, server_port
        );

        Ok(())
    }

    /// 仅更新凭证健康状态
    ///
    /// # Requirements
    /// - 7.2: 凭证健康状态变化时在 1 秒内更新托盘图标
    pub async fn update_credential_health(
        &self,
        credentials: &[CredentialHealth],
    ) -> Result<(), String> {
        let tray_guard = self.tray_manager.read().await;
        let tray_manager = tray_guard
            .as_ref()
            .ok_or_else(|| "托盘管理器未初始化".to_string())?;

        // 获取当前状态
        let mut current_state = tray_manager.get_state().await;

        // 更新凭证相关字段
        current_state.available_credentials = credentials.iter().filter(|c| c.is_valid).count();
        current_state.total_credentials = credentials.len();

        // 重新计算图标状态
        current_state.icon_status =
            calculate_icon_status(current_state.server_running, credentials);

        // 保存日志所需的值
        let available = current_state.available_credentials;
        let total = current_state.total_credentials;
        let icon_status = current_state.icon_status;

        // 更新托盘状态
        tray_manager
            .update_state(current_state)
            .await
            .map_err(|e| e.to_string())?;

        info!(
            "托盘凭证状态已更新: available={}/{}, icon_status={:?}",
            available, total, icon_status
        );

        Ok(())
    }

    /// 更新今日请求数
    pub async fn update_request_count(&self, today_requests: u64) -> Result<(), String> {
        let tray_guard = self.tray_manager.read().await;
        let tray_manager = tray_guard
            .as_ref()
            .ok_or_else(|| "托盘管理器未初始化".to_string())?;

        // 获取当前状态
        let mut current_state = tray_manager.get_state().await;

        // 更新请求数
        current_state.today_requests = today_requests;

        // 更新托盘状态（不改变图标）
        tray_manager
            .update_state(current_state)
            .await
            .map_err(|e| e.to_string())?;

        debug!("托盘请求数已更新: {}", today_requests);

        Ok(())
    }

    /// 更新自启动状态
    pub async fn update_auto_start(&self, enabled: bool) -> Result<(), String> {
        let tray_guard = self.tray_manager.read().await;
        let tray_manager = tray_guard
            .as_ref()
            .ok_or_else(|| "托盘管理器未初始化".to_string())?;

        // 获取当前状态
        let mut current_state = tray_manager.get_state().await;

        // 更新自启动状态
        current_state.auto_start_enabled = enabled;

        // 更新托盘状态（不改变图标）
        tray_manager
            .update_state(current_state)
            .await
            .map_err(|e| e.to_string())?;

        debug!("托盘自启动状态已更新: {}", enabled);

        Ok(())
    }
}

/// 从 ProviderPoolService 获取凭证健康状态
///
/// 将 ProviderPoolService 中的凭证状态转换为 CredentialHealth 列表
pub fn get_credential_health_from_pool(
    pool_credentials: &[(String, bool, bool, bool)], // (id, is_valid, is_expiring_soon, is_low_balance)
) -> Vec<CredentialHealth> {
    pool_credentials
        .iter()
        .map(
            |(_, is_valid, is_expiring_soon, is_low_balance)| CredentialHealth {
                is_valid: *is_valid,
                is_expiring_soon: *is_expiring_soon,
                is_low_balance: *is_low_balance,
            },
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_credential_health_from_pool() {
        let pool_data = vec![
            ("cred1".to_string(), true, false, false),
            ("cred2".to_string(), true, true, false),
            ("cred3".to_string(), false, false, false),
        ];

        let health = get_credential_health_from_pool(&pool_data);

        assert_eq!(health.len(), 3);
        assert!(health[0].is_valid);
        assert!(!health[0].is_expiring_soon);
        assert!(health[1].is_valid);
        assert!(health[1].is_expiring_soon);
        assert!(!health[2].is_valid);
    }

    #[test]
    fn test_get_credential_health_empty() {
        let pool_data: Vec<(String, bool, bool, bool)> = vec![];
        let health = get_credential_health_from_pool(&pool_data);
        assert!(health.is_empty());
    }
}
