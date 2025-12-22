//! 代码导出器
//!
//! 提供将 LLM Flow 导出为可执行代码的功能，支持 curl、Python、TypeScript 等格式。
//!
//! **Validates: Requirements 7.7, 7.8**

use serde::{Deserialize, Serialize};

use super::models::{LLMFlow, LLMRequest};

// ============================================================================
// 代码导出格式枚举
// ============================================================================

/// 代码导出格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeFormat {
    /// curl 命令
    Curl,
    /// Python 代码
    Python,
    /// TypeScript 代码
    TypeScript,
    /// JavaScript 代码
    JavaScript,
}

impl Default for CodeFormat {
    fn default() -> Self {
        CodeFormat::Curl
    }
}

// ============================================================================
// 代码导出器
// ============================================================================

/// 代码导出器
///
/// 将 LLM Flow 导出为可执行的代码格式。
pub struct CodeExporter;

impl CodeExporter {
    /// 导出为指定格式的代码
    ///
    /// # Arguments
    /// * `flow` - 要导出的 Flow
    /// * `format` - 导出格式
    ///
    /// # Returns
    /// 导出的代码字符串
    pub fn export(flow: &LLMFlow, format: CodeFormat) -> String {
        match format {
            CodeFormat::Curl => Self::to_curl(flow),
            CodeFormat::Python => Self::to_python(flow),
            CodeFormat::TypeScript => Self::to_typescript(flow),
            CodeFormat::JavaScript => Self::to_javascript(flow),
        }
    }

    /// 导出为 curl 命令
    ///
    /// **Validates: Requirements 7.7**
    ///
    /// # Arguments
    /// * `flow` - 要导出的 Flow
    ///
    /// # Returns
    /// curl 命令字符串
    pub fn to_curl(flow: &LLMFlow) -> String {
        Self::request_to_curl(
            &flow.request,
            flow.metadata.routing_info.target_url.as_deref(),
        )
    }

    /// 将请求转换为 curl 命令
    pub fn request_to_curl(request: &LLMRequest, base_url: Option<&str>) -> String {
        let mut parts = vec!["curl".to_string()];

        // 添加方法（如果不是 GET）
        if request.method != "GET" {
            parts.push(format!("-X {}", request.method));
        }

        // 构建 URL
        let url = if let Some(base) = base_url {
            format!("{}{}", base.trim_end_matches('/'), request.path)
        } else {
            format!("http://localhost{}", request.path)
        };
        parts.push(format!("'{}'", url));

        // 添加请求头
        for (key, value) in &request.headers {
            // 跳过敏感头部或使用占位符
            let header_value = if key.to_lowercase() == "authorization" {
                "$API_KEY".to_string()
            } else if key.to_lowercase() == "x-api-key" {
                "$API_KEY".to_string()
            } else {
                escape_shell_string(value)
            };
            parts.push(format!("-H '{}: {}'", key, header_value));
        }

        // 确保有 Content-Type 头
        if !request
            .headers
            .keys()
            .any(|k| k.to_lowercase() == "content-type")
        {
            parts.push("-H 'Content-Type: application/json'".to_string());
        }

        // 添加请求体
        if !request.body.is_null() {
            let body_str = serde_json::to_string(&request.body).unwrap_or_default();
            parts.push(format!("-d '{}'", escape_shell_string(&body_str)));
        }

        parts.join(" \\\n  ")
    }

    /// 导出为 Python 代码
    ///
    /// **Validates: Requirements 7.8**
    ///
    /// # Arguments
    /// * `flow` - 要导出的 Flow
    ///
    /// # Returns
    /// Python 代码字符串
    pub fn to_python(flow: &LLMFlow) -> String {
        Self::request_to_python(
            &flow.request,
            flow.metadata.routing_info.target_url.as_deref(),
        )
    }

    /// 将请求转换为 Python 代码
    pub fn request_to_python(request: &LLMRequest, base_url: Option<&str>) -> String {
        let mut code = String::new();

        // 导入语句
        code.push_str("import requests\n");
        code.push_str("import json\n\n");

        // URL
        let url = if let Some(base) = base_url {
            format!("{}{}", base.trim_end_matches('/'), request.path)
        } else {
            format!("http://localhost{}", request.path)
        };
        code.push_str(&format!("url = \"{}\"\n\n", url));

        // 请求头
        code.push_str("headers = {\n");
        let mut has_content_type = false;
        for (key, value) in &request.headers {
            if key.to_lowercase() == "content-type" {
                has_content_type = true;
            }
            let header_value = if key.to_lowercase() == "authorization" {
                "os.environ.get('API_KEY', '')".to_string()
            } else if key.to_lowercase() == "x-api-key" {
                "os.environ.get('API_KEY', '')".to_string()
            } else {
                format!("\"{}\"", escape_python_string(value))
            };

            if key.to_lowercase() == "authorization" || key.to_lowercase() == "x-api-key" {
                code.push_str(&format!("    \"{}\": {},\n", key, header_value));
            } else {
                code.push_str(&format!("    \"{}\": {},\n", key, header_value));
            }
        }
        if !has_content_type {
            code.push_str("    \"Content-Type\": \"application/json\",\n");
        }
        code.push_str("}\n\n");

        // 请求体
        if !request.body.is_null() {
            let body_str = serde_json::to_string_pretty(&request.body).unwrap_or_default();
            code.push_str(&format!("data = {}\n\n", body_str));
        } else {
            code.push_str("data = {}\n\n");
        }

        // 发送请求
        code.push_str(&format!(
            "response = requests.{}(\n    url,\n    headers=headers,\n    json=data\n)\n\n",
            request.method.to_lowercase()
        ));

        // 处理响应
        code.push_str("# 检查响应状态\n");
        code.push_str("response.raise_for_status()\n\n");
        code.push_str("# 解析响应\n");
        code.push_str("result = response.json()\n");
        code.push_str("print(json.dumps(result, indent=2, ensure_ascii=False))\n");

        code
    }

    /// 导出为 TypeScript 代码
    ///
    /// **Validates: Requirements 7.8**
    ///
    /// # Arguments
    /// * `flow` - 要导出的 Flow
    ///
    /// # Returns
    /// TypeScript 代码字符串
    pub fn to_typescript(flow: &LLMFlow) -> String {
        Self::request_to_typescript(
            &flow.request,
            flow.metadata.routing_info.target_url.as_deref(),
        )
    }

    /// 将请求转换为 TypeScript 代码
    pub fn request_to_typescript(request: &LLMRequest, base_url: Option<&str>) -> String {
        let mut code = String::new();

        // URL
        let url = if let Some(base) = base_url {
            format!("{}{}", base.trim_end_matches('/'), request.path)
        } else {
            format!("http://localhost{}", request.path)
        };

        code.push_str("const url = '");
        code.push_str(&url);
        code.push_str("';\n\n");

        // 请求头
        code.push_str("const headers: Record<string, string> = {\n");
        let mut has_content_type = false;
        for (key, value) in &request.headers {
            if key.to_lowercase() == "content-type" {
                has_content_type = true;
            }
            let header_value = if key.to_lowercase() == "authorization" {
                "process.env.API_KEY || ''".to_string()
            } else if key.to_lowercase() == "x-api-key" {
                "process.env.API_KEY || ''".to_string()
            } else {
                format!("'{}'", escape_js_string(value))
            };
            code.push_str(&format!("  '{}': {},\n", key, header_value));
        }
        if !has_content_type {
            code.push_str("  'Content-Type': 'application/json',\n");
        }
        code.push_str("};\n\n");

        // 请求体
        if !request.body.is_null() {
            let body_str = serde_json::to_string_pretty(&request.body).unwrap_or_default();
            code.push_str("const data = ");
            code.push_str(&body_str);
            code.push_str(";\n\n");
        } else {
            code.push_str("const data = {};\n\n");
        }

        // 发送请求（使用 async/await）
        code.push_str("async function makeRequest(): Promise<void> {\n");
        code.push_str("  const response = await fetch(url, {\n");
        code.push_str(&format!("    method: '{}',\n", request.method));
        code.push_str("    headers,\n");
        code.push_str("    body: JSON.stringify(data),\n");
        code.push_str("  });\n\n");
        code.push_str("  if (!response.ok) {\n");
        code.push_str("    throw new Error(`HTTP error! status: ${response.status}`);\n");
        code.push_str("  }\n\n");
        code.push_str("  const result = await response.json();\n");
        code.push_str("  console.log(JSON.stringify(result, null, 2));\n");
        code.push_str("}\n\n");
        code.push_str("makeRequest().catch(console.error);\n");

        code
    }

    /// 导出为 JavaScript 代码
    ///
    /// **Validates: Requirements 7.8**
    ///
    /// # Arguments
    /// * `flow` - 要导出的 Flow
    ///
    /// # Returns
    /// JavaScript 代码字符串
    pub fn to_javascript(flow: &LLMFlow) -> String {
        Self::request_to_javascript(
            &flow.request,
            flow.metadata.routing_info.target_url.as_deref(),
        )
    }

    /// 将请求转换为 JavaScript 代码
    pub fn request_to_javascript(request: &LLMRequest, base_url: Option<&str>) -> String {
        let mut code = String::new();

        // URL
        let url = if let Some(base) = base_url {
            format!("{}{}", base.trim_end_matches('/'), request.path)
        } else {
            format!("http://localhost{}", request.path)
        };

        code.push_str("const url = '");
        code.push_str(&url);
        code.push_str("';\n\n");

        // 请求头
        code.push_str("const headers = {\n");
        let mut has_content_type = false;
        for (key, value) in &request.headers {
            if key.to_lowercase() == "content-type" {
                has_content_type = true;
            }
            let header_value = if key.to_lowercase() == "authorization" {
                "process.env.API_KEY || ''".to_string()
            } else if key.to_lowercase() == "x-api-key" {
                "process.env.API_KEY || ''".to_string()
            } else {
                format!("'{}'", escape_js_string(value))
            };
            code.push_str(&format!("  '{}': {},\n", key, header_value));
        }
        if !has_content_type {
            code.push_str("  'Content-Type': 'application/json',\n");
        }
        code.push_str("};\n\n");

        // 请求体
        if !request.body.is_null() {
            let body_str = serde_json::to_string_pretty(&request.body).unwrap_or_default();
            code.push_str("const data = ");
            code.push_str(&body_str);
            code.push_str(";\n\n");
        } else {
            code.push_str("const data = {};\n\n");
        }

        // 发送请求（使用 async/await）
        code.push_str("async function makeRequest() {\n");
        code.push_str("  const response = await fetch(url, {\n");
        code.push_str(&format!("    method: '{}',\n", request.method));
        code.push_str("    headers,\n");
        code.push_str("    body: JSON.stringify(data),\n");
        code.push_str("  });\n\n");
        code.push_str("  if (!response.ok) {\n");
        code.push_str("    throw new Error(`HTTP error! status: ${response.status}`);\n");
        code.push_str("  }\n\n");
        code.push_str("  const result = await response.json();\n");
        code.push_str("  console.log(JSON.stringify(result, null, 2));\n");
        code.push_str("}\n\n");
        code.push_str("makeRequest().catch(console.error);\n");

        code
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 转义 shell 字符串中的特殊字符
fn escape_shell_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "'\\''")
}

/// 转义 Python 字符串中的特殊字符
fn escape_python_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// 转义 JavaScript 字符串中的特殊字符
fn escape_js_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_monitor::{
        FlowAnnotations, FlowMetadata, FlowState, FlowTimestamps, FlowType, Message,
        MessageContent, MessageRole, RequestParameters, RoutingInfo,
    };
    use crate::ProviderType;
    use chrono::Utc;
    use std::collections::HashMap;

    fn create_test_flow() -> LLMFlow {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        headers.insert(
            "Authorization".to_string(),
            "Bearer sk-test-key".to_string(),
        );

        let body = serde_json::json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "Hello, world!"}
            ],
            "temperature": 0.7
        });

        let request = LLMRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers,
            body,
            messages: vec![Message {
                role: MessageRole::User,
                content: MessageContent::Text("Hello, world!".to_string()),
                tool_calls: None,
                tool_result: None,
                name: None,
            }],
            system_prompt: None,
            tools: None,
            model: "gpt-4".to_string(),
            original_model: None,
            parameters: RequestParameters {
                temperature: Some(0.7),
                ..Default::default()
            },
            size_bytes: 100,
            timestamp: Utc::now(),
        };

        let mut metadata = FlowMetadata::default();
        metadata.provider = ProviderType::OpenAI;
        metadata.routing_info = RoutingInfo {
            target_url: Some("https://api.openai.com".to_string()),
            route_rule: None,
            load_balance_strategy: None,
        };

        LLMFlow {
            id: "test-flow-id".to_string(),
            flow_type: FlowType::ChatCompletions,
            request,
            response: None,
            error: None,
            metadata,
            timestamps: FlowTimestamps::default(),
            state: FlowState::Pending,
            annotations: FlowAnnotations::default(),
        }
    }

    #[test]
    fn test_to_curl() {
        let flow = create_test_flow();
        let curl = CodeExporter::to_curl(&flow);

        // 验证 curl 命令包含必要的部分
        assert!(curl.contains("curl"));
        assert!(curl.contains("-X POST"));
        assert!(curl.contains("https://api.openai.com/v1/chat/completions"));
        assert!(curl.contains("-H 'Content-Type: application/json'"));
        assert!(curl.contains("-H 'Authorization: $API_KEY'"));
        assert!(curl.contains("-d '"));
        assert!(curl.contains("gpt-4"));
    }

    #[test]
    fn test_to_python() {
        let flow = create_test_flow();
        let python = CodeExporter::to_python(&flow);

        // 验证 Python 代码包含必要的部分
        assert!(python.contains("import requests"));
        assert!(python.contains("import json"));
        assert!(python.contains("url = \"https://api.openai.com/v1/chat/completions\""));
        assert!(python.contains("headers = {"));
        assert!(python.contains("\"Content-Type\": \"application/json\""));
        assert!(python.contains("data = {"));
        assert!(python.contains("requests.post("));
        assert!(python.contains("response.raise_for_status()"));
        assert!(python.contains("response.json()"));
    }

    #[test]
    fn test_to_typescript() {
        let flow = create_test_flow();
        let typescript = CodeExporter::to_typescript(&flow);

        // 验证 TypeScript 代码包含必要的部分
        assert!(typescript.contains("const url = 'https://api.openai.com/v1/chat/completions'"));
        assert!(typescript.contains("const headers: Record<string, string> = {"));
        assert!(typescript.contains("'Content-Type': 'application/json'"));
        assert!(typescript.contains("const data = {"));
        assert!(typescript.contains("async function makeRequest(): Promise<void>"));
        assert!(typescript.contains("await fetch(url"));
        assert!(typescript.contains("method: 'POST'"));
        assert!(typescript.contains("await response.json()"));
    }

    #[test]
    fn test_to_javascript() {
        let flow = create_test_flow();
        let javascript = CodeExporter::to_javascript(&flow);

        // 验证 JavaScript 代码包含必要的部分
        assert!(javascript.contains("const url = 'https://api.openai.com/v1/chat/completions'"));
        assert!(javascript.contains("const headers = {"));
        assert!(javascript.contains("'Content-Type': 'application/json'"));
        assert!(javascript.contains("const data = {"));
        assert!(javascript.contains("async function makeRequest()"));
        assert!(javascript.contains("await fetch(url"));
        assert!(javascript.contains("method: 'POST'"));
        assert!(javascript.contains("await response.json()"));
        // TypeScript 和 JavaScript 的区别
        assert!(!javascript.contains(": Record<string, string>"));
        assert!(!javascript.contains(": Promise<void>"));
    }

    #[test]
    fn test_export_with_format() {
        let flow = create_test_flow();

        let curl = CodeExporter::export(&flow, CodeFormat::Curl);
        assert!(curl.contains("curl"));

        let python = CodeExporter::export(&flow, CodeFormat::Python);
        assert!(python.contains("import requests"));

        let typescript = CodeExporter::export(&flow, CodeFormat::TypeScript);
        assert!(typescript.contains("Record<string, string>"));

        let javascript = CodeExporter::export(&flow, CodeFormat::JavaScript);
        assert!(!javascript.contains("Record<string, string>"));
    }

    #[test]
    fn test_escape_shell_string() {
        assert_eq!(escape_shell_string("hello"), "hello");
        assert_eq!(escape_shell_string("it's"), "it'\\''s");
        assert_eq!(escape_shell_string("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn test_escape_python_string() {
        assert_eq!(escape_python_string("hello"), "hello");
        assert_eq!(escape_python_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_python_string("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_escape_js_string() {
        assert_eq!(escape_js_string("hello"), "hello");
        assert_eq!(escape_js_string("it's"), "it\\'s");
        assert_eq!(escape_js_string("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_curl_without_base_url() {
        let mut flow = create_test_flow();
        flow.metadata.routing_info.target_url = None;
        let curl = CodeExporter::to_curl(&flow);

        assert!(curl.contains("http://localhost/v1/chat/completions"));
    }

    #[test]
    fn test_api_key_placeholder() {
        let flow = create_test_flow();

        let curl = CodeExporter::to_curl(&flow);
        assert!(curl.contains("$API_KEY"));
        assert!(!curl.contains("sk-test-key"));

        let python = CodeExporter::to_python(&flow);
        assert!(python.contains("os.environ.get('API_KEY'"));
        assert!(!python.contains("sk-test-key"));

        let typescript = CodeExporter::to_typescript(&flow);
        assert!(typescript.contains("process.env.API_KEY"));
        assert!(!typescript.contains("sk-test-key"));
    }
}

// ============================================================================
// 属性测试模块
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::flow_monitor::{
        FlowAnnotations, FlowMetadata, FlowState, FlowTimestamps, FlowType, Message,
        MessageContent, MessageRole, RequestParameters, RoutingInfo,
    };
    use crate::ProviderType;
    use chrono::Utc;
    use proptest::prelude::*;
    use std::collections::HashMap;

    // ========================================================================
    // 生成器
    // ========================================================================

    /// 生成随机的 HTTP 方法
    fn arb_http_method() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("GET".to_string()),
            Just("POST".to_string()),
            Just("PUT".to_string()),
            Just("DELETE".to_string()),
            Just("PATCH".to_string()),
        ]
    }

    /// 生成随机的 API 路径
    fn arb_api_path() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("/v1/chat/completions".to_string()),
            Just("/v1/completions".to_string()),
            Just("/v1/embeddings".to_string()),
            Just("/v1/messages".to_string()),
            Just("/api/generate".to_string()),
        ]
    }

    /// 生成随机的模型名称
    fn arb_model_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4".to_string()),
            Just("gpt-3.5-turbo".to_string()),
            Just("claude-3-opus".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("gemini-pro".to_string()),
            "[a-z]{3,10}-[0-9]{1,2}".prop_map(|s| s),
        ]
    }

    /// 生成随机的 URL
    fn arb_base_url() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            Just(Some("https://api.openai.com".to_string())),
            Just(Some("https://api.anthropic.com".to_string())),
            Just(Some("http://localhost:8080".to_string())),
        ]
    }

    /// 生成随机的请求头
    fn arb_headers() -> impl Strategy<Value = HashMap<String, String>> {
        prop::collection::hash_map(
            prop_oneof![
                Just("Content-Type".to_string()),
                Just("Authorization".to_string()),
                Just("X-Api-Key".to_string()),
                Just("User-Agent".to_string()),
            ],
            "[a-zA-Z0-9-/]{5,30}",
            0..4,
        )
    }

    /// 生成随机的请求体
    fn arb_request_body() -> impl Strategy<Value = serde_json::Value> {
        prop_oneof![
            Just(serde_json::json!({})),
            Just(serde_json::json!({"model": "gpt-4"})),
            Just(serde_json::json!({
                "model": "gpt-4",
                "messages": [{"role": "user", "content": "Hello"}]
            })),
            Just(serde_json::json!({
                "model": "claude-3",
                "messages": [{"role": "user", "content": "Test"}],
                "temperature": 0.7
            })),
        ]
    }

    /// 生成随机的 LLMRequest
    fn arb_llm_request() -> impl Strategy<Value = LLMRequest> {
        (
            arb_http_method(),
            arb_api_path(),
            arb_model_name(),
            arb_headers(),
            arb_request_body(),
        )
            .prop_map(|(method, path, model, headers, body)| LLMRequest {
                method,
                path,
                headers,
                body,
                messages: vec![],
                system_prompt: None,
                tools: None,
                model,
                original_model: None,
                parameters: RequestParameters::default(),
                size_bytes: 0,
                timestamp: Utc::now(),
            })
    }

    /// 生成随机的 LLMFlow
    fn arb_llm_flow() -> impl Strategy<Value = LLMFlow> {
        (arb_llm_request(), arb_base_url()).prop_map(|(request, base_url)| {
            let mut metadata = FlowMetadata::default();
            metadata.provider = ProviderType::OpenAI;
            metadata.routing_info = RoutingInfo {
                target_url: base_url,
                route_rule: None,
                load_balance_strategy: None,
            };

            LLMFlow {
                id: uuid::Uuid::new_v4().to_string(),
                flow_type: FlowType::ChatCompletions,
                request,
                response: None,
                error: None,
                metadata,
                timestamps: FlowTimestamps::default(),
                state: FlowState::Pending,
                annotations: FlowAnnotations::default(),
            }
        })
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 13: curl 命令正确性**
        /// **Validates: Requirements 7.7**
        ///
        /// *对于任意* 有效的 LLM Flow，生成的 curl 命令应该包含正确的 HTTP 方法、URL 和请求体。
        #[test]
        fn prop_curl_command_correctness(flow in arb_llm_flow()) {
            let curl = CodeExporter::to_curl(&flow);

            // 验证 curl 命令以 "curl" 开头
            prop_assert!(curl.starts_with("curl"), "curl 命令应该以 'curl' 开头");

            // 验证包含正确的 HTTP 方法（如果不是 GET）
            if flow.request.method != "GET" {
                prop_assert!(
                    curl.contains(&format!("-X {}", flow.request.method)),
                    "curl 命令应该包含正确的 HTTP 方法: {}",
                    flow.request.method
                );
            }

            // 验证包含 URL
            let expected_path = &flow.request.path;
            prop_assert!(
                curl.contains(expected_path),
                "curl 命令应该包含请求路径: {}",
                expected_path
            );

            // 验证包含请求体（如果有）
            if !flow.request.body.is_null() {
                prop_assert!(
                    curl.contains("-d '"),
                    "curl 命令应该包含请求体"
                );
            }

            // 验证敏感信息被替换
            prop_assert!(
                !curl.contains("Bearer sk-") && !curl.contains("sk-ant-"),
                "curl 命令不应该包含真实的 API 密钥"
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 13: curl 命令 URL 正确性**
        /// **Validates: Requirements 7.7**
        ///
        /// *对于任意* 有效的 LLM Flow，生成的 curl 命令应该包含正确构建的 URL。
        #[test]
        fn prop_curl_url_correctness(flow in arb_llm_flow()) {
            let curl = CodeExporter::to_curl(&flow);

            // 构建预期的 URL
            let expected_url = if let Some(ref base) = flow.metadata.routing_info.target_url {
                format!("{}{}", base.trim_end_matches('/'), flow.request.path)
            } else {
                format!("http://localhost{}", flow.request.path)
            };

            prop_assert!(
                curl.contains(&expected_url),
                "curl 命令应该包含正确的 URL: {}, 实际: {}",
                expected_url,
                curl
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 14: Python 代码生成正确性**
        /// **Validates: Requirements 7.8**
        ///
        /// *对于任意* 有效的 LLM Flow，生成的 Python 代码应该是语法正确的。
        #[test]
        fn prop_python_code_correctness(flow in arb_llm_flow()) {
            let python = CodeExporter::to_python(&flow);

            // 验证包含必要的导入语句
            prop_assert!(
                python.contains("import requests"),
                "Python 代码应该包含 'import requests'"
            );
            prop_assert!(
                python.contains("import json"),
                "Python 代码应该包含 'import json'"
            );

            // 验证包含 URL 定义
            prop_assert!(
                python.contains("url = \""),
                "Python 代码应该包含 URL 定义"
            );

            // 验证包含 headers 定义
            prop_assert!(
                python.contains("headers = {"),
                "Python 代码应该包含 headers 定义"
            );

            // 验证包含 data 定义
            prop_assert!(
                python.contains("data = "),
                "Python 代码应该包含 data 定义"
            );

            // 验证包含 requests 调用
            prop_assert!(
                python.contains(&format!("requests.{}(", flow.request.method.to_lowercase())),
                "Python 代码应该包含正确的 requests 方法调用"
            );

            // 验证包含响应处理
            prop_assert!(
                python.contains("response.raise_for_status()"),
                "Python 代码应该包含错误处理"
            );
            prop_assert!(
                python.contains("response.json()"),
                "Python 代码应该包含 JSON 解析"
            );

            // 验证敏感信息被替换
            prop_assert!(
                !python.contains("Bearer sk-") && !python.contains("sk-ant-"),
                "Python 代码不应该包含真实的 API 密钥"
            );

            // 验证基本的 Python 语法结构
            // 检查括号匹配
            let open_parens = python.matches('(').count();
            let close_parens = python.matches(')').count();
            prop_assert_eq!(
                open_parens, close_parens,
                "Python 代码的括号应该匹配"
            );

            // 检查花括号匹配
            let open_braces = python.matches('{').count();
            let close_braces = python.matches('}').count();
            prop_assert_eq!(
                open_braces, close_braces,
                "Python 代码的花括号应该匹配"
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 14: TypeScript 代码生成正确性**
        /// **Validates: Requirements 7.8**
        ///
        /// *对于任意* 有效的 LLM Flow，生成的 TypeScript 代码应该是语法正确的。
        #[test]
        fn prop_typescript_code_correctness(flow in arb_llm_flow()) {
            let typescript = CodeExporter::to_typescript(&flow);

            // 验证包含 URL 定义
            prop_assert!(
                typescript.contains("const url = '"),
                "TypeScript 代码应该包含 URL 定义"
            );

            // 验证包含 headers 定义（带类型注解）
            prop_assert!(
                typescript.contains("const headers: Record<string, string> = {"),
                "TypeScript 代码应该包含带类型注解的 headers 定义"
            );

            // 验证包含 data 定义
            prop_assert!(
                typescript.contains("const data = "),
                "TypeScript 代码应该包含 data 定义"
            );

            // 验证包含 async 函数定义（带返回类型）
            prop_assert!(
                typescript.contains("async function makeRequest(): Promise<void>"),
                "TypeScript 代码应该包含带返回类型的 async 函数"
            );

            // 验证包含 fetch 调用
            prop_assert!(
                typescript.contains("await fetch(url"),
                "TypeScript 代码应该包含 fetch 调用"
            );

            // 验证包含正确的 HTTP 方法
            prop_assert!(
                typescript.contains(&format!("method: '{}'", flow.request.method)),
                "TypeScript 代码应该包含正确的 HTTP 方法"
            );

            // 验证包含错误处理
            prop_assert!(
                typescript.contains("if (!response.ok)"),
                "TypeScript 代码应该包含错误处理"
            );

            // 验证包含 JSON 解析
            prop_assert!(
                typescript.contains("await response.json()"),
                "TypeScript 代码应该包含 JSON 解析"
            );

            // 验证敏感信息被替换
            prop_assert!(
                !typescript.contains("Bearer sk-") && !typescript.contains("sk-ant-"),
                "TypeScript 代码不应该包含真实的 API 密钥"
            );

            // 验证基本的语法结构
            // 检查括号匹配
            let open_parens = typescript.matches('(').count();
            let close_parens = typescript.matches(')').count();
            prop_assert_eq!(
                open_parens, close_parens,
                "TypeScript 代码的括号应该匹配"
            );

            // 检查花括号匹配
            let open_braces = typescript.matches('{').count();
            let close_braces = typescript.matches('}').count();
            prop_assert_eq!(
                open_braces, close_braces,
                "TypeScript 代码的花括号应该匹配"
            );
        }

        /// **Feature: flow-monitor-enhancement, Property 14: JavaScript 代码生成正确性**
        /// **Validates: Requirements 7.8**
        ///
        /// *对于任意* 有效的 LLM Flow，生成的 JavaScript 代码应该是语法正确的，且不包含 TypeScript 类型注解。
        #[test]
        fn prop_javascript_code_correctness(flow in arb_llm_flow()) {
            let javascript = CodeExporter::to_javascript(&flow);

            // 验证包含 URL 定义
            prop_assert!(
                javascript.contains("const url = '"),
                "JavaScript 代码应该包含 URL 定义"
            );

            // 验证包含 headers 定义（不带类型注解）
            prop_assert!(
                javascript.contains("const headers = {"),
                "JavaScript 代码应该包含 headers 定义"
            );
            prop_assert!(
                !javascript.contains("Record<string, string>"),
                "JavaScript 代码不应该包含 TypeScript 类型注解"
            );

            // 验证包含 data 定义
            prop_assert!(
                javascript.contains("const data = "),
                "JavaScript 代码应该包含 data 定义"
            );

            // 验证包含 async 函数定义（不带返回类型）
            prop_assert!(
                javascript.contains("async function makeRequest()"),
                "JavaScript 代码应该包含 async 函数"
            );
            prop_assert!(
                !javascript.contains(": Promise<void>"),
                "JavaScript 代码不应该包含 TypeScript 返回类型"
            );

            // 验证包含 fetch 调用
            prop_assert!(
                javascript.contains("await fetch(url"),
                "JavaScript 代码应该包含 fetch 调用"
            );

            // 验证包含正确的 HTTP 方法
            prop_assert!(
                javascript.contains(&format!("method: '{}'", flow.request.method)),
                "JavaScript 代码应该包含正确的 HTTP 方法"
            );

            // 验证敏感信息被替换
            prop_assert!(
                !javascript.contains("Bearer sk-") && !javascript.contains("sk-ant-"),
                "JavaScript 代码不应该包含真实的 API 密钥"
            );

            // 验证基本的语法结构
            // 检查括号匹配
            let open_parens = javascript.matches('(').count();
            let close_parens = javascript.matches(')').count();
            prop_assert_eq!(
                open_parens, close_parens,
                "JavaScript 代码的括号应该匹配"
            );

            // 检查花括号匹配
            let open_braces = javascript.matches('{').count();
            let close_braces = javascript.matches('}').count();
            prop_assert_eq!(
                open_braces, close_braces,
                "JavaScript 代码的花括号应该匹配"
            );
        }
    }
}
