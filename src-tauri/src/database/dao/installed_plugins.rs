//! 已安装插件数据访问对象
//!
//! 提供已安装插件的 CRUD 操作。
//! _需求: 1.2, 4.2_

#![allow(dead_code)]

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;

/// 安装来源
#[derive(Debug, Clone)]
pub enum InstallSource {
    /// 本地文件
    Local { path: String },
    /// URL 下载
    Url { url: String },
    /// GitHub release
    GitHub {
        owner: String,
        repo: String,
        tag: String,
    },
}

/// 已安装插件信息
#[derive(Debug, Clone)]
pub struct InstalledPluginRecord {
    /// 插件 ID
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 描述
    pub description: Option<String>,
    /// 作者
    pub author: Option<String>,
    /// 安装路径
    pub install_path: PathBuf,
    /// 安装时间
    pub installed_at: DateTime<Utc>,
    /// 安装来源
    pub source: InstallSource,
    /// 是否启用
    pub enabled: bool,
}

/// 数据库行结构
struct PluginRow {
    id: String,
    name: String,
    version: String,
    description: Option<String>,
    author: Option<String>,
    install_path: String,
    installed_at: String,
    source_type: String,
    source_data: Option<String>,
    enabled: i32,
}

impl PluginRow {
    fn into_record(self) -> Result<InstalledPluginRecord, String> {
        let source = deserialize_source(&self.source_type, self.source_data.as_deref())?;
        let installed_at = DateTime::parse_from_rfc3339(&self.installed_at)
            .map_err(|e| format!("无效的时间格式: {}", e))?
            .with_timezone(&Utc);

        Ok(InstalledPluginRecord {
            id: self.id,
            name: self.name,
            version: self.version,
            description: self.description,
            author: self.author,
            install_path: PathBuf::from(self.install_path),
            installed_at,
            source,
            enabled: self.enabled != 0,
        })
    }
}

/// 序列化安装来源
fn serialize_source(source: &InstallSource) -> (String, Option<String>) {
    match source {
        InstallSource::Local { path } => ("local".to_string(), Some(path.clone())),
        InstallSource::Url { url } => ("url".to_string(), Some(url.clone())),
        InstallSource::GitHub { owner, repo, tag } => {
            let data = serde_json::json!({
                "owner": owner,
                "repo": repo,
                "tag": tag
            });
            ("github".to_string(), Some(data.to_string()))
        }
    }
}

/// 反序列化安装来源
fn deserialize_source(
    source_type: &str,
    source_data: Option<&str>,
) -> Result<InstallSource, String> {
    match source_type {
        "local" => Ok(InstallSource::Local {
            path: source_data.unwrap_or_default().to_string(),
        }),
        "url" => Ok(InstallSource::Url {
            url: source_data.unwrap_or_default().to_string(),
        }),
        "github" => {
            let data: serde_json::Value = serde_json::from_str(source_data.unwrap_or("{}"))
                .map_err(|e| format!("JSON 解析错误: {}", e))?;
            Ok(InstallSource::GitHub {
                owner: data["owner"].as_str().unwrap_or_default().to_string(),
                repo: data["repo"].as_str().unwrap_or_default().to_string(),
                tag: data["tag"].as_str().unwrap_or_default().to_string(),
            })
        }
        _ => Err(format!("未知的来源类型: {}", source_type)),
    }
}

pub struct InstalledPluginsDao;

impl InstalledPluginsDao {
    /// 注册插件
    ///
    /// _需求: 1.2_
    pub fn register(
        conn: &Connection,
        plugin: &InstalledPluginRecord,
    ) -> Result<(), rusqlite::Error> {
        let (source_type, source_data) = serialize_source(&plugin.source);

        conn.execute(
            "INSERT OR REPLACE INTO installed_plugins 
             (id, name, version, description, author, install_path, installed_at, source_type, source_data, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                plugin.id,
                plugin.name,
                plugin.version,
                plugin.description,
                plugin.author,
                plugin.install_path.to_string_lossy().to_string(),
                plugin.installed_at.to_rfc3339(),
                source_type,
                source_data,
                plugin.enabled as i32,
            ],
        )?;

        Ok(())
    }

    /// 注销插件
    ///
    /// _需求: 4.2_
    pub fn unregister(conn: &Connection, plugin_id: &str) -> Result<bool, rusqlite::Error> {
        let rows_affected = conn.execute(
            "DELETE FROM installed_plugins WHERE id = ?1",
            params![plugin_id],
        )?;

        Ok(rows_affected > 0)
    }

    /// 获取插件信息
    pub fn get(
        conn: &Connection,
        plugin_id: &str,
    ) -> Result<Option<InstalledPluginRecord>, String> {
        let result = conn
            .query_row(
                "SELECT id, name, version, description, author, install_path, installed_at, source_type, source_data, enabled
                 FROM installed_plugins WHERE id = ?1",
                params![plugin_id],
                |row| {
                    Ok(PluginRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        version: row.get(2)?,
                        description: row.get(3)?,
                        author: row.get(4)?,
                        install_path: row.get(5)?,
                        installed_at: row.get(6)?,
                        source_type: row.get(7)?,
                        source_data: row.get(8)?,
                        enabled: row.get(9)?,
                    })
                },
            )
            .optional()
            .map_err(|e| format!("数据库错误: {}", e))?;

        match result {
            Some(row) => Ok(Some(row.into_record()?)),
            None => Ok(None),
        }
    }

    /// 列出所有插件
    pub fn list(conn: &Connection) -> Result<Vec<InstalledPluginRecord>, String> {
        let mut stmt = conn
            .prepare(
                "SELECT id, name, version, description, author, install_path, installed_at, source_type, source_data, enabled
                 FROM installed_plugins ORDER BY installed_at DESC",
            )
            .map_err(|e| format!("数据库错误: {}", e))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(PluginRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    version: row.get(2)?,
                    description: row.get(3)?,
                    author: row.get(4)?,
                    install_path: row.get(5)?,
                    installed_at: row.get(6)?,
                    source_type: row.get(7)?,
                    source_data: row.get(8)?,
                    enabled: row.get(9)?,
                })
            })
            .map_err(|e| format!("数据库错误: {}", e))?;

        let mut plugins = Vec::new();
        for row in rows {
            let row = row.map_err(|e| format!("数据库错误: {}", e))?;
            plugins.push(row.into_record()?);
        }

        Ok(plugins)
    }

    /// 检查插件是否存在
    pub fn exists(conn: &Connection, plugin_id: &str) -> Result<bool, rusqlite::Error> {
        let count: i32 = conn.query_row(
            "SELECT COUNT(*) FROM installed_plugins WHERE id = ?1",
            params![plugin_id],
            |row| row.get(0),
        )?;

        Ok(count > 0)
    }

    /// 更新插件启用状态
    pub fn set_enabled(
        conn: &Connection,
        plugin_id: &str,
        enabled: bool,
    ) -> Result<bool, rusqlite::Error> {
        let rows_affected = conn.execute(
            "UPDATE installed_plugins SET enabled = ?1 WHERE id = ?2",
            params![enabled as i32, plugin_id],
        )?;

        Ok(rows_affected > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_connection() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS installed_plugins (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                version TEXT NOT NULL,
                description TEXT,
                author TEXT,
                install_path TEXT NOT NULL,
                installed_at TEXT NOT NULL,
                source_type TEXT NOT NULL,
                source_data TEXT,
                enabled INTEGER DEFAULT 1
            )",
            [],
        )
        .unwrap();
        conn
    }

    fn create_test_plugin(id: &str) -> InstalledPluginRecord {
        InstalledPluginRecord {
            id: id.to_string(),
            name: format!("Test Plugin {}", id),
            version: "1.0.0".to_string(),
            description: Some("A test plugin".to_string()),
            author: Some("Test Author".to_string()),
            install_path: PathBuf::from(format!("/plugins/{}", id)),
            installed_at: Utc::now(),
            source: InstallSource::Local {
                path: "/tmp/plugin.zip".to_string(),
            },
            enabled: true,
        }
    }

    #[test]
    fn test_register_and_get() {
        let conn = create_test_connection();
        let plugin = create_test_plugin("test-1");

        InstalledPluginsDao::register(&conn, &plugin).unwrap();

        let retrieved = InstalledPluginsDao::get(&conn, "test-1").unwrap().unwrap();
        assert_eq!(retrieved.id, "test-1");
        assert_eq!(retrieved.name, "Test Plugin test-1");
        assert_eq!(retrieved.version, "1.0.0");
    }

    #[test]
    fn test_unregister() {
        let conn = create_test_connection();
        let plugin = create_test_plugin("test-2");

        InstalledPluginsDao::register(&conn, &plugin).unwrap();
        assert!(InstalledPluginsDao::exists(&conn, "test-2").unwrap());

        let deleted = InstalledPluginsDao::unregister(&conn, "test-2").unwrap();
        assert!(deleted);
        assert!(!InstalledPluginsDao::exists(&conn, "test-2").unwrap());
    }

    #[test]
    fn test_unregister_not_found() {
        let conn = create_test_connection();
        let deleted = InstalledPluginsDao::unregister(&conn, "non-existent").unwrap();
        assert!(!deleted);
    }

    #[test]
    fn test_list() {
        let conn = create_test_connection();

        InstalledPluginsDao::register(&conn, &create_test_plugin("test-a")).unwrap();
        InstalledPluginsDao::register(&conn, &create_test_plugin("test-b")).unwrap();

        let plugins = InstalledPluginsDao::list(&conn).unwrap();
        assert_eq!(plugins.len(), 2);
    }

    #[test]
    fn test_set_enabled() {
        let conn = create_test_connection();
        let plugin = create_test_plugin("test-3");

        InstalledPluginsDao::register(&conn, &plugin).unwrap();

        InstalledPluginsDao::set_enabled(&conn, "test-3", false).unwrap();
        let retrieved = InstalledPluginsDao::get(&conn, "test-3").unwrap().unwrap();
        assert!(!retrieved.enabled);

        InstalledPluginsDao::set_enabled(&conn, "test-3", true).unwrap();
        let retrieved = InstalledPluginsDao::get(&conn, "test-3").unwrap().unwrap();
        assert!(retrieved.enabled);
    }

    #[test]
    fn test_github_source_serialization() {
        let conn = create_test_connection();
        let mut plugin = create_test_plugin("test-github");
        plugin.source = InstallSource::GitHub {
            owner: "user".to_string(),
            repo: "repo".to_string(),
            tag: "v1.0.0".to_string(),
        };

        InstalledPluginsDao::register(&conn, &plugin).unwrap();

        let retrieved = InstalledPluginsDao::get(&conn, "test-github")
            .unwrap()
            .unwrap();
        match retrieved.source {
            InstallSource::GitHub { owner, repo, tag } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert_eq!(tag, "v1.0.0");
            }
            _ => panic!("Expected GitHub source"),
        }
    }
}
