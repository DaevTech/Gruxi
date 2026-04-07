pub fn sanitize_log_entry(input: &str) -> String {
    // Remove all newlines and carriage returns to prevent log injection
    input
        .replace(['\n', '\r'], "")
        // Remove all other control characters (ASCII < 32) to prevent log injection and other issues
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_normal_text() {
        assert_eq!(sanitize_log_entry("hello world"), "hello world");
    }

    #[test]
    fn removes_newline() {
        assert_eq!(sanitize_log_entry("line1\nline2"), "line1line2");
    }

    #[test]
    fn removes_carriage_return() {
        assert_eq!(sanitize_log_entry("line1\rline2"), "line1line2");
    }

    #[test]
    fn removes_crlf() {
        assert_eq!(sanitize_log_entry("line1\r\nline2"), "line1line2");
    }

    #[test]
    fn removes_control_characters() {
        assert_eq!(sanitize_log_entry("abc\x00\x01\x07\x1bdef"), "abcdef");
    }

    #[test]
    fn preserves_unicode() {
        assert_eq!(sanitize_log_entry("héllo wörld 日本語"), "héllo wörld 日本語");
    }

    #[test]
    fn handles_empty_string() {
        assert_eq!(sanitize_log_entry(""), "");
    }

    #[test]
    fn removes_tab() {
        assert_eq!(sanitize_log_entry("col1\tcol2"), "col1col2");
    }

    #[test]
    fn handles_only_control_characters() {
        assert_eq!(sanitize_log_entry("\n\r\x00\x1b"), "");
    }

    #[test]
    fn prevents_log_injection() {
        let malicious = "normal log\nINFO fake log entry";
        assert_eq!(sanitize_log_entry(malicious), "normal logINFO fake log entry");
    }
}
