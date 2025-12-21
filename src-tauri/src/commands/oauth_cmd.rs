//! Unified OAuth Commands for Kiro/Gemini/Qwen Providers
//!
//! This module consolidates the OAuth credential management commands
//! for all three OAuth providers into a single set of parameterized commands.

use crate::providers;
use crate::AppState;
use crate::LogState;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::State;

/// Supported OAuth provider types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OAuthProvider {
    Kiro,
    Gemini,
    Qwen,
}

impl OAuthProvider {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "kiro" => Ok(OAuthProvider::Kiro),
            "gemini" => Ok(OAuthProvider::Gemini),
            "qwen" => Ok(OAuthProvider::Qwen),
            _ => Err(format!("Unknown provider: {s}")),
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            OAuthProvider::Kiro => "Kiro",
            OAuthProvider::Gemini => "Gemini",
            OAuthProvider::Qwen => "Qwen",
        }
    }
}

/// Unified credential status for all OAuth providers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthCredentialStatus {
    pub provider: String,
    pub loaded: bool,
    pub has_access_token: bool,
    pub has_refresh_token: bool,
    pub is_valid: bool,
    pub expiry_info: Option<String>,
    pub creds_path: String,
    /// Provider-specific additional info
    pub extra: serde_json::Value,
}

/// Environment variable representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVariable {
    pub key: String,
    pub value: String,
    pub masked: String,
}

/// Result of credential file change check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub changed: bool,
    pub new_hash: String,
    pub reloaded: bool,
}

fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() <= 12 {
        "****".to_string()
    } else {
        let prefix: String = chars[..6].iter().collect();
        let suffix: String = chars[chars.len() - 4..].iter().collect();
        format!("{prefix}****{suffix}")
    }
}

fn get_creds_path(provider: &OAuthProvider) -> PathBuf {
    match provider {
        OAuthProvider::Kiro => providers::kiro::KiroProvider::default_creds_path(),
        OAuthProvider::Gemini => providers::gemini::GeminiProvider::default_creds_path(),
        OAuthProvider::Qwen => providers::qwen::QwenProvider::default_creds_path(),
    }
}

/// Get OAuth credentials status for a provider
#[tauri::command]
pub async fn get_oauth_credentials(
    state: State<'_, AppState>,
    provider: String,
) -> Result<OAuthCredentialStatus, String> {
    let provider_type = OAuthProvider::from_str(&provider)?;
    let s = state.read().await;
    let path = get_creds_path(&provider_type);

    match provider_type {
        OAuthProvider::Kiro => {
            let creds = &s.kiro_provider.credentials;
            Ok(OAuthCredentialStatus {
                provider: provider.clone(),
                loaded: creds.access_token.is_some() || creds.refresh_token.is_some(),
                has_access_token: creds.access_token.is_some(),
                has_refresh_token: creds.refresh_token.is_some(),
                is_valid: creds.access_token.is_some() && !s.kiro_provider.is_token_expiring_soon(),
                expiry_info: creds.expires_at.clone(),
                creds_path: path.to_string_lossy().to_string(),
                extra: serde_json::json!({
                    "region": creds.region,
                    "auth_method": creds.auth_method,
                }),
            })
        }
        OAuthProvider::Gemini => {
            let creds = &s.gemini_provider.credentials;
            Ok(OAuthCredentialStatus {
                provider: provider.clone(),
                loaded: creds.access_token.is_some() || creds.refresh_token.is_some(),
                has_access_token: creds.access_token.is_some(),
                has_refresh_token: creds.refresh_token.is_some(),
                is_valid: s.gemini_provider.is_token_valid(),
                expiry_info: creds.expiry_date.map(|d| d.to_string()),
                creds_path: path.to_string_lossy().to_string(),
                extra: serde_json::json!({}),
            })
        }
        OAuthProvider::Qwen => {
            let creds = &s.qwen_provider.credentials;
            Ok(OAuthCredentialStatus {
                provider: provider.clone(),
                loaded: creds.access_token.is_some() || creds.refresh_token.is_some(),
                has_access_token: creds.access_token.is_some(),
                has_refresh_token: creds.refresh_token.is_some(),
                is_valid: s.qwen_provider.is_token_valid(),
                expiry_info: creds.expiry_date.map(|d| d.to_string()),
                creds_path: path.to_string_lossy().to_string(),
                extra: serde_json::json!({
                    "resource_url": creds.resource_url,
                }),
            })
        }
    }
}

/// Reload OAuth credentials from file
#[tauri::command]
pub async fn reload_oauth_credentials(
    state: State<'_, AppState>,
    logs: State<'_, LogState>,
    provider: String,
) -> Result<String, String> {
    let provider_type = OAuthProvider::from_str(&provider)?;
    let display_name = provider_type.display_name();

    logs.write()
        .await
        .add("info", &format!("[{display_name}] 正在加载凭证..."));

    let mut s = state.write().await;

    let result = match provider_type {
        OAuthProvider::Kiro => s.kiro_provider.load_credentials().await,
        OAuthProvider::Gemini => s.gemini_provider.load_credentials().await,
        OAuthProvider::Qwen => s.qwen_provider.load_credentials().await,
    };

    match result {
        Ok(_) => {
            logs.write()
                .await
                .add("info", &format!("[{display_name}] 凭证加载成功"));
            Ok(format!("{display_name} credentials reloaded"))
        }
        Err(e) => {
            logs.write()
                .await
                .add("error", &format!("[{display_name}] 凭证加载失败: {e}"));
            Err(e.to_string())
        }
    }
}

/// Refresh OAuth token for a provider
#[tauri::command]
pub async fn refresh_oauth_token(
    state: State<'_, AppState>,
    logs: State<'_, LogState>,
    provider: String,
) -> Result<String, String> {
    let provider_type = OAuthProvider::from_str(&provider)?;
    let display_name = provider_type.display_name();

    logs.write()
        .await
        .add("info", &format!("[{display_name}] 正在刷新 Token..."));

    let mut s = state.write().await;

    let result = match provider_type {
        OAuthProvider::Kiro => s.kiro_provider.refresh_token().await,
        OAuthProvider::Gemini => s.gemini_provider.refresh_token().await,
        OAuthProvider::Qwen => s.qwen_provider.refresh_token().await,
    };

    match result {
        Ok(_token) => {
            logs.write()
                .await
                .add("info", &format!("[{display_name}] Token 刷新成功"));
            // P0 安全修复：不返回明文 token
            Ok("Token 刷新成功".to_string())
        }
        Err(e) => {
            logs.write()
                .await
                .add("error", &format!("[{display_name}] Token 刷新失败: {e}"));
            Err(e.to_string())
        }
    }
}

/// Get environment variables for a provider
#[tauri::command]
pub async fn get_oauth_env_variables(
    state: State<'_, AppState>,
    provider: String,
) -> Result<Vec<EnvVariable>, String> {
    let provider_type = OAuthProvider::from_str(&provider)?;
    let s = state.read().await;
    let mut vars = Vec::new();

    match provider_type {
        OAuthProvider::Kiro => {
            let creds = &s.kiro_provider.credentials;
            // P0 安全修复：不返回明文敏感凭证
            if let Some(token) = &creds.access_token {
                vars.push(EnvVariable {
                    key: "KIRO_ACCESS_TOKEN".to_string(),
                    value: String::new(),
                    masked: mask_token(token),
                });
            }
            if let Some(token) = &creds.refresh_token {
                vars.push(EnvVariable {
                    key: "KIRO_REFRESH_TOKEN".to_string(),
                    value: String::new(),
                    masked: mask_token(token),
                });
            }
            if let Some(id) = &creds.client_id {
                vars.push(EnvVariable {
                    key: "KIRO_CLIENT_ID".to_string(),
                    value: String::new(),
                    masked: mask_token(id),
                });
            }
            if let Some(secret) = &creds.client_secret {
                vars.push(EnvVariable {
                    key: "KIRO_CLIENT_SECRET".to_string(),
                    value: String::new(),
                    masked: mask_token(secret),
                });
            }
            if let Some(arn) = &creds.profile_arn {
                vars.push(EnvVariable {
                    key: "KIRO_PROFILE_ARN".to_string(),
                    value: arn.clone(),
                    masked: arn.clone(),
                });
            }
            if let Some(region) = &creds.region {
                vars.push(EnvVariable {
                    key: "KIRO_REGION".to_string(),
                    value: region.clone(),
                    masked: region.clone(),
                });
            }
            if let Some(method) = &creds.auth_method {
                vars.push(EnvVariable {
                    key: "KIRO_AUTH_METHOD".to_string(),
                    value: method.clone(),
                    masked: method.clone(),
                });
            }
        }
        OAuthProvider::Gemini => {
            let creds = &s.gemini_provider.credentials;
            // P0 安全修复：不返回明文敏感凭证
            if let Some(token) = &creds.access_token {
                vars.push(EnvVariable {
                    key: "GEMINI_ACCESS_TOKEN".to_string(),
                    value: String::new(),
                    masked: mask_token(token),
                });
            }
            if let Some(token) = &creds.refresh_token {
                vars.push(EnvVariable {
                    key: "GEMINI_REFRESH_TOKEN".to_string(),
                    value: String::new(),
                    masked: mask_token(token),
                });
            }
            if let Some(expiry) = creds.expiry_date {
                let expiry_str = expiry.to_string();
                vars.push(EnvVariable {
                    key: "GEMINI_EXPIRY_DATE".to_string(),
                    value: expiry_str.clone(),
                    masked: expiry_str,
                });
            }
        }
        OAuthProvider::Qwen => {
            let creds = &s.qwen_provider.credentials;
            // P0 安全修复：不返回明文敏感凭证
            if let Some(token) = &creds.access_token {
                vars.push(EnvVariable {
                    key: "QWEN_ACCESS_TOKEN".to_string(),
                    value: String::new(),
                    masked: mask_token(token),
                });
            }
            if let Some(token) = &creds.refresh_token {
                vars.push(EnvVariable {
                    key: "QWEN_REFRESH_TOKEN".to_string(),
                    value: String::new(),
                    masked: mask_token(token),
                });
            }
            if let Some(url) = &creds.resource_url {
                vars.push(EnvVariable {
                    key: "QWEN_RESOURCE_URL".to_string(),
                    value: url.clone(),
                    masked: url.clone(),
                });
            }
            if let Some(expiry) = creds.expiry_date {
                let expiry_str = expiry.to_string();
                vars.push(EnvVariable {
                    key: "QWEN_EXPIRY_DATE".to_string(),
                    value: expiry_str.clone(),
                    masked: expiry_str,
                });
            }
        }
    }

    Ok(vars)
}

/// Get token file hash for a provider
#[tauri::command]
pub async fn get_oauth_token_file_hash(provider: String) -> Result<String, String> {
    let provider_type = OAuthProvider::from_str(&provider)?;
    let path = get_creds_path(&provider_type);

    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok("".to_string());
    }

    let content = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let hash = format!("{:x}", md5::compute(&content));
    Ok(hash)
}

/// Check credential file changes and auto-reload
#[tauri::command]
pub async fn check_and_reload_oauth_credentials(
    state: State<'_, AppState>,
    logs: State<'_, LogState>,
    provider: String,
    last_hash: String,
) -> Result<CheckResult, String> {
    let provider_type = OAuthProvider::from_str(&provider)?;
    let display_name = provider_type.display_name();
    let path = get_creds_path(&provider_type);

    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Ok(CheckResult {
            changed: false,
            new_hash: "".to_string(),
            reloaded: false,
        });
    }

    let content = tokio::fs::read(&path).await.map_err(|e| e.to_string())?;
    let new_hash = format!("{:x}", md5::compute(&content));

    if !last_hash.is_empty() && new_hash != last_hash {
        logs.write().await.add(
            "info",
            &format!("[{display_name}][自动检测] 凭证文件已变化，正在重新加载..."),
        );

        let mut s = state.write().await;
        let result = match provider_type {
            OAuthProvider::Kiro => s.kiro_provider.load_credentials().await,
            OAuthProvider::Gemini => s.gemini_provider.load_credentials().await,
            OAuthProvider::Qwen => s.qwen_provider.load_credentials().await,
        };

        match result {
            Ok(_) => {
                logs.write().await.add(
                    "info",
                    &format!("[{display_name}][自动检测] 凭证重新加载成功"),
                );
                Ok(CheckResult {
                    changed: true,
                    new_hash,
                    reloaded: true,
                })
            }
            Err(e) => {
                logs.write().await.add(
                    "error",
                    &format!("[{display_name}][自动检测] 凭证重新加载失败: {e}"),
                );
                Ok(CheckResult {
                    changed: true,
                    new_hash,
                    reloaded: false,
                })
            }
        }
    } else {
        Ok(CheckResult {
            changed: false,
            new_hash,
            reloaded: false,
        })
    }
}

/// Get all OAuth providers status at once
#[tauri::command]
pub async fn get_all_oauth_credentials(
    state: State<'_, AppState>,
) -> Result<Vec<OAuthCredentialStatus>, String> {
    let s = state.read().await;
    let mut results = Vec::new();

    // Kiro
    let kiro_creds = &s.kiro_provider.credentials;
    let kiro_path = providers::kiro::KiroProvider::default_creds_path();
    results.push(OAuthCredentialStatus {
        provider: "kiro".to_string(),
        loaded: kiro_creds.access_token.is_some() || kiro_creds.refresh_token.is_some(),
        has_access_token: kiro_creds.access_token.is_some(),
        has_refresh_token: kiro_creds.refresh_token.is_some(),
        is_valid: kiro_creds.access_token.is_some() && !s.kiro_provider.is_token_expiring_soon(),
        expiry_info: kiro_creds.expires_at.clone(),
        creds_path: kiro_path.to_string_lossy().to_string(),
        extra: serde_json::json!({
            "region": kiro_creds.region,
            "auth_method": kiro_creds.auth_method,
        }),
    });

    // Gemini
    let gemini_creds = &s.gemini_provider.credentials;
    let gemini_path = providers::gemini::GeminiProvider::default_creds_path();
    results.push(OAuthCredentialStatus {
        provider: "gemini".to_string(),
        loaded: gemini_creds.access_token.is_some() || gemini_creds.refresh_token.is_some(),
        has_access_token: gemini_creds.access_token.is_some(),
        has_refresh_token: gemini_creds.refresh_token.is_some(),
        is_valid: s.gemini_provider.is_token_valid(),
        expiry_info: gemini_creds.expiry_date.map(|d| d.to_string()),
        creds_path: gemini_path.to_string_lossy().to_string(),
        extra: serde_json::json!({}),
    });

    // Qwen
    let qwen_creds = &s.qwen_provider.credentials;
    let qwen_path = providers::qwen::QwenProvider::default_creds_path();
    results.push(OAuthCredentialStatus {
        provider: "qwen".to_string(),
        loaded: qwen_creds.access_token.is_some() || qwen_creds.refresh_token.is_some(),
        has_access_token: qwen_creds.access_token.is_some(),
        has_refresh_token: qwen_creds.refresh_token.is_some(),
        is_valid: s.qwen_provider.is_token_valid(),
        expiry_info: qwen_creds.expiry_date.map(|d| d.to_string()),
        creds_path: qwen_path.to_string_lossy().to_string(),
        extra: serde_json::json!({
            "resource_url": qwen_creds.resource_url,
        }),
    });

    Ok(results)
}
