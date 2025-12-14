use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub providers: ProvidersConfig,
    pub default_provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvidersConfig {
    pub kiro: ProviderConfig,
    pub gemini: ProviderConfig,
    pub qwen: ProviderConfig,
    pub openai: CustomProviderConfig,
    pub claude: CustomProviderConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub enabled: bool,
    pub credentials_path: Option<String>,
    pub region: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomProviderConfig {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8999,
                api_key: "proxy_cast".to_string(),
            },
            providers: ProvidersConfig {
                kiro: ProviderConfig {
                    enabled: true,
                    credentials_path: Some("~/.aws/sso/cache/kiro-auth-token.json".to_string()),
                    region: Some("us-east-1".to_string()),
                    project_id: None,
                },
                gemini: ProviderConfig {
                    enabled: false,
                    credentials_path: Some("~/.gemini/oauth_creds.json".to_string()),
                    region: None,
                    project_id: None,
                },
                qwen: ProviderConfig {
                    enabled: false,
                    credentials_path: Some("~/.qwen/oauth_creds.json".to_string()),
                    region: None,
                    project_id: None,
                },
                openai: CustomProviderConfig {
                    enabled: false,
                    api_key: None,
                    base_url: Some("https://api.openai.com/v1".to_string()),
                },
                claude: CustomProviderConfig {
                    enabled: false,
                    api_key: None,
                    base_url: Some("https://api.anthropic.com".to_string()),
                },
            },
            default_provider: "kiro".to_string(),
        }
    }
}

fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("proxycast")
        .join("config.json")
}

pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let path = config_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        Ok(Config::default())
    }
}

pub fn save_config(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, content)?;
    Ok(())
}
