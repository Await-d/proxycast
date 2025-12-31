//! 路径工具模块
//!
//! 提供路径处理相关的工具函数，包括 tilde (~) 路径展开

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// 展开路径中的 tilde (~) 为用户主目录
///
/// 支持以下格式：
/// - `~` -> 用户主目录
/// - `~/path` -> 用户主目录/path
/// - `~user/path` -> 不支持，返回原路径
/// - 其他路径 -> 返回原路径
///
/// # Arguments
/// * `path` - 要展开的路径字符串
///
/// # Returns
/// 展开后的 PathBuf
///
/// # Examples
/// ```ignore
/// use proxycast_lib::config::expand_tilde;
///
/// let expanded = expand_tilde("~/.proxycast/auth");
/// // 返回类似 "/Users/username/.proxycast/auth" 的路径
/// ```
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    let path_str = path.to_string_lossy();

    // 如果路径不以 ~ 开头，直接返回原路径
    if !path_str.starts_with('~') {
        return path.to_path_buf();
    }

    // 获取用户主目录
    let home_dir = match dirs::home_dir() {
        Some(dir) => dir,
        None => return path.to_path_buf(), // 无法获取主目录，返回原路径
    };

    // 处理不同的 tilde 格式
    if path_str == "~" {
        // 仅 ~
        home_dir
    } else if let Some(rest) = path_str.strip_prefix("~/") {
        // ~/path 格式
        // 跳过 "~/"
        home_dir.join(rest)
    } else {
        // ~user/path 格式，不支持，返回原路径
        path.to_path_buf()
    }
}

/// 将路径收缩为 tilde 格式（如果可能）
///
/// 如果路径以用户主目录开头，则将其替换为 ~
///
/// # Arguments
/// * `path` - 要收缩的路径
///
/// # Returns
/// 收缩后的路径字符串
///
/// # Examples
/// ```ignore
/// use proxycast_lib::config::collapse_tilde;
///
/// let collapsed = collapse_tilde("/Users/username/.proxycast/auth");
/// // 返回 "~/.proxycast/auth"
/// ```
pub fn collapse_tilde<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();

    // 获取用户主目录
    let home_dir = match dirs::home_dir() {
        Some(dir) => dir,
        None => return path.to_string_lossy().to_string(),
    };

    // 检查路径是否以主目录开头
    if let Ok(stripped) = path.strip_prefix(&home_dir) {
        if stripped.as_os_str().is_empty() {
            "~".to_string()
        } else {
            format!("~/{}", stripped.to_string_lossy())
        }
    } else {
        path.to_string_lossy().to_string()
    }
}

/// 检查路径是否包含 tilde
pub fn contains_tilde<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().to_string_lossy().starts_with('~')
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_expand_tilde_only() {
        let expanded = expand_tilde("~");
        let home = dirs::home_dir().expect("应该能获取主目录");
        assert_eq!(expanded, home);
    }

    #[test]
    fn test_expand_tilde_with_path() {
        let expanded = expand_tilde("~/.proxycast/auth");
        let home = dirs::home_dir().expect("应该能获取主目录");
        assert_eq!(expanded, home.join(".proxycast/auth"));
    }

    #[test]
    fn test_expand_tilde_nested_path() {
        let expanded = expand_tilde("~/a/b/c/d");
        let home = dirs::home_dir().expect("应该能获取主目录");
        assert_eq!(expanded, home.join("a/b/c/d"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let path = "/absolute/path/to/file";
        let expanded = expand_tilde(path);
        assert_eq!(expanded, PathBuf::from(path));
    }

    #[test]
    fn test_expand_tilde_relative_path() {
        let path = "relative/path/to/file";
        let expanded = expand_tilde(path);
        assert_eq!(expanded, PathBuf::from(path));
    }

    #[test]
    fn test_expand_tilde_user_format_not_supported() {
        // ~user/path 格式不支持，应返回原路径
        let path = "~otheruser/path";
        let expanded = expand_tilde(path);
        assert_eq!(expanded, PathBuf::from(path));
    }

    #[test]
    fn test_collapse_tilde_home_dir() {
        let home = dirs::home_dir().expect("应该能获取主目录");
        let collapsed = collapse_tilde(&home);
        assert_eq!(collapsed, "~");
    }

    #[test]
    fn test_collapse_tilde_with_subpath() {
        let home = dirs::home_dir().expect("应该能获取主目录");
        let path = home.join(".proxycast/auth");
        let collapsed = collapse_tilde(&path);
        assert_eq!(collapsed, "~/.proxycast/auth");
    }

    #[test]
    fn test_collapse_tilde_not_in_home() {
        let path = "/tmp/some/path";
        let collapsed = collapse_tilde(path);
        assert_eq!(collapsed, path);
    }

    #[test]
    fn test_contains_tilde() {
        assert!(contains_tilde("~"));
        assert!(contains_tilde("~/path"));
        assert!(contains_tilde("~user/path"));
        assert!(!contains_tilde("/absolute/path"));
        assert!(!contains_tilde("relative/path"));
    }

    #[test]
    fn test_expand_collapse_roundtrip() {
        // 对于 ~/path 格式，展开后再收缩应该得到原路径
        let original = "~/.proxycast/auth/token.json";
        let expanded = expand_tilde(original);
        let collapsed = collapse_tilde(&expanded);
        assert_eq!(collapsed, original);
    }
}
