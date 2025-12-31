//! 插件系统相关命令
//!
//! 提供插件管理和 UI 相关的 Tauri 命令：
//! - get_plugin_status: 获取插件服务状态
//! - get_plugins: 获取所有插件列表
//! - get_plugins_with_ui: 获取带有 UI 配置的已安装插件列表
//! - get_plugin_ui: 获取插件 UI 定义
//! - handle_plugin_action: 处理插件 UI 操作
//!
//! _需求: 3.1, 3.2, 3.3_

#![allow(dead_code)]

use crate::plugin::{PluginConfig, PluginInfo, PluginManager, PluginManifest};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::plugin_install_cmd::PluginInstallerState;

/// 插件管理器状态
pub struct PluginManagerState(pub Arc<RwLock<PluginManager>>);

/// 插件状态响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginServiceStatus {
    pub enabled: bool,
    pub plugin_count: usize,
    pub plugins_dir: String,
}

/// 插件配置请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfigRequest {
    pub enabled: bool,
    pub timeout_ms: u64,
    pub settings: serde_json::Value,
}

/// 获取插件服务状态
#[tauri::command]
pub async fn get_plugin_status(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<PluginServiceStatus, String> {
    let manager = state.0.read().await;
    Ok(PluginServiceStatus {
        enabled: true,
        plugin_count: manager.count(),
        plugins_dir: manager.plugins_dir().to_string_lossy().to_string(),
    })
}

/// 获取所有插件列表
#[tauri::command]
pub async fn get_plugins(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<Vec<PluginInfo>, String> {
    let manager = state.0.read().await;
    Ok(manager.list().await)
}

/// 获取单个插件信息
#[tauri::command]
pub async fn get_plugin_info(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<Option<PluginInfo>, String> {
    let manager = state.0.read().await;
    Ok(manager.get_info(&name).await)
}

/// 启用插件
#[tauri::command]
pub async fn enable_plugin(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<(), String> {
    let manager = state.0.read().await;
    manager.enable(&name).await.map_err(|e| e.to_string())
}

/// 禁用插件
#[tauri::command]
pub async fn disable_plugin(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<(), String> {
    let manager = state.0.read().await;
    manager.disable(&name).await.map_err(|e| e.to_string())
}

/// 更新插件配置
#[tauri::command]
pub async fn update_plugin_config(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
    config: PluginConfigRequest,
) -> Result<(), String> {
    let manager = state.0.read().await;
    let plugin_config = PluginConfig {
        enabled: config.enabled,
        timeout_ms: config.timeout_ms,
        settings: config.settings,
    };
    manager
        .update_config(&name, plugin_config)
        .await
        .map_err(|e| e.to_string())
}

/// 获取插件配置
#[tauri::command]
pub async fn get_plugin_config(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<Option<PluginConfig>, String> {
    let manager = state.0.read().await;
    Ok(manager.get_config(&name))
}

/// 重新加载所有插件
#[tauri::command]
pub async fn reload_plugins(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<Vec<String>, String> {
    let manager = state.0.read().await;
    manager.load_all().await.map_err(|e| e.to_string())
}

/// 卸载插件
#[tauri::command]
pub async fn unload_plugin(
    state: tauri::State<'_, PluginManagerState>,
    name: String,
) -> Result<(), String> {
    let manager = state.0.read().await;
    manager.unload(&name).await.map_err(|e| e.to_string())
}

/// 获取插件目录路径
#[tauri::command]
pub async fn get_plugins_dir(
    state: tauri::State<'_, PluginManagerState>,
) -> Result<String, String> {
    let manager = state.0.read().await;
    Ok(manager.plugins_dir().to_string_lossy().to_string())
}

// ============================================================================
// 插件 UI 注册系统
// ============================================================================

/// 插件 UI 信息
///
/// 用于前端显示带有 UI 的插件列表
/// _需求: 3.1, 3.3_
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUIInfo {
    /// 插件 ID
    pub plugin_id: String,
    /// 插件名称
    pub name: String,
    /// 插件描述
    pub description: String,
    /// 图标名称 (Lucide 图标)
    pub icon: String,
    /// UI 展示位置列表 (如 "tools", "sidebar", "main")
    pub surfaces: Vec<String>,
}

/// 从插件目录读取 manifest 文件
///
/// 尝试读取 plugin.json 文件并解析为 PluginManifest
fn read_plugin_manifest(install_path: &Path) -> Option<PluginManifest> {
    let manifest_path = install_path.join("plugin.json");
    if !manifest_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&manifest_path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 获取带有 UI 配置的已安装插件列表
///
/// 从已安装插件中筛选带有 UI 配置的插件，返回 PluginUIInfo 列表
/// _需求: 3.1, 3.3_
#[tauri::command]
pub async fn get_plugins_with_ui(
    installer_state: tauri::State<'_, PluginInstallerState>,
) -> Result<Vec<PluginUIInfo>, String> {
    let installer = installer_state.0.read().await;

    // 获取所有已安装插件
    let installed_plugins = installer.list_installed().map_err(|e| e.to_string())?;

    // 筛选带有 UI 配置的插件
    let ui_plugins: Vec<PluginUIInfo> = installed_plugins
        .into_iter()
        .filter_map(|plugin| {
            // 读取插件的 manifest 文件
            let manifest = read_plugin_manifest(&plugin.install_path)?;

            // 检查是否有 UI 配置
            let ui_config = manifest.ui?;

            // 只返回有 surfaces 配置的插件
            if ui_config.surfaces.is_empty() {
                return None;
            }

            Some(PluginUIInfo {
                plugin_id: plugin.id,
                name: plugin.name,
                description: plugin.description,
                icon: ui_config.icon.unwrap_or_else(|| "puzzle".to_string()),
                surfaces: ui_config.surfaces,
            })
        })
        .collect();

    Ok(ui_plugins)
}

// ============================================================================
// 插件 UI 相关命令
// ============================================================================

use crate::plugin::{UIMessage, UserAction};

/// 获取插件 UI 定义
/// 返回插件的初始 UI 消息列表
#[tauri::command]
pub async fn get_plugin_ui(
    state: tauri::State<'_, PluginManagerState>,
    plugin_id: String,
) -> Result<Vec<UIMessage>, String> {
    let manager = state.0.read().await;

    // 获取插件的 Surface 定义
    let surfaces = manager
        .get_plugin_surfaces(&plugin_id)
        .await
        .map_err(|e| e.to_string())?;

    // 转换为 UI 消息
    let messages: Vec<UIMessage> = surfaces.into_iter().flat_map(|s| s.to_messages()).collect();

    Ok(messages)
}

/// 处理插件 UI 操作
/// 将用户操作转发给插件并返回响应消息
#[tauri::command]
pub async fn handle_plugin_action(
    state: tauri::State<'_, PluginManagerState>,
    plugin_id: String,
    action: UserAction,
) -> Result<Vec<UIMessage>, String> {
    let mut manager = state.0.write().await;

    manager
        .handle_plugin_action(&plugin_id, action)
        .await
        .map_err(|e| e.to_string())
}
