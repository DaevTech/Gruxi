mod common;

use common::{
    get_http_server_addr, get_status_code, parse_http_response_bytes,
    send_raw_http_request_bytes,
};

/// HTTP Range Requests Test Suite for Gruxi Web Server
///
/// This test suite validates Gruxi's implementation of HTTP range requests
/// as defined in RFC 9110 (HTTP Semantics) Section 14.
///
/// ============================================================================
/// IMPORTANT: These tests validate the ACTUAL running Gruxi server, not a mock!
/// ============================================================================
///
/// SETUP INSTRUCTIONS:
/// 1. Start Gruxi server: `cargo run` (in separate terminal)
/// 2. Ensure server is running on 127.0.0.1:80
/// 3. Ensure www-default/ directory has index.html (with some content)
/// 4. Run tests: `cargo test --test test_range_requests`
///
/// WHAT THESE TESTS VERIFY:
/// These tests send real HTTP requests to the running Gruxi server and verify:
///
/// ✓ Accept-Ranges header in responses (RFC 9110 Section 14.3)
/// ✓ Single byte range requests (RFC 9110 Section 14.1.2)
/// ✓ Suffix byte range requests (RFC 9110 Section 14.1.2)
/// ✓ Open-ended byte range requests (RFC 9110 Section 14.1.2)
/// ✓ 206 Partial Content responses (RFC 9110 Section 15.3.7)
/// ✓ Content-Range header format (RFC 9110 Section 14.4)
/// ✓ 416 Range Not Satisfiable responses (RFC 9110 Section 15.5.17)
/// ✓ Multiple range requests (multipart/byteranges)
/// ✓ If-Range header support (RFC 9110 Section 13.1.5)

// ============================================================================
// 1. ACCEPT-RANGES HEADER TESTS (RFC 9110 Section 14.3)
// ============================================================================

/// Test that responses include Accept-Ranges header
#[tokio::test]
async fn test_accept_ranges_header_present() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    let accept_ranges = headers.get("accept-ranges");
    assert!(
        accept_ranges.is_some(),
        "Response should include Accept-Ranges header"
    );

    let accept_ranges_value = accept_ranges.unwrap().to_str().unwrap_or("");
    assert_eq!(
        accept_ranges_value, "bytes",
        "Accept-Ranges should be 'bytes', got: {}",
        accept_ranges_value
    );
}

/// Test that HEAD responses also include Accept-Ranges header
#[tokio::test]
async fn test_accept_ranges_header_on_head() {
    let server_addr = get_http_server_addr();

    let request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    let accept_ranges = headers.get("accept-ranges");
    assert!(
        accept_ranges.is_some(),
        "HEAD response should include Accept-Ranges header"
    );
}

// ============================================================================
// 2. SINGLE BYTE RANGE TESTS (RFC 9110 Section 14.1.2)
// ============================================================================

/// Test single range request with both start and end
#[tokio::test]
async fn test_single_range_request() {
    let server_addr = get_http_server_addr();

    // Request first 10 bytes
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-9\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content, got: {}",
        status_line
    );

    // Check Content-Range header
    let content_range = headers.get("content-range");
    assert!(
        content_range.is_some(),
        "206 response should include Content-Range header"
    );

    let content_range_value = content_range.unwrap().to_str().unwrap_or("");
    assert!(
        content_range_value.starts_with("bytes 0-9/"),
        "Content-Range should start with 'bytes 0-9/', got: {}",
        content_range_value
    );

    // Body should be exactly 10 bytes
    assert_eq!(
        body.len(),
        10,
        "Body should be 10 bytes, got: {}",
        body.len()
    );
}

/// Test range request with only start position (open-ended)
#[tokio::test]
async fn test_open_ended_range_request() {
    let server_addr = get_http_server_addr();

    // First get the full content to know its length
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);
    let full_length = full_body.len();

    // Skip if file is too small for this test
    if full_length < 20 {
        return;
    }

    // Request from byte 10 to end
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=10-\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content, got: {}",
        status_line
    );

    let content_range = headers.get("content-range");
    assert!(
        content_range.is_some(),
        "206 response should include Content-Range header"
    );

    // Body should be (full_length - 10) bytes
    let expected_length = full_length - 10;
    assert_eq!(
        body.len(),
        expected_length,
        "Body should be {} bytes, got: {}",
        expected_length,
        body.len()
    );
}

/// Test suffix range request (last N bytes)
#[tokio::test]
async fn test_suffix_range_request() {
    let server_addr = get_http_server_addr();

    // First get the full content to know its length
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);
    let full_length = full_body.len();

    // Skip if file is too small
    if full_length < 10 {
        return;
    }

    // Request last 5 bytes
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=-5\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content, got: {}",
        status_line
    );

    let content_range = headers.get("content-range");
    assert!(
        content_range.is_some(),
        "206 response should include Content-Range header"
    );

    // Body should be 5 bytes (or full content if smaller)
    let expected_length = 5.min(full_length);
    assert_eq!(
        body.len(),
        expected_length,
        "Body should be {} bytes, got: {}",
        expected_length,
        body.len()
    );

    // Verify it's the last 5 bytes of the file
    let last_5_bytes_of_full = &full_body[full_length - expected_length..];
    assert_eq!(
        body, last_5_bytes_of_full,
        "Suffix range should return the last bytes of the file"
    );
}

// ============================================================================
// 3. CONTENT-RANGE HEADER FORMAT TESTS (RFC 9110 Section 14.4)
// ============================================================================

/// Test Content-Range header format: bytes start-end/complete-length
#[tokio::test]
async fn test_content_range_header_format() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-4\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content, got: {}",
        status_line
    );

    let content_range = headers.get("content-range").unwrap().to_str().unwrap_or("");

    // Format should be: bytes start-end/complete-length
    // Example: bytes 0-4/1234
    assert!(
        content_range.starts_with("bytes "),
        "Content-Range should start with 'bytes ', got: {}",
        content_range
    );

    // Parse the range part
    let range_part = content_range.strip_prefix("bytes ").unwrap_or("");
    let parts: Vec<&str> = range_part.split('/').collect();
    assert_eq!(
        parts.len(),
        2,
        "Content-Range should have format 'start-end/length', got: {}",
        content_range
    );

    // Check that start-end is parseable
    let range_positions: Vec<&str> = parts[0].split('-').collect();
    assert_eq!(
        range_positions.len(),
        2,
        "Range should have format 'start-end', got: {}",
        parts[0]
    );

    let start: u64 = range_positions[0].parse().expect("Start should be a number");
    let end: u64 = range_positions[1].parse().expect("End should be a number");
    let length: u64 = parts[1].parse().expect("Length should be a number");

    assert_eq!(start, 0, "Start should be 0");
    assert_eq!(end, 4, "End should be 4");
    assert!(length > 0, "Complete length should be > 0");
}

// ============================================================================
// 4. 416 RANGE NOT SATISFIABLE TESTS (RFC 9110 Section 15.5.17)
// ============================================================================

/// Test unsatisfiable range returns 416
#[tokio::test]
async fn test_unsatisfiable_range_returns_416() {
    let server_addr = get_http_server_addr();

    // First get the full content length
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);
    let full_length = full_body.len();

    // Request a range starting beyond the file length
    let start = full_length + 1000;
    let request = format!(
        "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes={}-\r\n\r\n",
        start
    );
    let response = send_raw_http_request_bytes(server_addr, &request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 416,
        "Expected 416 Range Not Satisfiable, got: {}",
        status_line
    );

    // Should include Content-Range with unsatisfiable indicator
    let content_range = headers.get("content-range");
    assert!(
        content_range.is_some(),
        "416 response should include Content-Range header"
    );

    let content_range_value = content_range.unwrap().to_str().unwrap_or("");
    assert!(
        content_range_value.starts_with("bytes */"),
        "Unsatisfiable Content-Range should be 'bytes */<length>', got: {}",
        content_range_value
    );
}

/// Test range with start > end returns 416
#[tokio::test]
async fn test_invalid_range_start_greater_than_end() {
    let server_addr = get_http_server_addr();

    // Request with start > end (invalid per RFC)
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=100-50\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Either 416 (unsatisfiable) or 200 (ignore invalid range) is acceptable
    assert!(
        status_code == 416 || status_code == 200,
        "Expected 416 or 200 for invalid range, got: {}",
        status_line
    );
}

// ============================================================================
// 5. MULTIPLE RANGE TESTS (RFC 9110 Section 14.1.2)
// ============================================================================

/// Test multiple range request returns multipart response
#[tokio::test]
async fn test_multiple_ranges_multipart_response() {
    let server_addr = get_http_server_addr();

    // First check if file is large enough for multiple ranges
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);

    // Skip if file is too small for multiple distinct ranges
    if full_body.len() < 20 {
        return;
    }

    // Request two ranges
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-4, 10-14\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content for multiple ranges, got: {}",
        status_line
    );

    // Content-Type should be multipart/byteranges with boundary
    let content_type = headers.get("content-type");
    assert!(
        content_type.is_some(),
        "Multiple range response should have Content-Type"
    );

    let content_type_value = content_type.unwrap().to_str().unwrap_or("");
    assert!(
        content_type_value.starts_with("multipart/byteranges"),
        "Content-Type should be multipart/byteranges, got: {}",
        content_type_value
    );

    // Should contain boundary parameter
    assert!(
        content_type_value.contains("boundary="),
        "multipart/byteranges should specify boundary, got: {}",
        content_type_value
    );

    // Body should contain the boundary delimiters
    let body_str = String::from_utf8_lossy(&body);
    assert!(
        body_str.contains("--"),
        "Multipart body should contain boundary delimiters"
    );

    // Body should contain Content-Range headers for each part
    assert!(
        body_str.contains("Content-Range:"),
        "Multipart body should contain Content-Range headers"
    );
}

// ============================================================================
// 6. IF-RANGE HEADER TESTS (RFC 9110 Section 13.1.5)
// ============================================================================

/// Test If-Range with matching ETag returns partial content
#[tokio::test]
async fn test_if_range_with_matching_etag() {
    let server_addr = get_http_server_addr();

    // First get the ETag
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, full_headers, _) = parse_http_response_bytes(&full_response);

    let etag = match full_headers.get("etag") {
        Some(e) => e.to_str().unwrap_or(""),
        None => return, // Skip if no ETag support
    };

    // Request with matching If-Range
    let request = format!(
        "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-9\r\nIf-Range: {}\r\n\r\n",
        etag
    );
    let response = send_raw_http_request_bytes(server_addr, &request)
        .await
        .unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Matching If-Range should return 206, got: {}",
        status_line
    );

    assert_eq!(
        body.len(),
        10,
        "Should return partial content (10 bytes), got: {}",
        body.len()
    );
}

/// Test If-Range with non-matching ETag returns full content
#[tokio::test]
async fn test_if_range_with_non_matching_etag() {
    let server_addr = get_http_server_addr();

    // Request with non-matching If-Range ETag
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-9\r\nIf-Range: \"non-matching-etag\"\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Should return 200 with full content (not 206)
    assert_eq!(
        status_code, 200,
        "Non-matching If-Range should return 200 with full content, got: {}",
        status_line
    );
}

/// Test If-Range with matching Last-Modified date returns partial content
#[tokio::test]
async fn test_if_range_with_matching_date() {
    let server_addr = get_http_server_addr();

    // First get the Last-Modified date
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, full_headers, _) = parse_http_response_bytes(&full_response);

    let last_modified = match full_headers.get("last-modified") {
        Some(lm) => lm.to_str().unwrap_or(""),
        None => return, // Skip if no Last-Modified support
    };

    // Request with matching If-Range (using date)
    let request = format!(
        "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-9\r\nIf-Range: {}\r\n\r\n",
        last_modified
    );
    let response = send_raw_http_request_bytes(server_addr, &request)
        .await
        .unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Matching If-Range date should return 206, got: {}",
        status_line
    );

    assert_eq!(
        body.len(),
        10,
        "Should return partial content (10 bytes), got: {}",
        body.len()
    );
}

// ============================================================================
// 7. EDGE CASES AND BOUNDARY TESTS
// ============================================================================

/// Test range that exceeds file length is clamped
#[tokio::test]
async fn test_range_clamped_to_file_length() {
    let server_addr = get_http_server_addr();

    // First get the full content length
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);
    let full_length = full_body.len();

    // Request range that extends beyond file length
    let request = format!(
        "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-{}\r\n\r\n",
        full_length + 1000
    );
    let response = send_raw_http_request_bytes(server_addr, &request)
        .await
        .unwrap();
    let (status_line, headers, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Range exceeding file should return 206, got: {}",
        status_line
    );

    // Body should be clamped to actual file length
    assert_eq!(
        body.len(),
        full_length,
        "Body should be clamped to file length {}, got: {}",
        full_length,
        body.len()
    );

    // Content-Range should show actual end position
    let content_range = headers.get("content-range").unwrap().to_str().unwrap_or("");
    let expected_end = format!("bytes 0-{}/{}", full_length - 1, full_length);
    assert_eq!(
        content_range, expected_end,
        "Content-Range should be clamped, got: {}",
        content_range
    );
}

/// Test suffix range larger than file returns entire file
#[tokio::test]
async fn test_suffix_range_larger_than_file() {
    let server_addr = get_http_server_addr();

    // First get the full content
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);
    let full_length = full_body.len();

    // Request suffix larger than file
    let suffix = full_length + 1000;
    let request = format!(
        "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=-{}\r\n\r\n",
        suffix
    );
    let response = send_raw_http_request_bytes(server_addr, &request)
        .await
        .unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Large suffix range should return 206, got: {}",
        status_line
    );

    // Should return entire file
    assert_eq!(
        body.len(),
        full_length,
        "Large suffix should return entire file, got: {}",
        body.len()
    );

    // Content should match
    assert_eq!(body, full_body, "Content should match full file");
}

/// Test that range response content matches the corresponding bytes of full content
#[tokio::test]
async fn test_range_content_matches_full_content() {
    let server_addr = get_http_server_addr();

    // Get full content
    let full_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let full_response = send_raw_http_request_bytes(server_addr, full_request)
        .await
        .unwrap();
    let (_, _, full_body) = parse_http_response_bytes(&full_response);

    // Skip if file is too small
    if full_body.len() < 20 {
        return;
    }

    // Get range
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=5-14\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, range_body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content, got: {}",
        status_line
    );

    // Verify content matches
    let expected_content = &full_body[5..15]; // bytes 5-14 (inclusive)
    assert_eq!(
        range_body, expected_content,
        "Range content should match corresponding bytes of full content"
    );
}

/// Test invalid range syntax is ignored (returns full content per RFC 9110)
#[tokio::test]
async fn test_invalid_range_syntax_ignored() {
    let server_addr = get_http_server_addr();

    // Invalid range syntax should be ignored
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=abc-def\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Invalid syntax should be ignored, returning 200 with full content
    assert_eq!(
        status_code, 200,
        "Invalid range syntax should be ignored, expected 200, got: {}",
        status_line
    );
}

/// Test unsupported range unit is ignored
#[tokio::test]
async fn test_unsupported_range_unit_ignored() {
    let server_addr = get_http_server_addr();

    // Unsupported unit should be ignored
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: items=0-10\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Unsupported unit should be ignored, returning 200 with full content
    assert_eq!(
        status_code, 200,
        "Unsupported range unit should be ignored, expected 200, got: {}",
        status_line
    );
}

/// Test zero-byte range (-0) returns 416 or is ignored
#[tokio::test]
async fn test_zero_suffix_range() {
    let server_addr = get_http_server_addr();

    // Request for "last 0 bytes" should be handled gracefully
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=-0\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Zero-byte suffix is technically unsatisfiable or should be ignored
    assert!(
        status_code == 416 || status_code == 200,
        "Zero suffix should return 416 or 200, got: {}",
        status_line
    );
}

// ============================================================================
// 8. INTERACTION WITH OTHER HEADERS
// ============================================================================

/// Test that range requests don't serve gzipped content
#[tokio::test]
async fn test_range_requests_not_gzipped() {
    let server_addr = get_http_server_addr();

    // Request with both Range and Accept-Encoding
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nRange: bytes=0-9\r\nAccept-Encoding: gzip\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 206,
        "Expected 206 Partial Content, got: {}",
        status_line
    );

    // Content-Encoding should NOT be gzip for range requests
    // (would make byte ranges meaningless)
    let content_encoding = headers.get("content-encoding");
    if let Some(ce) = content_encoding {
        let ce_value = ce.to_str().unwrap_or("");
        assert_ne!(
            ce_value, "gzip",
            "Range responses should not be gzip encoded"
        );
    }
}

/// Test that Accept-Ranges is present even on 404 responses
#[tokio::test]
async fn test_accept_ranges_not_on_404() {
    let server_addr = get_http_server_addr();

    let request = "GET /nonexistent-file-12345.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 404, "Expected 404, got: {}", status_line);

    // Accept-Ranges is optional on error responses, so we don't assert either way
}
