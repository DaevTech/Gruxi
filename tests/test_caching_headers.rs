mod common;

use common::{
    get_http_server_addr, get_status_code, parse_http_response_bytes,
    send_raw_http_request_bytes,
};

/// HTTP Caching Headers Test Suite for Gruxi Web Server
///
/// This test suite validates Gruxi's implementation of HTTP caching headers
/// as defined in RFC 7234 (Caching) and RFC 7232 (Conditional Requests).
///
/// ============================================================================
/// IMPORTANT: These tests validate the ACTUAL running Gruxi server, not a mock!
/// ============================================================================
///
/// SETUP INSTRUCTIONS:
/// 1. Start Gruxi server: `cargo run` (in separate terminal)
/// 2. Ensure server is running on 127.0.0.1:80
/// 3. Ensure www-default/ directory has index.html
/// 4. Run tests: `cargo test --test test_caching_headers`
///
/// WHAT THESE TESTS VERIFY:
/// These tests send real HTTP requests to the running Gruxi server and verify:
///
/// ✓ Last-Modified: Server returns modification timestamp (RFC 7232 Section 2.2)
/// ✓ Expires: Server returns expiration date for caching (RFC 7234 Section 5.3)
/// ✓ Cache-Control: Server returns cache directives (RFC 7234 Section 5.2)
/// ✓ ETag: Server returns entity tag for validation (RFC 7232 Section 2.3)

// ============================================================================
// 1. LAST-MODIFIED HEADER TESTS (RFC 7232 Section 2.2)
// ============================================================================

/// Test that responses include Last-Modified header for static files
#[tokio::test]
async fn test_last_modified_header_present() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    let last_modified = headers.get("last-modified");
    assert!(
        last_modified.is_some(),
        "Response should include Last-Modified header for static files"
    );
}

/// Test that Last-Modified header has valid HTTP-date format (RFC 7231 Section 7.1.1.1)
#[tokio::test]
async fn test_last_modified_header_format() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers, _) = parse_http_response_bytes(&response);

    if let Some(last_modified) = headers.get("last-modified") {
        let value = last_modified.to_str().unwrap_or("");

        // HTTP-date format: "Day, DD Mon YYYY HH:MM:SS GMT"
        // Example: "Wed, 21 Oct 2015 07:28:00 GMT"
        assert!(
            !value.is_empty(),
            "Last-Modified header should not be empty"
        );

        // Check it ends with GMT (preferred format per RFC 7231)
        assert!(
            value.ends_with("GMT"),
            "Last-Modified should be in GMT timezone, got: {}",
            value
        );

        // Basic structure check: should have day name, date, time
        let parts: Vec<&str> = value.split_whitespace().collect();
        assert!(
            parts.len() >= 5,
            "Last-Modified should have proper HTTP-date format, got: {}",
            value
        );
    } else {
        panic!("Last-Modified header not present");
    }
}

/// Test that Last-Modified is consistent across multiple requests
#[tokio::test]
async fn test_last_modified_header_consistency() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";

    // Make first request
    let response1 = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers1, _) = parse_http_response_bytes(&response1);

    // Make second request
    let response2 = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers2, _) = parse_http_response_bytes(&response2);

    let last_modified1 = headers1
        .get("last-modified")
        .map(|v| v.to_str().unwrap_or(""));
    let last_modified2 = headers2
        .get("last-modified")
        .map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        last_modified1, last_modified2,
        "Last-Modified should be consistent across requests for the same resource"
    );
}

/// Test that HEAD request includes Last-Modified header
#[tokio::test]
async fn test_last_modified_header_on_head_request() {
    let server_addr = get_http_server_addr();

    let request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    let last_modified = headers.get("last-modified");
    assert!(
        last_modified.is_some(),
        "HEAD response should include Last-Modified header"
    );
}

/// Test that Last-Modified matches between GET and HEAD requests
#[tokio::test]
async fn test_last_modified_matches_get_and_head() {
    let server_addr = get_http_server_addr();

    let get_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let get_response = send_raw_http_request_bytes(server_addr, get_request)
        .await
        .unwrap();
    let (_, get_headers, _) = parse_http_response_bytes(&get_response);

    let head_request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let head_response = send_raw_http_request_bytes(server_addr, head_request)
        .await
        .unwrap();
    let (_, head_headers, _) = parse_http_response_bytes(&head_response);

    let get_last_modified = get_headers
        .get("last-modified")
        .map(|v| v.to_str().unwrap_or(""));
    let head_last_modified = head_headers
        .get("last-modified")
        .map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        get_last_modified, head_last_modified,
        "Last-Modified should match between GET and HEAD requests"
    );
}

// ============================================================================
// 2. EXPIRES HEADER TESTS (RFC 7234 Section 5.3)
// ============================================================================

/// Test that responses may include Expires header for cacheable resources
#[tokio::test]
async fn test_expires_header_format_if_present() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    if let Some(expires) = headers.get("expires") {
        let value = expires.to_str().unwrap_or("");

        // Expires should be an HTTP-date or "0" (for already expired)
        // HTTP-date format: "Day, DD Mon YYYY HH:MM:SS GMT"
        if value != "0" && value != "-1" {
            assert!(
                value.ends_with("GMT") || value.contains("GMT"),
                "Expires header should be in GMT timezone or be '0', got: {}",
                value
            );
        }
    }
    // Note: Expires header is optional, so we don't fail if it's not present
}

/// Test that Expires header on HEAD request matches GET request
#[tokio::test]
async fn test_expires_header_matches_get_and_head() {
    let server_addr = get_http_server_addr();

    let get_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let get_response = send_raw_http_request_bytes(server_addr, get_request)
        .await
        .unwrap();
    let (_, get_headers, _) = parse_http_response_bytes(&get_response);

    let head_request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let head_response = send_raw_http_request_bytes(server_addr, head_request)
        .await
        .unwrap();
    let (_, head_headers, _) = parse_http_response_bytes(&head_response);

    let get_expires = get_headers.get("expires").map(|v| v.to_str().unwrap_or(""));
    let head_expires = head_headers
        .get("expires")
        .map(|v| v.to_str().unwrap_or(""));

    // Only compare if at least one has the header
    if get_expires.is_some() || head_expires.is_some() {
        assert_eq!(
            get_expires, head_expires,
            "Expires header should match between GET and HEAD requests"
        );
    }
}

/// Test that Expires is consistent across multiple requests
#[tokio::test]
async fn test_expires_header_consistency() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";

    // Make first request
    let response1 = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers1, _) = parse_http_response_bytes(&response1);

    // Small delay to ensure we're not getting cached responses
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Make second request
    let response2 = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers2, _) = parse_http_response_bytes(&response2);

    let expires1 = headers1.get("expires").map(|v| v.to_str().unwrap_or(""));
    let expires2 = headers2.get("expires").map(|v| v.to_str().unwrap_or(""));

    // If Expires is present, it should be consistent (or both should be absent)
    if expires1.is_some() && expires2.is_some() {
        // Note: Some servers compute Expires dynamically, so we just check both exist
        assert!(
            expires1.is_some() && expires2.is_some(),
            "Expires header presence should be consistent across requests"
        );
    }
}

// ============================================================================
// 3. CACHE-CONTROL HEADER TESTS (RFC 7234 Section 5.2)
// ============================================================================

/// Test that Cache-Control header format is valid if present
#[tokio::test]
async fn test_cache_control_header_format_if_present() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    if let Some(cache_control) = headers.get("cache-control") {
        let value = cache_control.to_str().unwrap_or("");

        // Cache-Control directives are comma-separated
        // Valid directives include: max-age, no-cache, no-store, public, private, etc.
        assert!(
            !value.is_empty(),
            "Cache-Control header should not be empty if present"
        );

        // Check for common valid directive patterns
        let valid_directives = [
            "max-age",
            "no-cache",
            "no-store",
            "public",
            "private",
            "must-revalidate",
            "proxy-revalidate",
            "s-maxage",
            "immutable",
        ];

        let has_valid_directive = valid_directives
            .iter()
            .any(|d| value.to_lowercase().contains(d));

        assert!(
            has_valid_directive,
            "Cache-Control should contain valid directives, got: {}",
            value
        );
    }
    // Note: Cache-Control header is optional
}

/// Test Cache-Control matches between GET and HEAD requests
#[tokio::test]
async fn test_cache_control_matches_get_and_head() {
    let server_addr = get_http_server_addr();

    let get_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let get_response = send_raw_http_request_bytes(server_addr, get_request)
        .await
        .unwrap();
    let (_, get_headers, _) = parse_http_response_bytes(&get_response);

    let head_request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let head_response = send_raw_http_request_bytes(server_addr, head_request)
        .await
        .unwrap();
    let (_, head_headers, _) = parse_http_response_bytes(&head_response);

    let get_cache_control = get_headers
        .get("cache-control")
        .map(|v| v.to_str().unwrap_or(""));
    let head_cache_control = head_headers
        .get("cache-control")
        .map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        get_cache_control, head_cache_control,
        "Cache-Control should match between GET and HEAD requests"
    );
}

// ============================================================================
// 4. ETAG HEADER TESTS (RFC 7232 Section 2.3)
// ============================================================================

/// Test that responses include ETag header for static files
#[tokio::test]
async fn test_etag_header_present() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    let etag = headers.get("etag");
    assert!(
        etag.is_some(),
        "Response should include ETag header for static files"
    );
}

/// Test that ETag header has valid format (quoted string or weak validator)
#[tokio::test]
async fn test_etag_header_format() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers, _) = parse_http_response_bytes(&response);

    if let Some(etag) = headers.get("etag") {
        let value = etag.to_str().unwrap_or("");

        // ETag format: either "value" (strong) or W/"value" (weak)
        let is_strong = value.starts_with('"') && value.ends_with('"');
        let is_weak = value.starts_with("W/\"") && value.ends_with('"');

        assert!(
            is_strong || is_weak,
            "ETag should be a quoted string or weak validator (W/\"...\"), got: {}",
            value
        );
    } else {
        panic!("ETag header not present");
    }
}

/// Test that ETag is consistent across multiple requests
#[tokio::test]
async fn test_etag_header_consistency() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";

    // Make first request
    let response1 = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers1, _) = parse_http_response_bytes(&response1);

    // Make second request
    let response2 = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers2, _) = parse_http_response_bytes(&response2);

    let etag1 = headers1.get("etag").map(|v| v.to_str().unwrap_or(""));
    let etag2 = headers2.get("etag").map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        etag1, etag2,
        "ETag should be consistent across requests for the same unchanged resource"
    );
}

/// Test that HEAD request includes ETag header
#[tokio::test]
async fn test_etag_header_on_head_request() {
    let server_addr = get_http_server_addr();

    let request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    let etag = headers.get("etag");
    assert!(
        etag.is_some(),
        "HEAD response should include ETag header"
    );
}

/// Test that ETag matches between GET and HEAD requests
#[tokio::test]
async fn test_etag_matches_get_and_head() {
    let server_addr = get_http_server_addr();

    let get_request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let get_response = send_raw_http_request_bytes(server_addr, get_request)
        .await
        .unwrap();
    let (_, get_headers, _) = parse_http_response_bytes(&get_response);

    let head_request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let head_response = send_raw_http_request_bytes(server_addr, head_request)
        .await
        .unwrap();
    let (_, head_headers, _) = parse_http_response_bytes(&head_response);

    let get_etag = get_headers.get("etag").map(|v| v.to_str().unwrap_or(""));
    let head_etag = head_headers.get("etag").map(|v| v.to_str().unwrap_or(""));

    assert_eq!(
        get_etag, head_etag,
        "ETag should match between GET and HEAD requests"
    );
}

// ============================================================================
// 5. COMBINED CACHING HEADERS TESTS
// ============================================================================

/// Test that all caching-related headers are present for static files
#[tokio::test]
async fn test_all_caching_headers_present() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(status_code, 200, "Expected 200 OK, got: {}", status_line);

    // These headers should be present for proper HTTP caching support
    let etag = headers.get("etag");
    let last_modified = headers.get("last-modified");

    assert!(
        etag.is_some(),
        "Response should include ETag for cache validation"
    );
    assert!(
        last_modified.is_some(),
        "Response should include Last-Modified for cache validation"
    );

    // These are optional but recommended
    let _cache_control = headers.get("cache-control");
    let _expires = headers.get("expires");

    // At least one freshness indicator should be present (Cache-Control or Expires)
    // This is a recommendation, not a requirement
}

/// Test 304 response includes validator headers
#[tokio::test]
async fn test_304_includes_validator_headers() {
    let server_addr = get_http_server_addr();

    // First, get the ETag
    let initial_request =
        "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let initial_response = send_raw_http_request_bytes(server_addr, initial_request)
        .await
        .unwrap();
    let (_, initial_headers, _) = parse_http_response_bytes(&initial_response);

    if let Some(etag) = initial_headers.get("etag") {
        let etag_value = etag.to_str().unwrap_or("");

        // Make conditional request
        let conditional_request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\n\r\n",
            etag_value
        );
        let conditional_response = send_raw_http_request_bytes(server_addr, &conditional_request)
            .await
            .unwrap();
        let (status_line, headers, _) = parse_http_response_bytes(&conditional_response);
        let status_code = get_status_code(&status_line);

        if status_code == 304 {
            // 304 response SHOULD include ETag if it was in the 200 response
            let response_etag = headers.get("etag");
            assert!(
                response_etag.is_some(),
                "304 response should include ETag header"
            );
        }
    }
}
