use std::env;
use std::path::{Component, Path, PathBuf};

/// Sanitizes and resolves a file path into an absolute path.
/// - Expands relative paths to absolute.
/// - Normalizes separators to `/`.
/// - Cleans up `.` and `..`.
/// - Removes duplicate separators.
/// Works on both Windows and Unix.
pub fn get_full_file_path<P: AsRef<Path>>(input: P) -> std::io::Result<String> {
    let mut path = PathBuf::new();

    let input_path = input.as_ref();

    // If relative, start from current dir
    if input_path.is_relative() {
        path.push(env::current_dir()?);
    }

    path.push(input_path);

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

pub fn split_path(path_str: &str) -> (String, String) {
    let path = Path::new(path_str);

    // Extract file part
    let file = path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    // Extract directory part
    let dir = path.parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "".to_string());

    // Normalize both to forward slashes in one pass
    let dir = dir.replace('\\', "/");
    let file = file.replace('\\', "/");

    (dir, file)
}