//! 快速过滤器管理器
//!
//! 该模块实现快速过滤器功能，支持保存和使用常用的过滤条件，
//! 便于快速筛选 Flow。
//!
//! **Validates: Requirements 6.1-6.7**

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

use super::filter_parser::FilterParser;

// ============================================================================
// 错误类型
// ============================================================================

/// 快速过滤器错误
#[derive(Debug, Error)]
pub enum QuickFilterError {
    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("快速过滤器不存在: {0}")]
    FilterNotFound(String),

    #[error("无效的过滤表达式: {0}")]
    InvalidFilterExpr(String),

    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("无法删除预设过滤器")]
    CannotDeletePreset,

    #[error("过滤器名称已存在: {0}")]
    DuplicateName(String),
}

pub type Result<T> = std::result::Result<T, QuickFilterError>;

// ============================================================================
// 数据结构
// ============================================================================

/// 快速过滤器
///
/// **Validates: Requirements 6.1, 6.3**
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuickFilter {
    /// 唯一标识符
    pub id: String,
    /// 过滤器名称
    pub name: String,
    /// 过滤器描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 过滤表达式
    pub filter_expr: String,
    /// 分组名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 排序顺序
    pub order: i32,
    /// 是否为预设过滤器
    pub is_preset: bool,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl QuickFilter {
    /// 创建新的快速过滤器
    pub fn new(
        name: impl Into<String>,
        filter_expr: impl Into<String>,
        description: Option<String>,
        group: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description,
            filter_expr: filter_expr.into(),
            group,
            order: 0,
            is_preset: false,
            created_at: Utc::now(),
        }
    }

    /// 创建预设过滤器
    pub fn preset(
        name: impl Into<String>,
        filter_expr: impl Into<String>,
        description: impl Into<String>,
        order: i32,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: Some(description.into()),
            filter_expr: filter_expr.into(),
            group: Some("预设".to_string()),
            order,
            is_preset: true,
            created_at: Utc::now(),
        }
    }
}

/// 快速过滤器更新
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QuickFilterUpdate {
    /// 新名称
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 新描述
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    /// 新过滤表达式
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_expr: Option<String>,
    /// 新分组
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<Option<String>>,
    /// 新排序顺序
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

/// 快速过滤器导出数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickFilterExport {
    /// 版本号
    pub version: String,
    /// 导出时间
    pub exported_at: DateTime<Utc>,
    /// 过滤器列表
    pub filters: Vec<QuickFilter>,
}

impl QuickFilterExport {
    pub fn new(filters: Vec<QuickFilter>) -> Self {
        Self {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            filters,
        }
    }
}

// ============================================================================
// 预设过滤器
// ============================================================================

/// 预设快速过滤器
///
/// **Validates: Requirements 6.6**
pub const PRESET_FILTERS: &[(&str, &str, &str)] = &[
    ("最近失败", "~e", "显示所有失败的请求"),
    ("高延迟", "~latency >5s", "延迟超过 5 秒的请求"),
    ("大 Token", "~tokens >10000", "Token 数超过 10000 的请求"),
    ("有工具调用", "~t", "包含工具调用的请求"),
    ("有思维链", "~k", "包含思维链的请求"),
    ("已收藏", "~starred", "已收藏的请求"),
];

// ============================================================================
// 快速过滤器管理器
// ============================================================================

/// 快速过滤器管理器
///
/// **Validates: Requirements 6.1-6.7**
pub struct QuickFilterManager {
    /// SQLite 连接
    db: Mutex<Connection>,
}

impl QuickFilterManager {
    /// 创建新的快速过滤器管理器
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

        let manager = Self {
            db: Mutex::new(conn),
        };

        // 初始化预设过滤器
        manager.init_presets()?;

        Ok(manager)
    }

    /// 从现有连接创建快速过滤器管理器（用于测试）
    pub fn from_connection(conn: Connection) -> Result<Self> {
        Self::init_database(&conn)?;

        let manager = Self {
            db: Mutex::new(conn),
        };

        Ok(manager)
    }

    /// 初始化数据库表
    fn init_database(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- 快速过滤器表
            CREATE TABLE IF NOT EXISTS quick_filters (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                filter_expr TEXT NOT NULL,
                group_name TEXT,
                sort_order INTEGER DEFAULT 0,
                is_preset INTEGER DEFAULT 0,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_quick_filters_name ON quick_filters(name);
            CREATE INDEX IF NOT EXISTS idx_quick_filters_group ON quick_filters(group_name);
            CREATE INDEX IF NOT EXISTS idx_quick_filters_order ON quick_filters(sort_order);
            "#,
        )?;

        Ok(())
    }

    /// 初始化预设过滤器
    ///
    /// **Validates: Requirements 6.6**
    pub fn init_presets(&self) -> Result<()> {
        let conn = self.db.lock().unwrap();

        // 检查是否已有预设过滤器
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM quick_filters WHERE is_preset = 1",
            [],
            |row| row.get(0),
        )?;

        if count > 0 {
            return Ok(());
        }

        // 插入预设过滤器
        for (i, (name, expr, desc)) in PRESET_FILTERS.iter().enumerate() {
            let filter = QuickFilter::preset(*name, *expr, *desc, i as i32);
            conn.execute(
                r#"
                INSERT INTO quick_filters (id, name, description, filter_expr, group_name, sort_order, is_preset, created_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
                params![
                    filter.id,
                    filter.name,
                    filter.description,
                    filter.filter_expr,
                    filter.group,
                    filter.order,
                    filter.is_preset as i32,
                    filter.created_at.to_rfc3339(),
                ],
            )?;
        }

        Ok(())
    }

    /// 保存快速过滤器
    ///
    /// **Validates: Requirements 6.1**
    ///
    /// # Arguments
    /// * `name` - 过滤器名称
    /// * `filter_expr` - 过滤表达式
    /// * `description` - 描述（可选）
    /// * `group` - 分组（可选）
    ///
    /// # Returns
    /// 新创建的快速过滤器
    pub fn save(
        &self,
        name: impl Into<String>,
        filter_expr: impl Into<String>,
        description: Option<&str>,
        group: Option<&str>,
    ) -> Result<QuickFilter> {
        let name = name.into();
        let filter_expr = filter_expr.into();

        // 验证过滤表达式
        FilterParser::validate(&filter_expr)
            .map_err(|e| QuickFilterError::InvalidFilterExpr(e.to_string()))?;

        let filter = QuickFilter::new(
            name,
            filter_expr,
            description.map(String::from),
            group.map(String::from),
        );

        let conn = self.db.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO quick_filters (id, name, description, filter_expr, group_name, sort_order, is_preset, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                filter.id,
                filter.name,
                filter.description,
                filter.filter_expr,
                filter.group,
                filter.order,
                filter.is_preset as i32,
                filter.created_at.to_rfc3339(),
            ],
        )?;

        Ok(filter)
    }

    /// 获取快速过滤器
    ///
    /// # Arguments
    /// * `id` - 过滤器 ID
    ///
    /// # Returns
    /// 快速过滤器（如果存在）
    pub fn get(&self, id: &str) -> Result<Option<QuickFilter>> {
        let conn = self.db.lock().unwrap();

        let filter: Option<(String, String, Option<String>, String, Option<String>, i32, i32, String)> = conn
            .query_row(
                r#"
                SELECT id, name, description, filter_expr, group_name, sort_order, is_preset, created_at
                FROM quick_filters
                WHERE id = ?1
                "#,
                params![id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()?;

        match filter {
            Some((id, name, description, filter_expr, group, order, is_preset, created_at)) => {
                Ok(Some(QuickFilter {
                    id,
                    name,
                    description,
                    filter_expr,
                    group,
                    order,
                    is_preset: is_preset != 0,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                }))
            }
            None => Ok(None),
        }
    }

    /// 更新快速过滤器
    ///
    /// **Validates: Requirements 6.4**
    ///
    /// # Arguments
    /// * `id` - 过滤器 ID
    /// * `updates` - 更新内容
    ///
    /// # Returns
    /// 更新后的快速过滤器
    pub fn update(&self, id: &str, updates: QuickFilterUpdate) -> Result<QuickFilter> {
        // 验证新的过滤表达式（如果有）
        if let Some(ref expr) = updates.filter_expr {
            FilterParser::validate(expr)
                .map_err(|e| QuickFilterError::InvalidFilterExpr(e.to_string()))?;
        }

        let conn = self.db.lock().unwrap();

        // 检查过滤器是否存在
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM quick_filters WHERE id = ?1",
                params![id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if !exists {
            return Err(QuickFilterError::FilterNotFound(id.to_string()));
        }

        // 更新各字段
        if let Some(ref name) = updates.name {
            conn.execute(
                "UPDATE quick_filters SET name = ?1 WHERE id = ?2",
                params![name, id],
            )?;
        }

        if let Some(ref description) = updates.description {
            conn.execute(
                "UPDATE quick_filters SET description = ?1 WHERE id = ?2",
                params![description, id],
            )?;
        }

        if let Some(ref filter_expr) = updates.filter_expr {
            conn.execute(
                "UPDATE quick_filters SET filter_expr = ?1 WHERE id = ?2",
                params![filter_expr, id],
            )?;
        }

        if let Some(ref group) = updates.group {
            conn.execute(
                "UPDATE quick_filters SET group_name = ?1 WHERE id = ?2",
                params![group, id],
            )?;
        }

        if let Some(order) = updates.order {
            conn.execute(
                "UPDATE quick_filters SET sort_order = ?1 WHERE id = ?2",
                params![order, id],
            )?;
        }

        drop(conn);

        // 返回更新后的过滤器
        self.get(id)?
            .ok_or_else(|| QuickFilterError::FilterNotFound(id.to_string()))
    }

    /// 删除快速过滤器
    ///
    /// **Validates: Requirements 6.4**
    ///
    /// # Arguments
    /// * `id` - 过滤器 ID
    pub fn delete(&self, id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        // 检查是否为预设过滤器
        let is_preset: Option<i32> = conn
            .query_row(
                "SELECT is_preset FROM quick_filters WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;

        match is_preset {
            Some(1) => return Err(QuickFilterError::CannotDeletePreset),
            None => return Err(QuickFilterError::FilterNotFound(id.to_string())),
            _ => {}
        }

        conn.execute("DELETE FROM quick_filters WHERE id = ?1", params![id])?;

        Ok(())
    }

    /// 列出所有快速过滤器
    ///
    /// **Validates: Requirements 6.2, 6.5**
    ///
    /// # Returns
    /// 快速过滤器列表（按分组和排序顺序排列）
    pub fn list(&self) -> Result<Vec<QuickFilter>> {
        let conn = self.db.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT id, name, description, filter_expr, group_name, sort_order, is_preset, created_at
            FROM quick_filters
            ORDER BY group_name ASC, sort_order ASC, created_at ASC
            "#,
        )?;

        let filters = stmt
            .query_map([], |row| {
                Ok(QuickFilter {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    filter_expr: row.get(3)?,
                    group: row.get(4)?,
                    order: row.get(5)?,
                    is_preset: row.get::<_, i32>(6)? != 0,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(filters)
    }

    /// 按分组列出快速过滤器
    ///
    /// **Validates: Requirements 6.5**
    ///
    /// # Arguments
    /// * `group` - 分组名称（None 表示无分组的过滤器）
    ///
    /// # Returns
    /// 快速过滤器列表
    pub fn list_by_group(&self, group: Option<&str>) -> Result<Vec<QuickFilter>> {
        let conn = self.db.lock().unwrap();

        let mut stmt = if group.is_some() {
            conn.prepare(
                r#"
                SELECT id, name, description, filter_expr, group_name, sort_order, is_preset, created_at
                FROM quick_filters
                WHERE group_name = ?1
                ORDER BY sort_order ASC, created_at ASC
                "#,
            )?
        } else {
            conn.prepare(
                r#"
                SELECT id, name, description, filter_expr, group_name, sort_order, is_preset, created_at
                FROM quick_filters
                WHERE group_name IS NULL
                ORDER BY sort_order ASC, created_at ASC
                "#,
            )?
        };

        let filters = if let Some(g) = group {
            stmt.query_map(params![g], |row| {
                Ok(QuickFilter {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    filter_expr: row.get(3)?,
                    group: row.get(4)?,
                    order: row.get(5)?,
                    is_preset: row.get::<_, i32>(6)? != 0,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .filter_map(|r| r.ok())
            .collect()
        } else {
            stmt.query_map([], |row| {
                Ok(QuickFilter {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    filter_expr: row.get(3)?,
                    group: row.get(4)?,
                    order: row.get(5)?,
                    is_preset: row.get::<_, i32>(6)? != 0,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .filter_map(|r| r.ok())
            .collect()
        };

        Ok(filters)
    }

    /// 获取所有分组名称
    ///
    /// **Validates: Requirements 6.5**
    ///
    /// # Returns
    /// 分组名称列表
    pub fn list_groups(&self) -> Result<Vec<String>> {
        let conn = self.db.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT group_name
            FROM quick_filters
            WHERE group_name IS NOT NULL
            ORDER BY group_name ASC
            "#,
        )?;

        let groups: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(groups)
    }

    /// 导出快速过滤器
    ///
    /// **Validates: Requirements 6.7**
    ///
    /// # Arguments
    /// * `include_presets` - 是否包含预设过滤器
    ///
    /// # Returns
    /// JSON 格式的导出数据
    pub fn export(&self, include_presets: bool) -> Result<String> {
        let filters = if include_presets {
            self.list()?
        } else {
            self.list()?.into_iter().filter(|f| !f.is_preset).collect()
        };

        let export_data = QuickFilterExport::new(filters);
        let json = serde_json::to_string_pretty(&export_data)?;

        Ok(json)
    }

    /// 导入快速过滤器
    ///
    /// **Validates: Requirements 6.7**
    ///
    /// # Arguments
    /// * `data` - JSON 格式的导入数据
    /// * `overwrite` - 是否覆盖同名过滤器
    ///
    /// # Returns
    /// 导入的快速过滤器列表
    pub fn import(&self, data: &str, overwrite: bool) -> Result<Vec<QuickFilter>> {
        let export_data: QuickFilterExport = serde_json::from_str(data)?;

        let mut imported = Vec::new();
        let conn = self.db.lock().unwrap();

        for mut filter in export_data.filters {
            // 跳过预设过滤器
            if filter.is_preset {
                continue;
            }

            // 验证过滤表达式
            if FilterParser::validate(&filter.filter_expr).is_err() {
                continue;
            }

            // 检查是否存在同名过滤器
            let existing_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM quick_filters WHERE name = ?1 AND is_preset = 0",
                    params![filter.name],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(existing) = existing_id {
                if overwrite {
                    // 更新现有过滤器
                    conn.execute(
                        r#"
                        UPDATE quick_filters
                        SET description = ?1, filter_expr = ?2, group_name = ?3, sort_order = ?4
                        WHERE id = ?5
                        "#,
                        params![
                            filter.description,
                            filter.filter_expr,
                            filter.group,
                            filter.order,
                            existing,
                        ],
                    )?;
                    filter.id = existing;
                } else {
                    // 跳过已存在的过滤器
                    continue;
                }
            } else {
                // 生成新 ID
                filter.id = Uuid::new_v4().to_string();
                filter.created_at = Utc::now();

                conn.execute(
                    r#"
                    INSERT INTO quick_filters (id, name, description, filter_expr, group_name, sort_order, is_preset, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                    "#,
                    params![
                        filter.id,
                        filter.name,
                        filter.description,
                        filter.filter_expr,
                        filter.group,
                        filter.order,
                        filter.is_preset as i32,
                        filter.created_at.to_rfc3339(),
                    ],
                )?;
            }

            imported.push(filter);
        }

        Ok(imported)
    }

    /// 获取过滤器数量
    pub fn count(&self) -> Result<usize> {
        let conn = self.db.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM quick_filters", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// 获取非预设过滤器数量
    pub fn count_custom(&self) -> Result<usize> {
        let conn = self.db.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM quick_filters WHERE is_preset = 0",
            [],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// 按名称查找过滤器
    pub fn find_by_name(&self, name: &str) -> Result<Option<QuickFilter>> {
        let conn = self.db.lock().unwrap();

        let filter: Option<(String, String, Option<String>, String, Option<String>, i32, i32, String)> = conn
            .query_row(
                r#"
                SELECT id, name, description, filter_expr, group_name, sort_order, is_preset, created_at
                FROM quick_filters
                WHERE name = ?1
                "#,
                params![name],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                },
            )
            .optional()?;

        match filter {
            Some((id, name, description, filter_expr, group, order, is_preset, created_at)) => {
                Ok(Some(QuickFilter {
                    id,
                    name,
                    description,
                    filter_expr,
                    group,
                    order,
                    is_preset: is_preset != 0,
                    created_at: DateTime::parse_from_rfc3339(&created_at)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                }))
            }
            None => Ok(None),
        }
    }

    /// 清除所有非预设过滤器（用于测试）
    #[cfg(test)]
    pub fn clear_custom(&self) -> Result<()> {
        let conn = self.db.lock().unwrap();
        conn.execute("DELETE FROM quick_filters WHERE is_preset = 0", [])?;
        Ok(())
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> QuickFilterManager {
        let conn = Connection::open_in_memory().unwrap();
        QuickFilterManager::from_connection(conn).unwrap()
    }

    #[test]
    fn test_save_quick_filter() {
        let manager = create_test_manager();

        let filter = manager
            .save(
                "Test Filter",
                "~e",
                Some("A test filter"),
                Some("Test Group"),
            )
            .unwrap();

        assert!(!filter.id.is_empty());
        assert_eq!(filter.name, "Test Filter");
        assert_eq!(filter.filter_expr, "~e");
        assert_eq!(filter.description, Some("A test filter".to_string()));
        assert_eq!(filter.group, Some("Test Group".to_string()));
        assert!(!filter.is_preset);
    }

    #[test]
    fn test_get_quick_filter() {
        let manager = create_test_manager();

        let created = manager.save("Test", "~e", None, None).unwrap();
        let retrieved = manager.get(&created.id).unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.name, "Test");
        assert_eq!(retrieved.filter_expr, "~e");
    }

    #[test]
    fn test_list_quick_filters() {
        let manager = create_test_manager();

        manager.save("Filter 1", "~e", None, None).unwrap();
        manager.save("Filter 2", "~t", None, None).unwrap();

        let filters = manager.list().unwrap();
        // 包含预设过滤器
        assert!(filters.len() >= 2);
    }

    #[test]
    fn test_update_quick_filter() {
        let manager = create_test_manager();

        let filter = manager.save("Original", "~e", None, None).unwrap();

        let updates = QuickFilterUpdate {
            name: Some("Updated".to_string()),
            description: Some(Some("New description".to_string())),
            filter_expr: Some("~t".to_string()),
            group: Some(Some("New Group".to_string())),
            order: Some(10),
        };

        let updated = manager.update(&filter.id, updates).unwrap();

        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.description, Some("New description".to_string()));
        assert_eq!(updated.filter_expr, "~t");
        assert_eq!(updated.group, Some("New Group".to_string()));
        assert_eq!(updated.order, 10);
    }

    #[test]
    fn test_delete_quick_filter() {
        let manager = create_test_manager();

        let filter = manager.save("Test", "~e", None, None).unwrap();
        manager.delete(&filter.id).unwrap();

        let retrieved = manager.get(&filter.id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_cannot_delete_preset() {
        let manager = create_test_manager();
        manager.init_presets().unwrap();

        let presets: Vec<_> = manager
            .list()
            .unwrap()
            .into_iter()
            .filter(|f| f.is_preset)
            .collect();
        assert!(!presets.is_empty());

        let result = manager.delete(&presets[0].id);
        assert!(matches!(result, Err(QuickFilterError::CannotDeletePreset)));
    }

    #[test]
    fn test_invalid_filter_expr() {
        let manager = create_test_manager();

        let result = manager.save("Invalid", "invalid expression", None, None);
        assert!(matches!(
            result,
            Err(QuickFilterError::InvalidFilterExpr(_))
        ));
    }

    #[test]
    fn test_filter_not_found() {
        let manager = create_test_manager();

        let result = manager.update("non-existent", QuickFilterUpdate::default());
        assert!(matches!(result, Err(QuickFilterError::FilterNotFound(_))));
    }

    #[test]
    fn test_list_by_group() {
        let manager = create_test_manager();

        manager
            .save("Filter 1", "~e", None, Some("Group A"))
            .unwrap();
        manager
            .save("Filter 2", "~t", None, Some("Group A"))
            .unwrap();
        manager
            .save("Filter 3", "~k", None, Some("Group B"))
            .unwrap();

        let group_a = manager.list_by_group(Some("Group A")).unwrap();
        assert_eq!(group_a.len(), 2);

        let group_b = manager.list_by_group(Some("Group B")).unwrap();
        assert_eq!(group_b.len(), 1);
    }

    #[test]
    fn test_list_groups() {
        let manager = create_test_manager();
        manager.init_presets().unwrap();

        manager
            .save("Filter 1", "~e", None, Some("Custom"))
            .unwrap();

        let groups = manager.list_groups().unwrap();
        assert!(groups.contains(&"预设".to_string()));
        assert!(groups.contains(&"Custom".to_string()));
    }

    #[test]
    fn test_export_import() {
        let manager = create_test_manager();

        manager
            .save("Export Test 1", "~e", Some("Desc 1"), Some("Group"))
            .unwrap();
        manager
            .save("Export Test 2", "~t", Some("Desc 2"), None)
            .unwrap();

        // 导出（不包含预设）
        let exported = manager.export(false).unwrap();

        // 创建新管理器并导入
        let manager2 = create_test_manager();
        let imported = manager2.import(&exported, false).unwrap();

        assert_eq!(imported.len(), 2);

        // 验证导入的过滤器
        let filter1 = manager2.find_by_name("Export Test 1").unwrap().unwrap();
        assert_eq!(filter1.filter_expr, "~e");
        assert_eq!(filter1.description, Some("Desc 1".to_string()));
    }

    #[test]
    fn test_import_overwrite() {
        let manager = create_test_manager();

        manager
            .save("Test Filter", "~e", Some("Original"), None)
            .unwrap();

        // 创建导出数据
        let export_data = QuickFilterExport::new(vec![QuickFilter::new(
            "Test Filter",
            "~t",
            Some("Updated".to_string()),
            None,
        )]);
        let json = serde_json::to_string(&export_data).unwrap();

        // 导入并覆盖
        manager.import(&json, true).unwrap();

        let filter = manager.find_by_name("Test Filter").unwrap().unwrap();
        assert_eq!(filter.filter_expr, "~t");
        assert_eq!(filter.description, Some("Updated".to_string()));
    }

    #[test]
    fn test_import_no_overwrite() {
        let manager = create_test_manager();

        manager
            .save("Test Filter", "~e", Some("Original"), None)
            .unwrap();

        // 创建导出数据
        let export_data = QuickFilterExport::new(vec![QuickFilter::new(
            "Test Filter",
            "~t",
            Some("Updated".to_string()),
            None,
        )]);
        let json = serde_json::to_string(&export_data).unwrap();

        // 导入但不覆盖
        let imported = manager.import(&json, false).unwrap();
        assert!(imported.is_empty());

        let filter = manager.find_by_name("Test Filter").unwrap().unwrap();
        assert_eq!(filter.filter_expr, "~e");
        assert_eq!(filter.description, Some("Original".to_string()));
    }

    #[test]
    fn test_preset_filters_initialized() {
        let manager = create_test_manager();
        manager.init_presets().unwrap();

        let presets: Vec<_> = manager
            .list()
            .unwrap()
            .into_iter()
            .filter(|f| f.is_preset)
            .collect();
        assert_eq!(presets.len(), PRESET_FILTERS.len());

        // 验证预设过滤器内容
        for (name, expr, _) in PRESET_FILTERS {
            let filter = manager.find_by_name(name).unwrap();
            assert!(filter.is_some(), "Preset filter '{}' should exist", name);
            let filter = filter.unwrap();
            assert_eq!(filter.filter_expr, *expr);
            assert!(filter.is_preset);
        }
    }

    #[test]
    fn test_count() {
        let manager = create_test_manager();
        manager.init_presets().unwrap();

        let initial_count = manager.count().unwrap();
        assert_eq!(initial_count, PRESET_FILTERS.len());

        manager.save("Custom 1", "~e", None, None).unwrap();
        manager.save("Custom 2", "~t", None, None).unwrap();

        assert_eq!(manager.count().unwrap(), initial_count + 2);
        assert_eq!(manager.count_custom().unwrap(), 2);
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

    /// 生成随机的过滤器名称
    fn arb_filter_name() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 _-]{1,50}".prop_filter("Name should not be empty", |s| !s.trim().is_empty())
    }

    /// 生成随机的过滤器描述
    fn arb_filter_description() -> impl Strategy<Value = Option<String>> {
        prop::option::of("[a-zA-Z0-9 _-]{0,200}")
    }

    /// 生成随机的分组名称
    fn arb_group_name() -> impl Strategy<Value = Option<String>> {
        prop::option::of("[a-zA-Z0-9 _-]{1,30}")
    }

    /// 生成有效的过滤表达式
    fn arb_valid_filter_expr() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("~e".to_string()),
            Just("~t".to_string()),
            Just("~k".to_string()),
            Just("~starred".to_string()),
            Just("~latency >5s".to_string()),
            Just("~tokens >1000".to_string()),
            Just("~s completed".to_string()),
            Just("~s failed".to_string()),
            Just("~e | ~t".to_string()),
            Just("~e & ~t".to_string()),
            Just("!~e".to_string()),
            "[a-zA-Z0-9_-]{1,20}".prop_map(|s| format!("~m {}", s)),
            "[a-zA-Z0-9_-]{1,20}".prop_map(|s| format!("~p {}", s)),
            "[a-zA-Z0-9_-]{1,20}".prop_map(|s| format!("~tag {}", s)),
        ]
    }

    /// 生成随机的快速过滤器
    fn arb_quick_filter() -> impl Strategy<Value = (String, String, Option<String>, Option<String>)>
    {
        (
            arb_filter_name(),
            arb_valid_filter_expr(),
            arb_filter_description(),
            arb_group_name(),
        )
    }

    /// 生成多个快速过滤器
    fn arb_quick_filters(
        max_len: usize,
    ) -> impl Strategy<Value = Vec<(String, String, Option<String>, Option<String>)>> {
        prop::collection::vec(arb_quick_filter(), 1..max_len)
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 11: 快速过滤器 Round-Trip**
        /// **Validates: Requirements 6.1, 6.2**
        ///
        /// *对于任意* 快速过滤器，保存后再加载应该得到等价的过滤器。
        #[test]
        fn prop_quick_filter_roundtrip(
            (name, filter_expr, description, group) in arb_quick_filter()
        ) {
            let manager = create_test_manager();

            // 保存过滤器
            let saved = manager.save(&name, &filter_expr, description.as_deref(), group.as_deref()).unwrap();

            // 加载过滤器
            let loaded = manager.get(&saved.id).unwrap().unwrap();

            // 验证等价性
            prop_assert_eq!(saved.id, loaded.id);
            prop_assert_eq!(saved.name, loaded.name);
            prop_assert_eq!(saved.filter_expr, loaded.filter_expr);
            prop_assert_eq!(saved.description, loaded.description);
            prop_assert_eq!(saved.group, loaded.group);
            prop_assert_eq!(saved.is_preset, loaded.is_preset);
        }

        /// **Feature: flow-monitor-enhancement, Property 12: 快速过滤器导入导出 Round-Trip**
        /// **Validates: Requirements 6.7**
        ///
        /// *对于任意* 快速过滤器集合，导出后再导入应该得到等价的集合。
        #[test]
        fn prop_quick_filter_export_import_roundtrip(
            filters in arb_quick_filters(10)
        ) {
            let manager1 = create_test_manager();

            // 保存所有过滤器
            let mut saved_filters = Vec::new();
            for (name, filter_expr, description, group) in &filters {
                // 使用唯一名称避免冲突
                let unique_name = format!("{}_{}", name, saved_filters.len());
                let filter = manager1.save(&unique_name, filter_expr, description.as_deref(), group.as_deref()).unwrap();
                saved_filters.push(filter);
            }

            // 导出（不包含预设）
            let exported = manager1.export(false).unwrap();

            // 创建新管理器并导入
            let manager2 = create_test_manager();
            let imported = manager2.import(&exported, false).unwrap();

            // 验证导入数量
            prop_assert_eq!(imported.len(), saved_filters.len());

            // 验证每个过滤器的内容
            for saved in &saved_filters {
                let found = manager2.find_by_name(&saved.name).unwrap();
                prop_assert!(found.is_some(), "Filter '{}' should be imported", saved.name);

                let found = found.unwrap();
                prop_assert_eq!(&saved.name, &found.name);
                prop_assert_eq!(&saved.filter_expr, &found.filter_expr);
                prop_assert_eq!(&saved.description, &found.description);
                prop_assert_eq!(&saved.group, &found.group);
            }
        }

        /// 过滤器更新后应该保持一致性
        #[test]
        fn prop_filter_update_consistency(
            (name, filter_expr, description, group) in arb_quick_filter(),
            (new_name, new_filter_expr, new_description, new_group) in arb_quick_filter()
        ) {
            let manager = create_test_manager();

            // 保存原始过滤器
            let original = manager.save(&name, &filter_expr, description.as_deref(), group.as_deref()).unwrap();

            // 更新过滤器
            let updates = QuickFilterUpdate {
                name: Some(new_name.clone()),
                filter_expr: Some(new_filter_expr.clone()),
                description: Some(new_description.clone()),
                group: Some(new_group.clone()),
                order: None,
            };

            let updated = manager.update(&original.id, updates).unwrap();

            // 验证更新后的值
            prop_assert_eq!(updated.id, original.id);
            prop_assert_eq!(updated.name, new_name);
            prop_assert_eq!(updated.filter_expr, new_filter_expr);
            prop_assert_eq!(updated.description, new_description);
            prop_assert_eq!(updated.group, new_group);
        }

        /// 删除后过滤器应该不存在
        #[test]
        fn prop_filter_delete(
            (name, filter_expr, description, group) in arb_quick_filter()
        ) {
            let manager = create_test_manager();

            // 保存过滤器
            let filter = manager.save(&name, &filter_expr, description.as_deref(), group.as_deref()).unwrap();

            // 删除过滤器
            manager.delete(&filter.id).unwrap();

            // 验证不存在
            let found = manager.get(&filter.id).unwrap();
            prop_assert!(found.is_none());
        }

        /// 列表应该包含所有保存的过滤器
        #[test]
        fn prop_list_contains_all(
            filters in arb_quick_filters(5)
        ) {
            let manager = create_test_manager();

            // 保存所有过滤器
            let mut saved_ids = Vec::new();
            for (i, (name, filter_expr, description, group)) in filters.iter().enumerate() {
                let unique_name = format!("{}_{}", name, i);
                let filter = manager.save(&unique_name, filter_expr, description.as_deref(), group.as_deref()).unwrap();
                saved_ids.push(filter.id);
            }

            // 获取列表
            let list = manager.list().unwrap();

            // 验证所有保存的过滤器都在列表中
            for id in &saved_ids {
                prop_assert!(
                    list.iter().any(|f| &f.id == id),
                    "Filter with id '{}' should be in list",
                    id
                );
            }
        }
    }

    fn create_test_manager() -> QuickFilterManager {
        let conn = Connection::open_in_memory().unwrap();
        QuickFilterManager::from_connection(conn).unwrap()
    }
}
