use crate::{file::normalized_path::NormalizedPath, trace};

/// Check that the path is secure, by these tests:
/// - The path starts with the base path, to prevent directory traversal attacks
/// - The path does not contain any of the blocked file patterns
/// - Returns true if the path is secure, false otherwise
///
/// Used primarily by static file processors, to ensure that files being served are safe
/// Expected that both base_path and test_path are normalized paths without junk!
pub async fn check_path_secure(base_path: &str, test_path: &NormalizedPath, blocked_file_patterns: &[String]) -> bool {
    // Make sure the base_path ends with a slash for accurate checking, unless it's just "/"
    let base_path = if base_path != "/" && !base_path.ends_with('/') {
        format!("{}/", base_path)
    } else {
        base_path.to_string()
    };

    // Check that the test_path starts with the base_path
    if !test_path.get_full_path().starts_with(&base_path) {
        trace!("Path is blocked, as it does not start with the web root: '{}' file: '{}'", base_path, test_path.get_full_path());
        return false;
    }

    // Run through blocked patterns and see if any match
    let file_lowercase = test_path.get_path().to_lowercase();
    for pattern in blocked_file_patterns {
        if file_lowercase.contains(pattern) {
            trace!("Path is blocked due to blocked file pattern: {} file: {}", pattern, test_path.get_full_path());
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
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();

        let blocked_file_patterns = &config.core.server_settings.blocked_file_patterns;

        assert!(check_path_secure("/var/www", &NormalizedPath::new("/var/www", "index.html").unwrap(), blocked_file_patterns).await);
        assert!(check_path_secure("/var/www", &NormalizedPath::new("/var/www", "styles.css").unwrap(), blocked_file_patterns).await);
        assert!(check_path_secure("/var/www", &NormalizedPath::new("/var/www/mysubdir", "styles.css").unwrap(), blocked_file_patterns).await);

        assert!(!check_path_secure("/var/www", &NormalizedPath::new("/var/www", "index.php").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www", &NormalizedPath::new("/var", "index.html").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www/html", &NormalizedPath::new("/var/www", "index.php").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www/html", &NormalizedPath::new("/", "index.php").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www/html", &NormalizedPath::new("/", "etc/passwd").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www", &NormalizedPath::new("/var/www", "index.key").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www", &NormalizedPath::new("/var/www", "index.pem").unwrap(), blocked_file_patterns).await);
        assert!(!check_path_secure("/var/www", &NormalizedPath::new("/var/www-evil", "passwd").unwrap(), blocked_file_patterns).await);
    }
}
