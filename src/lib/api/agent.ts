/**
 * Agent API
 *
 * 原生 Rust Agent 的前端 API 封装
 */

import { invoke } from "@tauri-apps/api/core";

/**
 * Agent 状态
 */
export interface AgentProcessStatus {
  running: boolean;
  base_url?: string;
  port?: number;
}

/**
 * 创建会话响应
 */
export interface CreateSessionResponse {
  session_id: string;
  credential_name: string;
  credential_uuid: string;
  provider_type: string;
  model?: string;
}

/**
 * 会话信息
 */
export interface SessionInfo {
  session_id: string;
  provider_type: string;
  model?: string;
  created_at: string;
  last_activity: string;
  messages_count: number;
}

/**
 * 图片输入
 */
export interface ImageInput {
  data: string;
  media_type: string;
}

/**
 * 启动 Agent（初始化原生 Agent）
 */
export async function startAgentProcess(): Promise<AgentProcessStatus> {
  return await invoke("agent_start_process", {});
}

/**
 * 停止 Agent
 */
export async function stopAgentProcess(): Promise<void> {
  return await invoke("agent_stop_process");
}

/**
 * 获取 Agent 状态
 */
export async function getAgentProcessStatus(): Promise<AgentProcessStatus> {
  return await invoke("agent_get_process_status");
}

/**
 * Skill 信息
 */
export interface SkillInfo {
  name: string;
  description?: string;
  path?: string;
}

/**
 * 创建 Agent 会话
 */
export async function createAgentSession(
  providerType: string,
  model?: string,
  systemPrompt?: string,
  skills?: SkillInfo[],
): Promise<CreateSessionResponse> {
  return await invoke("agent_create_session", {
    providerType,
    model,
    systemPrompt,
    skills,
  });
}

/**
 * 发送消息到 Agent（支持连续对话）
 */
export async function sendAgentMessage(
  message: string,
  sessionId?: string,
  model?: string,
  images?: ImageInput[],
  webSearch?: boolean,
  thinking?: boolean,
): Promise<string> {
  return await invoke("agent_send_message", {
    sessionId,
    message,
    images,
    model,
    webSearch,
    thinking,
  });
}

/**
 * 获取会话列表
 */
export async function listAgentSessions(): Promise<SessionInfo[]> {
  return await invoke("agent_list_sessions");
}

/**
 * 获取会话详情
 */
export async function getAgentSession(sessionId: string): Promise<SessionInfo> {
  return await invoke("agent_get_session", {
    sessionId,
  });
}

/**
 * 删除会话
 */
export async function deleteAgentSession(sessionId: string): Promise<void> {
  return await invoke("agent_delete_session", {
    sessionId,
  });
}

// ============================================================
// Goose Agent API (基于 Goose 框架的完整 Agent 实现)
// ============================================================

/**
 * Goose Agent 状态
 */
export interface GooseAgentStatus {
  initialized: boolean;
  provider?: string;
  model?: string;
}

/**
 * Goose Provider 信息
 */
export interface GooseProviderInfo {
  name: string;
  display_name: string;
}

/**
 * Goose 创建会话响应
 */
export interface GooseCreateSessionResponse {
  session_id: string;
}

/**
 * 初始化 Goose Agent
 *
 * @param providerName - Provider 名称 (如 "anthropic", "openai", "ollama")
 * @param modelName - 模型名称 (如 "claude-sonnet-4-20250514", "gpt-4o")
 */
export async function initGooseAgent(
  providerName: string,
  modelName: string,
): Promise<GooseAgentStatus> {
  return await invoke("goose_agent_init", {
    providerName,
    modelName,
  });
}

/**
 * 获取 Goose Agent 状态
 */
export async function getGooseAgentStatus(): Promise<GooseAgentStatus> {
  return await invoke("goose_agent_status");
}

/**
 * 重置 Goose Agent
 */
export async function resetGooseAgent(): Promise<void> {
  return await invoke("goose_agent_reset");
}

/**
 * 创建 Goose Agent 会话
 */
export async function createGooseSession(
  name?: string,
): Promise<GooseCreateSessionResponse> {
  return await invoke("goose_agent_create_session", { name });
}

/**
 * 发送消息到 Goose Agent (流式响应)
 *
 * 通过 Tauri 事件接收响应流
 */
export async function sendGooseMessage(
  sessionId: string,
  message: string,
  eventName: string,
): Promise<void> {
  return await invoke("goose_agent_send_message", {
    request: {
      session_id: sessionId,
      message,
      event_name: eventName,
    },
  });
}

/**
 * 扩展 Goose Agent 系统提示词
 */
export async function extendGooseSystemPrompt(
  instruction: string,
): Promise<void> {
  return await invoke("goose_agent_extend_system_prompt", { instruction });
}

/**
 * 获取 Goose 支持的 Provider 列表
 */
export async function listGooseProviders(): Promise<GooseProviderInfo[]> {
  return await invoke("goose_agent_list_providers");
}
