use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;
use urlencoding::decode;

use crate::{
    debug,
    error::{gruxi_error::GruxiError, gruxi_error_enums::GruxiErrorKind},
    file::app_paths::get_app_paths,
};

#[derive(Clone, Debug)]
pub struct NormalizedPath {
    web_root: String,
    path: String,
    full_path: String,
    original_web_root: String,
    original_path: String,
}

const RESERVED_FILENAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

impl NormalizedPath {
    /// Get a new NormalizedPath instance, based on a trusted web_root and a user-supplied path.
    /// We expect web_root to be already sanitized and validated
    pub fn new(web_root: &str, path: &str, is_path_safe: bool) -> Result<Self, GruxiError> {
        let mut normalized_path = NormalizedPath {
            web_root: web_root.trim().to_string(),
            path: path.trim().to_string(),
            full_path: "".to_string(),
            original_web_root: web_root.trim().to_string(),
            original_path: path.trim().to_string(),
        };

        normalized_path.process(is_path_safe)?;

        Ok(normalized_path)
    }

    fn process_path(&mut self, is_path_safe: bool) -> Result<(), GruxiError> {
        // Make sure the path starts with a slash, as we expect it to be a URL path
        if !self.path.is_empty() && !self.path.starts_with('/') {
            self.path = format!("/{}", self.path);
        }

        // Normalize the path part, which is also decoded
        if !self.path.is_empty() && !is_path_safe {
            let normalized_path_cleaned_result = Self::clean_url_path(&self.path);
            self.path = match normalized_path_cleaned_result {
                Ok(p) => p,
                Err(_) => {
                    debug!("Failed to clean URL path in NormalizePath: {:?}", self);
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Internal("Failed to clean URL path")));
                }
            };
        }
        Ok(())
    }

    fn process_web_root(&mut self) -> Result<(), GruxiError> {
        if !Self::is_path_absolute(&self.web_root) {
            let full_path_result = Self::make_path_absolute(&self.web_root);
            self.web_root = match full_path_result {
                Ok(p) => p,
                Err(_) => {
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Internal("Failed to resolve relative path")));
                }
            };
        }
        self.web_root = self.web_root.replace('\\', "/");
        self.web_root = self.web_root.trim_end_matches('/').to_string();

        Ok(())
    }

    fn process_finalize(&mut self) -> Result<(), GruxiError> {
        self.full_path = format!("{}{}", self.web_root, self.path);

        if self.web_root.is_empty() && self.path.is_empty() {
            self.full_path = "".to_string();
        } else {
            self.full_path = self.full_path.replace('\\', "/");
        }
        Ok(())
    }

    fn process(&mut self, is_path_safe: bool) -> Result<(), GruxiError> {
        self.process_path(is_path_safe)?;
        self.process_web_root()?;
        self.process_finalize()?;

        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.original_web_root.is_empty() && self.original_path.is_empty()
    }

    pub fn get_full_path(&self) -> &str {
        &self.full_path
    }

    pub fn get_web_root(&self) -> &str {
        &self.web_root
    }

    pub fn set_web_root(&mut self, new_web_root: &str) -> Result<(), GruxiError> {
        self.web_root = new_web_root.trim().to_string();
        self.process_web_root()?;
        self.process_finalize()?;
        Ok(())
    }

    pub fn get_path(&self) -> &str {
        &self.path
    }

    pub fn set_path(&mut self, new_path: &str, is_path_safe: bool) -> Result<(), GruxiError> {
        self.path = new_path.trim().to_string();
        self.process_path(is_path_safe)?;
        self.process_finalize()?;
        Ok(())
    }

    fn decode_string_until_no_percentage(path: &str) -> Result<String, ()> {
        // Fast path: no percent-encoding present, return without allocating
        if !path.contains('%') {
            return Ok(path.to_string());
        }

        // Decode once, using Cow to avoid allocation if nothing changed
        let decoded = decode(path).map_err(|_| ())?;

        // If still contains percent-encoded sequences after decoding, reject it
        // (indicates double/triple encoding which is a potential attack vector)
        if decoded.contains('%') {
            return Err(());
        }

        Ok(decoded.into_owned())
    }

    fn clean_url_path(path: &str) -> Result<String, String> {
        // Reject excessively long paths to prevent DoS via repeated allocations
        if path.len() > 4096 {
            return Err("Path exceeds maximum allowed length".to_string());
        }

        // First, decode percent-encoded characters
        let decoded_path_result = Self::decode_string_until_no_percentage(path);
        let path = match decoded_path_result {
            Ok(p) => p,
            Err(_) => return Err("Failed to decode percent-encoded characters".to_string()),
        };

        let (mut buf, is_unicode) = if path.is_ascii() { (path, false) } else { (path.nfc().collect(), true) };

        // Handle unicode normalization
        if is_unicode {
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
                '\u{FF3C}' | // ＼ fullwidth reverse solidus

                // Dot-like
                '\u{FF0E}' | // ． fullwidth full stop
                '\u{3002}' | // 。 ideographic full stop
                '\u{2219}' | // ∙ bullet operator
                '\u{22C5}' // ⋅ dot operator
                ) {
                    return Err("Path contains confusable slash or dot characters".to_string());
                }
            }
        } else {
            // For ASCII paths, we can just check for control characters directly
            if buf.chars().any(|ch| ch.is_control()) {
                return Err("Path contains ASCII control characters".to_string());
            }
        }

        // If we have colon somewhere in the path, we call error
        if buf.contains(':') {
            return Err("Path cannot contain colon characters".to_string());
        }

        // Convert backward slashes to forward slashes (must happen before slash dedup)
        buf = buf.replace('\\', "/");

        // Remove duplicate slashes (// → /)
        while buf.contains("//") {
            buf = buf.replace("//", "/");
        }

        // Split by slash and collect non-empty segments
        let parts: Vec<&str> = buf.split('/').filter(|part| !part.is_empty()).collect();

        // Any dot segments should return error (to avoid path traversal), except for .well-known which is allowed to start with dot
        for &part in &parts {
            if part.starts_with('.') && part != ".well-known" {
                return Err("Path traversal segments are not allowed".to_string());
            }

            // Check for reserved filenames (Windows)
            // Also check stem before first dot, since e.g. CON.txt is still reserved
            let part_upper = part.to_uppercase();
            let stem = part_upper.split('.').next().unwrap_or(&part_upper);
            if RESERVED_FILENAMES.contains(&part_upper.as_str()) || RESERVED_FILENAMES.contains(&stem) {
                return Err("Path contains reserved filename".to_string());
            }

            // No tilde at start or end of segment
            if part.starts_with('~') || part.ends_with('~') {
                return Err("Path segments cannot start or end with tilde (~)".to_string());
            }

            // Reject trailing dots or spaces per segment (Windows silently strips these)
            if part.ends_with('.') || part.ends_with(' ') {
                return Err("Path segments cannot end with a dot or space".to_string());
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

    fn is_path_absolute(input_path: &str) -> bool {
        // On Unix, absolute paths start with '/'
        if input_path.starts_with('/') {
            return true;
        }

        // On Windows, absolute paths can start with a drive letter followed by ':\' or ':/'
        if input_path.len() > 2 && input_path.chars().nth(1) == Some(':') && (input_path.chars().nth(2) == Some('\\') || input_path.chars().nth(2) == Some('/')) {
            return true;
        }

        false
    }

    // Sanitizes and resolves a relative file path into an absolute path.
    // Works on both Windows and Unix.
    fn make_path_absolute(input_path: &str) -> Result<String, std::io::Error> {
        let app_paths = get_app_paths();

        // If it starts with ./, we replace with current dir
        if let Some(stripped) = input_path.strip_prefix("./") {
            return Ok(app_paths.working_dir.join(stripped).to_string_lossy().to_string());
        }

        // If it did not start with ./ but is still not absolute, we also prepend current dir
        Ok(app_paths.working_dir.join(input_path).to_string_lossy().to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    #[tokio::test]
    async fn test_normalized_path_basics() {
        let normalized = match NormalizedPath::new("/var/www", "/images/css/style.css", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for valid path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/images/css/style.css");
        assert_eq!(normalized.get_full_path(), "/var/www/images/css/style.css");

        let normalized = match NormalizedPath::new("/var/www", "/", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for root path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/");
        assert_eq!(normalized.get_full_path(), "/var/www/");

        let normalized = match NormalizedPath::new("/var/www", "/index.php", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for index.php path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/index.php");
        assert_eq!(normalized.get_full_path(), "/var/www/index.php");
    }

    #[tokio::test]
    async fn test_normalized_path_traversal_attempt_simple() {
        let normalized = NormalizedPath::new("/var/www", "/images/../css/style.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/./css/style.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/../../../index.php", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "../../../index.php", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "../../../../", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/b/c/../../", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/../../../etc/passwd", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/../../windows/system.ini", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "\\..\\..\\", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/b/..\\..\\a/", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/..;/../b", false);
        assert!(normalized.is_err());

        let normalized = match NormalizedPath::new("/var/www", "////", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for multiple slashes path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/");
        assert_eq!(normalized.get_full_path(), "/var/www/");
    }

    #[tokio::test]
    async fn test_normalized_path_traversal_attempt_encoded() {
        let normalized = NormalizedPath::new("/var/www", "/images/%2e%2e/css/style.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/%2e%2e%2fcss/style.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e%2e%2f%2e%2e%2findex.php", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "%2e%2e%2e%2f%2e%2e%2findex.php", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e/%2e%2e/", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/%2e%2e/b", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/b/%2e%2e/%2e%2e/", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2E%2E/", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2E/", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%252e%252e/", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%252e%252e%252f/b", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%252e%252e/etc/passwd", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a/%252e%252e/b", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e%2fetc%2fpasswd", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/%2e%2e%2fetc%2fpasswd", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_acceptable_dot_paths() {
        let normalized = match NormalizedPath::new("/var/www", "/.well-known/test.txt", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for .well-known path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/.well-known/test.txt");
        assert_eq!(normalized.get_full_path(), "/var/www/.well-known/test.txt");
    }

    #[tokio::test]
    async fn test_normalized_path_unacceptable_dot_paths() {
        let normalized = NormalizedPath::new("/var/www", "/.git/test.txt", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/.env", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_remove_ascii_control_chars_and_nul() {
        let normalized = NormalizedPath::new("/var/www", "/images/\x00\x1Fstyle.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/\x00style.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/\x127style.css", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_ending_on_dot() {
        let normalized = NormalizedPath::new("/var/www", "/images/style.", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_with_colon() {
        let normalized = NormalizedPath::new("/var/www", "/images/style.css::$DATA", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_with_reserved_names() {
        let normalized = NormalizedPath::new("/var/www", "/images/CON/style.css", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/CON", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/CON", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/NUL/style.css", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/NUL", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/NUL", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/LPT9/style.css", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/LPT9", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/LPT9", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_with_unicode_issue() {
        let normalized = match NormalizedPath::new("/var/www", "/images/style\u{0301}.css", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for unicode normalized path"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_path(), "/images/stylé.css");
        assert_eq!(normalized.get_full_path(), "/var/www/images/stylé.css");

        let normalized = NormalizedPath::new("/var/www", "/images/style\u{200E}.css", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/style\u{200B}file.js", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/style\u{FF0E}\u{FF0E}/secret", false);
        assert!(normalized.is_err());
        let normalized = NormalizedPath::new("/var/www", "/images/style/%E2%80%AEevil.js", false);
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

        let normalized = match NormalizedPath::new("./www-admin", "", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for ./www-admin path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/www-admin", current_dir));

        let normalized = match NormalizedPath::new("www-admin", "", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for www-admin path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/www-admin", current_dir));

        let normalized = match NormalizedPath::new("./www-admin", "/index.php", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for ./www-admin/index.php path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/www-admin/index.php", current_dir));

        let normalized = match NormalizedPath::new("", "/index.php", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for /index.php path"),
        };
        assert_eq!(normalized.get_full_path(), format!("{}/index.php", current_dir));
    }

    #[tokio::test]
    async fn test_normalized_path_reserved_names_with_extensions() {
        // Windows treats CON.txt, NUL.log, etc. as device names
        let normalized = NormalizedPath::new("/var/www", "/CON.txt", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/NUL.log", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/images/LPT1.pdf", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/AUX.tar.gz", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/PRN.doc", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/COM1.txt", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_backslash_converted_to_slash() {
        // Backslashes should be treated as path separators, not silently removed
        let normalized = match NormalizedPath::new("/var/www", "/a\\b\\c", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for backslash path"),
        };
        assert_eq!(normalized.get_path(), "/a/b/c");
        assert_eq!(normalized.get_full_path(), "/var/www/a/b/c");
    }

    #[tokio::test]
    async fn test_normalized_path_trailing_dot_per_segment() {
        // Trailing dot in a directory segment (Windows strips it silently)
        let normalized = NormalizedPath::new("/var/www", "/images/test./file.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a./b/c", false);
        assert!(normalized.is_err());

        // Trailing dot on final segment (file)
        let normalized = NormalizedPath::new("/var/www", "/images/style.", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_trailing_space_per_segment() {
        // Trailing space in a directory segment (Windows strips it silently)
        let normalized = NormalizedPath::new("/var/www", "/images/test /file.css", false);
        assert!(normalized.is_err());

        let normalized = NormalizedPath::new("/var/www", "/a /b/c", false);
        assert!(normalized.is_err());

        // Trailing space via percent-encoding (survives trim, decoded to space internally)
        let normalized = NormalizedPath::new("/var/www", "/images/style%20", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_web_root_multiple_trailing_slashes() {
        let normalized = match NormalizedPath::new("/var/www//", "/index.html", false) {
            Ok(n) => n,
            Err(_) => panic!("Expected Ok result for web root with multiple trailing slashes"),
        };
        assert_eq!(normalized.get_web_root(), "/var/www");
        assert_eq!(normalized.get_full_path(), "/var/www/index.html");
    }

    #[tokio::test]
    async fn test_normalized_path_fullwidth_reverse_solidus() {
        // Fullwidth reverse solidus ＼ should be rejected as a confusable
        let normalized = NormalizedPath::new("/var/www", "/a\u{FF3C}b", false);
        assert!(normalized.is_err());
    }

    #[tokio::test]
    async fn test_normalized_path_max_length() {
        // Path exceeding 4096 bytes should be rejected
        let long_path = format!("/{}", "a".repeat(4096));
        let normalized = NormalizedPath::new("/var/www", &long_path, false);
        assert!(normalized.is_err());

        // Path at exactly 4096 bytes should be accepted
        let ok_path = format!("/{}", "a".repeat(4095));
        let normalized = NormalizedPath::new("/var/www", &ok_path, false);
        assert!(normalized.is_ok());
    }
}
