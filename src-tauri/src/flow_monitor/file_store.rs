//! Flow 文件存储
//!
//! 该模块实现 LLM Flow 的文件持久化存储，支持 JSONL 格式写入、
//! SQLite 索引、文件轮转和自动清理功能。

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

use super::memory_store::FlowFilter;
use super::models::LLMFlow;

// ============================================================================
// 错误类型
// ============================================================================

/// 文件存储错误
#[derive(Debug, Error)]
pub enum FileStoreError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("存储目录不存在: {0}")]
    DirectoryNotFound(PathBuf),

    #[error("Flow 不存在: {0}")]
    FlowNotFound(String),

    #[error("文件轮转失败: {0}")]
    RotationFailed(String),
}

pub type Result<T> = std::result::Result<T, FileStoreError>;

// ============================================================================
// 配置结构
// ============================================================================

/// 文件轮转配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    /// 是否按日期轮转
    pub rotate_daily: bool,
    /// 单个文件最大大小（字节）
    pub max_file_size: u64,
    /// 保留天数
    pub retention_days: u32,
    /// 是否压缩旧文件
    pub compress_old: bool,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            rotate_daily: true,
            max_file_size: 100 * 1024 * 1024, // 100MB
            retention_days: 7,
            compress_old: false, // 暂不实现压缩
        }
    }
}

/// 清理结果
#[derive(Debug, Clone, Default)]
pub struct CleanupResult {
    /// 删除的文件数
    pub files_deleted: usize,
    /// 删除的 Flow 数
    pub flows_deleted: usize,
    /// 释放的空间（字节）
    pub bytes_freed: u64,
}

// ============================================================================
// 索引记录
// ============================================================================

/// Flow 索引记录（存储在 SQLite 中）
#[derive(Debug, Clone)]
pub struct FlowIndexRecord {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub status: String,
    pub duration_ms: Option<i64>,
    pub input_tokens: Option<i32>,
    pub output_tokens: Option<i32>,
    pub has_error: bool,
    pub has_tool_calls: bool,
    pub has_thinking: bool,
    pub file_path: String,
    pub file_offset: i64,
    pub content_preview: Option<String>,
    pub request_preview: Option<String>,
}

/// FTS 搜索结果
#[derive(Debug, Clone)]
pub struct FtsSearchResult {
    /// Flow ID
    pub id: String,
    /// 创建时间（RFC3339 格式字符串）
    pub created_at: String,
    /// 模型名称
    pub model: String,
    /// 提供商
    pub provider: String,
    /// 匹配的内容片段
    pub snippet: String,
}

impl FlowIndexRecord {
    /// 从 LLMFlow 创建索引记录
    pub fn from_flow(flow: &LLMFlow, file_path: &str, file_offset: i64) -> Self {
        let content_preview = flow
            .response
            .as_ref()
            .map(|r| r.content.chars().take(200).collect::<String>());

        let request_preview = flow
            .request
            .system_prompt
            .as_ref()
            .map(|s| s.chars().take(200).collect::<String>())
            .or_else(|| {
                flow.request.messages.first().map(|m| {
                    m.content
                        .get_all_text()
                        .chars()
                        .take(200)
                        .collect::<String>()
                })
            });

        Self {
            id: flow.id.clone(),
            created_at: flow.timestamps.created,
            provider: format!("{:?}", flow.metadata.provider),
            model: flow.request.model.clone(),
            status: format!("{:?}", flow.state),
            duration_ms: Some(flow.timestamps.duration_ms as i64),
            input_tokens: flow.response.as_ref().map(|r| r.usage.input_tokens as i32),
            output_tokens: flow.response.as_ref().map(|r| r.usage.output_tokens as i32),
            has_error: flow.error.is_some(),
            has_tool_calls: flow
                .response
                .as_ref()
                .map_or(false, |r| !r.tool_calls.is_empty()),
            has_thinking: flow
                .response
                .as_ref()
                .map_or(false, |r| r.thinking.is_some()),
            file_path: file_path.to_string(),
            file_offset,
            content_preview,
            request_preview,
        }
    }
}

// ============================================================================
// 文件写入器
// ============================================================================

/// JSONL 文件写入器
struct FlowWriter {
    file: BufWriter<File>,
    path: PathBuf,
    current_offset: u64,
    current_size: u64,
}

impl FlowWriter {
    /// 创建新的写入器
    fn new(path: PathBuf) -> Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        let current_size = file.metadata()?.len();
        let current_offset = current_size;

        Ok(Self {
            file: BufWriter::new(file),
            path,
            current_offset,
            current_size,
        })
    }

    /// 写入 Flow 并返回偏移量
    fn write(&mut self, flow: &LLMFlow) -> Result<u64> {
        let offset = self.current_offset;
        let json = serde_json::to_string(flow)?;
        let line = format!("{}\n", json);
        let bytes = line.as_bytes();

        self.file.write_all(bytes)?;
        self.file.flush()?;

        self.current_offset += bytes.len() as u64;
        self.current_size += bytes.len() as u64;

        Ok(offset)
    }

    /// 获取当前文件大小
    fn size(&self) -> u64 {
        self.current_size
    }

    /// 获取文件路径
    fn path(&self) -> &Path {
        &self.path
    }
}

// ============================================================================
// Flow 文件存储
// ============================================================================

/// Flow 文件存储
///
/// 使用 JSONL 格式存储 Flow，SQLite 索引支持快速查询。
pub struct FlowFileStore {
    /// 存储目录
    base_dir: PathBuf,
    /// 当前写入器
    current_writer: Mutex<Option<FlowWriter>>,
    /// 当前日期（用于日期轮转）
    current_date: Mutex<NaiveDate>,
    /// 当前文件序号
    current_file_index: Mutex<u32>,
    /// 轮转配置
    rotation_config: RotationConfig,
    /// SQLite 连接
    index_db: Mutex<Connection>,
}

impl FlowFileStore {
    /// 创建新的文件存储
    ///
    /// # 参数
    /// - `base_dir`: 存储目录
    /// - `config`: 轮转配置
    pub fn new(base_dir: PathBuf, config: RotationConfig) -> Result<Self> {
        // 创建存储目录
        fs::create_dir_all(&base_dir)?;

        // 创建全局索引数据库
        let db_path = base_dir.join("global_index.sqlite");
        let conn = Connection::open(&db_path)?;

        // 初始化数据库表
        Self::init_database(&conn)?;

        let today = Utc::now().date_naive();

        Ok(Self {
            base_dir,
            current_writer: Mutex::new(None),
            current_date: Mutex::new(today),
            current_file_index: Mutex::new(1),
            rotation_config: config,
            index_db: Mutex::new(conn),
        })
    }

    /// 初始化数据库表
    fn init_database(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- 全局索引表
            CREATE TABLE IF NOT EXISTS flow_index (
                id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER,
                input_tokens INTEGER,
                output_tokens INTEGER,
                has_error INTEGER DEFAULT 0,
                has_tool_calls INTEGER DEFAULT 0,
                has_thinking INTEGER DEFAULT 0,
                file_path TEXT NOT NULL,
                file_offset INTEGER NOT NULL,
                content_preview TEXT,
                request_preview TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_created_at ON flow_index(created_at);
            CREATE INDEX IF NOT EXISTS idx_provider ON flow_index(provider);
            CREATE INDEX IF NOT EXISTS idx_model ON flow_index(model);
            CREATE INDEX IF NOT EXISTS idx_status ON flow_index(status);

            -- 标注表
            CREATE TABLE IF NOT EXISTS flow_annotations (
                flow_id TEXT PRIMARY KEY,
                starred INTEGER DEFAULT 0,
                marker TEXT,
                comment TEXT,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (flow_id) REFERENCES flow_index(id)
            );

            -- 标签表
            CREATE TABLE IF NOT EXISTS flow_tags (
                flow_id TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (flow_id, tag),
                FOREIGN KEY (flow_id) REFERENCES flow_index(id)
            );

            CREATE INDEX IF NOT EXISTS idx_tags ON flow_tags(tag);

            -- 全文搜索表（FTS5）
            -- 注意：这是一个独立的 FTS5 表，不使用 content= 选项
            -- 数据通过 INSERT 语句直接插入
            CREATE VIRTUAL TABLE IF NOT EXISTS flow_fts USING fts5(
                id,
                content_text,
                request_text,
                model
            );
            "#,
        )?;

        Ok(())
    }

    /// 获取存储目录
    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// 获取轮转配置
    pub fn rotation_config(&self) -> &RotationConfig {
        &self.rotation_config
    }

    /// 写入 Flow 到文件
    ///
    /// # 参数
    /// - `flow`: 要写入的 Flow
    pub fn write(&self, flow: &LLMFlow) -> Result<()> {
        // 检查是否需要轮转
        self.check_rotation()?;

        // 获取或创建写入器
        let mut writer_guard = self.current_writer.lock().unwrap();
        if writer_guard.is_none() {
            *writer_guard = Some(self.create_writer()?);
        }

        let writer = writer_guard.as_mut().unwrap();

        // 写入 Flow
        let offset = writer.write(flow)?;
        let file_path = writer.path().to_string_lossy().to_string();

        // 更新索引
        self.update_index(flow, &file_path, offset as i64)?;

        // 检查文件大小是否需要轮转
        if writer.size() >= self.rotation_config.max_file_size {
            drop(writer_guard);
            self.rotate()?;
        }

        Ok(())
    }

    /// 创建新的写入器
    fn create_writer(&self) -> Result<FlowWriter> {
        let date = *self.current_date.lock().unwrap();
        let index = *self.current_file_index.lock().unwrap();

        // 创建日期目录
        let date_dir = self.base_dir.join(date.format("%Y-%m-%d").to_string());
        fs::create_dir_all(&date_dir)?;

        // 创建文件路径
        let file_name = format!("flows_{:03}.jsonl", index);
        let file_path = date_dir.join(file_name);

        FlowWriter::new(file_path)
    }

    /// 检查是否需要日期轮转
    fn check_rotation(&self) -> Result<()> {
        if !self.rotation_config.rotate_daily {
            return Ok(());
        }

        let today = Utc::now().date_naive();
        let mut current_date = self.current_date.lock().unwrap();

        if *current_date != today {
            // 日期变化，需要轮转
            *current_date = today;
            *self.current_file_index.lock().unwrap() = 1;
            *self.current_writer.lock().unwrap() = None;
        }

        Ok(())
    }

    /// 轮转到新文件
    pub fn rotate(&self) -> Result<()> {
        // 关闭当前写入器
        *self.current_writer.lock().unwrap() = None;

        // 增加文件序号
        let mut index = self.current_file_index.lock().unwrap();
        *index += 1;

        Ok(())
    }

    /// 更新索引
    fn update_index(&self, flow: &LLMFlow, file_path: &str, file_offset: i64) -> Result<()> {
        let record = FlowIndexRecord::from_flow(flow, file_path, file_offset);
        let conn = self.index_db.lock().unwrap();

        conn.execute(
            r#"
            INSERT OR REPLACE INTO flow_index (
                id, created_at, provider, model, status,
                duration_ms, input_tokens, output_tokens,
                has_error, has_tool_calls, has_thinking,
                file_path, file_offset, content_preview, request_preview
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5,
                ?6, ?7, ?8,
                ?9, ?10, ?11,
                ?12, ?13, ?14, ?15
            )
            "#,
            params![
                record.id,
                record.created_at.to_rfc3339(),
                record.provider,
                record.model,
                record.status,
                record.duration_ms,
                record.input_tokens,
                record.output_tokens,
                record.has_error as i32,
                record.has_tool_calls as i32,
                record.has_thinking as i32,
                record.file_path,
                record.file_offset,
                record.content_preview,
                record.request_preview,
            ],
        )?;

        // 更新标注
        if flow.annotations.starred
            || flow.annotations.marker.is_some()
            || flow.annotations.comment.is_some()
        {
            conn.execute(
                r#"
                INSERT OR REPLACE INTO flow_annotations (
                    flow_id, starred, marker, comment, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5)
                "#,
                params![
                    flow.id,
                    flow.annotations.starred as i32,
                    flow.annotations.marker,
                    flow.annotations.comment,
                    Utc::now().to_rfc3339(),
                ],
            )?;
        }

        // 更新标签
        if !flow.annotations.tags.is_empty() {
            // 先删除旧标签
            conn.execute("DELETE FROM flow_tags WHERE flow_id = ?1", params![flow.id])?;

            // 插入新标签
            for tag in &flow.annotations.tags {
                conn.execute(
                    "INSERT INTO flow_tags (flow_id, tag) VALUES (?1, ?2)",
                    params![flow.id, tag],
                )?;
            }
        }

        // 更新 FTS5 索引
        let content_text = flow
            .response
            .as_ref()
            .map_or(String::new(), |r| r.content.clone());
        let request_text = Self::get_request_text_for_fts(flow);

        // 先删除旧的 FTS 记录
        conn.execute("DELETE FROM flow_fts WHERE id = ?1", params![flow.id])?;

        // 插入新的 FTS 记录
        conn.execute(
            "INSERT INTO flow_fts (id, content_text, request_text, model) VALUES (?1, ?2, ?3, ?4)",
            params![flow.id, content_text, request_text, flow.request.model],
        )?;

        Ok(())
    }

    /// 获取请求文本（用于 FTS 索引）
    fn get_request_text_for_fts(flow: &LLMFlow) -> String {
        let mut text = String::new();

        // 添加系统提示词
        if let Some(ref system) = flow.request.system_prompt {
            text.push_str(system);
            text.push('\n');
        }

        // 添加消息内容
        for msg in &flow.request.messages {
            text.push_str(&msg.content.get_all_text());
            text.push('\n');
        }

        text
    }

    /// 根据 ID 获取 Flow
    pub fn get(&self, id: &str) -> Result<Option<LLMFlow>> {
        let conn = self.index_db.lock().unwrap();

        let result: Option<(String, i64)> = conn
            .query_row(
                "SELECT file_path, file_offset FROM flow_index WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        match result {
            Some((file_path, file_offset)) => self.read_flow_from_file(&file_path, file_offset),
            None => Ok(None),
        }
    }

    /// 从文件读取 Flow
    fn read_flow_from_file(&self, file_path: &str, file_offset: i64) -> Result<Option<LLMFlow>> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Ok(None);
        }

        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        // 跳转到指定偏移量
        reader.seek(SeekFrom::Start(file_offset as u64))?;

        // 读取一行
        let mut line = String::new();
        reader.read_line(&mut line)?;

        if line.is_empty() {
            return Ok(None);
        }

        let flow: LLMFlow = serde_json::from_str(&line)?;
        Ok(Some(flow))
    }

    /// 查询 Flow（从索引）
    pub fn query(&self, filter: &FlowFilter, limit: usize, offset: usize) -> Result<Vec<LLMFlow>> {
        // 先获取所有文件位置信息
        let file_locations = self.query_index(filter, limit, offset)?;

        // 读取 Flow
        let mut flows = Vec::new();
        for (file_path, file_offset) in file_locations {
            if let Some(flow) = self.read_flow_from_file(&file_path, file_offset)? {
                // 再次用内存过滤器验证（处理复杂条件）
                if filter.matches(&flow) {
                    flows.push(flow);
                }
            }
        }

        Ok(flows)
    }

    /// 从索引查询文件位置
    fn query_index(
        &self,
        filter: &FlowFilter,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<(String, i64)>> {
        let conn = self.index_db.lock().unwrap();

        // 构建查询条件
        let mut conditions: Vec<String> = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        // 时间范围
        if let Some(ref time_range) = filter.time_range {
            if let Some(start) = time_range.start {
                conditions.push("created_at >= ?".to_string());
                params_vec.push(Box::new(start.to_rfc3339()));
            }
            if let Some(end) = time_range.end {
                conditions.push("created_at <= ?".to_string());
                params_vec.push(Box::new(end.to_rfc3339()));
            }
        }

        // 提供商过滤
        if let Some(ref providers) = filter.providers {
            if !providers.is_empty() {
                let placeholders: Vec<String> = providers.iter().map(|_| "?".to_string()).collect();
                conditions.push(format!("provider IN ({})", placeholders.join(", ")));
                for p in providers {
                    params_vec.push(Box::new(format!("{:?}", p)));
                }
            }
        }

        // 状态过滤
        if let Some(ref states) = filter.states {
            if !states.is_empty() {
                let placeholders: Vec<String> = states.iter().map(|_| "?".to_string()).collect();
                conditions.push(format!("status IN ({})", placeholders.join(", ")));
                for s in states {
                    params_vec.push(Box::new(format!("{:?}", s)));
                }
            }
        }

        // 错误过滤
        if let Some(has_error) = filter.has_error {
            conditions.push("has_error = ?".to_string());
            params_vec.push(Box::new(has_error as i32));
        }

        // 工具调用过滤
        if let Some(has_tool_calls) = filter.has_tool_calls {
            conditions.push("has_tool_calls = ?".to_string());
            params_vec.push(Box::new(has_tool_calls as i32));
        }

        // 思维链过滤
        if let Some(has_thinking) = filter.has_thinking {
            conditions.push("has_thinking = ?".to_string());
            params_vec.push(Box::new(has_thinking as i32));
        }

        // 构建 SQL
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT file_path, file_offset FROM flow_index {} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            where_clause
        );

        params_vec.push(Box::new(limit as i64));
        params_vec.push(Box::new(offset as i64));

        // 执行查询
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// 获取索引中的 Flow 数量
    pub fn count(&self) -> Result<usize> {
        let conn = self.index_db.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM flow_index", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// 全文搜索
    ///
    /// 使用 SQLite FTS5 进行全文搜索
    ///
    /// # 参数
    /// - `query`: 搜索关键词
    /// - `limit`: 最大返回数量
    ///
    /// # 返回
    /// 匹配的 Flow ID、创建时间、模型、提供商和匹配片段
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<FtsSearchResult>> {
        let conn = self.index_db.lock().unwrap();

        // 转义特殊字符并构建 FTS5 查询
        let escaped_query = Self::escape_fts_query(query);

        let sql = r#"
            SELECT 
                f.id,
                f.created_at,
                f.model,
                f.provider,
                snippet(flow_fts, 1, '<mark>', '</mark>', '...', 32) as snippet
            FROM flow_fts
            JOIN flow_index f ON flow_fts.id = f.id
            WHERE flow_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
        "#;

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![escaped_query, limit as i64], |row| {
            Ok(FtsSearchResult {
                id: row.get(0)?,
                created_at: row.get(1)?,
                model: row.get(2)?,
                provider: row.get(3)?,
                snippet: row.get(4)?,
            })
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }

        Ok(results)
    }

    /// 转义 FTS5 查询中的特殊字符
    fn escape_fts_query(query: &str) -> String {
        // FTS5 特殊字符: " * - ^ : ( )
        // 对于简单搜索，我们使用双引号包裹整个查询
        format!("\"{}\"", query.replace('"', "\"\""))
    }

    /// 更新 Flow 标注
    ///
    /// # 参数
    /// - `flow_id`: Flow ID
    /// - `annotations`: 新的标注信息
    pub fn update_annotations(
        &self,
        flow_id: &str,
        annotations: &crate::flow_monitor::models::FlowAnnotations,
    ) -> Result<()> {
        let conn = self.index_db.lock().unwrap();

        // 更新或插入标注
        conn.execute(
            r#"
            INSERT OR REPLACE INTO flow_annotations (
                flow_id, starred, marker, comment, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                flow_id,
                annotations.starred as i32,
                annotations.marker,
                annotations.comment,
                Utc::now().to_rfc3339(),
            ],
        )?;

        // 更新标签
        // 先删除旧标签
        conn.execute("DELETE FROM flow_tags WHERE flow_id = ?1", params![flow_id])?;

        // 插入新标签
        for tag in &annotations.tags {
            conn.execute(
                "INSERT INTO flow_tags (flow_id, tag) VALUES (?1, ?2)",
                params![flow_id, tag],
            )?;
        }

        Ok(())
    }

    /// 清理过期数据
    ///
    /// # 参数
    /// - `before`: 清理此时间之前的数据
    pub fn cleanup(&self, before: DateTime<Utc>) -> Result<CleanupResult> {
        let mut result = CleanupResult::default();

        // 获取要删除的文件列表和执行删除操作
        let file_paths = {
            let conn = self.index_db.lock().unwrap();

            // 获取要删除的文件列表
            let mut stmt =
                conn.prepare("SELECT DISTINCT file_path FROM flow_index WHERE created_at < ?1")?;

            let file_paths: Vec<String> = stmt
                .query_map(params![before.to_rfc3339()], |row| row.get(0))?
                .filter_map(|r| r.ok())
                .collect();

            // 统计要删除的 Flow 数量
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM flow_index WHERE created_at < ?1",
                params![before.to_rfc3339()],
                |row| row.get(0),
            )?;
            result.flows_deleted = count as usize;

            // 删除索引记录
            conn.execute(
                "DELETE FROM flow_annotations WHERE flow_id IN (SELECT id FROM flow_index WHERE created_at < ?1)",
                params![before.to_rfc3339()],
            )?;

            conn.execute(
                "DELETE FROM flow_tags WHERE flow_id IN (SELECT id FROM flow_index WHERE created_at < ?1)",
                params![before.to_rfc3339()],
            )?;

            conn.execute(
                "DELETE FROM flow_index WHERE created_at < ?1",
                params![before.to_rfc3339()],
            )?;

            file_paths
        }; // conn 在这里被释放

        // 删除文件
        for file_path in file_paths {
            let path = Path::new(&file_path);
            if path.exists() {
                if let Ok(metadata) = fs::metadata(path) {
                    result.bytes_freed += metadata.len();
                }
                if fs::remove_file(path).is_ok() {
                    result.files_deleted += 1;
                }
            }
        }

        // 清理空目录
        self.cleanup_empty_dirs()?;

        Ok(result)
    }

    /// 清理空目录
    fn cleanup_empty_dirs(&self) -> Result<()> {
        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // 检查目录是否为空（除了 .sqlite 文件）
                    if let Ok(mut dir_entries) = fs::read_dir(&path) {
                        let has_jsonl = dir_entries.any(|e| {
                            e.ok()
                                .map(|e| e.path().extension().map_or(false, |ext| ext == "jsonl"))
                                .unwrap_or(false)
                        });

                        if !has_jsonl {
                            // 删除目录中的所有文件
                            if let Ok(files) = fs::read_dir(&path) {
                                for file in files.flatten() {
                                    let _ = fs::remove_file(file.path());
                                }
                            }
                            let _ = fs::remove_dir(&path);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// 根据保留天数清理
    pub fn cleanup_by_retention(&self) -> Result<CleanupResult> {
        let retention_days = self.rotation_config.retention_days;
        let before = Utc::now() - chrono::Duration::days(retention_days as i64);
        self.cleanup(before)
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_monitor::models::{FlowMetadata, FlowType, LLMRequest, RequestParameters};
    use crate::ProviderType;
    use tempfile::TempDir;

    /// 创建测试用的 Flow
    fn create_test_flow(id: &str, model: &str, provider: ProviderType) -> LLMFlow {
        let request = LLMRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            model: model.to_string(),
            parameters: RequestParameters {
                stream: false,
                ..Default::default()
            },
            ..Default::default()
        };

        let metadata = FlowMetadata {
            provider,
            ..Default::default()
        };

        LLMFlow::new(id.to_string(), FlowType::ChatCompletions, request, metadata)
    }

    #[test]
    fn test_file_store_creation() {
        let temp_dir = TempDir::new().unwrap();
        let store = FlowFileStore::new(temp_dir.path().to_path_buf(), RotationConfig::default());

        assert!(store.is_ok());
        let store = store.unwrap();
        assert!(store.base_dir().exists());
    }

    #[test]
    fn test_file_store_write_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let store =
            FlowFileStore::new(temp_dir.path().to_path_buf(), RotationConfig::default()).unwrap();

        let flow = create_test_flow("test-1", "gpt-4", ProviderType::OpenAI);
        store.write(&flow).unwrap();

        // 验证可以读取
        let retrieved = store.get("test-1").unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test-1");
        assert_eq!(retrieved.request.model, "gpt-4");
    }

    #[test]
    fn test_file_store_multiple_writes() {
        let temp_dir = TempDir::new().unwrap();
        let store =
            FlowFileStore::new(temp_dir.path().to_path_buf(), RotationConfig::default()).unwrap();

        // 写入多个 Flow
        for i in 0..10 {
            let flow = create_test_flow(&format!("flow-{}", i), "gpt-4", ProviderType::OpenAI);
            store.write(&flow).unwrap();
        }

        // 验证数量
        assert_eq!(store.count().unwrap(), 10);

        // 验证可以读取每个
        for i in 0..10 {
            let retrieved = store.get(&format!("flow-{}", i)).unwrap();
            assert!(retrieved.is_some());
        }
    }

    #[test]
    fn test_file_store_query() {
        let temp_dir = TempDir::new().unwrap();
        let store =
            FlowFileStore::new(temp_dir.path().to_path_buf(), RotationConfig::default()).unwrap();

        // 写入不同提供商的 Flow
        store
            .write(&create_test_flow("flow-1", "gpt-4", ProviderType::OpenAI))
            .unwrap();
        store
            .write(&create_test_flow(
                "flow-2",
                "claude-3",
                ProviderType::Claude,
            ))
            .unwrap();
        store
            .write(&create_test_flow(
                "flow-3",
                "gpt-4-turbo",
                ProviderType::OpenAI,
            ))
            .unwrap();

        // 查询所有
        let filter = FlowFilter::default();
        let results = store.query(&filter, 100, 0).unwrap();
        assert_eq!(results.len(), 3);

        // 按提供商过滤
        let filter = FlowFilter {
            providers: Some(vec![ProviderType::OpenAI]),
            ..Default::default()
        };
        let results = store.query(&filter, 100, 0).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_file_store_rotation() {
        let temp_dir = TempDir::new().unwrap();
        let config = RotationConfig {
            max_file_size: 100, // 很小的文件大小，强制轮转
            ..Default::default()
        };
        let store = FlowFileStore::new(temp_dir.path().to_path_buf(), config).unwrap();

        // 写入多个 Flow，应该触发轮转
        for i in 0..5 {
            let flow = create_test_flow(&format!("flow-{}", i), "gpt-4", ProviderType::OpenAI);
            store.write(&flow).unwrap();
        }

        // 验证所有 Flow 都可以读取
        for i in 0..5 {
            let retrieved = store.get(&format!("flow-{}", i)).unwrap();
            assert!(retrieved.is_some());
        }
    }

    #[test]
    fn test_file_store_cleanup() {
        let temp_dir = TempDir::new().unwrap();
        let store =
            FlowFileStore::new(temp_dir.path().to_path_buf(), RotationConfig::default()).unwrap();

        // 写入一些 Flow
        for i in 0..5 {
            let flow = create_test_flow(&format!("flow-{}", i), "gpt-4", ProviderType::OpenAI);
            store.write(&flow).unwrap();
        }

        assert_eq!(store.count().unwrap(), 5);

        // 清理未来时间之前的数据（应该清理所有）
        let future = Utc::now() + chrono::Duration::days(1);
        let result = store.cleanup(future).unwrap();

        assert_eq!(result.flows_deleted, 5);
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_index_record_from_flow() {
        let flow = create_test_flow("test-1", "gpt-4", ProviderType::OpenAI);
        let record = FlowIndexRecord::from_flow(&flow, "/path/to/file.jsonl", 0);

        assert_eq!(record.id, "test-1");
        assert_eq!(record.model, "gpt-4");
        assert_eq!(record.provider, "OpenAI");
        assert_eq!(record.status, "Pending");
        assert!(!record.has_error);
        assert!(!record.has_tool_calls);
        assert!(!record.has_thinking);
    }
}

// ============================================================================
// 属性测试模块
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::flow_monitor::models::{
        FlowAnnotations, FlowMetadata, FlowType, LLMRequest, LLMResponse, Message, MessageContent,
        MessageRole, RequestParameters, TokenUsage,
    };
    use crate::ProviderType;
    use proptest::prelude::*;
    use tempfile::TempDir;

    // ========================================================================
    // 生成器
    // ========================================================================

    /// 生成随机的 ProviderType
    fn arb_provider_type() -> impl Strategy<Value = ProviderType> {
        prop_oneof![
            Just(ProviderType::Kiro),
            Just(ProviderType::Gemini),
            Just(ProviderType::Qwen),
            Just(ProviderType::OpenAI),
            Just(ProviderType::Claude),
            Just(ProviderType::Antigravity),
        ]
    }

    /// 生成随机的 FlowType
    fn arb_flow_type() -> impl Strategy<Value = FlowType> {
        prop_oneof![
            Just(FlowType::ChatCompletions),
            Just(FlowType::AnthropicMessages),
            Just(FlowType::GeminiGenerateContent),
            Just(FlowType::Embeddings),
        ]
    }

    /// 生成随机的 Flow ID
    fn arb_flow_id() -> impl Strategy<Value = String> {
        "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
    }

    /// 生成随机的模型名称
    fn arb_model_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4".to_string()),
            Just("gpt-4-turbo".to_string()),
            Just("gpt-3.5-turbo".to_string()),
            Just("claude-3-opus".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("gemini-pro".to_string()),
        ]
    }

    /// 生成随机的 MessageContent
    fn arb_message_content() -> impl Strategy<Value = MessageContent> {
        "[a-zA-Z0-9 ]{1,100}".prop_map(MessageContent::Text)
    }

    /// 生成随机的 Message
    fn arb_message() -> impl Strategy<Value = Message> {
        (
            prop_oneof![
                Just(MessageRole::System),
                Just(MessageRole::User),
                Just(MessageRole::Assistant),
            ],
            arb_message_content(),
        )
            .prop_map(|(role, content)| Message {
                role,
                content,
                tool_calls: None,
                tool_result: None,
                name: None,
            })
    }

    /// 生成随机的 LLMRequest
    fn arb_llm_request() -> impl Strategy<Value = LLMRequest> {
        (
            arb_model_name(),
            prop::collection::vec(arb_message(), 0..3),
            prop::option::of("[a-zA-Z0-9 ]{10,50}"),
            any::<bool>(),
        )
            .prop_map(|(model, messages, system_prompt, stream)| LLMRequest {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                model,
                messages,
                system_prompt,
                parameters: RequestParameters {
                    stream,
                    temperature: Some(0.7),
                    max_tokens: Some(1000),
                    ..Default::default()
                },
                ..Default::default()
            })
    }

    /// 生成随机的 FlowMetadata
    fn arb_flow_metadata() -> impl Strategy<Value = FlowMetadata> {
        arb_provider_type().prop_map(|provider| FlowMetadata {
            provider,
            ..Default::default()
        })
    }

    /// 生成随机的 LLMResponse
    fn arb_llm_response() -> impl Strategy<Value = Option<LLMResponse>> {
        prop::option::of(
            ("[a-zA-Z0-9 ]{10,200}", 0u32..1000u32, 0u32..500u32).prop_map(
                |(content, input_tokens, output_tokens)| LLMResponse {
                    status_code: 200,
                    status_text: "OK".to_string(),
                    content,
                    usage: TokenUsage {
                        input_tokens,
                        output_tokens,
                        total_tokens: input_tokens + output_tokens,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            ),
        )
    }

    /// 生成随机的 FlowAnnotations
    fn arb_flow_annotations() -> impl Strategy<Value = FlowAnnotations> {
        (
            any::<bool>(),
            prop::option::of("[a-zA-Z0-9 ]{5,20}"),
            prop::collection::vec("[a-z]{3,10}", 0..3),
        )
            .prop_map(|(starred, comment, tags)| FlowAnnotations {
                starred,
                comment,
                tags,
                marker: None,
            })
    }

    /// 生成随机的 LLMFlow
    fn arb_llm_flow() -> impl Strategy<Value = LLMFlow> {
        (
            arb_flow_id(),
            arb_flow_type(),
            arb_llm_request(),
            arb_flow_metadata(),
            arb_llm_response(),
            arb_flow_annotations(),
        )
            .prop_map(
                |(id, flow_type, request, metadata, response, annotations)| {
                    let mut flow = LLMFlow::new(id, flow_type, request, metadata);
                    flow.response = response;
                    flow.annotations = annotations;
                    flow
                },
            )
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: llm-flow-monitor, Property 3: 存储 Round-Trip**
        /// **Validates: Requirements 3.3, 3.5**
        ///
        /// *对于任意* 有效的 LLMFlow，存储到 Flow_Store 后再读取，
        /// 读取的 Flow 应该与原始 Flow 等价。
        #[test]
        fn prop_file_store_roundtrip(
            flow in arb_llm_flow(),
        ) {
            let temp_dir = TempDir::new().unwrap();
            let store = FlowFileStore::new(
                temp_dir.path().to_path_buf(),
                RotationConfig::default(),
            ).unwrap();

            let original_id = flow.id.clone();
            let original_model = flow.request.model.clone();
            let original_provider = flow.metadata.provider.clone();
            let original_state = flow.state.clone();
            let original_content = flow.response.as_ref().map(|r| r.content.clone());
            let original_starred = flow.annotations.starred;

            // 写入
            store.write(&flow).unwrap();

            // 读取
            let retrieved = store.get(&original_id).unwrap();
            prop_assert!(retrieved.is_some(), "Flow 应该能够被读取");

            let retrieved = retrieved.unwrap();

            // 验证关键字段一致
            prop_assert_eq!(&retrieved.id, &original_id, "ID 应该一致");
            prop_assert_eq!(&retrieved.request.model, &original_model, "模型应该一致");
            prop_assert_eq!(&retrieved.metadata.provider, &original_provider, "Provider 应该一致");
            prop_assert_eq!(&retrieved.state, &original_state, "状态应该一致");
            prop_assert_eq!(
                retrieved.response.as_ref().map(|r| r.content.clone()),
                original_content,
                "响应内容应该一致"
            );
            prop_assert_eq!(retrieved.annotations.starred, original_starred, "收藏状态应该一致");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: llm-flow-monitor, Property 3b: 多 Flow 存储 Round-Trip**
        /// **Validates: Requirements 3.3, 3.5**
        ///
        /// *对于任意* 多个有效的 LLMFlow，存储后都应该能够正确读取。
        #[test]
        fn prop_file_store_multiple_roundtrip(
            flow_count in 1usize..=20usize,
        ) {
            let temp_dir = TempDir::new().unwrap();
            let store = FlowFileStore::new(
                temp_dir.path().to_path_buf(),
                RotationConfig::default(),
            ).unwrap();

            // 创建并写入多个 Flow
            let mut original_flows = Vec::new();
            for i in 0..flow_count {
                let id = format!("flow-{:04}", i);
                let request = LLMRequest {
                    method: "POST".to_string(),
                    path: "/v1/chat/completions".to_string(),
                    model: "gpt-4".to_string(),
                    ..Default::default()
                };
                let metadata = FlowMetadata {
                    provider: ProviderType::OpenAI,
                    ..Default::default()
                };
                let flow = LLMFlow::new(id, FlowType::ChatCompletions, request, metadata);
                store.write(&flow).unwrap();
                original_flows.push(flow);
            }

            // 验证所有 Flow 都可以读取
            for original in &original_flows {
                let retrieved = store.get(&original.id).unwrap();
                prop_assert!(retrieved.is_some(), "Flow {} 应该能够被读取", original.id);

                let retrieved = retrieved.unwrap();
                prop_assert_eq!(&retrieved.id, &original.id, "ID 应该一致");
                prop_assert_eq!(&retrieved.request.model, &original.request.model, "模型应该一致");
            }

            // 验证索引数量正确
            prop_assert_eq!(
                store.count().unwrap(),
                flow_count,
                "索引中的 Flow 数量应该正确"
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(50))]

        /// **Feature: llm-flow-monitor, Property 3c: 文件轮转后 Round-Trip**
        /// **Validates: Requirements 3.3, 3.4**
        ///
        /// *对于任意* Flow 序列，即使触发文件轮转，所有 Flow 都应该能够正确读取。
        #[test]
        fn prop_file_store_rotation_roundtrip(
            flow_count in 5usize..=15usize,
        ) {
            let temp_dir = TempDir::new().unwrap();
            // 使用很小的文件大小强制轮转
            let config = RotationConfig {
                max_file_size: 500, // 500 字节，强制频繁轮转
                ..Default::default()
            };
            let store = FlowFileStore::new(temp_dir.path().to_path_buf(), config).unwrap();

            // 创建并写入多个 Flow
            let mut original_ids = Vec::new();
            for i in 0..flow_count {
                let id = format!("rotation-flow-{:04}", i);
                let request = LLMRequest {
                    method: "POST".to_string(),
                    path: "/v1/chat/completions".to_string(),
                    model: "gpt-4".to_string(),
                    ..Default::default()
                };
                let metadata = FlowMetadata {
                    provider: ProviderType::OpenAI,
                    ..Default::default()
                };
                let flow = LLMFlow::new(id.clone(), FlowType::ChatCompletions, request, metadata);
                store.write(&flow).unwrap();
                original_ids.push(id);
            }

            // 验证所有 Flow 都可以读取（即使跨多个文件）
            for id in &original_ids {
                let retrieved = store.get(id).unwrap();
                prop_assert!(retrieved.is_some(), "Flow {} 应该能够被读取（即使在轮转后）", id);
            }
        }
    }
}
