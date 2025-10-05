use std::env;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use cached::proc_macro::cached;

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
/// - If `path_str` starts with `base_path`, returns the relative directory (with forward slashes, no leading slash) and file name.
/// - If not, returns ("", file_name).
pub fn split_path(base_path: &str, path_str: &str) -> (String, String) {
    let base = Path::new(base_path).components().collect::<PathBuf>();
    let path = Path::new(path_str);

    // If path_str starts with base_path, strip base_path prefix
    let rel = match path.strip_prefix(&base) {
        Ok(rel) => rel,
        Err(_) => path,
    };

    let file = rel.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .replace('\\', "/");

    let dir = rel.parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| "".to_string());

    (dir.trim_start_matches('/').to_string(), file)
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