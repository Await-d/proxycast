//! Vertex AI Provider
//!
//! Provides API key authentication for Google Vertex AI models.
//! Supports model alias mappings and load balancing across multiple credentials.

#![allow(dead_code)]

use crate::config::VertexApiKeyEntry;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;

/// Default Vertex AI base URL
const DEFAULT_VERTEX_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Vertex AI supported models
#[allow(dead_code)]
pub const VERTEX_MODELS: &[&str] = &[
    "gemini-2.0-flash",
    "gemini-2.0-flash-lite",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
];

/// Vertex AI Provider configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VertexConfig {
    /// API Key
    pub api_key: Option<String>,
    /// Base URL
    pub base_url: Option<String>,
    /// Whether the provider is enabled
    pub enabled: bool,
    /// Model alias mappings (alias -> upstream model name)
    #[serde(default)]
    pub model_aliases: HashMap<String, String>,
    /// Per-key proxy URL
    pub proxy_url: Option<String>,
}

/// Vertex AI Provider
///
/// Handles API key authentication and model alias resolution for Vertex AI.
pub struct VertexProvider {
    /// Provider configuration
    pub config: VertexConfig,
    /// HTTP client
    pub client: Client,
}

impl Default for VertexProvider {
    fn default() -> Self {
        Self {
            config: VertexConfig::default(),
            client: Client::new(),
        }
    }
}

impl VertexProvider {
    /// Create a new Vertex AI provider
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a provider with API key and optional base URL
    pub fn with_config(api_key: String, base_url: Option<String>) -> Self {
        Self {
            config: VertexConfig {
                api_key: Some(api_key),
                base_url,
                enabled: true,
                model_aliases: HashMap::new(),
                proxy_url: None,
            },
            client: Client::new(),
        }
    }

    /// Create a provider from a VertexApiKeyEntry configuration
    pub fn from_entry(entry: &VertexApiKeyEntry) -> Self {
        let mut model_aliases = HashMap::new();
        for alias_mapping in &entry.models {
            model_aliases.insert(alias_mapping.alias.clone(), alias_mapping.name.clone());
        }

        Self {
            config: VertexConfig {
                api_key: Some(entry.api_key.clone()),
                base_url: entry.base_url.clone(),
                enabled: !entry.disabled,
                model_aliases,
                proxy_url: entry.proxy_url.clone(),
            },
            client: Client::new(),
        }
    }

    /// Create a provider with a custom HTTP client (for proxy support)
    pub fn with_client(mut self, client: Client) -> Self {
        self.client = client;
        self
    }

    /// Add a model alias mapping
    pub fn with_model_alias(mut self, alias: &str, model: &str) -> Self {
        self.config
            .model_aliases
            .insert(alias.to_string(), model.to_string());
        self
    }

    /// Set proxy URL
    pub fn with_proxy(mut self, proxy_url: Option<String>) -> Self {
        self.config.proxy_url = proxy_url;
        self
    }

    /// Get the base URL for API requests
    pub fn get_base_url(&self) -> String {
        self.config
            .base_url
            .clone()
            .unwrap_or_else(|| DEFAULT_VERTEX_BASE_URL.to_string())
    }

    /// Get the API key
    pub fn get_api_key(&self) -> Option<&str> {
        self.config.api_key.as_deref()
    }

    /// Check if the provider is properly configured
    pub fn is_configured(&self) -> bool {
        self.config.api_key.is_some() && self.config.enabled
    }

    /// Resolve a model alias to the upstream model name
    ///
    /// If the model is an alias, returns the mapped upstream model name.
    /// Otherwise, returns the original model name.
    pub fn resolve_model_alias(&self, model: &str) -> String {
        self.config
            .model_aliases
            .get(model)
            .cloned()
            .unwrap_or_else(|| model.to_string())
    }

    /// Check if a model name is an alias
    pub fn is_alias(&self, model: &str) -> bool {
        self.config.model_aliases.contains_key(model)
    }

    /// Get all configured model aliases
    pub fn get_model_aliases(&self) -> &HashMap<String, String> {
        &self.config.model_aliases
    }

    /// Call the Vertex AI chat completions API
    ///
    /// Automatically injects the x-goog-api-key header and resolves model aliases.
    pub async fn chat_completions(
        &self,
        request: &serde_json::Value,
    ) -> Result<reqwest::Response, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Vertex AI API key not configured")?;

        // Resolve model alias if present
        let mut request = request.clone();
        if let Some(model) = request.get("model").and_then(|m| m.as_str()) {
            let resolved_model = self.resolve_model_alias(model);
            request["model"] = serde_json::json!(resolved_model);
        }

        let base_url = self.get_base_url();
        let model = request
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("gemini-2.0-flash");

        // Vertex AI uses a different URL pattern
        let url = format!("{}/models/{}:generateContent", base_url, model);

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        Ok(resp)
    }

    /// Call the Vertex AI streaming chat completions API
    pub async fn chat_completions_stream(
        &self,
        request: &serde_json::Value,
    ) -> Result<reqwest::Response, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Vertex AI API key not configured")?;

        // Resolve model alias if present
        let mut request = request.clone();
        if let Some(model) = request.get("model").and_then(|m| m.as_str()) {
            let resolved_model = self.resolve_model_alias(model);
            request["model"] = serde_json::json!(resolved_model);
        }

        let base_url = self.get_base_url();
        let model = request
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("gemini-2.0-flash");

        // Streaming endpoint
        let url = format!("{}/models/{}:streamGenerateContent", base_url, model);

        let resp = self
            .client
            .post(&url)
            .header("x-goog-api-key", api_key)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await?;

        Ok(resp)
    }

    /// List available models
    pub async fn list_models(&self) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or("Vertex AI API key not configured")?;

        let base_url = self.get_base_url();
        let url = format!("{}/models", base_url);

        let resp = self
            .client
            .get(&url)
            .header("x-goog-api-key", api_key)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Failed to list models: {} - {}", status, body).into());
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vertex_provider_new() {
        let provider = VertexProvider::new();
        assert!(!provider.is_configured());
        assert_eq!(provider.get_base_url(), DEFAULT_VERTEX_BASE_URL);
    }

    #[test]
    fn test_vertex_provider_with_config() {
        let provider = VertexProvider::with_config(
            "test-api-key".to_string(),
            Some("https://custom.api.com".to_string()),
        );
        assert!(provider.is_configured());
        assert_eq!(provider.get_api_key(), Some("test-api-key"));
        assert_eq!(provider.get_base_url(), "https://custom.api.com");
    }

    #[test]
    fn test_vertex_provider_from_entry() {
        use crate::config::VertexModelAlias;

        let entry = VertexApiKeyEntry {
            id: "test-vertex".to_string(),
            api_key: "vk-test-key".to_string(),
            base_url: Some("https://vertex.example.com".to_string()),
            models: vec![
                VertexModelAlias {
                    name: "gemini-2.0-flash".to_string(),
                    alias: "vertex-flash".to_string(),
                },
                VertexModelAlias {
                    name: "gemini-2.5-pro".to_string(),
                    alias: "vertex-pro".to_string(),
                },
            ],
            proxy_url: Some("http://proxy:8080".to_string()),
            disabled: false,
        };

        let provider = VertexProvider::from_entry(&entry);
        assert!(provider.is_configured());
        assert_eq!(provider.get_api_key(), Some("vk-test-key"));
        assert_eq!(provider.get_base_url(), "https://vertex.example.com");
        assert_eq!(
            provider.config.proxy_url,
            Some("http://proxy:8080".to_string())
        );
    }

    #[test]
    fn test_model_alias_resolution() {
        let provider = VertexProvider::with_config("test-key".to_string(), None)
            .with_model_alias("vertex-flash", "gemini-2.0-flash")
            .with_model_alias("vertex-pro", "gemini-2.5-pro");

        // Alias should resolve to upstream model
        assert_eq!(
            provider.resolve_model_alias("vertex-flash"),
            "gemini-2.0-flash"
        );
        assert_eq!(provider.resolve_model_alias("vertex-pro"), "gemini-2.5-pro");

        // Non-alias should return as-is
        assert_eq!(
            provider.resolve_model_alias("gemini-2.0-flash"),
            "gemini-2.0-flash"
        );
        assert_eq!(
            provider.resolve_model_alias("unknown-model"),
            "unknown-model"
        );
    }

    #[test]
    fn test_is_alias() {
        let provider = VertexProvider::with_config("test-key".to_string(), None)
            .with_model_alias("vertex-flash", "gemini-2.0-flash");

        assert!(provider.is_alias("vertex-flash"));
        assert!(!provider.is_alias("gemini-2.0-flash"));
        assert!(!provider.is_alias("unknown"));
    }

    #[test]
    fn test_get_model_aliases() {
        let provider = VertexProvider::with_config("test-key".to_string(), None)
            .with_model_alias("alias1", "model1")
            .with_model_alias("alias2", "model2");

        let aliases = provider.get_model_aliases();
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases.get("alias1"), Some(&"model1".to_string()));
        assert_eq!(aliases.get("alias2"), Some(&"model2".to_string()));
    }

    #[test]
    fn test_disabled_provider() {
        let entry = VertexApiKeyEntry {
            id: "disabled-vertex".to_string(),
            api_key: "vk-test-key".to_string(),
            base_url: None,
            models: vec![],
            proxy_url: None,
            disabled: true,
        };

        let provider = VertexProvider::from_entry(&entry);
        assert!(!provider.is_configured()); // disabled = true means not configured
    }
}
