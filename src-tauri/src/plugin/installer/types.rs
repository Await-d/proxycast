//! 插件安装器类型定义
//!
//! 定义安装相关的错误类型、进度类型和数据结构

#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// 安装错误类型
///
/// 定义所有安装相关的错误变体
/// _需求: 1.4, 2.3, 3.4_
#[derive(Error, Debug)]
pub enum InstallError {
    /// 下载失败
    #[error("下载失败: {0}")]
    DownloadFailed(String),

    /// 包格式无效
    #[error("包格式无效: {0}")]
    InvalidPackage(String),

    /// 清单无效
    #[error("清单无效: {0}")]
    InvalidManifest(String),

    /// 解压失败
    #[error("解压失败: {0}")]
    ExtractFailed(String),

    /// 安装失败
    #[error("安装失败: {0}")]
    InstallFailed(String),

    /// 插件已存在
    #[error("插件已存在: {0}")]
    AlreadyExists(String),

    /// 插件不存在
    #[error("插件不存在: {0}")]
    NotFound(String),

    /// 验证失败
    #[error("验证失败: {0}")]
    ValidationFailed(String),

    /// 校验和不匹配
    #[error("校验和不匹配: 期望 {expected}, 实际 {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// IO 错误
    #[error("IO 错误: {0}")]
    IoError(#[from] std::io::Error),

    /// 网络错误
    #[error("网络错误: {0}")]
    NetworkError(String),

    /// JSON 解析错误
    #[error("JSON 解析错误: {0}")]
    JsonError(#[from] serde_json::Error),

    /// 数据库错误
    #[error("数据库错误: {0}")]
    DatabaseError(String),

    /// URL 解析错误
    #[error("URL 解析错误: {0}")]
    UrlParseError(String),

    /// 不支持的平台
    #[error("不支持的平台: {0}")]
    UnsupportedPlatform(String),
}

/// 安装阶段
///
/// 表示安装过程中的各个阶段
/// _需求: 2.4, 3.1, 3.2_
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallStage {
    /// 下载中
    Downloading,
    /// 验证中
    Validating,
    /// 解压中
    Extracting,
    /// 安装中
    Installing,
    /// 注册中
    Registering,
    /// 完成
    Complete,
    /// 失败
    Failed,
}

impl std::fmt::Display for InstallStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallStage::Downloading => write!(f, "downloading"),
            InstallStage::Validating => write!(f, "validating"),
            InstallStage::Extracting => write!(f, "extracting"),
            InstallStage::Installing => write!(f, "installing"),
            InstallStage::Registering => write!(f, "registering"),
            InstallStage::Complete => write!(f, "complete"),
            InstallStage::Failed => write!(f, "failed"),
        }
    }
}

/// 安装进度
///
/// 表示安装过程中的进度信息
/// _需求: 2.4, 3.1, 3.2_
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    /// 当前阶段
    pub stage: InstallStage,
    /// 进度百分比 (0-100)
    pub percent: u8,
    /// 状态消息
    pub message: String,
}

impl InstallProgress {
    /// 创建新的进度实例
    pub fn new(stage: InstallStage, percent: u8, message: impl Into<String>) -> Self {
        Self {
            stage,
            percent: percent.min(100),
            message: message.into(),
        }
    }

    /// 创建下载阶段进度
    pub fn downloading(percent: u8, message: impl Into<String>) -> Self {
        Self::new(InstallStage::Downloading, percent, message)
    }

    /// 创建验证阶段进度
    pub fn validating(message: impl Into<String>) -> Self {
        Self::new(InstallStage::Validating, 0, message)
    }

    /// 创建解压阶段进度
    pub fn extracting(percent: u8, message: impl Into<String>) -> Self {
        Self::new(InstallStage::Extracting, percent, message)
    }

    /// 创建安装阶段进度
    pub fn installing(percent: u8, message: impl Into<String>) -> Self {
        Self::new(InstallStage::Installing, percent, message)
    }

    /// 创建注册阶段进度
    pub fn registering(message: impl Into<String>) -> Self {
        Self::new(InstallStage::Registering, 90, message)
    }

    /// 创建完成状态
    pub fn complete(message: impl Into<String>) -> Self {
        Self::new(InstallStage::Complete, 100, message)
    }

    /// 创建失败状态
    pub fn failed(message: impl Into<String>) -> Self {
        Self::new(InstallStage::Failed, 0, message)
    }
}

/// 进度回调 trait
///
/// 用于接收安装进度更新
pub trait ProgressCallback: Send + Sync {
    /// 进度更新回调
    fn on_progress(&self, progress: InstallProgress);
}

/// 空进度回调实现
///
/// 用于不需要进度回调的场景
pub struct NoopProgressCallback;

impl ProgressCallback for NoopProgressCallback {
    fn on_progress(&self, _progress: InstallProgress) {
        // 不做任何事
    }
}

/// 函数进度回调实现
///
/// 将闭包包装为 ProgressCallback
pub struct FnProgressCallback<F>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    callback: F,
}

impl<F> FnProgressCallback<F>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    /// 创建新的函数回调
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> ProgressCallback for FnProgressCallback<F>
where
    F: Fn(InstallProgress) + Send + Sync,
{
    fn on_progress(&self, progress: InstallProgress) {
        (self.callback)(progress);
    }
}

/// 包格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    /// ZIP 格式
    Zip,
    /// tar.gz 格式
    TarGz,
}

impl PackageFormat {
    /// 从文件扩展名检测格式
    pub fn from_extension(path: &std::path::Path) -> Option<Self> {
        let file_name = path.file_name()?.to_str()?;
        if file_name.ends_with(".tar.gz") || file_name.ends_with(".tgz") {
            Some(PackageFormat::TarGz)
        } else if file_name.ends_with(".zip") {
            Some(PackageFormat::Zip)
        } else {
            None
        }
    }
}

/// 安装来源
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InstallSource {
    /// 本地文件
    Local {
        /// 原始文件路径
        path: String,
    },
    /// URL 下载
    Url {
        /// 下载 URL
        url: String,
    },
    /// GitHub release
    GitHub {
        /// 仓库 owner
        owner: String,
        /// 仓库名
        repo: String,
        /// release tag
        tag: String,
    },
}

/// GitHub release 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubRelease {
    /// 仓库 owner
    pub owner: String,
    /// 仓库名
    pub repo: String,
    /// release tag
    pub tag: String,
    /// 资产文件名
    pub asset_name: Option<String>,
}

impl GitHubRelease {
    /// 构建下载 URL
    pub fn download_url(&self, asset_name: &str) -> String {
        format!(
            "https://github.com/{}/{}/releases/download/{}/{}",
            self.owner, self.repo, self.tag, asset_name
        )
    }

    /// 构建 API URL（获取 release 信息）
    pub fn api_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/releases/tags/{}",
            self.owner, self.repo, self.tag
        )
    }

    /// 构建最新 release API URL
    pub fn latest_api_url(&self) -> String {
        format!(
            "https://api.github.com/repos/{}/{}/releases/latest",
            self.owner, self.repo
        )
    }
}

/// 已安装插件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// 插件 ID (通常是 name)
    pub id: String,
    /// 插件名称
    pub name: String,
    /// 版本
    pub version: String,
    /// 描述
    pub description: String,
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

impl InstalledPlugin {
    /// 创建新的已安装插件信息
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
        install_path: PathBuf,
        source: InstallSource,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: description.into(),
            author: None,
            install_path,
            installed_at: Utc::now(),
            source,
            enabled: true,
        }
    }

    /// 设置作者
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// 设置启用状态
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_progress_creation() {
        let progress = InstallProgress::downloading(50, "下载中...");
        assert_eq!(progress.stage, InstallStage::Downloading);
        assert_eq!(progress.percent, 50);
        assert_eq!(progress.message, "下载中...");
    }

    #[test]
    fn test_install_progress_percent_capped() {
        let progress = InstallProgress::new(InstallStage::Installing, 150, "测试");
        assert_eq!(progress.percent, 100);
    }

    #[test]
    fn test_package_format_detection() {
        use std::path::Path;

        assert_eq!(
            PackageFormat::from_extension(Path::new("plugin.zip")),
            Some(PackageFormat::Zip)
        );
        assert_eq!(
            PackageFormat::from_extension(Path::new("plugin.tar.gz")),
            Some(PackageFormat::TarGz)
        );
        assert_eq!(
            PackageFormat::from_extension(Path::new("plugin.tgz")),
            Some(PackageFormat::TarGz)
        );
        assert_eq!(PackageFormat::from_extension(Path::new("plugin.txt")), None);
    }

    #[test]
    fn test_github_release_urls() {
        let release = GitHubRelease {
            owner: "user".to_string(),
            repo: "repo".to_string(),
            tag: "v1.0.0".to_string(),
            asset_name: None,
        };

        assert_eq!(
            release.download_url("plugin.zip"),
            "https://github.com/user/repo/releases/download/v1.0.0/plugin.zip"
        );
        assert_eq!(
            release.api_url(),
            "https://api.github.com/repos/user/repo/releases/tags/v1.0.0"
        );
    }

    #[test]
    fn test_install_source_serialization() {
        let source = InstallSource::GitHub {
            owner: "user".to_string(),
            repo: "repo".to_string(),
            tag: "v1.0.0".to_string(),
        };

        let json = serde_json::to_string(&source).unwrap();
        assert!(json.contains("\"type\":\"github\""));

        let parsed: InstallSource = serde_json::from_str(&json).unwrap();
        match parsed {
            InstallSource::GitHub { owner, repo, tag } => {
                assert_eq!(owner, "user");
                assert_eq!(repo, "repo");
                assert_eq!(tag, "v1.0.0");
            }
            _ => panic!("Expected GitHub source"),
        }
    }
}
