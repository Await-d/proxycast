//! 书签管理器
//!
//! 该模块实现 Flow 书签功能，支持快速定位和导航到重要的 Flow。
//!
//! **Validates: Requirements 8.1, 8.3, 8.6**

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// 错误类型
// ============================================================================

/// 书签管理错误
#[derive(Debug, Error)]
pub enum BookmarkError {
    #[error("SQLite 错误: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("书签不存在: {0}")]
    BookmarkNotFound(String),

    #[error("Flow 不存在: {0}")]
    FlowNotFound(String),

    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),

    #[error("书签已存在: flow_id={0}")]
    BookmarkAlreadyExists(String),
}

pub type Result<T> = std::result::Result<T, BookmarkError>;

// ============================================================================
// 数据结构
// ============================================================================

/// Flow 书签
///
/// **Validates: Requirements 8.1, 8.3**
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlowBookmark {
    /// 唯一标识符
    pub id: String,
    /// 关联的 Flow ID
    pub flow_id: String,
    /// 书签名称（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 分组名称（可选）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

impl FlowBookmark {
    /// 创建新书签
    pub fn new(flow_id: impl Into<String>, name: Option<String>, group: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            flow_id: flow_id.into(),
            name,
            group,
            created_at: Utc::now(),
        }
    }
}

/// 书签导出数据
///
/// **Validates: Requirements 8.6**
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkExport {
    /// 版本号
    pub version: String,
    /// 导出时间
    pub exported_at: DateTime<Utc>,
    /// 书签列表
    pub bookmarks: Vec<FlowBookmark>,
}

impl BookmarkExport {
    pub fn new(bookmarks: Vec<FlowBookmark>) -> Self {
        Self {
            version: "1.0".to_string(),
            exported_at: Utc::now(),
            bookmarks,
        }
    }
}

// ============================================================================
// 书签管理器
// ============================================================================

/// 书签管理器
///
/// **Validates: Requirements 8.1, 8.3, 8.6**
pub struct BookmarkManager {
    /// SQLite 连接
    db: Mutex<Connection>,
}

impl BookmarkManager {
    /// 创建新的书签管理器
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
        })
    }

    /// 从现有连接创建书签管理器（用于测试）
    pub fn from_connection(conn: Connection) -> Result<Self> {
        Self::init_database(&conn)?;

        Ok(Self {
            db: Mutex::new(conn),
        })
    }

    /// 初始化数据库表
    fn init_database(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            r#"
            -- 书签表
            CREATE TABLE IF NOT EXISTS flow_bookmarks (
                id TEXT PRIMARY KEY,
                flow_id TEXT NOT NULL,
                name TEXT,
                group_name TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_bookmarks_flow ON flow_bookmarks(flow_id);
            CREATE INDEX IF NOT EXISTS idx_bookmarks_group ON flow_bookmarks(group_name);
            CREATE INDEX IF NOT EXISTS idx_bookmarks_created ON flow_bookmarks(created_at);
            "#,
        )?;

        Ok(())
    }

    /// 添加书签
    ///
    /// **Validates: Requirements 8.1**
    ///
    /// # Arguments
    /// * `flow_id` - Flow ID
    /// * `name` - 书签名称（可选）
    /// * `group` - 分组名称（可选）
    ///
    /// # Returns
    /// 新创建的书签
    pub fn add(
        &self,
        flow_id: impl Into<String>,
        name: Option<&str>,
        group: Option<&str>,
    ) -> Result<FlowBookmark> {
        let flow_id = flow_id.into();
        let bookmark = FlowBookmark::new(&flow_id, name.map(String::from), group.map(String::from));

        let conn = self.db.lock().unwrap();

        conn.execute(
            r#"
            INSERT INTO flow_bookmarks (id, flow_id, name, group_name, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                bookmark.id,
                bookmark.flow_id,
                bookmark.name,
                bookmark.group,
                bookmark.created_at.to_rfc3339(),
            ],
        )?;

        Ok(bookmark)
    }

    /// 获取书签
    ///
    /// # Arguments
    /// * `bookmark_id` - 书签 ID
    ///
    /// # Returns
    /// 书签信息（如果存在）
    pub fn get(&self, bookmark_id: &str) -> Result<Option<FlowBookmark>> {
        let conn = self.db.lock().unwrap();

        let bookmark: Option<(String, String, Option<String>, Option<String>, String)> = conn
            .query_row(
                r#"
                SELECT id, flow_id, name, group_name, created_at
                FROM flow_bookmarks
                WHERE id = ?1
                "#,
                params![bookmark_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        match bookmark {
            Some((id, flow_id, name, group, created_at)) => Ok(Some(FlowBookmark {
                id,
                flow_id,
                name,
                group,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })),
            None => Ok(None),
        }
    }

    /// 根据 Flow ID 获取书签
    ///
    /// # Arguments
    /// * `flow_id` - Flow ID
    ///
    /// # Returns
    /// 书签信息（如果存在）
    pub fn get_by_flow_id(&self, flow_id: &str) -> Result<Option<FlowBookmark>> {
        let conn = self.db.lock().unwrap();

        let bookmark: Option<(String, String, Option<String>, Option<String>, String)> = conn
            .query_row(
                r#"
                SELECT id, flow_id, name, group_name, created_at
                FROM flow_bookmarks
                WHERE flow_id = ?1
                "#,
                params![flow_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;

        match bookmark {
            Some((id, flow_id, name, group, created_at)) => Ok(Some(FlowBookmark {
                id,
                flow_id,
                name,
                group,
                created_at: DateTime::parse_from_rfc3339(&created_at)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            })),
            None => Ok(None),
        }
    }

    /// 移除书签
    ///
    /// **Validates: Requirements 8.1**
    ///
    /// # Arguments
    /// * `bookmark_id` - 书签 ID
    pub fn remove(&self, bookmark_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        let rows_affected = conn.execute(
            "DELETE FROM flow_bookmarks WHERE id = ?1",
            params![bookmark_id],
        )?;

        if rows_affected == 0 {
            return Err(BookmarkError::BookmarkNotFound(bookmark_id.to_string()));
        }

        Ok(())
    }

    /// 根据 Flow ID 移除书签
    ///
    /// # Arguments
    /// * `flow_id` - Flow ID
    pub fn remove_by_flow_id(&self, flow_id: &str) -> Result<()> {
        let conn = self.db.lock().unwrap();

        conn.execute(
            "DELETE FROM flow_bookmarks WHERE flow_id = ?1",
            params![flow_id],
        )?;

        Ok(())
    }

    /// 更新书签
    ///
    /// # Arguments
    /// * `bookmark_id` - 书签 ID
    /// * `name` - 新名称（可选）
    /// * `group` - 新分组（可选）
    pub fn update(
        &self,
        bookmark_id: &str,
        name: Option<Option<&str>>,
        group: Option<Option<&str>>,
    ) -> Result<FlowBookmark> {
        let conn = self.db.lock().unwrap();

        // 检查书签是否存在
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM flow_bookmarks WHERE id = ?1",
                params![bookmark_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        if !exists {
            return Err(BookmarkError::BookmarkNotFound(bookmark_id.to_string()));
        }

        // 更新名称
        if let Some(new_name) = name {
            conn.execute(
                "UPDATE flow_bookmarks SET name = ?1 WHERE id = ?2",
                params![new_name, bookmark_id],
            )?;
        }

        // 更新分组
        if let Some(new_group) = group {
            conn.execute(
                "UPDATE flow_bookmarks SET group_name = ?1 WHERE id = ?2",
                params![new_group, bookmark_id],
            )?;
        }

        drop(conn);

        // 返回更新后的书签
        self.get(bookmark_id)?
            .ok_or_else(|| BookmarkError::BookmarkNotFound(bookmark_id.to_string()))
    }

    /// 列出所有书签
    ///
    /// **Validates: Requirements 8.3**
    ///
    /// # Arguments
    /// * `group` - 分组名称（可选，None 表示所有书签）
    ///
    /// # Returns
    /// 书签列表
    pub fn list(&self, group: Option<&str>) -> Result<Vec<FlowBookmark>> {
        let conn = self.db.lock().unwrap();

        let mut stmt = if let Some(g) = group {
            let mut stmt = conn.prepare(
                r#"
                SELECT id, flow_id, name, group_name, created_at
                FROM flow_bookmarks
                WHERE group_name = ?1
                ORDER BY created_at DESC
                "#,
            )?;
            let bookmarks: Vec<FlowBookmark> = stmt
                .query_map(params![g], |row| {
                    Ok(FlowBookmark {
                        id: row.get(0)?,
                        flow_id: row.get(1)?,
                        name: row.get(2)?,
                        group: row.get(3)?,
                        created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                    })
                })?
                .filter_map(|r| r.ok())
                .collect();
            return Ok(bookmarks);
        } else {
            conn.prepare(
                r#"
                SELECT id, flow_id, name, group_name, created_at
                FROM flow_bookmarks
                ORDER BY created_at DESC
                "#,
            )?
        };

        let bookmarks: Vec<FlowBookmark> = stmt
            .query_map([], |row| {
                Ok(FlowBookmark {
                    id: row.get(0)?,
                    flow_id: row.get(1)?,
                    name: row.get(2)?,
                    group: row.get(3)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(bookmarks)
    }

    /// 获取所有分组名称
    ///
    /// **Validates: Requirements 8.3**
    ///
    /// # Returns
    /// 分组名称列表
    pub fn list_groups(&self) -> Result<Vec<String>> {
        let conn = self.db.lock().unwrap();

        let mut stmt = conn.prepare(
            r#"
            SELECT DISTINCT group_name
            FROM flow_bookmarks
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

    /// 检查 Flow 是否已添加书签
    ///
    /// # Arguments
    /// * `flow_id` - Flow ID
    ///
    /// # Returns
    /// 是否已添加书签
    pub fn is_bookmarked(&self, flow_id: &str) -> Result<bool> {
        let conn = self.db.lock().unwrap();

        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM flow_bookmarks WHERE flow_id = ?1",
                params![flow_id],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);

        Ok(exists)
    }

    /// 获取书签数量
    pub fn count(&self) -> Result<usize> {
        let conn = self.db.lock().unwrap();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM flow_bookmarks", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// 导出书签
    ///
    /// **Validates: Requirements 8.6**
    ///
    /// # Returns
    /// JSON 格式的导出数据
    pub fn export(&self) -> Result<String> {
        let bookmarks = self.list(None)?;
        let export_data = BookmarkExport::new(bookmarks);
        let json = serde_json::to_string_pretty(&export_data)?;
        Ok(json)
    }

    /// 导入书签
    ///
    /// **Validates: Requirements 8.6**
    ///
    /// # Arguments
    /// * `data` - JSON 格式的导入数据
    /// * `overwrite` - 是否覆盖已存在的书签（按 flow_id 判断）
    ///
    /// # Returns
    /// 导入的书签列表
    pub fn import(&self, data: &str, overwrite: bool) -> Result<Vec<FlowBookmark>> {
        let export_data: BookmarkExport = serde_json::from_str(data)?;

        let mut imported = Vec::new();
        let conn = self.db.lock().unwrap();

        for mut bookmark in export_data.bookmarks {
            // 检查是否存在相同 flow_id 的书签
            let existing_id: Option<String> = conn
                .query_row(
                    "SELECT id FROM flow_bookmarks WHERE flow_id = ?1",
                    params![bookmark.flow_id],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(existing) = existing_id {
                if overwrite {
                    // 更新现有书签
                    conn.execute(
                        r#"
                        UPDATE flow_bookmarks
                        SET name = ?1, group_name = ?2
                        WHERE id = ?3
                        "#,
                        params![bookmark.name, bookmark.group, existing],
                    )?;
                    bookmark.id = existing;
                } else {
                    // 跳过已存在的书签
                    continue;
                }
            } else {
                // 生成新 ID
                bookmark.id = Uuid::new_v4().to_string();
                bookmark.created_at = Utc::now();

                conn.execute(
                    r#"
                    INSERT INTO flow_bookmarks (id, flow_id, name, group_name, created_at)
                    VALUES (?1, ?2, ?3, ?4, ?5)
                    "#,
                    params![
                        bookmark.id,
                        bookmark.flow_id,
                        bookmark.name,
                        bookmark.group,
                        bookmark.created_at.to_rfc3339(),
                    ],
                )?;
            }

            imported.push(bookmark);
        }

        Ok(imported)
    }

    /// 清除所有书签（用于测试）
    #[cfg(test)]
    pub fn clear(&self) -> Result<()> {
        let conn = self.db.lock().unwrap();
        conn.execute("DELETE FROM flow_bookmarks", [])?;
        Ok(())
    }
}

// ============================================================================
// 测试模块
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_manager() -> BookmarkManager {
        let conn = Connection::open_in_memory().unwrap();
        BookmarkManager::from_connection(conn).unwrap()
    }

    #[test]
    fn test_add_bookmark() {
        let manager = create_test_manager();

        let bookmark = manager
            .add("flow-1", Some("Test Bookmark"), Some("Test Group"))
            .unwrap();

        assert!(!bookmark.id.is_empty());
        assert_eq!(bookmark.flow_id, "flow-1");
        assert_eq!(bookmark.name, Some("Test Bookmark".to_string()));
        assert_eq!(bookmark.group, Some("Test Group".to_string()));
    }

    #[test]
    fn test_get_bookmark() {
        let manager = create_test_manager();

        let created = manager.add("flow-1", Some("Test"), None).unwrap();
        let retrieved = manager.get(&created.id).unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, created.id);
        assert_eq!(retrieved.flow_id, "flow-1");
        assert_eq!(retrieved.name, Some("Test".to_string()));
    }

    #[test]
    fn test_get_by_flow_id() {
        let manager = create_test_manager();

        let created = manager.add("flow-1", Some("Test"), None).unwrap();
        let retrieved = manager.get_by_flow_id("flow-1").unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, created.id);
    }

    #[test]
    fn test_remove_bookmark() {
        let manager = create_test_manager();

        let bookmark = manager.add("flow-1", None, None).unwrap();
        manager.remove(&bookmark.id).unwrap();

        let retrieved = manager.get(&bookmark.id).unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_remove_by_flow_id() {
        let manager = create_test_manager();

        manager.add("flow-1", None, None).unwrap();
        manager.remove_by_flow_id("flow-1").unwrap();

        let retrieved = manager.get_by_flow_id("flow-1").unwrap();
        assert!(retrieved.is_none());
    }

    #[test]
    fn test_update_bookmark() {
        let manager = create_test_manager();

        let bookmark = manager.add("flow-1", Some("Original"), None).unwrap();

        let updated = manager
            .update(&bookmark.id, Some(Some("Updated")), Some(Some("New Group")))
            .unwrap();

        assert_eq!(updated.name, Some("Updated".to_string()));
        assert_eq!(updated.group, Some("New Group".to_string()));
    }

    #[test]
    fn test_list_bookmarks() {
        let manager = create_test_manager();

        manager.add("flow-1", None, None).unwrap();
        manager.add("flow-2", None, None).unwrap();
        manager.add("flow-3", None, None).unwrap();

        let bookmarks = manager.list(None).unwrap();
        assert_eq!(bookmarks.len(), 3);
    }

    #[test]
    fn test_list_by_group() {
        let manager = create_test_manager();

        manager.add("flow-1", None, Some("Group A")).unwrap();
        manager.add("flow-2", None, Some("Group A")).unwrap();
        manager.add("flow-3", None, Some("Group B")).unwrap();

        let group_a = manager.list(Some("Group A")).unwrap();
        assert_eq!(group_a.len(), 2);

        let group_b = manager.list(Some("Group B")).unwrap();
        assert_eq!(group_b.len(), 1);
    }

    #[test]
    fn test_list_groups() {
        let manager = create_test_manager();

        manager.add("flow-1", None, Some("Group A")).unwrap();
        manager.add("flow-2", None, Some("Group B")).unwrap();
        manager.add("flow-3", None, None).unwrap();

        let groups = manager.list_groups().unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups.contains(&"Group A".to_string()));
        assert!(groups.contains(&"Group B".to_string()));
    }

    #[test]
    fn test_is_bookmarked() {
        let manager = create_test_manager();

        manager.add("flow-1", None, None).unwrap();

        assert!(manager.is_bookmarked("flow-1").unwrap());
        assert!(!manager.is_bookmarked("flow-2").unwrap());
    }

    #[test]
    fn test_count() {
        let manager = create_test_manager();

        assert_eq!(manager.count().unwrap(), 0);

        manager.add("flow-1", None, None).unwrap();
        manager.add("flow-2", None, None).unwrap();

        assert_eq!(manager.count().unwrap(), 2);
    }

    #[test]
    fn test_bookmark_not_found() {
        let manager = create_test_manager();

        let result = manager.remove("non-existent");
        assert!(matches!(result, Err(BookmarkError::BookmarkNotFound(_))));
    }

    #[test]
    fn test_export_import() {
        let manager = create_test_manager();

        manager
            .add("flow-1", Some("Bookmark 1"), Some("Group"))
            .unwrap();
        manager.add("flow-2", Some("Bookmark 2"), None).unwrap();

        // 导出
        let exported = manager.export().unwrap();

        // 创建新管理器并导入
        let manager2 = create_test_manager();
        let imported = manager2.import(&exported, false).unwrap();

        assert_eq!(imported.len(), 2);

        // 验证导入的书签
        let bookmark1 = manager2.get_by_flow_id("flow-1").unwrap().unwrap();
        assert_eq!(bookmark1.name, Some("Bookmark 1".to_string()));
        assert_eq!(bookmark1.group, Some("Group".to_string()));
    }

    #[test]
    fn test_import_overwrite() {
        let manager = create_test_manager();

        manager.add("flow-1", Some("Original"), None).unwrap();

        // 创建导出数据
        let export_data = BookmarkExport::new(vec![FlowBookmark::new(
            "flow-1",
            Some("Updated".to_string()),
            Some("New Group".to_string()),
        )]);
        let json = serde_json::to_string(&export_data).unwrap();

        // 导入并覆盖
        manager.import(&json, true).unwrap();

        let bookmark = manager.get_by_flow_id("flow-1").unwrap().unwrap();
        assert_eq!(bookmark.name, Some("Updated".to_string()));
        assert_eq!(bookmark.group, Some("New Group".to_string()));
    }

    #[test]
    fn test_import_no_overwrite() {
        let manager = create_test_manager();

        manager.add("flow-1", Some("Original"), None).unwrap();

        // 创建导出数据
        let export_data = BookmarkExport::new(vec![FlowBookmark::new(
            "flow-1",
            Some("Updated".to_string()),
            None,
        )]);
        let json = serde_json::to_string(&export_data).unwrap();

        // 导入但不覆盖
        let imported = manager.import(&json, false).unwrap();
        assert!(imported.is_empty());

        let bookmark = manager.get_by_flow_id("flow-1").unwrap().unwrap();
        assert_eq!(bookmark.name, Some("Original".to_string()));
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

    /// 生成随机的 Flow ID
    fn arb_flow_id() -> impl Strategy<Value = String> {
        "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
    }

    /// 生成随机的书签名称
    fn arb_bookmark_name() -> impl Strategy<Value = Option<String>> {
        prop::option::of("[a-zA-Z0-9 _-]{1,50}")
    }

    /// 生成随机的分组名称
    fn arb_group_name() -> impl Strategy<Value = Option<String>> {
        prop::option::of("[a-zA-Z0-9 _-]{1,30}")
    }

    /// 生成随机的书签数据
    fn arb_bookmark_data() -> impl Strategy<Value = (String, Option<String>, Option<String>)> {
        (arb_flow_id(), arb_bookmark_name(), arb_group_name())
    }

    /// 生成多个书签数据
    fn arb_bookmarks(
        max_len: usize,
    ) -> impl Strategy<Value = Vec<(String, Option<String>, Option<String>)>> {
        prop::collection::vec(arb_bookmark_data(), 1..max_len)
    }

    // ========================================================================
    // 属性测试
    // ========================================================================

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: flow-monitor-enhancement, Property 15: 书签 Round-Trip**
        /// **Validates: Requirements 8.1**
        ///
        /// *对于任意* 书签操作，添加后应该能够正确检索到该书签。
        #[test]
        fn prop_bookmark_roundtrip(
            (flow_id, name, group) in arb_bookmark_data()
        ) {
            let manager = create_test_manager();

            // 添加书签
            let added = manager.add(&flow_id, name.as_deref(), group.as_deref()).unwrap();

            // 通过 ID 检索
            let retrieved_by_id = manager.get(&added.id).unwrap().unwrap();
            prop_assert_eq!(&added.id, &retrieved_by_id.id);
            prop_assert_eq!(&added.flow_id, &retrieved_by_id.flow_id);
            prop_assert_eq!(&added.name, &retrieved_by_id.name);
            prop_assert_eq!(&added.group, &retrieved_by_id.group);

            // 通过 Flow ID 检索
            let retrieved_by_flow = manager.get_by_flow_id(&flow_id).unwrap().unwrap();
            prop_assert_eq!(&added.id, &retrieved_by_flow.id);
            prop_assert_eq!(&added.flow_id, &retrieved_by_flow.flow_id);
        }

        /// **Feature: flow-monitor-enhancement, Property 16: 书签导入导出 Round-Trip**
        /// **Validates: Requirements 8.6**
        ///
        /// *对于任意* 书签集合，导出后再导入应该得到等价的集合。
        #[test]
        fn prop_bookmark_export_import_roundtrip(
            bookmarks in arb_bookmarks(10)
        ) {
            let manager1 = create_test_manager();

            // 添加所有书签（使用唯一的 flow_id）
            let mut added_bookmarks = Vec::new();
            for (i, (flow_id, name, group)) in bookmarks.iter().enumerate() {
                // 确保 flow_id 唯一
                let unique_flow_id = format!("{}_{}", flow_id, i);
                let bookmark = manager1.add(&unique_flow_id, name.as_deref(), group.as_deref()).unwrap();
                added_bookmarks.push(bookmark);
            }

            // 导出
            let exported = manager1.export().unwrap();

            // 创建新管理器并导入
            let manager2 = create_test_manager();
            let imported = manager2.import(&exported, false).unwrap();

            // 验证导入数量
            prop_assert_eq!(imported.len(), added_bookmarks.len());

            // 验证每个书签的内容
            for added in &added_bookmarks {
                let found = manager2.get_by_flow_id(&added.flow_id).unwrap();
                prop_assert!(found.is_some(), "Bookmark for flow '{}' should be imported", added.flow_id);

                let found = found.unwrap();
                prop_assert_eq!(&added.flow_id, &found.flow_id);
                prop_assert_eq!(&added.name, &found.name);
                prop_assert_eq!(&added.group, &found.group);
            }
        }

        /// 书签删除后应该不存在
        #[test]
        fn prop_bookmark_delete(
            (flow_id, name, group) in arb_bookmark_data()
        ) {
            let manager = create_test_manager();

            // 添加书签
            let bookmark = manager.add(&flow_id, name.as_deref(), group.as_deref()).unwrap();

            // 删除书签
            manager.remove(&bookmark.id).unwrap();

            // 验证不存在
            let found = manager.get(&bookmark.id).unwrap();
            prop_assert!(found.is_none());

            let found_by_flow = manager.get_by_flow_id(&flow_id).unwrap();
            prop_assert!(found_by_flow.is_none());
        }

        /// 书签更新后应该保持一致性
        #[test]
        fn prop_bookmark_update_consistency(
            (flow_id, name, group) in arb_bookmark_data(),
            (_, new_name, new_group) in arb_bookmark_data()
        ) {
            let manager = create_test_manager();

            // 添加书签
            let original = manager.add(&flow_id, name.as_deref(), group.as_deref()).unwrap();

            // 更新书签
            let updated = manager.update(
                &original.id,
                Some(new_name.as_deref()),
                Some(new_group.as_deref()),
            ).unwrap();

            // 验证更新后的值
            prop_assert_eq!(updated.id, original.id);
            prop_assert_eq!(updated.flow_id, original.flow_id);
            prop_assert_eq!(updated.name, new_name);
            prop_assert_eq!(updated.group, new_group);
        }

        /// 列表应该包含所有添加的书签
        #[test]
        fn prop_list_contains_all(
            bookmarks in arb_bookmarks(5)
        ) {
            let manager = create_test_manager();

            // 添加所有书签
            let mut added_ids = Vec::new();
            for (i, (flow_id, name, group)) in bookmarks.iter().enumerate() {
                let unique_flow_id = format!("{}_{}", flow_id, i);
                let bookmark = manager.add(&unique_flow_id, name.as_deref(), group.as_deref()).unwrap();
                added_ids.push(bookmark.id);
            }

            // 获取列表
            let list = manager.list(None).unwrap();

            // 验证所有添加的书签都在列表中
            for id in &added_ids {
                prop_assert!(
                    list.iter().any(|b| &b.id == id),
                    "Bookmark with id '{}' should be in list",
                    id
                );
            }
        }

        /// is_bookmarked 应该正确反映书签状态
        #[test]
        fn prop_is_bookmarked_consistency(
            (flow_id, name, group) in arb_bookmark_data()
        ) {
            let manager = create_test_manager();

            // 初始状态：未添加书签
            prop_assert!(!manager.is_bookmarked(&flow_id).unwrap());

            // 添加书签
            let bookmark = manager.add(&flow_id, name.as_deref(), group.as_deref()).unwrap();
            prop_assert!(manager.is_bookmarked(&flow_id).unwrap());

            // 删除书签
            manager.remove(&bookmark.id).unwrap();
            prop_assert!(!manager.is_bookmarked(&flow_id).unwrap());
        }
    }

    fn create_test_manager() -> BookmarkManager {
        let conn = Connection::open_in_memory().unwrap();
        BookmarkManager::from_connection(conn).unwrap()
    }
}
