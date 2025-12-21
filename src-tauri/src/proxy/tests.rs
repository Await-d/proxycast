//! 代理模块属性测试
//!
//! 使用 proptest 进行属性测试

use crate::proxy::{ProxyClientFactory, ProxyError, ProxyProtocol};
use proptest::prelude::*;

/// 生成有效的 socks5 代理 URL
fn arb_socks5_url() -> impl Strategy<Value = String> {
    (
        "[a-z][a-z0-9]{0,19}", // host: 必须以字母开头
        1024u16..65535u16,     // port
    )
        .prop_map(|(host, port)| format!("socks5://{}:{}", host, port))
}

/// 生成有效的 http 代理 URL
fn arb_http_url() -> impl Strategy<Value = String> {
    (
        "[a-z][a-z0-9]{0,19}", // host: 必须以字母开头
        1024u16..65535u16,     // port
    )
        .prop_map(|(host, port)| format!("http://{}:{}", host, port))
}

/// 生成有效的 https 代理 URL
fn arb_https_url() -> impl Strategy<Value = String> {
    (
        "[a-z][a-z0-9]{0,19}", // host: 必须以字母开头
        1024u16..65535u16,     // port
    )
        .prop_map(|(host, port)| format!("https://{}:{}", host, port))
}

/// 生成任意有效的代理 URL
fn arb_valid_proxy_url() -> impl Strategy<Value = String> {
    prop_oneof![arb_socks5_url(), arb_http_url(), arb_https_url(),]
}

/// 生成无效的代理 URL（不支持的协议）
fn arb_invalid_proxy_url() -> impl Strategy<Value = String> {
    prop_oneof![
        // FTP 协议
        ("[a-z0-9]{1,20}", 1024u16..65535u16)
            .prop_map(|(host, port)| format!("ftp://{}:{}", host, port)),
        // 无协议
        ("[a-z0-9]{1,20}", 1024u16..65535u16).prop_map(|(host, port)| format!("{}:{}", host, port)),
        // 无效协议
        ("[a-z0-9]{1,20}", 1024u16..65535u16)
            .prop_map(|(host, port)| format!("invalid://{}:{}", host, port)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: cliproxyapi-parity, Property 13: Proxy URL Protocol Parsing**
    /// *For any* valid proxy URL with protocol (socks5/http/https), the proxy client
    /// SHALL correctly identify and use the protocol.
    /// **Validates: Requirements 7.3**
    #[test]
    fn prop_proxy_url_protocol_parsing_socks5(url in arb_socks5_url()) {
        let result = ProxyClientFactory::parse_proxy_url(&url);
        prop_assert!(result.is_ok(), "SOCKS5 URL 应该解析成功: {}", url);
        prop_assert_eq!(
            result.unwrap(),
            ProxyProtocol::Socks5,
            "SOCKS5 URL 应该解析为 Socks5 协议: {}",
            url
        );
    }

    /// **Feature: cliproxyapi-parity, Property 13: Proxy URL Protocol Parsing**
    /// *For any* valid HTTP proxy URL, the parser SHALL identify it as HTTP protocol.
    /// **Validates: Requirements 7.3**
    #[test]
    fn prop_proxy_url_protocol_parsing_http(url in arb_http_url()) {
        let result = ProxyClientFactory::parse_proxy_url(&url);
        prop_assert!(result.is_ok(), "HTTP URL 应该解析成功: {}", url);
        prop_assert_eq!(
            result.unwrap(),
            ProxyProtocol::Http,
            "HTTP URL 应该解析为 Http 协议: {}",
            url
        );
    }

    /// **Feature: cliproxyapi-parity, Property 13: Proxy URL Protocol Parsing**
    /// *For any* valid HTTPS proxy URL, the parser SHALL identify it as HTTPS protocol.
    /// **Validates: Requirements 7.3**
    #[test]
    fn prop_proxy_url_protocol_parsing_https(url in arb_https_url()) {
        let result = ProxyClientFactory::parse_proxy_url(&url);
        prop_assert!(result.is_ok(), "HTTPS URL 应该解析成功: {}", url);
        prop_assert_eq!(
            result.unwrap(),
            ProxyProtocol::Https,
            "HTTPS URL 应该解析为 Https 协议: {}",
            url
        );
    }

    /// **Feature: cliproxyapi-parity, Property 13: Proxy URL Protocol Parsing**
    /// *For any* invalid proxy URL, the parser SHALL return an error.
    /// **Validates: Requirements 7.3**
    #[test]
    fn prop_proxy_url_protocol_parsing_invalid(url in arb_invalid_proxy_url()) {
        let result = ProxyClientFactory::parse_proxy_url(&url);
        prop_assert!(
            result.is_err(),
            "无效 URL 应该解析失败: {}",
            url
        );
        prop_assert!(
            matches!(result, Err(ProxyError::UnsupportedProtocol(_))),
            "无效 URL 应该返回 UnsupportedProtocol 错误: {}",
            url
        );
    }

    /// **Feature: cliproxyapi-parity, Property 13: Proxy URL Protocol Parsing**
    /// *For any* valid proxy URL, creating a client SHALL succeed.
    /// **Validates: Requirements 7.3**
    #[test]
    fn prop_proxy_url_client_creation(url in arb_valid_proxy_url()) {
        let factory = ProxyClientFactory::new();
        let result = factory.create_client(Some(&url));
        prop_assert!(
            result.is_ok(),
            "有效代理 URL 应该能创建客户端: {}",
            url
        );
    }

    /// **Feature: cliproxyapi-parity, Property 14: Per-Key Proxy Selection**
    /// *For any* credential with proxy_url set, requests using that credential
    /// SHALL use the per-key proxy; otherwise, the global proxy SHALL be used.
    /// **Validates: Requirements 7.1, 7.2**
    #[test]
    fn prop_per_key_proxy_selection_with_per_key(
        global_proxy in arb_valid_proxy_url(),
        per_key_proxy in arb_valid_proxy_url()
    ) {
        let factory = ProxyClientFactory::new()
            .with_global_proxy(Some(global_proxy.clone()));

        // Per-Key 代理应该优先于全局代理
        let selected = factory.select_proxy(Some(&per_key_proxy));
        prop_assert_eq!(
            selected,
            Some(per_key_proxy.as_str()),
            "Per-Key 代理应该优先于全局代理"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 14: Per-Key Proxy Selection**
    /// *For any* credential without proxy_url, the global proxy SHALL be used.
    /// **Validates: Requirements 7.1, 7.2**
    #[test]
    fn prop_per_key_proxy_selection_fallback_to_global(
        global_proxy in arb_valid_proxy_url()
    ) {
        let factory = ProxyClientFactory::new()
            .with_global_proxy(Some(global_proxy.clone()));

        // 无 Per-Key 代理时应该使用全局代理
        let selected = factory.select_proxy(None);
        prop_assert_eq!(
            selected,
            Some(global_proxy.as_str()),
            "无 Per-Key 代理时应该使用全局代理"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 14: Per-Key Proxy Selection**
    /// *For any* configuration without global proxy and without per-key proxy,
    /// no proxy SHALL be used.
    /// **Validates: Requirements 7.1, 7.2**
    #[test]
    fn prop_per_key_proxy_selection_no_proxy(_dummy in 0..1i32) {
        let factory = ProxyClientFactory::new();

        // 无全局代理且无 Per-Key 代理时应该不使用代理
        let selected = factory.select_proxy(None);
        prop_assert_eq!(
            selected,
            None,
            "无全局代理且无 Per-Key 代理时应该不使用代理"
        );
    }

    /// **Feature: cliproxyapi-parity, Property 13: Proxy URL Protocol Parsing**
    /// *For any* valid proxy URL, the protocol parsing is case-insensitive.
    /// **Validates: Requirements 7.3**
    #[test]
    fn prop_proxy_url_case_insensitive(
        host in "[a-z0-9]{1,20}",
        port in 1024u16..65535u16,
        protocol_idx in 0usize..3usize
    ) {
        let protocols = ["socks5", "http", "https"];
        let expected_protocols = [ProxyProtocol::Socks5, ProxyProtocol::Http, ProxyProtocol::Https];

        let protocol = protocols[protocol_idx];
        let expected = expected_protocols[protocol_idx];

        // 测试小写
        let url_lower = format!("{}://{}:{}", protocol, host, port);
        let result_lower = ProxyClientFactory::parse_proxy_url(&url_lower);
        prop_assert!(result_lower.is_ok());
        prop_assert_eq!(result_lower.unwrap(), expected);

        // 测试大写
        let url_upper = format!("{}://{}:{}", protocol.to_uppercase(), host, port);
        let result_upper = ProxyClientFactory::parse_proxy_url(&url_upper);
        prop_assert!(result_upper.is_ok());
        prop_assert_eq!(result_upper.unwrap(), expected);

        // 测试混合大小写
        let protocol_mixed: String = protocol
            .chars()
            .enumerate()
            .map(|(i, c)| if i % 2 == 0 { c.to_uppercase().next().unwrap() } else { c })
            .collect();
        let url_mixed = format!("{}://{}:{}", protocol_mixed, host, port);
        let result_mixed = ProxyClientFactory::parse_proxy_url(&url_mixed);
        prop_assert!(result_mixed.is_ok());
        prop_assert_eq!(result_mixed.unwrap(), expected);
    }
}
