use std::{
    env,
    path::{Path, PathBuf},
};

use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;
use urlencoding::decode;

use crate::logging::syslog::debug;

#[derive(Clone, Debug)]
pub struct NormalizedPath {
    web_root: String,
    path: String,
    full_path: String,
}

const RESERVED_FILENAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

impl NormalizedPath {
    /// Get a new NormalizedPath instance, based on a trusted web_root and a user-supplied path.
    /// We expect web_root to be already sanitized and validated ,as it comes from our configuration.
    pub fn new(web_root: &str, path: &str) -> Result<Self, ()> {
        let mut normalized_path = NormalizedPath {
            web_root: web_root.trim().to_string(),
            path: path.trim().to_string(),
            full_path: "".to_string(),
        };

        // Normalize the path part, which is also decoded
        if !path.is_empty() {
            let normalized_path_cleaned_result = Self::clean_url_path(&path);
            normalized_path.path = match normalized_path_cleaned_result {
                Ok(p) => p,
                Err(_) => {
                    debug(format!("Failed to clean URL path in NormalizePath: {:?}", normalized_path));
                    return Err(());
                }
            };
        }

        // Remove ending / from web root
        if normalized_path.web_root.ends_with('/') {
            normalized_path.web_root.pop();
        }

        // Set the full path and return
        normalized_path.full_path = format!("{}{}", normalized_path.web_root, normalized_path.path);

        if normalized_path.web_root.is_empty() && normalized_path.path.is_empty() {
            normalized_path.full_path = "".to_string();
        } else {
            let full_path_result = Self::resolve_relative_path(&normalized_path.full_path);
            normalized_path.full_path = match full_path_result {
                Ok(p) => p,
                Err(_) => {
                    return Err(());
                }
            };

            while normalized_path.full_path.contains("\\") {
                normalized_path.full_path = normalized_path.full_path.replace("\\", "/");
            }
        }

        Ok(normalized_path)
    }

    pub fn get_full_path(&self) -> String {
        self.full_path.to_string()
    }

    pub fn get_web_root(&self) -> String {
        self.web_root.to_string()
    }

    pub fn get_path(&self) -> String {
        self.path.to_string()
    }

    fn decode_string_until_no_percentage(path: &str) -> Result<String, ()> {
        let mut decoded = path.to_string();

        let max_rounds = 10; // Prevent infinite loops

        for _ in 0..max_rounds {
            let decoded_result = decode(&decoded);
            let new_decoded = match decoded_result {
                Ok(d) => d.to_string(),
                Err(_) => return Err(()),
            };

            if new_decoded == decoded {
                return Ok(decoded);
            }
            decoded = new_decoded;
        }

        Err(())
    }

    fn clean_url_path(path: &str) -> Result<String, String> {
        // First, decode percent-encoded characters
        let decoded_path_result = Self::decode_string_until_no_percentage(path);
        let path = match decoded_path_result {
            Ok(p) => p,
            Err(_) => return Err("Failed to decode percent-encoded characters".to_string()),
        };

        // Handle unicode normalization
        let mut buf: String = path.nfc().collect();
        for ch in buf.chars() {
            // Reject Unicode format characters (Cf)
            let gc = get_general_category(ch);
            if gc == GeneralCategory::Format {
                return Err("Path contains forbidden Unicode format characters".to_string());
            }
            if gc == GeneralCategory::Control {
                return Err("Path contains forbidden Unicode control characters".to_string());
            }

            // Reject confusable slashes or dots
            if matches!(
                ch,
                // Slash-like
                '\u{2215}' | // ∕ division slash
                '\u{2044}' | // ⁄ fraction slash
                '\u{FF0F}' | // ／ fullwidth solidus
                '\u{29F8}' | // ⧸ big solidus
                '\u{FE68}' | // ﹨ small reverse solidus

                // Dot-like
                '\u{FF0E}' | // ． fullwidth full stop
                '\u{3002}' | // 。 ideographic full stop
                '\u{2219}' | // ∙ bullet operator
                '\u{22C5}' // ⋅ dot operator
            ) {
                return Err("Path contains confusable slash or dot characters".to_string());
            }
        }

        // Return error on ascii control characters and NUL characters
        if buf.chars().any(|c| c.is_control() || c == '\0') {
            return Err("Path contains ASCII control characters or NUL characters".to_string());
        }

        // If last characters is dot, we call error (to avoid trailing dots)
        if buf.ends_with('.') {
            return Err("Path cannot end with a dot".to_string());
        }

        // If we have colon somewhere in the path, we call error
        if buf.contains(':') {
            return Err("Path cannot contain colon characters".to_string());
        }

        // Remove duplicate slashes (// → /)
        while buf.contains("//") {
            buf = buf.replace("//", "/");
        }

        // Remove backward slashes (\ → '')
        while buf.contains("\\") {
            buf = buf.replace("\\", "");
        }

        // Split by slash and process each part
        let mut parts = Vec::new();
        for part in buf.split('/') {
            match part {
                "" => continue,
                "." | ".." => return Err("Path traversal segments are not allowed".to_string()),
                _ => parts.push(part),
            }

            // Check for reserved filenames (Windows)
            let part_upper = part.to_uppercase();
            if RESERVED_FILENAMES.contains(&part_upper.as_str()) {
                return Err("Path contains reserved filename".to_string());
            }

            // No tilde at start or end of segment
            if part.starts_with("~") || part.ends_with("~") {
                return Err("Path segments cannot start or end with tilde (~)".to_string());
            }

            // No segments starting with .
            if part.starts_with(".") && part != ".well-known" {
                return Err("Path segments cannot start with a dot".to_string());
            }

            // No segments starting with .#
            if part.starts_with(".#") {
                return Err("Path segments cannot start with .#".to_string());
            }
        }

        // Remove any leading dot segments in the start only, such as "..../test.txt"
        // We dont want dot segments at the start of the path
        while let Some(first) = parts.first() {
            if first.starts_with("..") || first == &"." {
                return Err("Path traversal segments are not allowed".to_string());
            } else {
                break;
            }
        }

        // Join parts and ensure no trailing slash
        let result = parts.join("/");

        // If nothing left, return "/"
        if result.is_empty() {
            return Ok("/".to_string());
        }

        // If path does not start with slash, we add it
        let result = if !result.starts_with('/') { format!("/{}", result) } else { result };

        Ok(result)
    }

    /// Sanitizes and resolves a file path into an absolute path.
    /// - Expands relative paths to absolute.
    /// Works on both Windows and Unix.
    fn resolve_relative_path(input_path: &str) -> Result<String, std::io::Error> {
        let mut path = PathBuf::new();

        // If it starts with ./, we replace with current dir
        if input_path.starts_with("./") {
            let mut current_dir_result = env::current_dir()?;
            current_dir_result.push(&input_path[2..]);
            return Ok(current_dir_result.to_string_lossy().to_string());
        }

        // Treat Unix-rooted paths like "/var/www" as rooted even on Windows,
        // otherwise Path::is_relative() may incorrectly cause us to prepend CWD (and a drive letter).
        let input_path_path = Path::new(input_path);
        let is_effectively_absolute = input_path_path.is_absolute() || input_path.starts_with('/');

        if !is_effectively_absolute {
            let current_dir_result = env::current_dir()?;
            path.push(current_dir_result);
        }

        path.push(input_path);

        // Convert to string and normalize slashes
        Ok(path.to_string_lossy().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_normalized_path_basics() {
        let normalized = match NormalizedPath::new("/var/www", "/images/css/style.css") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for valid path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/images/css/style.css");
        assert_eq!(normalized.get_full_path(), "/var/www/images/css/style.css");

        let normalized = match NormalizedPath::new("/var/www", "/") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for root path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/");
        assert_eq!(normalized.get_full_path(), "/var/www/");

        let normalized = match NormalizedPath::new("/var/www", "/index.php") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for index.php path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/index.php");
        assert_eq!(normalized.get_full_path(), "/var/www/index.php");
    }

    #[tokio::test]
    async fn test_normalized_path_traversal_attempt_simple() {
        let normalized = NormalizedPath::new("/var/www", "/images/../css/style.css");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/./css/style.css");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/../../../index.php");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "../../../index.php");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "../../../../");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/b/c/../../");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/../../../etc/passwd");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/../../windows/system.ini");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "\\..\\..\\");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/b/..\\..\\a/");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/..;/../b");
        assert!(normalized.is_err());

        let normalized = match NormalizedPath::new("/var/www", "////") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for multiple slashes path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/");
        assert_eq!(normalized.get_full_path(), "/var/www/");
    }

    #[tokio::test]
    async fn test_normalized_path_traversal_attempt_encoded() {
        let normalized = NormalizedPath::new("/var/www", "/images/%2e%2e/css/style.css");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/%2e%2e%2fcss/style.css");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e%2e%2f%2e%2e%2findex.php");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "%2e%2e%2e%2f%2e%2e%2findex.php");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e/%2e%2e/");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/%2e%2e/b");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/b/%2e%2e/%2e%2e/");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2E%2E/");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2E/");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%252e%252e/");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%252e%252e%252f/b");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%252e%252e/etc/passwd");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/%252e%252e/b");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e%2fetc%2fpasswd");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e%2fetc%2fpasswd");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_acceptable_dot_paths() {
        let normalized = match NormalizedPath::new("/var/www", "/.well-known/test.txt") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for .well-known path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/.well-known/test.txt");
        assert_eq!(normalized.get_full_path(), "/var/www/.well-known/test.txt");
    }

    #[tokio::test]
    async fn test_normalized_path_unacceptable_dot_paths() {
        let normalized = NormalizedPath::new("/var/www", "/.git/test.txt");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/.env");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_remove_ascii_control_chars_and_nul() {
        let normalized = NormalizedPath::new("/var/www", "/images/\x00\x1Fstyle.css");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/\x00style.css");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/\x127style.css");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_ending_on_dot() {
        let normalized = NormalizedPath::new("/var/www", "/images/style.");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_with_colon() {
        let normalized = NormalizedPath::new("/var/www", "/images/style.css::$DATA");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_with_reserved_names() {
        let normalized = NormalizedPath::new("/var/www", "/images/CON/style.css");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/CON");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/CON");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/NUL/style.css");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/NUL");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/NUL");
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/LPT9/style.css");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/LPT9");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/LPT9");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_with_unicode_issue() {
        let normalized = match NormalizedPath::new("/var/www", "/images/style\u{0301}.css") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for unicode normalized path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/images/stylé.css");
        assert_eq!(normalized.get_full_path(), "/var/www/images/stylé.css");

        let normalized = NormalizedPath::new("/var/www", "/images/style\u{200E}.css");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/style\u{200B}file.js");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/style\u{FF0E}\u{FF0E}/secret");
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/style/%E2%80%AEevil.js");
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_relative_paths() {
        let mut current_dir = match env::current_dir() {
            Ok(dir) => dir.to_string_lossy().to_string(),
            Err(_) => panic!("Failed to get current directory"),
        };
        while current_dir.contains("\\") {
            current_dir = current_dir.replace("\\", "/");
        }

        let normalized = match NormalizedPath::new("./www-admin", "") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for ./www-admin path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/www-admin", current_dir));

        let normalized = match NormalizedPath::new("www-admin", "") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for www-admin path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/www-admin", current_dir));

        let normalized = match NormalizedPath::new("./www-admin", "/index.php") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for ./www-admin/index.php path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/www-admin/index.php", current_dir));

        let normalized = match NormalizedPath::new("", "/index.php") {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for /index.php path"),
        };
        assert_eq!(normalized.get_full_path(), "/index.php");
    }
}
