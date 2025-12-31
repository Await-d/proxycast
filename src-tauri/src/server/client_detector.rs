//! 客户端类型检测模块
//!
//! 通过解析 HTTP 请求的 User-Agent 头来识别客户端类型。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// 客户端类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientType {
    /// Cursor 编辑器
    Cursor,
    /// Claude Code 客户端
    ClaudeCode,
    /// OpenAI Codex CLI
    Codex,
    /// Windsurf 编辑器
    Windsurf,
    /// Kiro IDE
    Kiro,
    /// 未识别的客户端
    Other,
}

impl ClientType {
    /// 从 User-Agent 字符串检测客户端类型
    ///
    /// 支持大小写不敏感匹配。
    ///
    /// # 参数
    /// - `user_agent`: HTTP 请求的 User-Agent 头值
    ///
    /// # 返回
    /// 检测到的客户端类型
    ///
    /// # 示例
    /// ```ignore
    /// use proxycast_lib::server::client_detector::ClientType;
    ///
    /// assert_eq!(ClientType::from_user_agent("Cursor/1.0"), ClientType::Cursor);
    /// assert_eq!(ClientType::from_user_agent("claude-code/2.0"), ClientType::ClaudeCode);
    /// assert_eq!(ClientType::from_user_agent("Unknown"), ClientType::Other);
    /// ```
    pub fn from_user_agent(user_agent: &str) -> Self {
        let ua_lower = user_agent.to_lowercase();

        if ua_lower.contains("cursor") {
            ClientType::Cursor
        } else if ua_lower.contains("claude-code") || ua_lower.contains("claude_code") {
            ClientType::ClaudeCode
        } else if ua_lower.contains("codex") {
            ClientType::Codex
        } else if ua_lower.contains("windsurf") {
            ClientType::Windsurf
        } else if ua_lower.contains("kiro") {
            ClientType::Kiro
        } else {
            ClientType::Other
        }
    }

    /// 获取配置键名
    ///
    /// 返回用于配置文件中的键名。
    ///
    /// # 返回
    /// 配置键名字符串
    pub fn config_key(&self) -> &'static str {
        match self {
            ClientType::Cursor => "cursor",
            ClientType::ClaudeCode => "claude_code",
            ClientType::Codex => "codex",
            ClientType::Windsurf => "windsurf",
            ClientType::Kiro => "kiro",
            ClientType::Other => "other",
        }
    }

    /// 获取所有客户端类型
    ///
    /// 返回所有支持的客户端类型列表。
    pub fn all() -> &'static [ClientType] {
        &[
            ClientType::Cursor,
            ClientType::ClaudeCode,
            ClientType::Codex,
            ClientType::Windsurf,
            ClientType::Kiro,
            ClientType::Other,
        ]
    }

    /// 从配置键名解析客户端类型
    ///
    /// # 参数
    /// - `key`: 配置键名
    ///
    /// # 返回
    /// 如果键名有效，返回对应的客户端类型；否则返回 None
    pub fn from_config_key(key: &str) -> Option<Self> {
        match key {
            "cursor" => Some(ClientType::Cursor),
            "claude_code" => Some(ClientType::ClaudeCode),
            "codex" => Some(ClientType::Codex),
            "windsurf" => Some(ClientType::Windsurf),
            "kiro" => Some(ClientType::Kiro),
            "other" => Some(ClientType::Other),
            _ => None,
        }
    }
}

impl std::fmt::Display for ClientType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.config_key())
    }
}

/// 根据客户端类型和端点配置选择 Provider
///
/// **Validates: Requirements 1.3, 1.4, 3.4**
///
/// 优先级：端点 Provider 配置 > 默认 Provider
///
/// # 参数
/// - `client_type`: 检测到的客户端类型
/// - `endpoint_provider`: 端点配置中该客户端类型对应的 Provider（可选）
/// - `default_provider`: 默认 Provider
///
/// # 返回
/// 选择的 Provider 名称
pub fn select_provider(
    _client_type: ClientType,
    endpoint_provider: Option<&String>,
    default_provider: &str,
) -> String {
    match endpoint_provider {
        Some(provider) => provider.clone(),
        None => default_provider.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_user_agent_cursor() {
        assert_eq!(
            ClientType::from_user_agent("Cursor/1.0"),
            ClientType::Cursor
        );
        assert_eq!(ClientType::from_user_agent("cursor"), ClientType::Cursor);
        assert_eq!(ClientType::from_user_agent("CURSOR"), ClientType::Cursor);
        assert_eq!(
            ClientType::from_user_agent("Mozilla/5.0 Cursor"),
            ClientType::Cursor
        );
    }

    #[test]
    fn test_from_user_agent_claude_code() {
        assert_eq!(
            ClientType::from_user_agent("Claude-Code/2.0"),
            ClientType::ClaudeCode
        );
        assert_eq!(
            ClientType::from_user_agent("claude-code"),
            ClientType::ClaudeCode
        );
        assert_eq!(
            ClientType::from_user_agent("CLAUDE-CODE"),
            ClientType::ClaudeCode
        );
        assert_eq!(
            ClientType::from_user_agent("claude_code"),
            ClientType::ClaudeCode
        );
        assert_eq!(
            ClientType::from_user_agent("CLAUDE_CODE"),
            ClientType::ClaudeCode
        );
    }

    #[test]
    fn test_from_user_agent_codex() {
        assert_eq!(ClientType::from_user_agent("Codex/1.0"), ClientType::Codex);
        assert_eq!(ClientType::from_user_agent("codex"), ClientType::Codex);
        assert_eq!(ClientType::from_user_agent("CODEX"), ClientType::Codex);
    }

    #[test]
    fn test_from_user_agent_windsurf() {
        assert_eq!(
            ClientType::from_user_agent("Windsurf/1.0"),
            ClientType::Windsurf
        );
        assert_eq!(
            ClientType::from_user_agent("windsurf"),
            ClientType::Windsurf
        );
        assert_eq!(
            ClientType::from_user_agent("WINDSURF"),
            ClientType::Windsurf
        );
    }

    #[test]
    fn test_from_user_agent_kiro() {
        assert_eq!(ClientType::from_user_agent("Kiro/1.0"), ClientType::Kiro);
        assert_eq!(ClientType::from_user_agent("kiro"), ClientType::Kiro);
        assert_eq!(ClientType::from_user_agent("KIRO"), ClientType::Kiro);
    }

    #[test]
    fn test_from_user_agent_other() {
        assert_eq!(ClientType::from_user_agent("Unknown"), ClientType::Other);
        assert_eq!(ClientType::from_user_agent(""), ClientType::Other);
        assert_eq!(
            ClientType::from_user_agent("Mozilla/5.0"),
            ClientType::Other
        );
    }

    #[test]
    fn test_config_key() {
        assert_eq!(ClientType::Cursor.config_key(), "cursor");
        assert_eq!(ClientType::ClaudeCode.config_key(), "claude_code");
        assert_eq!(ClientType::Codex.config_key(), "codex");
        assert_eq!(ClientType::Windsurf.config_key(), "windsurf");
        assert_eq!(ClientType::Kiro.config_key(), "kiro");
        assert_eq!(ClientType::Other.config_key(), "other");
    }

    #[test]
    fn test_from_config_key() {
        assert_eq!(
            ClientType::from_config_key("cursor"),
            Some(ClientType::Cursor)
        );
        assert_eq!(
            ClientType::from_config_key("claude_code"),
            Some(ClientType::ClaudeCode)
        );
        assert_eq!(
            ClientType::from_config_key("codex"),
            Some(ClientType::Codex)
        );
        assert_eq!(
            ClientType::from_config_key("windsurf"),
            Some(ClientType::Windsurf)
        );
        assert_eq!(ClientType::from_config_key("kiro"), Some(ClientType::Kiro));
        assert_eq!(
            ClientType::from_config_key("other"),
            Some(ClientType::Other)
        );
        assert_eq!(ClientType::from_config_key("invalid"), None);
    }

    #[test]
    fn test_all_client_types() {
        let all = ClientType::all();
        assert_eq!(all.len(), 6);
        assert!(all.contains(&ClientType::Cursor));
        assert!(all.contains(&ClientType::ClaudeCode));
        assert!(all.contains(&ClientType::Codex));
        assert!(all.contains(&ClientType::Windsurf));
        assert!(all.contains(&ClientType::Kiro));
        assert!(all.contains(&ClientType::Other));
    }

    #[test]
    fn test_display() {
        assert_eq!(format!("{}", ClientType::Cursor), "cursor");
        assert_eq!(format!("{}", ClientType::ClaudeCode), "claude_code");
    }

    #[test]
    fn test_serialization() {
        let cursor = ClientType::Cursor;
        let json = serde_json::to_string(&cursor).unwrap();
        assert_eq!(json, "\"cursor\"");

        let claude_code = ClientType::ClaudeCode;
        let json = serde_json::to_string(&claude_code).unwrap();
        assert_eq!(json, "\"claude_code\"");
    }

    #[test]
    fn test_deserialization() {
        let cursor: ClientType = serde_json::from_str("\"cursor\"").unwrap();
        assert_eq!(cursor, ClientType::Cursor);

        let claude_code: ClientType = serde_json::from_str("\"claude_code\"").unwrap();
        assert_eq!(claude_code, ClientType::ClaudeCode);
    }
}

// ============================================================================
// Property 2: Provider 选择优先级属性测试
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::config::EndpointProvidersConfig;
    use proptest::prelude::*;

    /// 生成随机的客户端类型
    fn arb_client_type() -> impl Strategy<Value = ClientType> {
        prop_oneof![
            Just(ClientType::Cursor),
            Just(ClientType::ClaudeCode),
            Just(ClientType::Codex),
            Just(ClientType::Windsurf),
            Just(ClientType::Kiro),
            Just(ClientType::Other),
        ]
    }

    /// 生成随机的 Provider 名称
    fn arb_provider_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("kiro".to_string()),
            Just("gemini".to_string()),
            Just("qwen".to_string()),
            Just("openai".to_string()),
            Just("claude".to_string()),
            Just("codex".to_string()),
        ]
    }

    /// 生成可选的 Provider 名称
    fn arb_optional_provider() -> impl Strategy<Value = Option<String>> {
        prop_oneof![Just(None), arb_provider_name().prop_map(Some),]
    }

    /// 生成随机的 EndpointProvidersConfig
    fn arb_endpoint_providers_config() -> impl Strategy<Value = EndpointProvidersConfig> {
        (
            arb_optional_provider(),
            arb_optional_provider(),
            arb_optional_provider(),
            arb_optional_provider(),
            arb_optional_provider(),
            arb_optional_provider(),
        )
            .prop_map(|(cursor, claude_code, codex, windsurf, kiro, other)| {
                EndpointProvidersConfig {
                    cursor,
                    claude_code,
                    codex,
                    windsurf,
                    kiro,
                    other,
                }
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: endpoint-provider-config, Property 2: Provider 选择优先级**
        /// *对于任意* 客户端类型和配置：
        /// - 当 endpoint_providers[client_type] 有值时，应使用该 Provider
        /// - 当 endpoint_providers[client_type] 为空时，应使用 default_provider
        /// **Validates: Requirements 1.3, 1.4, 3.4**
        #[test]
        fn prop_provider_selection_priority(
            client_type in arb_client_type(),
            endpoint_config in arb_endpoint_providers_config(),
            default_provider in arb_provider_name()
        ) {
            // 获取端点配置中该客户端类型对应的 Provider
            let endpoint_provider = endpoint_config.get_provider(client_type.config_key());

            // 调用 select_provider 函数
            let selected = select_provider(client_type, endpoint_provider, &default_provider);

            // 验证选择逻辑
            match endpoint_provider {
                Some(provider) => {
                    // 当端点配置有值时，应使用端点配置的 Provider
                    prop_assert_eq!(
                        selected,
                        provider.clone(),
                        "当端点配置有值时，应使用端点配置的 Provider"
                    );
                }
                None => {
                    // 当端点配置为空时，应使用默认 Provider
                    prop_assert_eq!(
                        selected,
                        default_provider,
                        "当端点配置为空时，应使用默认 Provider"
                    );
                }
            }
        }

        /// **Feature: endpoint-provider-config, Property 2: Provider 选择优先级（端点配置优先）**
        /// *对于任意* 客户端类型，当端点配置有值时，应始终使用端点配置的 Provider，
        /// 而不是默认 Provider。
        /// **Validates: Requirements 1.3, 3.4**
        #[test]
        fn prop_endpoint_config_takes_priority(
            client_type in arb_client_type(),
            endpoint_provider in arb_provider_name(),
            default_provider in arb_provider_name()
        ) {
            // 调用 select_provider 函数，端点配置有值
            let selected = select_provider(
                client_type,
                Some(&endpoint_provider),
                &default_provider
            );

            // 验证：端点配置优先于默认配置
            prop_assert_eq!(
                selected,
                endpoint_provider,
                "端点配置应优先于默认配置"
            );
        }

        /// **Feature: endpoint-provider-config, Property 2: Provider 选择优先级（回退到默认）**
        /// *对于任意* 客户端类型，当端点配置为空时，应使用默认 Provider。
        /// **Validates: Requirements 1.4**
        #[test]
        fn prop_fallback_to_default_provider(
            client_type in arb_client_type(),
            default_provider in arb_provider_name()
        ) {
            // 调用 select_provider 函数，端点配置为空
            let selected = select_provider(
                client_type,
                None,
                &default_provider
            );

            // 验证：回退到默认 Provider
            prop_assert_eq!(
                selected,
                default_provider,
                "当端点配置为空时，应回退到默认 Provider"
            );
        }
    }
}
