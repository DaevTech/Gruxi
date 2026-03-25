use crate::trace;
use cached::proc_macro::cached;

/// Splits `path_str` into (relative_dir, file_name) based on `base_path`.
/// - If `path_str` starts with `base_path`, returns (base_path, remaining_path).
/// - If not, returns ("", path_str).
pub fn split_path(base_path: &str, path_str: &str) -> (String, String) {
    let base_path_cleaned = base_path.replace('\\', "/").trim_end_matches('/').to_string();
    let path_str_cleaned = path_str.replace('\\', "/");

    if path_str_cleaned.starts_with(&base_path_cleaned) {
        let remaining = &path_str_cleaned[base_path_cleaned.len()..];
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

/// Check that the path is secure, by these tests:
/// - The path starts with the base path, to prevent directory traversal attacks
/// - The path does not contain any of the blocked file patterns
/// - Returns true if the path is secure, false otherwise
///
/// Used primarily by static file processors, to ensure that files being served are safe
/// Expected that both base_path and test_path are normalized paths without junk!
pub async fn check_path_secure(base_path: &str, test_path: &str) -> bool {
    // Check that the test_path starts with the base_path
    if !test_path.starts_with(base_path) {
        trace!("Path is blocked, as it does not start with the web root: {} file: {}", base_path, test_path);
        return false;
    }

    let (_path, file) = split_path(base_path, test_path);

    trace!("Check if file pattern is blocked because of extension: {}", &file);

    // Check the blacklisted file patterns
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration().await;

    // Run through blocked patterns and see if any match
    let file_lowercase = file.to_lowercase();
    for pattern in &config.core.server_settings.blocked_file_patterns {
        if file_lowercase.contains(pattern) {
            trace!("Path is blocked due to blocked file pattern: {} file: {}", pattern, test_path);
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_check_path_secure_blocked_extensions_matching() {
        assert!(check_path_secure("/var/www", "/var/www/index.html").await);
        assert!(check_path_secure("/var/www", "/var/www/styles.css").await);
        assert!(check_path_secure("/var/www", "/var/www/mysubdir/styles.css").await);

        assert!(!check_path_secure("/var/www", "/var/www/index.php").await);
        assert!(!check_path_secure("/var/www", "/var/index.html").await);
        assert!(!check_path_secure("/var/www/html", "/var/www/index.php").await);
        assert!(!check_path_secure("/var/www/html", "/index.php").await);
        assert!(!check_path_secure("/var/www/html", "/etc/passwd").await);
        assert!(!check_path_secure("/var/www", "/var/www/index.key").await);
        assert!(!check_path_secure("/var/www", "/var/www/index.pem").await);
    }

    #[test]
    fn test_split_path_unix_path() {
        let (dir, file) = split_path("/path1/path2", "/path1/path2/index.php");
        assert_eq!(dir, "/path1/path2");
        assert_eq!(file, "/index.php");
    }

    #[test]
    fn test_split_path_multiple_paths_file() {
        let (dir, file) = split_path("C:/test/test2/test3", "C:/test/test2/test3/test4/test5/file.txt");
        assert_eq!(dir, "C:/test/test2/test3");
        assert_eq!(file, "/test4/test5/file.txt");
    }
}
