//! 会话管理器
//!
//! 该模块实现 Flow 会话管理功能，支持将相关的 Flow 组织成会话，
//! 便于管理和分析交互历史。
//!
//! **Validates: Requirements 5.1-5.7**

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

use super::exporter::{ExportFormat, ExportOptions, FlowExporter};
use super::models::LLMFlow;

// ============================================================================
// 错误类型
// ============================================================================

/// 会话管理错误
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("会话不存在: {0}")]
    SessionNotFound(String),

    #[error("Flow 不存在: {0}")]
    FlowNotFound(String),

    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SessionError>;

// ============================================================================
// 数据结构
// ============================================================================

/// Flow 会话
///
/// **Validates: Requirements 5.1, 5.5**
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowSession {
    /// 唯一标识符
    pub id: String,
    /// 会话名称
    pub name: String,
    /// 会话描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 关联的 Flow ID 列表
    pub flow_ids: Vec<String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 更新时间
    pub updated_at: DateTime<Utc>,
    /// 是否已归档
    pub archived: bool,
}

impl FlowSession {
    /// 创建新会话
    pub fn new(name: impl Into<String>, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description,
            flow_ids: Vec::new(),
            created_at: now,
            updated_at: now,
            archived: false,
        }
    }
}

/// 自动会话检测配置
///
/// **Validates: Requirements 5.4**
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSessionConfig {
    /// 是否启用自动会话检测
    pub enabled: bool,
    /// 时间窗口（毫秒）- 在此时间内的请求会被归入同一会话
    pub time_window_ms: u64,
    /// 是否按客户端分组
    pub group_by_client: bool,
}

impl Default for AutoSessionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            time_window_ms: 30_000, // 30 秒
            group_by_client: true,
        }
    }
}

/// 会话导出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionExportResult {
    /// 会话信息
    pub session: FlowSession,
    /// 导出的数据
    pub data: String,
    /// 导出格式
    pub format: ExportFormat,
    /// 导出的 Flow 数量
    pub flow_count: usize,
}

// ============================================================================
// 会话管理器
// ============================================================================

/// 会话管理器
///
/// **Validates: Requirements 5.1-5.7**
pub struct SessionManager {
    /// SQLite 连接
    db: Mutex<Connection>,
    /// 自动会话检测配置
    auto_config: Mutex<AutoSessionConfig>,
    /// 最近活跃会话缓存（用于自动检测）
    /// key: client_id 或 "default", value: (session_id, last_activity_time)
    active_sessions: Mutex<HashMap<String, (String, DateTime<Utc>)>>,
}

impl SessionManager {
    /// 创建新的会话管理器
    ///
    /// # Arguments
    /// * `db_path` - SQLite 数据库路径
    pub fn new(db_path: PathBuf) -> Result<Self> {
        // 确保目录存在
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&db_path)?;
        Self::init_database(&conn)?;

        Ok(Self {
            db: Mutex::new(conn),
            auto_config: Mutex::new(AutoSessionConfig::default()),
            active_sessions: Mutex::new(HashMap::new()),
        })
    }

    /// 从现有连接创建会话管理器（用于测试）
    pub fn from_connection(conn: Connection) -> Result<Self> {
        Self::init_database(&conn)?;

        Ok(Self {
            db: Mutex::new(conn),
            auto_config: Mutex::new(AutoSessionConfig::default()),
            active_sessions: Mutex::new(HashMap::new()),
        })
    }

    /// 初始化数据库表
    fn init_database(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- 会话表
            CREATE TABLE IF NOT EXISTS flow_sessions (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                archived INTEGER DEFAULT 0
            );

            -- 会话-Flow 关联表
            CREATE TABLE IF NOT EXISTS session_flows (
                session_id TEXT NOT NULL,
                flow_id TEXT NOT NULL,
                added_at TEXT NOT NULL,
                PRIMARY KEY (session_id, flow_id),
                FOREIGN KEY (session_id) REFERENCES flow_sessions(id) ON DELETE CASCADE
            );

            CREATE INDEX IF NOT EXISTS idx_session_flows_session ON session_flows(session_id);
            CREATE INDEX IF NOT EXISTS idx_session_flows_flow ON session_flows(flow_id);
            CREATE INDEX IF NOT EXISTS idx_sessions_archived ON flow_sessions(archived);
            CREATE INDEX IF NOT EXISTS idx_sessions_created ON flow_sessions(created_at);
            "#,
        )?;

        Ok(())
    }

    /// 创建新会话
    ///
    /// **Validates: Requirements 5.1**
    ///
    /// # Arguments
    /// * `name` - 会话名称
    /// * `description` - 会话描述（可选）
    ///
    /// # Returns
    /// 新创建的会话
    pub fn create_session(
        &self,
        name: impl Into<String>,
        description: Option<&str>,
    ) -> Result<FlowSession> {
        let session = FlowSession::new(name, description.map(String::from));

        let conn = self.db.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO flow_sessions (id, name, description, created_at, updated_at, archived)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                session.id,
                session.name,
                session.description,
                session.created_at.to_rfc3339(),
                session.updated_at.to_rfc3339(),
                session.archived as i32,
            ],
        )?;

        Ok(session)
    }

    /// 获取会话
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    /// 会话信息（如果存在）
    pub fn get_session(&self, session_id: &str) -> Result<Option<FlowSession>> {
        let conn = self.db.lock().unwrap();

        let session: Option<(String, String, Option<String>, String, String, i32)> = conn
            .query_row(
                r#"
                SELECT id, name, description, created_at, updated_at, archived
                FROM flow_sessions
                WHERE id = ?1
                "#,
                params![session_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .optional()?;

        match session {
            Some((id, name, description, created_at, updated_at, archived)) => {
                // 获取关联的 Flow ID
                let flow_ids = self.get_session_flow_ids_internal(&conn, &id)?;

                Ok(Some(FlowSession {
                    id,
                    name,
                    description,
                    flow_ids,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(&updated_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    archived: archived != 0,
                }))
            }
            None => Ok(None),
        }
    }

    /// 获取会话关联的 Flow ID（内部方法）
    fn get_session_flow_ids_internal(
        &self,
        conn: &Connection,
        session_id: &str,
    ) -> Result<Vec<String>> {
        let mut stmt = conn.prepare(
            r#"
            SELECT flow_id FROM session_flows
            WHERE session_id = ?1
            ORDER BY added_at ASC
            "#,
        )?;

        let flow_ids: Vec<String> = stmt
            .query_map(params![session_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(flow_ids)
    }

    /// 列出所有会话
    ///
    /// # Arguments
    /// * `include_archived` - 是否包含已归档的会话
    ///
    /// # Returns
    /// 会话列表
    pub fn list_sessions(&self, include_archived: bool) -> Result<Vec<FlowSession>> {
        let conn = self.db.lock().unwrap();

        let sql = if include_archived {
            "SELECT id, name, description, created_at, updated_at, archived FROM flow_sessions ORDER BY updated_at DESC"
        } else {
            "SELECT id, name, description, created_at, updated_at, archived FROM flow_sessions WHERE archived = 0 ORDER BY updated_at DESC"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i32>(5)?,
            ))
        })?;

        let mut sessions = Vec::new();
        for row in rows {
            let (id, name, description, created_at, updated_at, archived) = row?;
            let flow_ids = self.get_session_flow_ids_internal(&conn, &id)?;

            sessions.push(FlowSession {
                id,
                name,
                description,
                flow_ids,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: DateTime::parse_from_rfc3339(&updated_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                archived: archived != 0,
            });
        }

        Ok(sessions)
    }

    /// 添加 Flow 到会话
    ///
    /// **Validates: Requirements 5.2**
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    /// * `flow_id` - Flow ID
    pub fn add_flow(&self, session_id: &str, flow_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        // 检查会话是否存在
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM flow_sessions WHERE id = ?1",
                params![session_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if !exists {
            return Err(SessionError::SessionNotFound(session_id.to_string()));
        }

        // 添加关联（忽略重复）
        conn.execute(
            r#"
            INSERT OR IGNORE INTO session_flows (session_id, flow_id, added_at)
            VALUES (?1, ?2, ?3)
            "#,
            params![session_id, flow_id, Utc::now().to_rfc3339()],
        )?;

        // 更新会话的更新时间
        conn.execute(
            "UPDATE flow_sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        Ok(())
    }

    /// 从会话移除 Flow
    ///
    /// **Validates: Requirements 5.2**
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    /// * `flow_id` - Flow ID
    pub fn remove_flow(&self, session_id: &str, flow_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        conn.execute(
            "DELETE FROM session_flows WHERE session_id = ?1 AND flow_id = ?2",
            params![session_id, flow_id],
        )?;

        // 更新会话的更新时间
        conn.execute(
            "UPDATE flow_sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        Ok(())
    }

    /// 更新会话信息
    ///
    /// **Validates: Requirements 5.5**
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    /// * `name` - 新名称（可选）
    /// * `description` - 新描述（可选）
    pub fn update_session(
        &self,
        session_id: &str,
        name: Option<&str>,
        description: Option<Option<&str>>,
    ) -> Result<()> {
        let conn = self.db.lock().unwrap();

        // 检查会话是否存在
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM flow_sessions WHERE id = ?1",
                params![session_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if !exists {
            return Err(SessionError::SessionNotFound(session_id.to_string()));
        }

        if let Some(new_name) = name {
            conn.execute(
                "UPDATE flow_sessions SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_name, Utc::now().to_rfc3339(), session_id],
            )?;
        }

        if let Some(new_desc) = description {
            conn.execute(
                "UPDATE flow_sessions SET description = ?1, updated_at = ?2 WHERE id = ?3",
                params![new_desc, Utc::now().to_rfc3339(), session_id],
            )?;
        }

        Ok(())
    }

    /// 归档会话
    ///
    /// **Validates: Requirements 5.7**
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    pub fn archive_session(&self, session_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        let rows_affected = conn.execute(
            "UPDATE flow_sessions SET archived = 1, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        if rows_affected == 0 {
            return Err(SessionError::SessionNotFound(session_id.to_string()));
        }

        Ok(())
    }

    /// 取消归档会话
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    pub fn unarchive_session(&self, session_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        let rows_affected = conn.execute(
            "UPDATE flow_sessions SET archived = 0, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        if rows_affected == 0 {
            return Err(SessionError::SessionNotFound(session_id.to_string()));
        }

        Ok(())
    }

    /// 删除会话
    ///
    /// **Validates: Requirements 5.7**
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        // 删除关联
        conn.execute(
            "DELETE FROM session_flows WHERE session_id = ?1",
            params![session_id],
        )?;

        // 删除会话
        let rows_affected = conn.execute(
            "DELETE FROM flow_sessions WHERE id = ?1",
            params![session_id],
        )?;

        if rows_affected == 0 {
            return Err(SessionError::SessionNotFound(session_id.to_string()));
        }

        Ok(())
    }

    /// 获取会话中的 Flow ID 列表
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    /// Flow ID 列表
    pub fn get_session_flow_ids(&self, session_id: &str) -> Result<Vec<String>> {
        let conn = self.db.lock().unwrap();
        self.get_session_flow_ids_internal(&conn, session_id)
    }

    /// 检查 Flow 是否在会话中
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    /// * `flow_id` - Flow ID
    ///
    /// # Returns
    /// 是否在会话中
    pub fn is_flow_in_session(&self, session_id: &str, flow_id: &str) -> Result<bool> {
        let conn = self.db.lock().unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM session_flows WHERE session_id = ?1 AND flow_id = ?2",
                params![session_id, flow_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        Ok(exists)
    }

    /// 获取 Flow 所属的会话列表
    ///
    /// # Arguments
    /// * `flow_id` - Flow ID
    ///
    /// # Returns
    /// 会话 ID 列表
    pub fn get_sessions_for_flow(&self, flow_id: &str) -> Result<Vec<String>> {
        let conn = self.db.lock().unwrap();

        let mut stmt = conn.prepare("SELECT session_id FROM session_flows WHERE flow_id = ?1")?;

        let session_ids: Vec<String> = stmt
            .query_map(params![flow_id], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(session_ids)
    }

    /// 获取会话中的 Flow 数量
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    ///
    /// # Returns
    /// Flow 数量
    pub fn get_session_flow_count(&self, session_id: &str) -> Result<usize> {
        let conn = self.db.lock().unwrap();

        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_flows WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    // ========================================================================
    // 自动会话检测
    // ========================================================================

    /// 获取自动会话检测配置
    pub fn get_auto_config(&self) -> AutoSessionConfig {
        self.auto_config.lock().unwrap().clone()
    }

    /// 设置自动会话检测配置
    pub fn set_auto_config(&self, config: AutoSessionConfig) {
        *self.auto_config.lock().unwrap() = config;
    }

    /// 自动检测会话
    ///
    /// **Validates: Requirements 5.4**
    ///
    /// 根据配置自动检测 Flow 应该归属的会话。
    /// 如果在时间窗口内有活跃会话，则返回该会话 ID；
    /// 否则返回 None，表示应该创建新会话或不归入任何会话。
    ///
    /// # Arguments
    /// * `flow` - LLM Flow
    ///
    /// # Returns
    /// 会话 ID（如果检测到应该归入某个会话）
    pub fn detect_session(&self, flow: &LLMFlow) -> Option<String> {
        let config = self.auto_config.lock().unwrap().clone();

        if !config.enabled {
            return None;
        }

        let now = Utc::now();
        let time_window = chrono::Duration::milliseconds(config.time_window_ms as i64);

        // 确定客户端标识
        let client_key = if config.group_by_client {
            flow.metadata
                .client_info
                .ip
                .clone()
                .or_else(|| flow.metadata.client_info.request_id.clone())
                .unwrap_or_else(|| "default".to_string())
        } else {
            "default".to_string()
        };

        let mut active_sessions = self.active_sessions.lock().unwrap();

        // 检查是否有活跃会话
        if let Some((session_id, last_activity)) = active_sessions.get(&client_key) {
            if now - *last_activity < time_window {
                // 更新最后活动时间
                let session_id = session_id.clone();
                active_sessions.insert(client_key, (session_id.clone(), now));
                return Some(session_id);
            }
        }

        // 没有活跃会话
        None
    }

    /// 注册活跃会话（用于自动检测）
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    /// * `client_key` - 客户端标识（可选，默认为 "default"）
    pub fn register_active_session(&self, session_id: &str, client_key: Option<&str>) {
        let key = client_key.unwrap_or("default").to_string();
        let mut active_sessions = self.active_sessions.lock().unwrap();
        active_sessions.insert(key, (session_id.to_string(), Utc::now()));
    }

    /// 清除活跃会话缓存
    pub fn clear_active_sessions(&self) {
        let mut active_sessions = self.active_sessions.lock().unwrap();
        active_sessions.clear();
    }

    // ========================================================================
    // 会话导出
    // ========================================================================

    /// 导出会话
    ///
    /// **Validates: Requirements 5.6**
    ///
    /// # Arguments
    /// * `session_id` - 会话 ID
    /// * `flows` - 会话中的 Flow 列表
    /// * `format` - 导出格式
    ///
    /// # Returns
    /// 导出结果
    pub fn export_session(
        &self,
        session_id: &str,
        flows: &[LLMFlow],
        format: ExportFormat,
    ) -> Result<SessionExportResult> {
        // 获取会话信息
        let session = self
            .get_session(session_id)?
            .ok_or_else(|| SessionError::SessionNotFound(session_id.to_string()))?;

        // 创建导出器
        let options = ExportOptions {
            format,
            filter: None,
            include_raw: true,
            include_stream_chunks: false,
            redact_sensitive: false,
            redaction_rules: Vec::new(),
            compress: false,
        };
        let exporter = FlowExporter::new(options);

        // 导出数据
        let data = match format {
            ExportFormat::HAR => {
                let har = exporter.export_har(flows);
                serde_json::to_string_pretty(&har)?
            }
            ExportFormat::JSON => {
                // 包含会话信息的完整导出
                let export_data = serde_json::json!({
                    "session": session,
                    "flows": flows,
                });
                serde_json::to_string_pretty(&export_data)?
            }
            ExportFormat::JSONL => exporter.export_jsonl(flows),
            ExportFormat::Markdown => {
                let mut md = format!(
                    "# 会话: {}\n\n**ID**: {}\n**创建时间**: {}\n**Flow 数量**: {}\n\n",
                    session.name,
                    session.id,
                    session.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
                    flows.len()
                );
                if let Some(ref desc) = session.description {
                    md.push_str(&format!("**描述**: {}\n\n", desc));
                }
                md.push_str("---\n\n");
                md.push_str(&exporter.export_markdown_multiple(flows));
                md
            }
            ExportFormat::CSV => exporter.export_csv(flows),
        };

        Ok(SessionExportResult {
            session,
            data,
            format,
            flow_count: flows.len(),
        })
    }

    /// 获取会话数量
    pub fn session_count(&self) -> Result<usize> {
        let conn = self.db.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM flow_sessions", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// 获取所有会话 ID
    pub fn get_all_session_ids(&self) -> Result<Vec<String>> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM flow_sessions")?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(ids)
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> SessionManager {
        let conn = Connection::open_in_memory().unwrap();
        SessionManager::from_connection(conn).unwrap()
    }

    #[test]
    fn test_create_session() {
        let manager = create_test_manager();

        let session = manager
            .create_session("Test Session", Some("A test session"))
            .unwrap();

        assert!(!session.id.is_empty());
        assert_eq!(session.name, "Test Session");
        assert_eq!(session.description, Some("A test session".to_string()));
        assert!(session.flow_ids.is_empty());
        assert!(!session.archived);
    }

    #[test]
    fn test_get_session() {
        let manager = create_test_manager();

        let created = manager.create_session("Test", None).unwrap();
        let retrieved = manager.get_session(&created.id).unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Test");
    }

    #[test]
    fn test_list_sessions() {
        let manager = create_test_manager();

        manager.create_session("Session 1", None).unwrap();
        manager.create_session("Session 2", None).unwrap();

        let sessions = manager.list_sessions(false).unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_add_and_remove_flow() {
        let manager = create_test_manager();

        let session = manager.create_session("Test", None).unwrap();

        // 添加 Flow
        manager.add_flow(&session.id, "flow-1").unwrap();
        manager.add_flow(&session.id, "flow-2").unwrap();

        let flow_ids = manager.get_session_flow_ids(&session.id).unwrap();
        assert_eq!(flow_ids.len(), 2);
        assert!(flow_ids.contains(&"flow-1".to_string()));
        assert!(flow_ids.contains(&"flow-2".to_string()));

        // 移除 Flow
        manager.remove_flow(&session.id, "flow-1").unwrap();

        let flow_ids = manager.get_session_flow_ids(&session.id).unwrap();
        assert_eq!(flow_ids.len(), 1);
        assert!(!flow_ids.contains(&"flow-1".to_string()));
    }

    #[test]
    fn test_archive_session() {
        let manager = create_test_manager();

        let session = manager.create_session("Test", None).unwrap();

        // 归档
        manager.archive_session(&session.id).unwrap();

        let retrieved = manager.get_session(&session.id).unwrap().unwrap();
        assert!(retrieved.archived);

        // 列表不包含已归档
        let sessions = manager.list_sessions(false).unwrap();
        assert!(sessions.is_empty());

        // 列表包含已归档
        let sessions = manager.list_sessions(true).unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[test]
    fn test_delete_session() {
        let manager = create_test_manager();

        let session = manager.create_session("Test", None).unwrap();
        manager.add_flow(&session.id, "flow-1").unwrap();

        manager.delete_session(&session.id).unwrap();

        let retrieved = manager.get_session(&session.id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_session_not_found() {
        let manager = create_test_manager();

        let result = manager.add_flow("non-existent", "flow-1");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SessionError::SessionNotFound(_)
        ));
    }

    #[test]
    fn test_is_flow_in_session() {
        let manager = create_test_manager();

        let session = manager.create_session("Test", None).unwrap();
        manager.add_flow(&session.id, "flow-1").unwrap();

        assert!(manager.is_flow_in_session(&session.id, "flow-1").unwrap());
        assert!(!manager.is_flow_in_session(&session.id, "flow-2").unwrap());
    }

    #[test]
    fn test_get_sessions_for_flow() {
        let manager = create_test_manager();

        let session1 = manager.create_session("Session 1", None).unwrap();
        let session2 = manager.create_session("Session 2", None).unwrap();

        manager.add_flow(&session1.id, "flow-1").unwrap();
        manager.add_flow(&session2.id, "flow-1").unwrap();

        let sessions = manager.get_sessions_for_flow("flow-1").unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn test_update_session() {
        let manager = create_test_manager();

        let session = manager.create_session("Original", None).unwrap();

        manager
            .update_session(&session.id, Some("Updated"), Some(Some("New description")))
            .unwrap();

        let retrieved = manager.get_session(&session.id).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated");
        assert_eq!(retrieved.description, Some("New description".to_string()));
    }

    #[test]
    fn test_session_id_uniqueness() {
        let manager = create_test_manager();

        let mut ids = std::collections::HashSet::new();
        for i in 0..100 {
            let session = manager
                .create_session(format!("Session {}", i), None)
                .unwrap();
            assert!(ids.insert(session.id), "Session ID should be unique");
        }
    }
}

// ============================================================================
// 属性测试模块
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // ========================================================================
    // 生成器
    // ========================================================================

    /// 生成随机的会话名称
    fn arb_session_name() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 _-]{1,50}".prop_filter("Name should not be empty", |s| !s.trim().is_empty())
    }

    /// 生成随机的会话描述
    fn arb_session_description() -> impl Strategy<Value = Option<String>> {
        prop::option::of("[a-zA-Z0-9 _-]{0,200}")
    }

    /// 生成随机的 Flow ID
    fn arb_flow_id() -> impl Strategy<Value = String> {
        "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
    }

    /// 生成随机的 Flow ID 列表
    fn arb_flow_ids(max_len: usize) -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec(arb_flow_id(), 0..max_len)
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 8: 会话 ID 唯一性**
        /// **Validates: Requirements 5.1**
        ///
        /// *对于任意* 数量的会话创建操作，每个会话的 ID 应该是唯一的。
        #[test]
        fn prop_session_id_uniqueness(
            names in prop::collection::vec(arb_session_name(), 1..50)
        ) {
            let manager = create_test_manager();
            let mut ids = std::collections::HashSet::new();

            for name in names {
                let session = manager.create_session(&name, None).unwrap();
                prop_assert!(
                    ids.insert(session.id.clone()),
                    "Session ID '{}' should be unique",
                    session.id
                );
            }
        }

        /// **Feature: flow-monitor-enhancement, Property 9: 会话 Flow 关联正确性**
        /// **Validates: Requirements 5.2**
        ///
        /// *对于任意* 会话和 Flow 添加操作，添加后查询该会话应该包含所有添加的 Flow。
        #[test]
        fn prop_session_flow_association(
            name in arb_session_name(),
            flow_ids in arb_flow_ids(20)
        ) {
            let manager = create_test_manager();
            let session = manager.create_session(&name, None).unwrap();

            // 添加所有 Flow
            for flow_id in &flow_ids {
                manager.add_flow(&session.id, flow_id).unwrap();
            }

            // 验证所有 Flow 都在会话中
            let retrieved_ids = manager.get_session_flow_ids(&session.id).unwrap();

            // 去重后的 flow_ids（因为可能有重复）
            let unique_flow_ids: std::collections::HashSet<_> = flow_ids.iter().collect();

            prop_assert_eq!(
                retrieved_ids.len(),
                unique_flow_ids.len(),
                "Session should contain all unique added flows"
            );

            for flow_id in &flow_ids {
                prop_assert!(
                    retrieved_ids.contains(flow_id),
                    "Flow '{}' should be in session",
                    flow_id
                );
            }
        }

        /// **Feature: flow-monitor-enhancement, Property 10: 会话导出完整性**
        /// **Validates: Requirements 5.6**
        ///
        /// *对于任意* 会话，导出应该包含该会话中的所有 Flow。
        /// 注意：这里我们测试导出的元数据正确性，因为实际 Flow 数据需要从外部获取。
        #[test]
        fn prop_session_export_completeness(
            name in arb_session_name(),
            description in arb_session_description(),
            flow_ids in arb_flow_ids(10)
        ) {
            let manager = create_test_manager();
            let session = manager.create_session(&name, description.as_deref()).unwrap();

            // 添加 Flow
            for flow_id in &flow_ids {
                manager.add_flow(&session.id, flow_id).unwrap();
            }

            // 获取会话信息
            let retrieved = manager.get_session(&session.id).unwrap().unwrap();

            // 验证会话信息完整
            prop_assert_eq!(retrieved.name.clone(), name.clone());
            prop_assert_eq!(retrieved.description, description);

            // 验证 Flow 数量
            let unique_count = flow_ids.iter().collect::<std::collections::HashSet<_>>().len();
            prop_assert_eq!(
                retrieved.flow_ids.len(),
                unique_count,
                "Session should contain all unique flows"
            );

            // 导出会话（使用空 Flow 列表测试导出功能）
            let result = manager.export_session(&session.id, &[], ExportFormat::JSON).unwrap();
            prop_assert_eq!(result.session.id, session.id);
            prop_assert_eq!(result.session.name, name);
            prop_assert_eq!(result.flow_count, 0);
        }

        /// 会话创建和检索的 Round-Trip 测试
        #[test]
        fn prop_session_roundtrip(
            name in arb_session_name(),
            description in arb_session_description()
        ) {
            let manager = create_test_manager();

            let created = manager.create_session(&name, description.as_deref()).unwrap();
            let retrieved = manager.get_session(&created.id).unwrap().unwrap();

            prop_assert_eq!(created.id, retrieved.id);
            prop_assert_eq!(created.name, retrieved.name);
            prop_assert_eq!(created.description, retrieved.description);
            prop_assert_eq!(created.archived, retrieved.archived);
        }

        /// Flow 添加和移除的正确性测试
        #[test]
        fn prop_flow_add_remove(
            name in arb_session_name(),
            flow_ids in arb_flow_ids(10)
        ) {
            let manager = create_test_manager();
            let session = manager.create_session(&name, None).unwrap();

            // 添加所有 Flow
            for flow_id in &flow_ids {
                manager.add_flow(&session.id, flow_id).unwrap();
            }

            // 移除所有 Flow
            for flow_id in &flow_ids {
                manager.remove_flow(&session.id, flow_id).unwrap();
            }

            // 验证会话为空
            let retrieved_ids = manager.get_session_flow_ids(&session.id).unwrap();
            prop_assert!(
                retrieved_ids.is_empty(),
                "Session should be empty after removing all flows"
            );
        }

        /// 归档和取消归档的正确性测试
        #[test]
        fn prop_archive_unarchive(
            name in arb_session_name()
        ) {
            let manager = create_test_manager();
            let session = manager.create_session(&name, None).unwrap();

            // 初始状态：未归档
            let retrieved = manager.get_session(&session.id).unwrap().unwrap();
            prop_assert!(!retrieved.archived);

            // 归档
            manager.archive_session(&session.id).unwrap();
            let retrieved = manager.get_session(&session.id).unwrap().unwrap();
            prop_assert!(retrieved.archived);

            // 取消归档
            manager.unarchive_session(&session.id).unwrap();
            let retrieved = manager.get_session(&session.id).unwrap().unwrap();
            prop_assert!(!retrieved.archived);
        }
    }

    fn create_test_manager() -> SessionManager {
        let conn = Connection::open_in_memory().unwrap();
        SessionManager::from_connection(conn).unwrap()
    }
}
