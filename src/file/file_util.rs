use std::env;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use cached::proc_macro::cached;
use log::trace;

use crate::http::file_pattern_matching::{get_blocked_file_pattern_matching, get_whitelisted_file_pattern_matching};

/// Sanitizes and resolves a file path into an absolute path.
/// - Expands relative paths to absolute.
/// - Normalizes separators to `/`.
/// - Cleans up `.` and `..`.
/// - Removes duplicate separators.
/// Works on both Windows and Unix.
#[cached(
    size = 100,
    time = 10, // Cache for 10 seconds
    result = true,
    key = "String",
    convert = r#"{ input_path.to_string() }"#
)]
pub fn get_full_file_path(input_path: &String) -> Result<String, std::io::Error> {
    let mut path = PathBuf::new();

    // If relative, start from current dir
    if Path::new(&input_path).is_relative() {
        let current_dir_result = env::current_dir()?;
        path.push(current_dir_result);
    }

    path.push(&input_path);

    // Normalize components manually
    let mut normalized = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {
                // Skip "."
            }
            Component::ParentDir => {
                // Skip ".."
            }
            other => normalized.push(other),
        }
    }

    // Convert to string and normalize slashes
    let mut result = normalized
        .to_string_lossy()
        .replace('\\', "/");

    // Remove duplicate slashes (// → /)
    while result.contains("//") {
        result = result.replace("//", "/");
    }

    Ok(result)
}

/// Splits `path_str` into (relative_dir, file_name) based on `base_path`.
/// - If `path_str` starts with `base_path`, returns (base_path, remaining_path).
/// - If not, returns ("", path_str).
pub fn split_path(base_path: &str, path_str: &str) -> (String, String) {
    let base_path_cleaned = base_path.replace('\\', "/").trim_end_matches('/').to_string();
    let path_str_cleaned = path_str.replace('\\', "/");

    if path_str_cleaned.starts_with(&base_path_cleaned) {
        let remaining = &path_str_cleaned[base_path_cleaned.len()..];
        let remaining = remaining.trim_start_matches('/'); // Remove leading slash if present
        (base_path_cleaned, remaining.to_string())
    } else {
        ("".to_string(), path_str_cleaned)
    }
}

// We expect all web roots to be cleaned, with forward slashes and absolute paths and should be able to handle replacing webroot from Windows to Unix style paths and vice versa
#[cached(
    size = 100,
    time = 10, // Cache for 10 seconds
    key = "String",
    convert = r#"{ format!("{}|{}|{}", original_path, old_web_root, new_web_root) }"#
)]
pub fn replace_web_root_in_path(original_path: &str, old_web_root: &str, new_web_root: &str) -> String {
    let old_web_root_cleaned = old_web_root.replace('\\', "/").trim_end_matches('/').to_string();
    let new_web_root_cleaned = new_web_root.replace('\\', "/").trim_end_matches('/').to_string();

    if original_path.starts_with(&old_web_root_cleaned) {
        let relative_part = &original_path[old_web_root_cleaned.len()..];
        let relative_part = relative_part.trim_start_matches('/'); // Remove leading slash if present
        if relative_part.is_empty() {
            new_web_root_cleaned.clone()
        } else {
            format!("{}/{}", new_web_root_cleaned, relative_part)
        }
    } else {
        original_path.to_string() // Return original if it doesn't start with old web root
    }
}

// Check that the path is secure, by these tests:
// - The path starts with the base path, to prevent directory traversal attacks
// - The path does not contain any of the blocked file patterns
pub async fn check_path_secure(base_path: &str, test_path: &str) -> bool {
    // Check that the test_path starts with the base_path
    let base_path_cleaned = base_path.replace('\\', "/").trim_end_matches('/').to_string();
    let test_path_cleaned = test_path.replace('\\', "/");
    if !test_path_cleaned.starts_with(&base_path_cleaned) {
        trace!("Path is blocked, as it does not start with the web root: {} file: {}", base_path_cleaned, test_path_cleaned);
        return false;
    }

    let (_path, file) = split_path(&base_path_cleaned, &test_path_cleaned);

    trace!("Check if file pattern is blocked or whitelisted: {}", &file);

    // Check if it is whitelisted first
    let pattern_whitelisting = get_whitelisted_file_pattern_matching().await;
    if pattern_whitelisting.is_file_pattern_whitelisted(&test_path_cleaned) {
        trace!("File pattern is whitelisted: {}", &test_path_cleaned);
        return true;
    }

    // Check the blacklisted file patterns
    let pattern_blocking = get_blocked_file_pattern_matching().await;
    if pattern_blocking.is_file_pattern_blocked(&file) {
        trace!("File pattern is blocked: {}", &file);
        return false;
    }

    true
}



#[test]
fn test_full_path_is_unchanged() {
    let cwd = env::current_dir().unwrap();
    let abs = cwd.join("foo/bar.txt");
    let abs_str = abs.to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&abs_str).unwrap();
    assert_eq!(result, abs_str);
}

#[test]
fn test_relative_path_is_expanded() {
    let cwd = env::current_dir().unwrap();
    let rel = "foo/bar.txt";
    let expected = cwd.join(rel).to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
#[cfg(not(windows))]
fn test_dot_and_dotdot_are_cleaned() {
    let cwd = env::current_dir().unwrap();
    let rel = "foo/./bar/../baz.txt";
    let expected = cwd.join("foo/baz.txt").to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_duplicate_slashes() {
    let cwd = env::current_dir().unwrap();
    let rel = "foo//bar///baz.txt";
    let expected = cwd.join("foo/bar/baz.txt").to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_windows_path() {
    // Simulate a Windows-style path on any platform
    let cwd = env::current_dir().unwrap();
    let rel = "foo\\bar\\baz.txt";
    let expected = cwd.join("foo/bar/baz.txt").to_string_lossy().replace('\\', "/");
    let result = get_full_file_path(&rel.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_absolute_windows_path_cross_platform() {
    // This test ensures Windows-style absolute paths are normalized on all platforms
    let win_abs = "C:\\foo\\bar.txt";
    let expected = "C:/foo/bar.txt";
    let result = get_full_file_path(&win_abs.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[cfg(windows)]
#[test]
fn test_absolute_windows_path_native() {
    // Only run this test on Windows for platform-specific normalization
    let win_abs = "C:\\foo\\bar.txt";
    let expected = "C:/foo/bar.txt";
    let result = get_full_file_path(&win_abs.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
#[cfg(not(windows))]
fn test_absolute_linux_path() {
    let abs = "/tmp/foo/bar.txt";
    let expected = "/tmp/foo/bar.txt";
    let result = get_full_file_path(&abs.to_string()).unwrap();
    assert_eq!(result, expected);
}

#[test]
fn test_split_path_unix_path() {
    let (dir, file) = split_path("/path1/path2", "/path1/path2/index.php");
    assert_eq!(dir, "/path1/path2");
    assert_eq!(file, "index.php");
}

#[test]
fn test_split_path_multiple_paths_file() {
    let (dir, file) = split_path("C:/test/test2/test3", "C:/test/test2/test3/test4/test5/file.txt");
    assert_eq!(dir, "C:/test/test2/test3");
    assert_eq!(file, "test4/test5/file.txt");
}