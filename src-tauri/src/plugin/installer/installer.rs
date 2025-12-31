//! 插件安装器核心实现
//!
//! 提供插件安装、卸载的核心逻辑：
//! - install_from_file: 从本地文件安装
//! - install_from_url: 从 URL 下载安装
//! - uninstall: 卸载插件
//!
//! _需求: 1.1, 1.2, 1.3, 2.1, 2.2, 4.2_

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use super::downloader::PluginDownloader;
use super::registry::PluginRegistry;
use super::types::{
    InstallError, InstallProgress, InstallSource, InstalledPlugin, PackageFormat, ProgressCallback,
};
use super::validator::PackageValidator;

/// 插件安装器
///
/// 负责协调整个安装流程
pub struct PluginInstaller {
    /// 插件目录
    plugins_dir: PathBuf,
    /// 临时目录
    temp_dir: PathBuf,
    /// 注册表
    registry: PluginRegistry,
    /// 下载器
    downloader: PluginDownloader,
    /// 验证器
    validator: PackageValidator,
}

impl PluginInstaller {
    /// 创建新的安装器实例
    pub fn new(plugins_dir: PathBuf, temp_dir: PathBuf, db_conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            plugins_dir,
            temp_dir,
            registry: PluginRegistry::new(db_conn),
            downloader: PluginDownloader::new(),
            validator: PackageValidator::new(),
        }
    }

    /// 从数据库路径创建安装器
    pub fn from_paths(
        plugins_dir: PathBuf,
        temp_dir: PathBuf,
        db_path: &Path,
    ) -> Result<Self, InstallError> {
        let registry = PluginRegistry::from_path(db_path)?;
        registry.init_tables()?;

        Ok(Self {
            plugins_dir,
            temp_dir,
            registry,
            downloader: PluginDownloader::new(),
            validator: PackageValidator::new(),
        })
    }

    /// 从本地文件安装插件
    ///
    /// 流程: 验证 → 解压 → 注册 → 复制文件
    /// _需求: 1.1, 1.2, 1.3_
    pub async fn install_from_file(
        &self,
        path: &Path,
        progress: &dyn ProgressCallback,
    ) -> Result<InstalledPlugin, InstallError> {
        // 阶段 1: 验证包格式
        progress.on_progress(InstallProgress::validating("验证包格式..."));
        let format = self.validator.validate_format(path)?;

        // 阶段 2: 提取并验证清单
        progress.on_progress(InstallProgress::validating("验证清单文件..."));
        let manifest = self.validator.extract_and_validate_manifest(path, format)?;

        // 检查插件是否已存在
        if self.registry.exists(&manifest.name)? {
            return Err(InstallError::AlreadyExists(manifest.name.clone()));
        }

        // 阶段 3: 解压到临时目录
        progress.on_progress(InstallProgress::extracting(0, "解压插件包..."));
        let temp_extract_dir = self.extract_package(path, format, progress)?;

        // 阶段 4: 复制文件到插件目录
        progress.on_progress(InstallProgress::installing(50, "安装插件文件..."));
        let install_path = self.copy_to_plugins_dir(&manifest.name, &temp_extract_dir, progress)?;

        // 阶段 5: 注册插件
        progress.on_progress(InstallProgress::registering("注册插件..."));
        let installed_plugin = InstalledPlugin::new(
            manifest.name.clone(),
            manifest.name.clone(),
            manifest.version.clone(),
            manifest.description.clone(),
            install_path.clone(),
            InstallSource::Local {
                path: path.to_string_lossy().to_string(),
            },
        )
        .with_author(manifest.author.clone().unwrap_or_default());

        self.registry.register(&installed_plugin)?;

        // 清理临时目录
        let _ = fs::remove_dir_all(&temp_extract_dir);

        progress.on_progress(InstallProgress::complete(format!(
            "插件 {} v{} 安装成功",
            manifest.name, manifest.version
        )));

        Ok(installed_plugin)
    }

    /// 从 URL 安装插件
    ///
    /// 流程: 下载 → 验证 → 解压 → 注册 → 复制文件
    /// _需求: 2.1, 2.2_
    pub async fn install_from_url(
        &self,
        url: &str,
        progress: &dyn ProgressCallback,
    ) -> Result<InstalledPlugin, InstallError> {
        // 确保临时目录存在
        fs::create_dir_all(&self.temp_dir)?;

        // 阶段 1: 下载插件包
        let download_path = self.temp_dir.join("download_package.zip");
        self.downloader
            .download(url, &download_path, progress)
            .await?;

        // 阶段 2: 验证包格式
        progress.on_progress(InstallProgress::validating("验证包格式..."));
        let format = self.validator.validate_format(&download_path)?;

        // 阶段 3: 提取并验证清单
        progress.on_progress(InstallProgress::validating("验证清单文件..."));
        let manifest = self
            .validator
            .extract_and_validate_manifest(&download_path, format)?;

        // 检查插件是否已存在
        if self.registry.exists(&manifest.name)? {
            // 清理下载文件
            let _ = fs::remove_file(&download_path);
            return Err(InstallError::AlreadyExists(manifest.name.clone()));
        }

        // 阶段 4: 解压到临时目录
        progress.on_progress(InstallProgress::extracting(0, "解压插件包..."));
        let temp_extract_dir = self.extract_package(&download_path, format, progress)?;

        // 阶段 5: 复制文件到插件目录
        progress.on_progress(InstallProgress::installing(50, "安装插件文件..."));
        let install_path = self.copy_to_plugins_dir(&manifest.name, &temp_extract_dir, progress)?;

        // 阶段 6: 注册插件
        progress.on_progress(InstallProgress::registering("注册插件..."));

        // 解析安装来源
        let source = if let Ok(github_release) = self.downloader.parse_github_url(url) {
            InstallSource::GitHub {
                owner: github_release.owner,
                repo: github_release.repo,
                tag: github_release.tag,
            }
        } else {
            InstallSource::Url {
                url: url.to_string(),
            }
        };

        let installed_plugin = InstalledPlugin::new(
            manifest.name.clone(),
            manifest.name.clone(),
            manifest.version.clone(),
            manifest.description.clone(),
            install_path.clone(),
            source,
        )
        .with_author(manifest.author.clone().unwrap_or_default());

        self.registry.register(&installed_plugin)?;

        // 清理临时文件
        let _ = fs::remove_file(&download_path);
        let _ = fs::remove_dir_all(&temp_extract_dir);

        progress.on_progress(InstallProgress::complete(format!(
            "插件 {} v{} 安装成功",
            manifest.name, manifest.version
        )));

        Ok(installed_plugin)
    }

    /// 卸载插件
    ///
    /// 流程: 删除文件 → 注销注册表
    /// _需求: 4.2_
    pub async fn uninstall(&self, plugin_id: &str) -> Result<(), InstallError> {
        // 获取插件信息
        let plugin = self
            .registry
            .get(plugin_id)?
            .ok_or_else(|| InstallError::NotFound(plugin_id.to_string()))?;

        // 删除插件文件
        if plugin.install_path.exists() {
            fs::remove_dir_all(&plugin.install_path)?;
        }

        // 注销注册表
        self.registry.unregister(plugin_id)?;

        Ok(())
    }

    /// 获取已安装插件列表
    pub fn list_installed(&self) -> Result<Vec<InstalledPlugin>, InstallError> {
        self.registry.list()
    }

    /// 获取插件信息
    pub fn get_plugin(&self, plugin_id: &str) -> Result<Option<InstalledPlugin>, InstallError> {
        self.registry.get(plugin_id)
    }

    /// 检查插件是否已安装
    pub fn is_installed(&self, plugin_id: &str) -> Result<bool, InstallError> {
        self.registry.exists(plugin_id)
    }

    /// 解压插件包到临时目录
    fn extract_package(
        &self,
        path: &Path,
        format: PackageFormat,
        progress: &dyn ProgressCallback,
    ) -> Result<PathBuf, InstallError> {
        // 创建临时解压目录
        let extract_dir = self.temp_dir.join(format!(
            "extract_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        fs::create_dir_all(&extract_dir)?;

        match format {
            PackageFormat::Zip => self.extract_zip(path, &extract_dir, progress)?,
            PackageFormat::TarGz => self.extract_targz(path, &extract_dir, progress)?,
        }

        // 查找实际的插件根目录（可能在子目录中）
        let plugin_root = self.find_plugin_root(&extract_dir)?;

        Ok(plugin_root)
    }

    /// 解压 ZIP 文件
    fn extract_zip(
        &self,
        path: &Path,
        dest: &Path,
        progress: &dyn ProgressCallback,
    ) -> Result<(), InstallError> {
        let file = File::open(path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| InstallError::ExtractFailed(format!("无法读取 ZIP 文件: {}", e)))?;

        let total = archive.len();
        for i in 0..total {
            let mut file = archive.by_index(i).map_err(|e| {
                InstallError::ExtractFailed(format!("无法读取 ZIP 条目 {}: {}", i, e))
            })?;

            let outpath = match file.enclosed_name() {
                Some(path) => dest.join(path),
                None => continue,
            };

            // 跳过 macOS 元数据
            if outpath.to_string_lossy().contains("__MACOSX") {
                continue;
            }

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;

                // 设置文件权限 (Unix)
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = file.unix_mode() {
                        fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                    }
                }
            }

            // 更新进度
            let percent = ((i + 1) as f64 / total as f64 * 100.0) as u8;
            progress.on_progress(InstallProgress::extracting(
                percent,
                format!("解压中 ({}/{})", i + 1, total),
            ));
        }

        Ok(())
    }

    /// 解压 tar.gz 文件
    fn extract_targz(
        &self,
        path: &Path,
        dest: &Path,
        progress: &dyn ProgressCallback,
    ) -> Result<(), InstallError> {
        let file = File::open(path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        // 先计算总条目数
        let file_for_count = File::open(path)?;
        let gz_for_count = flate2::read::GzDecoder::new(file_for_count);
        let mut archive_for_count = tar::Archive::new(gz_for_count);
        let total = archive_for_count
            .entries()
            .map_err(|e| InstallError::ExtractFailed(format!("无法读取 tar.gz: {}", e)))?
            .count();

        let mut count = 0;
        for entry in archive
            .entries()
            .map_err(|e| InstallError::ExtractFailed(format!("无法读取 tar.gz: {}", e)))?
        {
            let mut entry = entry
                .map_err(|e| InstallError::ExtractFailed(format!("tar.gz 条目读取失败: {}", e)))?;

            entry
                .unpack_in(dest)
                .map_err(|e| InstallError::ExtractFailed(format!("解压失败: {}", e)))?;

            count += 1;
            let percent = (count as f64 / total as f64 * 100.0) as u8;
            progress.on_progress(InstallProgress::extracting(
                percent,
                format!("解压中 ({}/{})", count, total),
            ));
        }

        Ok(())
    }

    /// 查找插件根目录（包含 plugin.json 的目录）
    fn find_plugin_root(&self, extract_dir: &Path) -> Result<PathBuf, InstallError> {
        // 首先检查根目录
        if extract_dir.join("plugin.json").exists() {
            return Ok(extract_dir.to_path_buf());
        }

        // 检查一级子目录
        for entry in fs::read_dir(extract_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join("plugin.json").exists() {
                return Ok(path);
            }
        }

        Err(InstallError::InvalidPackage(
            "未找到 plugin.json 文件".to_string(),
        ))
    }

    /// 复制文件到插件目录
    fn copy_to_plugins_dir(
        &self,
        plugin_name: &str,
        source_dir: &Path,
        progress: &dyn ProgressCallback,
    ) -> Result<PathBuf, InstallError> {
        let dest_dir = self.plugins_dir.join(plugin_name);

        // 如果目标目录已存在，先删除
        if dest_dir.exists() {
            fs::remove_dir_all(&dest_dir)?;
        }

        // 创建目标目录
        fs::create_dir_all(&dest_dir)?;

        // 复制所有文件
        self.copy_dir_recursive(source_dir, &dest_dir, progress)?;

        Ok(dest_dir)
    }

    /// 递归复制目录
    fn copy_dir_recursive(
        &self,
        src: &Path,
        dst: &Path,
        progress: &dyn ProgressCallback,
    ) -> Result<(), InstallError> {
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());

            if src_path.is_dir() {
                fs::create_dir_all(&dst_path)?;
                self.copy_dir_recursive(&src_path, &dst_path, progress)?;
            } else {
                fs::copy(&src_path, &dst_path)?;

                // 保持可执行权限
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let metadata = fs::metadata(&src_path)?;
                    let mode = metadata.permissions().mode();
                    if mode & 0o111 != 0 {
                        // 如果源文件可执行
                        fs::set_permissions(&dst_path, fs::Permissions::from_mode(mode))?;
                    }
                }
            }
        }

        Ok(())
    }

    /// 获取注册表引用
    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    /// 获取下载器引用
    pub fn downloader(&self) -> &PluginDownloader {
        &self.downloader
    }

    /// 获取验证器引用
    pub fn validator(&self) -> &PackageValidator {
        &self.validator
    }

    /// 获取插件目录
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// 获取临时目录
    pub fn temp_dir(&self) -> &Path {
        &self.temp_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::installer::NoopProgressCallback;
    use std::io::Write;
    use tempfile::TempDir;

    /// 创建测试用的安装器
    fn create_test_installer() -> (PluginInstaller, TempDir, TempDir, TempDir) {
        let plugins_dir = TempDir::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("test.db");

        let installer = PluginInstaller::from_paths(
            plugins_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            &db_path,
        )
        .unwrap();

        // 返回所有 TempDir 以保持它们存活
        (installer, plugins_dir, temp_dir, db_dir)
    }

    /// 创建有效的测试插件包 (ZIP)
    fn create_test_plugin_zip(dir: &Path, name: &str, version: &str) -> PathBuf {
        let file_path = dir.join(format!("{}.zip", name));

        let manifest_json = format!(
            r#"{{
                "name": "{}",
                "version": "{}",
                "description": "Test plugin",
                "entry": "config.json",
                "plugin_type": "script",
                "hooks": []
            }}"#,
            name, version
        );

        let config_json = r#"{"enabled": true}"#;

        let file = File::create(&file_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file("plugin.json", options).unwrap();
        zip.write_all(manifest_json.as_bytes()).unwrap();

        zip.start_file("config.json", options).unwrap();
        zip.write_all(config_json.as_bytes()).unwrap();

        zip.finish().unwrap();

        file_path
    }

    #[tokio::test]
    async fn test_install_from_file_success() {
        let (installer, plugins_dir, temp_dir, _db_dir) = create_test_installer();
        let package_path = create_test_plugin_zip(temp_dir.path(), "test-plugin", "1.0.0");

        let progress = NoopProgressCallback;
        let result = installer.install_from_file(&package_path, &progress).await;

        assert!(result.is_ok(), "安装应该成功: {:?}", result);

        let installed = result.unwrap();
        assert_eq!(installed.name, "test-plugin");
        assert_eq!(installed.version, "1.0.0");

        // 验证文件已复制
        let plugin_dir = plugins_dir.path().join("test-plugin");
        assert!(plugin_dir.exists(), "插件目录应该存在");
        assert!(
            plugin_dir.join("plugin.json").exists(),
            "plugin.json 应该存在"
        );

        // 验证注册表
        assert!(installer.is_installed("test-plugin").unwrap());
    }

    #[tokio::test]
    async fn test_install_from_file_already_exists() {
        let (installer, _plugins_dir, temp_dir, _db_dir) = create_test_installer();
        let package_path = create_test_plugin_zip(temp_dir.path(), "duplicate-plugin", "1.0.0");

        let progress = NoopProgressCallback;

        // 第一次安装
        let result1 = installer.install_from_file(&package_path, &progress).await;
        assert!(result1.is_ok());

        // 第二次安装应该失败
        let result2 = installer.install_from_file(&package_path, &progress).await;
        assert!(result2.is_err());
        match result2.unwrap_err() {
            InstallError::AlreadyExists(name) => {
                assert_eq!(name, "duplicate-plugin");
            }
            e => panic!("期望 AlreadyExists 错误，实际: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_install_from_file_invalid_package() {
        let (installer, _plugins_dir, temp_dir, _db_dir) = create_test_installer();

        // 创建无效的包（不是 ZIP 格式）
        let invalid_path = temp_dir.path().join("invalid.zip");
        fs::write(&invalid_path, "not a zip file").unwrap();

        let progress = NoopProgressCallback;
        let result = installer.install_from_file(&invalid_path, &progress).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            InstallError::InvalidPackage(_) => {}
            e => panic!("期望 InvalidPackage 错误，实际: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_uninstall_success() {
        let (installer, plugins_dir, temp_dir, _db_dir) = create_test_installer();
        let package_path = create_test_plugin_zip(temp_dir.path(), "uninstall-test", "1.0.0");

        let progress = NoopProgressCallback;

        // 先安装
        installer
            .install_from_file(&package_path, &progress)
            .await
            .unwrap();

        // 验证已安装
        assert!(installer.is_installed("uninstall-test").unwrap());
        let plugin_dir = plugins_dir.path().join("uninstall-test");
        assert!(plugin_dir.exists());

        // 卸载
        let result = installer.uninstall("uninstall-test").await;
        assert!(result.is_ok(), "卸载应该成功: {:?}", result);

        // 验证已卸载
        assert!(!installer.is_installed("uninstall-test").unwrap());
        assert!(!plugin_dir.exists(), "插件目录应该被删除");
    }

    #[tokio::test]
    async fn test_uninstall_not_found() {
        let (installer, _plugins_dir, _temp_dir, _db_dir) = create_test_installer();

        let result = installer.uninstall("non-existent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            InstallError::NotFound(name) => {
                assert_eq!(name, "non-existent");
            }
            e => panic!("期望 NotFound 错误，实际: {:?}", e),
        }
    }

    #[tokio::test]
    async fn test_list_installed() {
        let (installer, _plugins_dir, temp_dir, _db_dir) = create_test_installer();

        let progress = NoopProgressCallback;

        // 安装多个插件
        let pkg1 = create_test_plugin_zip(temp_dir.path(), "plugin-a", "1.0.0");
        let pkg2 = create_test_plugin_zip(temp_dir.path(), "plugin-b", "2.0.0");

        installer.install_from_file(&pkg1, &progress).await.unwrap();
        installer.install_from_file(&pkg2, &progress).await.unwrap();

        // 列出已安装插件
        let plugins = installer.list_installed().unwrap();
        assert_eq!(plugins.len(), 2);

        let names: Vec<&str> = plugins.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"plugin-a"));
        assert!(names.contains(&"plugin-b"));
    }

    #[tokio::test]
    async fn test_get_plugin() {
        let (installer, _plugins_dir, temp_dir, _db_dir) = create_test_installer();
        let package_path = create_test_plugin_zip(temp_dir.path(), "get-test", "1.2.3");

        let progress = NoopProgressCallback;
        installer
            .install_from_file(&package_path, &progress)
            .await
            .unwrap();

        // 获取存在的插件
        let plugin = installer.get_plugin("get-test").unwrap();
        assert!(plugin.is_some());
        let plugin = plugin.unwrap();
        assert_eq!(plugin.name, "get-test");
        assert_eq!(plugin.version, "1.2.3");

        // 获取不存在的插件
        let not_found = installer.get_plugin("not-found").unwrap();
        assert!(not_found.is_none());
    }
}

/// 属性测试模块
///
/// **Feature: plugin-installation**
#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::plugin::installer::NoopProgressCallback;
    use proptest::prelude::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// 生成有效的插件名称
    fn arb_valid_plugin_name() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9_-]{2,20}".prop_map(|s| s)
    }

    /// 生成有效的版本号
    fn arb_valid_version() -> impl Strategy<Value = String> {
        (1u32..10, 0u32..10, 0u32..10)
            .prop_map(|(major, minor, patch)| format!("{}.{}.{}", major, minor, patch))
    }

    /// 创建测试用的安装器
    fn create_test_installer_for_prop() -> (PluginInstaller, TempDir, TempDir, TempDir) {
        let plugins_dir = TempDir::new().unwrap();
        let temp_dir = TempDir::new().unwrap();
        let db_dir = TempDir::new().unwrap();
        let db_path = db_dir.path().join("test.db");

        let installer = PluginInstaller::from_paths(
            plugins_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            &db_path,
        )
        .unwrap();

        (installer, plugins_dir, temp_dir, db_dir)
    }

    /// 创建有效的测试插件包 (ZIP)
    fn create_valid_plugin_zip(dir: &Path, name: &str, version: &str) -> PathBuf {
        let file_path = dir.join(format!("{}.zip", name));

        let manifest_json = format!(
            r#"{{
                "name": "{}",
                "version": "{}",
                "description": "Test plugin",
                "entry": "config.json",
                "plugin_type": "script",
                "hooks": []
            }}"#,
            name, version
        );

        let config_json = r#"{"enabled": true}"#;

        let file = File::create(&file_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        zip.start_file("plugin.json", options).unwrap();
        zip.write_all(manifest_json.as_bytes()).unwrap();

        zip.start_file("config.json", options).unwrap();
        zip.write_all(config_json.as_bytes()).unwrap();

        zip.finish().unwrap();

        file_path
    }

    /// 创建无效的插件包（缺少 plugin.json）
    fn create_invalid_plugin_zip_no_manifest(dir: &Path, name: &str) -> PathBuf {
        let file_path = dir.join(format!("{}-invalid.zip", name));

        let file = File::create(&file_path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);

        // 只添加一个普通文件，不添加 plugin.json
        zip.start_file("readme.txt", options).unwrap();
        zip.write_all(b"This is not a valid plugin").unwrap();

        zip.finish().unwrap();

        file_path
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(5))]

        /// **Feature: plugin-installation, 属性 2: 安装原子性**
        ///
        /// *对于任意*插件安装尝试，要么插件完全安装成功（文件已复制、注册表已更新），
        /// 要么系统不做任何更改（失败时回滚）。
        ///
        /// **验证需求: 1.2, 1.3, 3.4**
        #[test]
        fn prop_install_atomicity_success(
            name in arb_valid_plugin_name(),
            version in arb_valid_version()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let (installer, plugins_dir, temp_dir, _db_dir) = create_test_installer_for_prop();
                let package_path = create_valid_plugin_zip(temp_dir.path(), &name, &version);

                let progress = NoopProgressCallback;
                let result = installer.install_from_file(&package_path, &progress).await;

                // 成功安装时，验证所有状态都已更新
                prop_assert!(result.is_ok(), "安装应该成功: {:?}", result);

                let installed = result.unwrap();

                // 验证注册表已更新
                let is_registered = installer.is_installed(&name).unwrap();
                prop_assert!(is_registered, "插件应该在注册表中");

                // 验证文件已复制
                let plugin_dir = plugins_dir.path().join(&name);
                prop_assert!(plugin_dir.exists(), "插件目录应该存在");
                prop_assert!(
                    plugin_dir.join("plugin.json").exists(),
                    "plugin.json 应该存在"
                );

                // 验证返回的信息正确
                prop_assert_eq!(installed.name, name);
                prop_assert_eq!(installed.version, version);

                Ok(())
            })?;
        }

        /// **Feature: plugin-installation, 属性 2: 安装原子性 (失败情况)**
        ///
        /// *对于任意*无效的插件包，安装失败后系统状态应该保持不变。
        ///
        /// **验证需求: 1.2, 1.3, 3.4**
        #[test]
        fn prop_install_atomicity_failure(name in arb_valid_plugin_name()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let (installer, plugins_dir, temp_dir, _db_dir) = create_test_installer_for_prop();

                // 记录安装前的状态
                let plugins_before = installer.list_installed().unwrap();
                let plugins_dir_empty_before = fs::read_dir(plugins_dir.path())
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(true);

                // 创建无效的插件包
                let invalid_package = create_invalid_plugin_zip_no_manifest(temp_dir.path(), &name);

                let progress = NoopProgressCallback;
                let result = installer.install_from_file(&invalid_package, &progress).await;

                // 安装应该失败
                prop_assert!(result.is_err(), "无效包安装应该失败");

                // 验证注册表未更改
                let plugins_after = installer.list_installed().unwrap();
                prop_assert_eq!(
                    plugins_before.len(),
                    plugins_after.len(),
                    "注册表不应该有变化"
                );

                // 验证插件目录未更改
                let plugins_dir_empty_after = fs::read_dir(plugins_dir.path())
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(true);
                prop_assert_eq!(
                    plugins_dir_empty_before,
                    plugins_dir_empty_after,
                    "插件目录不应该有变化"
                );

                Ok(())
            })?;
        }

        /// **Feature: plugin-installation, 属性 3: 卸载完整性**
        ///
        /// *对于任意*已安装的插件，卸载后，插件文件必须从 plugins 目录中删除，
        /// 且注册表条目必须被删除。
        ///
        /// **验证需求: 4.2, 4.3**
        #[test]
        fn prop_uninstall_completeness(
            name in arb_valid_plugin_name(),
            version in arb_valid_version()
        ) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let (installer, plugins_dir, temp_dir, _db_dir) = create_test_installer_for_prop();

                // 先安装插件
                let package_path = create_valid_plugin_zip(temp_dir.path(), &name, &version);
                let progress = NoopProgressCallback;
                let install_result = installer.install_from_file(&package_path, &progress).await;
                prop_assert!(install_result.is_ok(), "安装应该成功");

                // 验证安装成功
                let plugin_dir = plugins_dir.path().join(&name);
                prop_assert!(plugin_dir.exists(), "安装后插件目录应该存在");
                prop_assert!(installer.is_installed(&name).unwrap(), "安装后应该在注册表中");

                // 执行卸载
                let uninstall_result = installer.uninstall(&name).await;
                prop_assert!(uninstall_result.is_ok(), "卸载应该成功: {:?}", uninstall_result);

                // 验证文件已删除
                prop_assert!(
                    !plugin_dir.exists(),
                    "卸载后插件目录应该被删除"
                );

                // 验证注册表条目已删除
                prop_assert!(
                    !installer.is_installed(&name).unwrap(),
                    "卸载后不应该在注册表中"
                );

                // 验证获取插件返回 None
                let plugin = installer.get_plugin(&name).unwrap();
                prop_assert!(plugin.is_none(), "卸载后获取插件应该返回 None");

                Ok(())
            })?;
        }

        /// **Feature: plugin-installation, 属性 3: 卸载完整性 (不存在的插件)**
        ///
        /// *对于任意*不存在的插件 ID，卸载应该返回 NotFound 错误。
        ///
        /// **验证需求: 4.2, 4.3**
        #[test]
        fn prop_uninstall_not_found(name in arb_valid_plugin_name()) {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                let (installer, _plugins_dir, _temp_dir, _db_dir) = create_test_installer_for_prop();

                // 尝试卸载不存在的插件
                let result = installer.uninstall(&name).await;

                // 应该返回 NotFound 错误
                prop_assert!(result.is_err(), "卸载不存在的插件应该失败");
                match result.unwrap_err() {
                    InstallError::NotFound(id) => {
                        prop_assert_eq!(id, name, "错误应该包含正确的插件 ID");
                    }
                    e => {
                        prop_assert!(false, "期望 NotFound 错误，实际: {:?}", e);
                    }
                }

                Ok(())
            })?;
        }
    }
}
