/// HTTP Range Requests implementation per RFC 9110
/// https://www.rfc-editor.org/rfc/rfc9110#section-14
///
/// Supports:
/// - Single byte ranges (bytes=0-499, bytes=500-, bytes=-500)
/// - Multiple byte ranges (bytes=0-499,500-999)
/// - Accept-Ranges header
/// - 206 Partial Content responses
/// - 416 Range Not Satisfiable responses
/// - Conditional range requests with If-Range header

use std::time::SystemTime;

use http::HeaderValue;
use hyper::body::Bytes;

use crate::{
    trace,
    http::request_response::gruxi_request::GruxiRequest,
};

/// Represents a parsed byte range from the Range header
#[derive(Debug, Clone, PartialEq)]
pub struct ByteRange {
    /// Start position (inclusive), None means "from the end"
    pub start: Option<u64>,
    /// End position (inclusive), None means "to the end"
    pub end: Option<u64>,
}

impl ByteRange {
    /// Create a new ByteRange
    pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
        Self { start, end }
    }

    /// Resolve this range against the total content length
    /// Returns (start, end) as inclusive byte positions, or None if the range is not satisfiable
    pub fn resolve(&self, content_length: u64) -> Option<(u64, u64)> {
        if content_length == 0 {
            return None;
        }

        match (self.start, self.end) {
            // bytes=start-end (both specified)
            (Some(start), Some(end)) => {
                if start > end || start >= content_length {
                    None
                } else {
                    // Clamp end to content_length - 1
                    let resolved_end = end.min(content_length - 1);
                    Some((start, resolved_end))
                }
            }
            // bytes=start- (from start to end of file)
            (Some(start), None) => {
                if start >= content_length {
                    None
                } else {
                    Some((start, content_length - 1))
                }
            }
            // bytes=-suffix (last N bytes)
            (None, Some(suffix_length)) => {
                if suffix_length == 0 {
                    None
                } else if suffix_length >= content_length {
                    // Suffix length exceeds content, return entire content
                    Some((0, content_length - 1))
                } else {
                    let start = content_length - suffix_length;
                    Some((start, content_length - 1))
                }
            }
            // Invalid: neither start nor end specified
            (None, None) => None,
        }
    }

    /// Get the length of this range when resolved against content_length
    pub fn resolved_length(&self, content_length: u64) -> Option<u64> {
        self.resolve(content_length).map(|(start, end)| end - start + 1)
    }
}

/// Result of parsing the Range header
#[derive(Debug, Clone)]
pub enum RangeParseResult {
    /// No Range header present - serve full content
    NoRangeHeader,
    /// Valid range(s) parsed
    Ranges(Vec<ByteRange>),
    /// Invalid range syntax - ignore the Range header (serve full content per RFC 9110)
    InvalidSyntax,
    /// Range unit not supported (only "bytes" is supported) - serve full content
    UnsupportedUnit,
}

/// Parse the Range header value according to RFC 9110 Section 14.1.2
/// Range = ranges-specifier
/// ranges-specifier = range-unit "=" range-set
/// range-unit = token
/// range-set = 1#range-spec
/// range-spec = int-range / suffix-range
/// int-range = first-pos "-" [ last-pos ]
/// suffix-range = "-" suffix-length
pub fn parse_range_header(range_header: &str) -> RangeParseResult {
    let range_header = range_header.trim();

    // Check for "bytes=" prefix (case-insensitive per RFC 9110)
    if !range_header.to_ascii_lowercase().starts_with("bytes=") {
        return RangeParseResult::UnsupportedUnit;
    }

    let range_set = &range_header[6..]; // Skip "bytes="
    if range_set.is_empty() {
        return RangeParseResult::InvalidSyntax;
    }

    let mut ranges = Vec::new();

    // Split by comma to handle multiple ranges
    for range_spec in range_set.split(',') {
        let range_spec = range_spec.trim();
        if range_spec.is_empty() {
            continue;
        }

        match parse_range_spec(range_spec) {
            Some(range) => ranges.push(range),
            None => {
                return RangeParseResult::InvalidSyntax;
            }
        }
    }

    if ranges.is_empty() {
        RangeParseResult::InvalidSyntax
    } else {
        RangeParseResult::Ranges(ranges)
    }
}

/// Parse a single range-spec (either int-range or suffix-range)
fn parse_range_spec(spec: &str) -> Option<ByteRange> {
    let spec = spec.trim();

    // Find the hyphen
    let hyphen_pos = spec.find('-')?;

    let before_hyphen = spec[..hyphen_pos].trim();
    let after_hyphen = spec[hyphen_pos + 1..].trim();

    // Suffix range: -suffix-length
    if before_hyphen.is_empty() {
        if after_hyphen.is_empty() {
            return None; // Just "-" is invalid
        }
        let suffix_length: u64 = after_hyphen.parse().ok()?;
        return Some(ByteRange::new(None, Some(suffix_length)));
    }

    // Int range: first-pos "-" [ last-pos ]
    let first_pos: u64 = before_hyphen.parse().ok()?;

    if after_hyphen.is_empty() {
        // bytes=N- (from N to end)
        Some(ByteRange::new(Some(first_pos), None))
    } else {
        // bytes=N-M
        let last_pos: u64 = after_hyphen.parse().ok()?;
        Some(ByteRange::new(Some(first_pos), Some(last_pos)))
    }
}

/// Check if we should process the range request based on preconditions
/// Implements the If-Range header check per RFC 9110 Section 13.1.5
pub fn should_process_range(
    gruxi_request: &GruxiRequest,
    etag: Option<&str>,
    last_modified: &SystemTime,
) -> bool {
    // Check for If-Range header
    let if_range = match gruxi_request.get_headers().get("If-Range") {
        Some(value) => match value.to_str() {
            Ok(s) => s,
            Err(_) => return true, // No valid If-Range, process the range
        },
        None => return true, // No If-Range header, process the range
    };

    let if_range = if_range.trim();

    // If-Range can be either an HTTP-date or an entity-tag
    // Try entity-tag first (starts with " or W/)
    if if_range.starts_with('"') || if_range.starts_with("W/") {
        // It's an entity-tag
        if let Some(current_etag) = etag {
            // Strong comparison required for If-Range (RFC 9110 Section 13.1.5)
            let matches = !if_range.starts_with("W/")
                && !current_etag.starts_with("W/")
                && if_range == current_etag;
            trace!("If-Range ETag comparison: {} vs {} = {}", if_range, current_etag, matches);
            return matches;
        }
        return false; // No ETag to compare against
    }

    // Try parsing as HTTP-date
    if let Ok(if_range_time) = httpdate::parse_http_date(if_range) {
        // Compare with last-modified time (exact match required per RFC 9110)
        let last_modified_secs = last_modified
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let if_range_secs = if_range_time
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let matches = last_modified_secs == if_range_secs;
        trace!("If-Range date comparison: {} == {} = {}", last_modified_secs, if_range_secs, matches);
        return matches;
    }

    // Invalid If-Range value - ignore range request per RFC 9110
    trace!("If-Range header has invalid value: {}", if_range);
    false
}

/// Format a Content-Range header value for a single range
/// Format: bytes start-end/complete-length
pub fn format_content_range(start: u64, end: u64, complete_length: u64) -> String {
    format!("bytes {}-{}/{}", start, end, complete_length)
}

/// Format a Content-Range header for an unsatisfiable range
/// Format: bytes */complete-length
pub fn format_content_range_unsatisfiable(complete_length: u64) -> String {
    format!("bytes */{}", complete_length)
}

/// Generate a multipart boundary string
pub fn generate_multipart_boundary() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("gruxi_boundary_{:x}", timestamp)
}

/// Build a multipart/byteranges body for multiple ranges
/// Returns the body bytes and the content-type header value (with boundary)
pub fn build_multipart_body(
    ranges: &[(u64, u64)], // Resolved ranges
    content: &[u8],
    content_type: &str,
    complete_length: u64,
) -> (Bytes, String) {
    let boundary = generate_multipart_boundary();
    let mut body = Vec::new();

    for (start, end) in ranges {
        // Boundary delimiter
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");

        // Content-Type header for this part
        body.extend_from_slice(b"Content-Type: ");
        body.extend_from_slice(content_type.as_bytes());
        body.extend_from_slice(b"\r\n");

        // Content-Range header for this part
        body.extend_from_slice(b"Content-Range: ");
        let range_header = format_content_range(*start, *end, complete_length);
        body.extend_from_slice(range_header.as_bytes());
        body.extend_from_slice(b"\r\n");

        // Empty line before content
        body.extend_from_slice(b"\r\n");

        // The actual content for this range
        let range_start = *start as usize;
        let range_end = (*end + 1) as usize;
        if range_start < content.len() && range_end <= content.len() {
            body.extend_from_slice(&content[range_start..range_end]);
        }
        body.extend_from_slice(b"\r\n");
    }

    // Final boundary delimiter
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");

    let content_type_header = format!("multipart/byteranges; boundary={}", boundary);
    (Bytes::from(body), content_type_header)
}

/// Build a multipart/byteranges body from pre-read range parts
/// This is used when reading ranges from uncached files to avoid re-reading
/// Returns the body bytes and the content-type header value (with boundary)
pub fn build_multipart_body_from_parts(
    ranges: &[(u64, u64)], // Resolved ranges
    parts: &[Vec<u8>],     // Pre-read content for each range
    content_type: &str,
    complete_length: u64,
) -> (Bytes, String) {
    let boundary = generate_multipart_boundary();
    let mut body = Vec::new();

    for (i, (start, end)) in ranges.iter().enumerate() {
        // Boundary delimiter
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");

        // Content-Type header for this part
        body.extend_from_slice(b"Content-Type: ");
        body.extend_from_slice(content_type.as_bytes());
        body.extend_from_slice(b"\r\n");

        // Content-Range header for this part
        body.extend_from_slice(b"Content-Range: ");
        let range_header = format_content_range(*start, *end, complete_length);
        body.extend_from_slice(range_header.as_bytes());
        body.extend_from_slice(b"\r\n");

        // Empty line before content
        body.extend_from_slice(b"\r\n");

        // The actual content for this range (from pre-read parts)
        if let Some(part) = parts.get(i) {
            body.extend_from_slice(part);
        }
        body.extend_from_slice(b"\r\n");
    }

    // Final boundary delimiter
    body.extend_from_slice(b"--");
    body.extend_from_slice(boundary.as_bytes());
    body.extend_from_slice(b"--\r\n");

    let content_type_header = format!("multipart/byteranges; boundary={}", boundary);
    (Bytes::from(body), content_type_header)
}

/// Extract a slice of bytes for the given range
pub fn extract_range_bytes(content: &[u8], start: u64, end: u64) -> Bytes {
    let start = start as usize;
    let end = (end + 1) as usize; // end is inclusive, so we add 1

    if start >= content.len() {
        return Bytes::new();
    }

    let actual_end = end.min(content.len());
    Bytes::copy_from_slice(&content[start..actual_end])
}

/// Create the Accept-Ranges header value
pub fn accept_ranges_bytes() -> HeaderValue {
    HeaderValue::from_static("bytes")
}

/// Check if the Range header is present in the request
pub fn has_range_header(gruxi_request: &GruxiRequest) -> bool {
    gruxi_request.get_headers().get("Range").is_some()
}

/// Get the Range header value from the request
pub fn get_range_header(gruxi_request: &GruxiRequest) -> Option<&HeaderValue> {
    gruxi_request.get_headers().get("Range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_range() {
        match parse_range_header("bytes=0-499") {
            RangeParseResult::Ranges(ranges) => {
                assert_eq!(ranges.len(), 1);
                assert_eq!(ranges[0], ByteRange::new(Some(0), Some(499)));
            }
            _ => panic!("Expected Ranges result"),
        }
    }

    #[test]
    fn test_parse_open_ended_range() {
        match parse_range_header("bytes=500-") {
            RangeParseResult::Ranges(ranges) => {
                assert_eq!(ranges.len(), 1);
                assert_eq!(ranges[0], ByteRange::new(Some(500), None));
            }
            _ => panic!("Expected Ranges result"),
        }
    }

    #[test]
    fn test_parse_suffix_range() {
        match parse_range_header("bytes=-500") {
            RangeParseResult::Ranges(ranges) => {
                assert_eq!(ranges.len(), 1);
                assert_eq!(ranges[0], ByteRange::new(None, Some(500)));
            }
            _ => panic!("Expected Ranges result"),
        }
    }

    #[test]
    fn test_parse_multiple_ranges() {
        match parse_range_header("bytes=0-499, 500-999, 1000-1499") {
            RangeParseResult::Ranges(ranges) => {
                assert_eq!(ranges.len(), 3);
                assert_eq!(ranges[0], ByteRange::new(Some(0), Some(499)));
                assert_eq!(ranges[1], ByteRange::new(Some(500), Some(999)));
                assert_eq!(ranges[2], ByteRange::new(Some(1000), Some(1499)));
            }
            _ => panic!("Expected Ranges result"),
        }
    }

    #[test]
    fn test_resolve_range() {
        // Normal range
        let range = ByteRange::new(Some(0), Some(499));
        assert_eq!(range.resolve(1000), Some((0, 499)));

        // Range exceeds content
        let range = ByteRange::new(Some(0), Some(1500));
        assert_eq!(range.resolve(1000), Some((0, 999)));

        // Open-ended range
        let range = ByteRange::new(Some(500), None);
        assert_eq!(range.resolve(1000), Some((500, 999)));

        // Suffix range
        let range = ByteRange::new(None, Some(200));
        assert_eq!(range.resolve(1000), Some((800, 999)));

        // Suffix exceeds content
        let range = ByteRange::new(None, Some(1500));
        assert_eq!(range.resolve(1000), Some((0, 999)));

        // Invalid: start beyond content
        let range = ByteRange::new(Some(1500), Some(2000));
        assert_eq!(range.resolve(1000), None);
    }

    #[test]
    fn test_unsupported_unit() {
        match parse_range_header("items=0-499") {
            RangeParseResult::UnsupportedUnit => {}
            _ => panic!("Expected UnsupportedUnit result"),
        }
    }

    #[test]
    fn test_invalid_syntax() {
        match parse_range_header("bytes=") {
            RangeParseResult::InvalidSyntax => {}
            _ => panic!("Expected InvalidSyntax result"),
        }

        match parse_range_header("bytes=-") {
            RangeParseResult::InvalidSyntax => {}
            _ => panic!("Expected InvalidSyntax result"),
        }

        match parse_range_header("bytes=abc-def") {
            RangeParseResult::InvalidSyntax => {}
            _ => panic!("Expected InvalidSyntax result"),
        }
    }

    #[test]
    fn test_format_content_range() {
        assert_eq!(
            format_content_range(0, 499, 1000),
            "bytes 0-499/1000"
        );
        assert_eq!(
            format_content_range_unsatisfiable(1000),
            "bytes */1000"
        );
    }

    #[test]
    fn test_extract_range_bytes() {
        let content = b"Hello, World!";

        // First 5 bytes
        let result = extract_range_bytes(content, 0, 4);
        assert_eq!(&result[..], b"Hello");

        // Middle bytes
        let result = extract_range_bytes(content, 7, 11);
        assert_eq!(&result[..], b"World");

        // Last byte
        let result = extract_range_bytes(content, 12, 12);
        assert_eq!(&result[..], b"!");
    }
}
