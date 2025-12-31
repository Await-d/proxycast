# Agent 模块

<!-- 一旦我所属的文件夹有所变化，请更新我 -->

## 架构说明

AI Agent 集成模块，提供原生 Rust Agent 功能，支持**连续对话**和**工具调用**。

参考 [goose](https://github.com/block/goose) 项目的 Agent 设计：
- **Conversation History**: 维护完整的消息历史，支持多轮对话
- **Tools/Skills**: 预留工具调用接口（MCP 协议兼容）

### 设计决策

- **原生 Rust 实现**：直接在 Rust 中处理 Agent 功能，复用现有 provider 和流式处理能力
- **会话管理**：支持多会话，每个会话独立维护消息历史和系统提示词
- **连续对话**：每次请求携带 session_id，自动包含历史消息
- **流式响应**：通过 Tauri 事件系统向前端推送流式内容

## 文件索引

| 文件 | 说明 |
|------|------|
| `mod.rs` | 模块入口，导出公共类型 |
| `types.rs` | Agent 相关类型定义（会话、消息、工具、配置） |
| `native_agent.rs` | 原生 Rust Agent 实现（NativeAgent、NativeAgentState） |

## 核心类型

### 会话管理
- `AgentSession`: 会话状态，包含消息历史和系统提示词
- `AgentMessage`: 消息结构，支持文本、图片、工具调用

### 消息内容
- `MessageContent`: 消息内容（文本或多部分）
- `ContentPart`: 内容部分（文本/图片）

### 工具支持（预留）
- `ToolCall`: 工具调用请求
- `ToolDefinition`: 工具定义
- `FunctionDefinition`: 函数定义

### Agent 实现
- `NativeAgent`: Agent 核心实现
- `NativeAgentState`: Tauri 状态管理器

## 使用示例

```rust
// 创建会话
let session_id = agent_state.create_session(
    Some("claude-sonnet-4-20250514".to_string()),
    Some("你是一个有帮助的助手".to_string()),
)?;

// 发送消息（自动包含历史）
let request = NativeChatRequest {
    session_id: Some(session_id.clone()),
    message: "你好".to_string(),
    model: None,
    images: None,
    stream: false,
};
let response = agent_state.chat(request).await?;
```

## 更新提醒

任何文件变更后，请更新此文档和相关的上级文档。
