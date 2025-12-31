//! Claude OAuth Provider
//!
//! 实现 Anthropic Claude OAuth 认证流程，与 claude-relay-service 对齐。
//!
//! ## 支持的授权方式
//!
//! 1. **标准 OAuth 流程** - 使用官方 redirect_uri，用户需手动复制授权码
//! 2. **Cookie 自动授权** - 使用 sessionKey 自动完成整个 OAuth 流程
//! 3. **Setup Token** - 只需推理权限，无 refresh_token
//!

#![allow(dead_code)]
//! ## 主要功能
//!
//! - Token 刷新和重试机制
//! - 统一凭证格式
//! - 组织信息获取

use super::error::{
    create_auth_error, create_config_error, create_token_refresh_error, ProviderError,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::path::PathBuf;

// OAuth 端点和凭证 - 与 claude-relay-service 完全一致
const CLAUDE_AUTH_URL: &str = "https://claude.ai/oauth/authorize";
const CLAUDE_TOKEN_URL: &str = "https://console.anthropic.com/v1/oauth/token";
const CLAUDE_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
// 使用 Anthropic 官方 redirect_uri（用户需手动复制授权码）
const CLAUDE_REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";
// OAuth scopes - 与 claude-relay-service 一致
const CLAUDE_SCOPES: &str = "org:create_api_key user:profile user:inference";
// Setup Token 只需要推理权限
const CLAUDE_SCOPES_SETUP: &str = "user:inference";

/// Claude OAuth 凭证存储
///
/// 与 CLIProxyAPI 的 ClaudeTokenStorage 格式兼容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeOAuthCredentials {
    /// 访问令牌
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    /// 刷新令牌
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// 用户邮箱
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// 过期时间（RFC3339 格式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire: Option<String>,
    /// 最后刷新时间（RFC3339 格式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<String>,
    /// 凭证类型标识
    #[serde(default = "default_claude_type", rename = "type")]
    pub cred_type: String,
}

fn default_claude_type() -> String {
    "claude_oauth".to_string()
}

impl Default for ClaudeOAuthCredentials {
    fn default() -> Self {
        Self {
            access_token: None,
            refresh_token: None,
            email: None,
            expire: None,
            last_refresh: None,
            cred_type: default_claude_type(),
        }
    }
}

/// PKCE codes for OAuth2 authorization
#[derive(Debug, Clone)]
pub struct PKCECodes {
    /// Cryptographically random string for code verification
    pub code_verifier: String,
    /// SHA256 hash of code_verifier, base64url-encoded
    pub code_challenge: String,
}

impl PKCECodes {
    /// Generate new PKCE codes
    pub fn generate() -> Result<Self, Box<dyn Error + Send + Sync>> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        use rand::RngCore;
        use sha2::{Digest, Sha256};

        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        let code_verifier = URL_SAFE_NO_PAD.encode(bytes);

        let mut hasher = Sha256::new();
        hasher.update(code_verifier.as_bytes());
        let hash = hasher.finalize();
        let code_challenge = URL_SAFE_NO_PAD.encode(hash);

        Ok(Self {
            code_verifier,
            code_challenge,
        })
    }
}

/// Claude OAuth Provider
///
/// 处理 Anthropic Claude 的 OAuth 认证和 API 调用
pub struct ClaudeOAuthProvider {
    /// OAuth 凭证
    pub credentials: ClaudeOAuthCredentials,
    /// HTTP 客户端
    pub client: Client,
    /// 凭证文件路径
    pub creds_path: Option<PathBuf>,
}

impl Default for ClaudeOAuthProvider {
    fn default() -> Self {
        Self {
            credentials: ClaudeOAuthCredentials::default(),
            client: Client::new(),
            creds_path: None,
        }
    }
}

impl ClaudeOAuthProvider {
    /// 创建新的 ClaudeOAuthProvider 实例
    pub fn new() -> Self {
        Self::default()
    }

    /// 使用自定义 HTTP 客户端创建
    pub fn with_client(client: Client) -> Self {
        Self {
            client,
            ..Self::default()
        }
    }

    /// 获取默认凭证文件路径
    pub fn default_creds_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("oauth_creds.json")
    }

    /// 从默认路径加载凭证
    pub async fn load_credentials(&mut self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = Self::default_creds_path();
        self.load_credentials_from_path_internal(&path).await
    }

    /// 从指定路径加载凭证
    pub async fn load_credentials_from_path(
        &mut self,
        path: &str,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = PathBuf::from(path);
        self.load_credentials_from_path_internal(&path).await
    }

    async fn load_credentials_from_path_internal(
        &mut self,
        path: &PathBuf,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if tokio::fs::try_exists(&path).await.unwrap_or(false) {
            let content = tokio::fs::read_to_string(&path).await?;
            let creds: ClaudeOAuthCredentials = serde_json::from_str(&content)?;
            tracing::info!(
                "[CLAUDE_OAUTH] 凭证已加载: has_access={}, has_refresh={}, email={:?}",
                creds.access_token.is_some(),
                creds.refresh_token.is_some(),
                creds.email
            );
            self.credentials = creds;
            self.creds_path = Some(path.clone());
        } else {
            tracing::warn!("[CLAUDE_OAUTH] 凭证文件不存在: {:?}", path);
        }
        Ok(())
    }

    /// 保存凭证到文件
    pub async fn save_credentials(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self
            .creds_path
            .clone()
            .unwrap_or_else(Self::default_creds_path);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(&self.credentials)?;
        tokio::fs::write(&path, content).await?;
        tracing::info!("[CLAUDE_OAUTH] 凭证已保存到 {:?}", path);
        Ok(())
    }

    /// 检查 Token 是否有效
    pub fn is_token_valid(&self) -> bool {
        if self.credentials.access_token.is_none() {
            return false;
        }

        if let Some(expire_str) = &self.credentials.expire {
            if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(expire_str) {
                let now = chrono::Utc::now();
                return expires > now + chrono::Duration::minutes(5);
            }
        }

        true
    }

    /// 刷新 Token - 与 CLIProxyAPI 对齐，使用 JSON 格式
    pub async fn refresh_token(&mut self) -> Result<String, Box<dyn Error + Send + Sync>> {
        let refresh_token = self
            .credentials
            .refresh_token
            .as_ref()
            .ok_or_else(|| create_config_error("没有可用的 refresh_token"))?;

        tracing::info!("[CLAUDE_OAUTH] 正在刷新 Token");

        // 与 CLIProxyAPI 对齐：使用 JSON 格式请求体
        let body = serde_json::json!({
            "client_id": CLAUDE_CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token
        });

        let resp = self
            .client
            .post(CLAUDE_TOKEN_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| Box::new(ProviderError::from(e)) as Box<dyn Error + Send + Sync>)?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!("[CLAUDE_OAUTH] Token 刷新失败: {} - {}", status, body);
            self.mark_invalid();
            return Err(create_token_refresh_error(status, &body, "CLAUDE_OAUTH"));
        }

        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Box::new(ProviderError::from(e)) as Box<dyn Error + Send + Sync>)?;

        let new_access_token = data["access_token"]
            .as_str()
            .ok_or_else(|| create_auth_error("响应中没有 access_token"))?
            .to_string();

        self.credentials.access_token = Some(new_access_token.clone());

        if let Some(rt) = data["refresh_token"].as_str() {
            self.credentials.refresh_token = Some(rt.to_string());
        }

        // 从响应中提取用户邮箱
        if let Some(email) = data["account"]["email_address"].as_str() {
            self.credentials.email = Some(email.to_string());
        }

        // 更新过期时间
        let expires_in = data["expires_in"].as_i64().unwrap_or(3600);
        let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in);
        self.credentials.expire = Some(expires_at.to_rfc3339());
        self.credentials.last_refresh = Some(chrono::Utc::now().to_rfc3339());

        self.save_credentials().await?;

        tracing::info!("[CLAUDE_OAUTH] Token 刷新成功");
        Ok(new_access_token)
    }

    /// 带重试机制的 Token 刷新
    pub async fn refresh_token_with_retry(
        &mut self,
        max_retries: u32,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        let mut last_error = None;

        for attempt in 0..max_retries {
            if attempt > 0 {
                let delay = std::time::Duration::from_secs(1 << attempt);
                tracing::info!("[CLAUDE_OAUTH] 第 {} 次重试，等待 {:?}", attempt + 1, delay);
                tokio::time::sleep(delay).await;
            }

            match self.refresh_token().await {
                Ok(token) => return Ok(token),
                Err(e) => {
                    tracing::warn!(
                        "[CLAUDE_OAUTH] Token 刷新第 {} 次尝试失败: {}",
                        attempt + 1,
                        e
                    );
                    last_error = Some(e);
                }
            }
        }

        self.mark_invalid();
        tracing::error!("[CLAUDE_OAUTH] Token 刷新在 {} 次尝试后失败", max_retries);
        Err(last_error.unwrap_or_else(|| create_auth_error("Token 刷新失败，请重新登录")))
    }

    /// 确保 Token 有效，必要时自动刷新
    pub async fn ensure_valid_token(&mut self) -> Result<String, Box<dyn Error + Send + Sync>> {
        if !self.is_token_valid() {
            tracing::info!("[CLAUDE_OAUTH] Token 需要刷新");
            self.refresh_token_with_retry(3).await
        } else {
            self.credentials
                .access_token
                .clone()
                .ok_or_else(|| create_config_error("没有可用的 access_token"))
        }
    }

    /// 标记凭证为无效
    pub fn mark_invalid(&mut self) {
        tracing::warn!("[CLAUDE_OAUTH] 标记凭证为无效");
        self.credentials.access_token = None;
        self.credentials.expire = None;
    }

    /// 获取 OAuth 授权 URL
    pub fn get_auth_url(&self) -> &'static str {
        CLAUDE_AUTH_URL
    }

    /// 获取 OAuth Token URL
    pub fn get_token_url(&self) -> &'static str {
        CLAUDE_TOKEN_URL
    }

    /// 获取 OAuth Client ID
    pub fn get_client_id(&self) -> &'static str {
        CLAUDE_CLIENT_ID
    }

    /// 获取 redirect URI（官方 Anthropic 回调地址）
    pub fn get_redirect_uri(&self) -> &'static str {
        CLAUDE_REDIRECT_URI
    }

    /// 获取 OAuth scopes
    pub fn get_scopes(&self) -> &'static str {
        CLAUDE_SCOPES
    }
}

// ============================================================================
// OAuth 登录功能
// ============================================================================

use uuid::Uuid;

/// OAuth 登录成功后的凭证信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeOAuthResult {
    pub credentials: ClaudeOAuthCredentials,
    pub creds_file_path: String,
}

/// 生成 Claude OAuth 授权 URL（使用官方 redirect_uri）
///
/// 用户需要：
/// 1. 打开此 URL 进行授权
/// 2. 授权后从浏览器地址栏复制授权码
/// 3. 将授权码粘贴回应用
pub fn generate_claude_auth_url(state: &str, code_challenge: &str) -> String {
    let params = [
        ("code", "true"),
        ("client_id", CLAUDE_CLIENT_ID),
        ("response_type", "code"),
        ("redirect_uri", CLAUDE_REDIRECT_URI),
        ("scope", CLAUDE_SCOPES),
        ("state", state),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
    ];

    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    format!("{}?{}", CLAUDE_AUTH_URL, query)
}

/// 生成 Setup Token 授权 URL（只需要推理权限）
pub fn generate_claude_setup_token_auth_url(state: &str, code_challenge: &str) -> String {
    let params = [
        ("code", "true"),
        ("client_id", CLAUDE_CLIENT_ID),
        ("response_type", "code"),
        ("redirect_uri", CLAUDE_REDIRECT_URI),
        ("scope", CLAUDE_SCOPES_SETUP),
        ("state", state),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
    ];

    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    format!("{}?{}", CLAUDE_AUTH_URL, query)
}

/// 用授权码交换 Token（使用官方 redirect_uri）
pub async fn exchange_claude_code_for_token(
    client: &Client,
    code: &str,
    code_verifier: &str,
    state: &str,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    // 清理授权码，移除 URL 片段（与 claude-relay-service 一致）
    let cleaned_code = code.split('#').next().unwrap_or(code);
    let cleaned_code = cleaned_code.split('&').next().unwrap_or(cleaned_code);

    let body = serde_json::json!({
        "grant_type": "authorization_code",
        "client_id": CLAUDE_CLIENT_ID,
        "code": cleaned_code,
        "redirect_uri": CLAUDE_REDIRECT_URI,
        "code_verifier": code_verifier,
        "state": state
    });

    tracing::info!(
        "[CLAUDE_OAUTH] 正在交换授权码，code 长度: {}",
        cleaned_code.len()
    );

    let resp = client
        .post(CLAUDE_TOKEN_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", "claude-cli/1.0.56 (external, cli)")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!("[CLAUDE_OAUTH] Token 交换失败: {} - {}", status, body);
        return Err(format!("Token 交换失败: {} - {}", status, body).into());
    }

    let data: serde_json::Value = resp.json().await?;
    tracing::info!("[CLAUDE_OAUTH] Token 交换成功");
    Ok(data)
}

/// OAuth 参数（用于手动授权码流程）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeOAuthParams {
    /// 授权 URL
    pub auth_url: String,
    /// PKCE code_verifier（需要保存用于后续交换 token）
    pub code_verifier: String,
    /// state 参数
    pub state: String,
    /// code_challenge
    pub code_challenge: String,
}

/// 生成 OAuth 授权参数（不启动服务器）
///
/// 返回授权 URL 和 PKCE 参数，用户需要：
/// 1. 打开 auth_url 进行授权
/// 2. 授权后从页面复制授权码
/// 3. 调用 exchange_claude_authorization_code 交换 token
pub fn generate_claude_oauth_params() -> Result<ClaudeOAuthParams, Box<dyn Error + Send + Sync>> {
    let pkce_codes = PKCECodes::generate()?;
    let state = Uuid::new_v4().to_string();

    let auth_url = generate_claude_auth_url(&state, &pkce_codes.code_challenge);

    tracing::info!(
        "[CLAUDE_OAUTH] 生成授权参数，state: {}, auth_url: {}",
        state,
        auth_url
    );

    Ok(ClaudeOAuthParams {
        auth_url,
        code_verifier: pkce_codes.code_verifier,
        state,
        code_challenge: pkce_codes.code_challenge,
    })
}

/// 生成 Setup Token 授权参数
pub fn generate_claude_setup_token_params(
) -> Result<ClaudeOAuthParams, Box<dyn Error + Send + Sync>> {
    let pkce_codes = PKCECodes::generate()?;
    let state = Uuid::new_v4().to_string();

    let auth_url = generate_claude_setup_token_auth_url(&state, &pkce_codes.code_challenge);

    tracing::info!("[CLAUDE_OAUTH] 生成 Setup Token 授权参数，state: {}", state);

    Ok(ClaudeOAuthParams {
        auth_url,
        code_verifier: pkce_codes.code_verifier,
        state,
        code_challenge: pkce_codes.code_challenge,
    })
}

/// 解析授权码（支持完整 URL 或直接授权码）
pub fn parse_claude_authorization_code(
    input: &str,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let trimmed = input.trim();

    // 情况1: 完整 URL
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        if let Ok(url) = url::Url::parse(trimmed) {
            if let Some(code) = url
                .query_pairs()
                .find(|(k, _)| k == "code")
                .map(|(_, v)| v.to_string())
            {
                return Ok(code);
            }
        }
        return Err("回调 URL 中未找到授权码 (code 参数)".into());
    }

    // 情况2: 直接授权码（可能包含 URL fragments）
    let cleaned = trimmed.split('#').next().unwrap_or(trimmed);
    let cleaned = cleaned.split('&').next().unwrap_or(cleaned);

    if cleaned.len() < 10 {
        return Err("授权码格式无效，请确保复制了完整的授权码".into());
    }

    Ok(cleaned.to_string())
}

/// 使用授权码交换 Token 并保存凭证
pub async fn exchange_claude_authorization_code(
    authorization_code: &str,
    code_verifier: &str,
    state: &str,
) -> Result<ClaudeOAuthResult, Box<dyn Error + Send + Sync>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // 解析授权码
    let code = parse_claude_authorization_code(authorization_code)?;

    // 交换 Token
    let token_data = exchange_claude_code_for_token(&client, &code, code_verifier, state).await?;

    let access_token = token_data["access_token"].as_str().unwrap_or_default();
    let refresh_token = token_data["refresh_token"].as_str().map(|s| s.to_string());
    let expires_in = token_data["expires_in"].as_i64();

    // 从响应中提取用户邮箱
    let email = token_data["account"]["email_address"]
        .as_str()
        .map(|s| s.to_string());

    // 构建凭证
    let now = chrono::Utc::now();
    let credentials = ClaudeOAuthCredentials {
        access_token: Some(access_token.to_string()),
        refresh_token,
        email: email.clone(),
        expire: expires_in.map(|e| (now + chrono::Duration::seconds(e)).to_rfc3339()),
        last_refresh: Some(now.to_rfc3339()),
        cred_type: "claude_oauth".to_string(),
    };

    // 保存凭证到应用数据目录
    let creds_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("proxycast")
        .join("credentials")
        .join("claude_oauth");

    std::fs::create_dir_all(&creds_dir)?;

    // 生成唯一文件名
    let uuid = Uuid::new_v4().to_string();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let filename = format!("claude_oauth_{}_{}.json", &uuid[..8], timestamp);
    let creds_file_path = creds_dir.join(&filename);

    // 保存凭证
    let creds_json = serde_json::to_string_pretty(&credentials)?;
    std::fs::write(&creds_file_path, &creds_json)?;

    tracing::info!(
        "[CLAUDE_OAUTH] 凭证已保存到: {:?}, email: {:?}",
        creds_file_path,
        email
    );

    Ok(ClaudeOAuthResult {
        credentials,
        creds_file_path: creds_file_path.to_string_lossy().to_string(),
    })
}

/// 启动 Claude OAuth 登录流程（打开浏览器，返回授权参数）
///
/// 新流程：
/// 1. 生成授权参数
/// 2. 打开浏览器
/// 3. 返回参数供后续使用（用户需手动输入授权码）
pub async fn start_claude_oauth_login() -> Result<ClaudeOAuthParams, Box<dyn Error + Send + Sync>> {
    let params = generate_claude_oauth_params()?;

    tracing::info!("[CLAUDE_OAUTH] 打开浏览器进行授权: {}", params.auth_url);

    // 打开浏览器
    if let Err(e) = open::that(&params.auth_url) {
        tracing::warn!(
            "[CLAUDE_OAUTH] 无法打开浏览器: {}. 请手动打开 URL: {}",
            e,
            params.auth_url
        );
    }

    Ok(params)
}

// ============================================================================
// Cookie 自动授权功能（参考 claude-relay-service 实现）
// ============================================================================

/// Cookie 自动授权配置
const CLAUDE_AI_URL: &str = "https://claude.ai";
const CLAUDE_ORGANIZATIONS_URL: &str = "https://claude.ai/api/organizations";

/// 组织信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInfo {
    pub uuid: String,
    pub capabilities: Vec<String>,
}

/// Cookie 自动授权结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieOAuthResult {
    pub credentials: ClaudeOAuthCredentials,
    pub creds_file_path: String,
    pub organization_uuid: Option<String>,
    pub capabilities: Vec<String>,
}

/// 构建带 Cookie 的请求头
fn build_cookie_headers(session_key: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Accept", "application/json".parse().unwrap());
    headers.insert("Accept-Language", "en-US,en;q=0.9".parse().unwrap());
    headers.insert("Cache-Control", "no-cache".parse().unwrap());
    headers.insert(
        "Cookie",
        format!("sessionKey={}", session_key).parse().unwrap(),
    );
    headers.insert("Origin", CLAUDE_AI_URL.parse().unwrap());
    headers.insert("Referer", format!("{}/new", CLAUDE_AI_URL).parse().unwrap());
    headers.insert(
        "User-Agent",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36"
            .parse()
            .unwrap(),
    );
    headers
}

/// 使用 Cookie 获取组织信息
async fn get_organization_info(
    client: &Client,
    session_key: &str,
) -> Result<OrganizationInfo, Box<dyn Error + Send + Sync>> {
    let headers = build_cookie_headers(session_key);

    tracing::info!("[CLAUDE_OAUTH] 使用 Cookie 获取组织信息");

    let resp = client
        .get(CLAUDE_ORGANIZATIONS_URL)
        .headers(headers)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        if status.as_u16() == 403 || status.as_u16() == 401 {
            return Err("Cookie 授权失败：无效的 sessionKey 或已过期".into());
        }
        if status.as_u16() == 302 {
            return Err("请求被 Cloudflare 拦截，请稍后重试".into());
        }
        return Err(format!("获取组织信息失败：HTTP {}", status).into());
    }

    let data: serde_json::Value = resp.json().await?;

    if !data.is_array() {
        return Err("获取组织信息失败：响应格式无效".into());
    }

    let orgs = data.as_array().unwrap();

    // 找到具有 chat 能力且能力最多的组织
    let mut best_org: Option<OrganizationInfo> = None;
    let mut max_capabilities = 0;

    for org in orgs {
        let capabilities: Vec<String> = org["capabilities"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // 必须有 chat 能力
        if !capabilities.contains(&"chat".to_string()) {
            continue;
        }

        // 选择能力最多的组织
        if capabilities.len() > max_capabilities {
            if let Some(uuid) = org["uuid"].as_str() {
                best_org = Some(OrganizationInfo {
                    uuid: uuid.to_string(),
                    capabilities: capabilities.clone(),
                });
                max_capabilities = capabilities.len();
            }
        }
    }

    best_org.ok_or_else(|| "未找到具有 chat 能力的组织".into())
}

/// 使用 Cookie 自动获取授权码
async fn authorize_with_cookie(
    client: &Client,
    session_key: &str,
    organization_uuid: &str,
    scope: &str,
) -> Result<(String, String, String), Box<dyn Error + Send + Sync>> {
    // 生成 PKCE 参数
    let pkce_codes = PKCECodes::generate()?;
    let state = Uuid::new_v4().to_string();

    // 构建授权 URL
    let authorize_url = format!("https://claude.ai/v1/oauth/{}/authorize", organization_uuid);

    // 构建请求 payload
    let payload = serde_json::json!({
        "response_type": "code",
        "client_id": CLAUDE_CLIENT_ID,
        "organization_uuid": organization_uuid,
        "redirect_uri": CLAUDE_REDIRECT_URI,
        "scope": scope,
        "state": state,
        "code_challenge": pkce_codes.code_challenge,
        "code_challenge_method": "S256"
    });

    let mut headers = build_cookie_headers(session_key);
    headers.insert("Content-Type", "application/json".parse().unwrap());

    tracing::info!("[CLAUDE_OAUTH] 使用 Cookie 请求授权，scope: {}", scope);

    let resp = client
        .post(&authorize_url)
        .headers(headers)
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        if status.as_u16() == 403 || status.as_u16() == 401 {
            return Err("Cookie 授权失败：无效的 sessionKey 或已过期".into());
        }
        if status.as_u16() == 302 {
            return Err("请求被 Cloudflare 拦截，请稍后重试".into());
        }
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("授权请求失败：HTTP {} - {}", status, body).into());
    }

    let data: serde_json::Value = resp.json().await?;

    // 从响应中获取 redirect_uri
    let redirect_uri = data["redirect_uri"]
        .as_str()
        .ok_or("授权响应中未找到 redirect_uri")?;

    tracing::info!(
        "[CLAUDE_OAUTH] 获取到 redirect_uri: {}...",
        &redirect_uri[..redirect_uri.len().min(80)]
    );

    // 解析 redirect_uri 获取授权码
    let url = url::Url::parse(redirect_uri)?;
    let authorization_code = url
        .query_pairs()
        .find(|(k, _)| k == "code")
        .map(|(_, v)| v.to_string())
        .ok_or("redirect_uri 中未找到授权码")?;

    tracing::info!(
        "[CLAUDE_OAUTH] 通过 Cookie 获取授权码成功，长度: {}",
        authorization_code.len()
    );

    Ok((authorization_code, pkce_codes.code_verifier, state))
}

/// 完整的 Cookie 自动授权流程
///
/// 参考 claude-relay-service 的 oauthWithCookie 实现
///
/// # 参数
/// - `session_key`: 从浏览器 Cookie 中获取的 sessionKey
/// - `is_setup_token`: 是否为 Setup Token 模式（只需要推理权限）
///
/// # 返回
/// - 成功时返回凭证信息和组织信息
pub async fn oauth_with_cookie(
    session_key: &str,
    is_setup_token: bool,
) -> Result<CookieOAuthResult, Box<dyn Error + Send + Sync>> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none()) // 禁止自动重定向
        .build()?;

    tracing::info!(
        "[CLAUDE_OAUTH] 开始 Cookie 自动授权流程，is_setup_token: {}",
        is_setup_token
    );

    // 步骤1：获取组织信息
    tracing::info!("[CLAUDE_OAUTH] 步骤 1/3: 获取组织信息...");
    let org_info = get_organization_info(&client, session_key).await?;
    tracing::info!(
        "[CLAUDE_OAUTH] 找到组织: uuid={}, capabilities={:?}",
        org_info.uuid,
        org_info.capabilities
    );

    // 步骤2：确定 scope 并获取授权码
    let scope = if is_setup_token {
        CLAUDE_SCOPES_SETUP
    } else {
        "user:profile user:inference"
    };

    tracing::info!("[CLAUDE_OAUTH] 步骤 2/3: 获取授权码...");
    let (authorization_code, code_verifier, state) =
        authorize_with_cookie(&client, session_key, &org_info.uuid, scope).await?;

    // 步骤3：交换 Token
    tracing::info!("[CLAUDE_OAUTH] 步骤 3/3: 交换 Token...");
    let token_data =
        exchange_claude_code_for_token(&client, &authorization_code, &code_verifier, &state)
            .await?;

    let access_token = token_data["access_token"].as_str().unwrap_or_default();
    let refresh_token = if is_setup_token {
        None // Setup Token 没有 refresh_token
    } else {
        token_data["refresh_token"].as_str().map(|s| s.to_string())
    };
    let expires_in = token_data["expires_in"].as_i64();

    // 从响应中提取用户邮箱
    let email = token_data["account"]["email_address"]
        .as_str()
        .map(|s| s.to_string());

    // 构建凭证
    let now = chrono::Utc::now();
    let credentials = ClaudeOAuthCredentials {
        access_token: Some(access_token.to_string()),
        refresh_token,
        email: email.clone(),
        expire: expires_in.map(|e| (now + chrono::Duration::seconds(e)).to_rfc3339()),
        last_refresh: Some(now.to_rfc3339()),
        cred_type: if is_setup_token {
            "claude_setup_token".to_string()
        } else {
            "claude_oauth".to_string()
        },
    };

    // 保存凭证到应用数据目录
    let creds_dir = dirs::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("proxycast")
        .join("credentials")
        .join("claude_oauth");

    std::fs::create_dir_all(&creds_dir)?;

    // 生成唯一文件名
    let uuid = Uuid::new_v4().to_string();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token_type = if is_setup_token { "setup" } else { "oauth" };
    let filename = format!("claude_{}_{}_{}.json", token_type, &uuid[..8], timestamp);
    let creds_file_path = creds_dir.join(&filename);

    // 保存凭证
    let creds_json = serde_json::to_string_pretty(&credentials)?;
    std::fs::write(&creds_file_path, &creds_json)?;

    tracing::info!(
        "[CLAUDE_OAUTH] Cookie 自动授权成功，凭证已保存到: {:?}, email: {:?}",
        creds_file_path,
        email
    );

    Ok(CookieOAuthResult {
        credentials,
        creds_file_path: creds_file_path.to_string_lossy().to_string(),
        organization_uuid: Some(org_info.uuid),
        capabilities: org_info.capabilities,
    })
}
