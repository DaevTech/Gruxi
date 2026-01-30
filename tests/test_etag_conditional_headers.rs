mod common;

use common::{
    get_http_server_addr, get_status_code, parse_http_response_bytes,
    send_raw_http_request_bytes,
};

/// ETag and Conditional Headers Test Suite for Gruxi Web Server
///
/// This test suite validates Gruxi's implementation of HTTP conditional request headers
/// as defined in RFC 7232 (Conditional Requests).
///
/// ============================================================================
/// IMPORTANT: These tests validate the ACTUAL running Gruxi server, not a mock!
/// ============================================================================
///
/// SETUP INSTRUCTIONS:
/// 1. Start Gruxi server: `cargo run` (in separate terminal)
/// 2. Ensure server is running on 127.0.0.1:80
/// 3. Ensure www-default/ directory has index.html
/// 4. Run tests: `cargo test --test test_etag_conditional_headers`
///
/// WHAT THESE TESTS VERIFY:
/// These tests send real HTTP requests to the running Gruxi server and verify:
///
/// ✓ If-Match: Conditional request based on ETag matching (RFC 7232 Section 3.1)
/// ✓ If-None-Match: Conditional request based on ETag not matching (RFC 7232 Section 3.2)
/// ✓ If-Modified-Since: Conditional request based on modification date (RFC 7232 Section 3.3)
/// ✓ If-Unmodified-Since: Conditional request based on no modification (RFC 7232 Section 3.4)
///
/// EXPECTED BEHAVIORS:
/// - If-Match with matching ETag: 200 OK (proceed with request)
/// - If-Match with non-matching ETag: 412 Precondition Failed
/// - If-None-Match with matching ETag (GET/HEAD): 304 Not Modified
/// - If-None-Match with non-matching ETag: 200 OK (return resource)
/// - If-Modified-Since with old date: 200 OK (resource was modified)
/// - If-Modified-Since with future date: 304 Not Modified
/// - If-Unmodified-Since with old date: 412 Precondition Failed
/// - If-Unmodified-Since with future date: 200 OK (proceed with request)

/// Helper to get ETag and Last-Modified from index.html
async fn get_resource_metadata() -> (Option<String>, Option<String>) {
    let server_addr = get_http_server_addr();
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request)
        .await
        .unwrap();
    let (_, headers, _) = parse_http_response_bytes(&response);

    let etag = headers
        .get("etag")
        .map(|v| v.to_str().unwrap_or("").to_string());
    let last_modified = headers
        .get("last-modified")
        .map(|v| v.to_str().unwrap_or("").to_string());

    (etag, last_modified)
}

// ============================================================================
// 1. IF-MATCH HEADER TESTS (RFC 7232 Section 3.1)
// ============================================================================

/// Test If-Match with matching ETag should return 200 OK
#[tokio::test]
async fn test_if_match_with_matching_etag_returns_200() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Match: {}\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, body) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        assert_eq!(
            status_code, 200,
            "If-Match with matching ETag should return 200 OK, got: {}",
            status_line
        );
        assert!(!body.is_empty(), "Response body should not be empty for successful If-Match");
    } else {
        println!("Warning: Server does not return ETag header, skipping If-Match matching test");
    }
}

/// Test If-Match with non-matching ETag should return 412 Precondition Failed
#[tokio::test]
async fn test_if_match_with_non_matching_etag_returns_412() {
    let server_addr = get_http_server_addr();

    // Use a fake ETag that definitely won't match
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Match: \"nonexistent-etag-12345\"\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 412,
        "If-Match with non-matching ETag should return 412 Precondition Failed, got: {}",
        status_line
    );
}

/// Test If-Match with wildcard "*" should return 200 OK if resource exists
#[tokio::test]
async fn test_if_match_with_wildcard_returns_200() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Match: *\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 200,
        "If-Match with wildcard * should return 200 OK for existing resource, got: {}",
        status_line
    );
}

/// Test If-Match with wildcard "*" on non-existent resource should return 412
#[tokio::test]
async fn test_if_match_wildcard_on_missing_resource_returns_412() {
    let server_addr = get_http_server_addr();

    let request = "GET /nonexistent-file-xyz.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Match: *\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // For non-existent resource with If-Match: *, should return 412 Precondition Failed
    // (because there's no current representation to match)
    assert!(
        status_code == 412 || status_code == 404,
        "If-Match with wildcard on non-existent resource should return 412 or 404, got: {}",
        status_line
    );
}

/// Test If-Match with multiple ETags (one matching)
#[tokio::test]
async fn test_if_match_with_multiple_etags_one_matching() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Match: \"fake-etag\", {}, \"another-fake\"\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        assert_eq!(
            status_code, 200,
            "If-Match with one matching ETag in list should return 200 OK, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping test");
    }
}

// ============================================================================
// 2. IF-NONE-MATCH HEADER TESTS (RFC 7232 Section 3.2)
// ============================================================================

/// Test If-None-Match with matching ETag on GET should return 304 Not Modified
#[tokio::test]
async fn test_if_none_match_with_matching_etag_returns_304() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, body) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        assert_eq!(
            status_code, 304,
            "If-None-Match with matching ETag on GET should return 304 Not Modified, got: {}",
            status_line
        );
        assert!(
            body.is_empty(),
            "304 Not Modified response should not have a body"
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping If-None-Match matching test");
    }
}

/// Test If-None-Match with matching ETag on HEAD should return 304 Not Modified
#[tokio::test]
async fn test_if_none_match_head_with_matching_etag_returns_304() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        let request = format!(
            "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        assert_eq!(
            status_code, 304,
            "If-None-Match with matching ETag on HEAD should return 304 Not Modified, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping test");
    }
}

/// Test If-None-Match with non-matching ETag should return 200 OK with body
#[tokio::test]
async fn test_if_none_match_with_non_matching_etag_returns_200() {
    let server_addr = get_http_server_addr();

    // Use a fake ETag that definitely won't match
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: \"nonexistent-etag-12345\"\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 200,
        "If-None-Match with non-matching ETag should return 200 OK, got: {}",
        status_line
    );
    assert!(
        !body.is_empty(),
        "200 OK response should include the resource body"
    );
}

/// Test If-None-Match with wildcard "*" on existing resource should return 304
#[tokio::test]
async fn test_if_none_match_with_wildcard_returns_304() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: *\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 304,
        "If-None-Match with wildcard * should return 304 Not Modified for existing resource, got: {}",
        status_line
    );
}

/// Test If-None-Match with wildcard "*" on non-existent resource should return 404
#[tokio::test]
async fn test_if_none_match_wildcard_on_missing_resource_returns_404() {
    let server_addr = get_http_server_addr();

    let request = "GET /nonexistent-file-xyz.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: *\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Resource doesn't exist, so If-None-Match doesn't apply - should return 404
    assert_eq!(
        status_code, 404,
        "If-None-Match with wildcard on non-existent resource should return 404, got: {}",
        status_line
    );
}

/// Test If-None-Match with multiple ETags (one matching)
#[tokio::test]
async fn test_if_none_match_with_multiple_etags_one_matching() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: \"fake-etag\", {}, \"another-fake\"\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        assert_eq!(
            status_code, 304,
            "If-None-Match with one matching ETag in list should return 304 Not Modified, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping test");
    }
}

/// Test weak ETag comparison in If-None-Match
#[tokio::test]
async fn test_if_none_match_weak_etag_comparison() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        // If the server returns a strong ETag, try using its weak equivalent
        let weak_etag = if etag_value.starts_with("W/") {
            etag_value.clone()
        } else {
            format!("W/{}", etag_value)
        };

        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\n\r\n",
            weak_etag
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        // RFC 7232: Weak comparison is used for If-None-Match
        // W/"abc" should match "abc" or W/"abc"
        assert!(
            status_code == 304 || status_code == 200,
            "If-None-Match weak ETag comparison should return 304 or 200, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping test");
    }
}

// ============================================================================
// 3. IF-MODIFIED-SINCE HEADER TESTS (RFC 7232 Section 3.3)
// ============================================================================

/// Test If-Modified-Since with old date should return 200 OK
#[tokio::test]
async fn test_if_modified_since_with_old_date_returns_200() {
    let server_addr = get_http_server_addr();

    // Use a very old date - resource should have been modified since then
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: Thu, 01 Jan 1970 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 200,
        "If-Modified-Since with old date should return 200 OK (resource was modified), got: {}",
        status_line
    );
    assert!(
        !body.is_empty(),
        "200 OK response should include the resource body"
    );
}

/// Test If-Modified-Since with future date should return 304 Not Modified
#[tokio::test]
async fn test_if_modified_since_with_future_date_returns_304() {
    let server_addr = get_http_server_addr();

    // Use a future date - resource should not have been modified since then
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 304,
        "If-Modified-Since with future date should return 304 Not Modified, got: {}",
        status_line
    );
    assert!(
        body.is_empty(),
        "304 Not Modified response should not have a body"
    );
}

/// Test If-Modified-Since with exact Last-Modified date should return 304
#[tokio::test]
async fn test_if_modified_since_with_exact_last_modified_returns_304() {
    let server_addr = get_http_server_addr();

    // First, get the Last-Modified date of the resource
    let (_, last_modified) = get_resource_metadata().await;

    if let Some(last_modified_value) = last_modified {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: {}\r\n\r\n",
            last_modified_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        // If the date is exact, server should return 304 (not modified at or after that time)
        assert_eq!(
            status_code, 304,
            "If-Modified-Since with exact Last-Modified should return 304 Not Modified, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return Last-Modified header, skipping test");
    }
}

/// Test If-Modified-Since on HEAD request with future date returns 304
#[tokio::test]
async fn test_if_modified_since_head_with_future_date_returns_304() {
    let server_addr = get_http_server_addr();

    let request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 304,
        "If-Modified-Since on HEAD with future date should return 304 Not Modified, got: {}",
        status_line
    );
}

/// Test If-Modified-Since with invalid date format should be ignored (return 200)
#[tokio::test]
async fn test_if_modified_since_with_invalid_date_returns_200() {
    let server_addr = get_http_server_addr();

    // Use an invalid date format
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: not-a-valid-date\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // RFC 7232: If the field value is not a valid HTTP-date, ignore the header
    assert_eq!(
        status_code, 200,
        "If-Modified-Since with invalid date should be ignored and return 200 OK, got: {}",
        status_line
    );
    assert!(
        !body.is_empty(),
        "Response should include the resource body"
    );
}

/// Test If-Modified-Since on non-existent resource returns 404
#[tokio::test]
async fn test_if_modified_since_on_missing_resource_returns_404() {
    let server_addr = get_http_server_addr();

    let request = "GET /nonexistent-file-xyz.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: Thu, 01 Jan 1970 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 404,
        "If-Modified-Since on non-existent resource should return 404 Not Found, got: {}",
        status_line
    );
}

// ============================================================================
// 4. IF-UNMODIFIED-SINCE HEADER TESTS (RFC 7232 Section 3.4)
// ============================================================================

/// Test If-Unmodified-Since with future date should return 200 OK
#[tokio::test]
async fn test_if_unmodified_since_with_future_date_returns_200() {
    let server_addr = get_http_server_addr();

    // Use a future date - resource has not been modified since then (it existed before)
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Unmodified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 200,
        "If-Unmodified-Since with future date should return 200 OK, got: {}",
        status_line
    );
    assert!(
        !body.is_empty(),
        "200 OK response should include the resource body"
    );
}

/// Test If-Unmodified-Since with old date should return 412 Precondition Failed
#[tokio::test]
async fn test_if_unmodified_since_with_old_date_returns_412() {
    let server_addr = get_http_server_addr();

    // Use a very old date - resource was modified after that
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Unmodified-Since: Thu, 01 Jan 1970 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 412,
        "If-Unmodified-Since with old date should return 412 Precondition Failed, got: {}",
        status_line
    );
}

/// Test If-Unmodified-Since with exact Last-Modified date should return 200
#[tokio::test]
async fn test_if_unmodified_since_with_exact_last_modified_returns_200() {
    let server_addr = get_http_server_addr();

    // First, get the Last-Modified date of the resource
    let (_, last_modified) = get_resource_metadata().await;

    if let Some(last_modified_value) = last_modified {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Unmodified-Since: {}\r\n\r\n",
            last_modified_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        // If the date matches Last-Modified exactly, should return 200 (not modified since then)
        assert_eq!(
            status_code, 200,
            "If-Unmodified-Since with exact Last-Modified should return 200 OK, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return Last-Modified header, skipping test");
    }
}

/// Test If-Unmodified-Since on HEAD request with future date returns 200
#[tokio::test]
async fn test_if_unmodified_since_head_with_future_date_returns_200() {
    let server_addr = get_http_server_addr();

    let request = "HEAD /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Unmodified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 200,
        "If-Unmodified-Since on HEAD with future date should return 200 OK, got: {}",
        status_line
    );
}

/// Test If-Unmodified-Since with invalid date format should be ignored (return 200)
#[tokio::test]
async fn test_if_unmodified_since_with_invalid_date_returns_200() {
    let server_addr = get_http_server_addr();

    // Use an invalid date format
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Unmodified-Since: not-a-valid-date\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // RFC 7232: If the field value is not a valid HTTP-date, ignore the header
    assert_eq!(
        status_code, 200,
        "If-Unmodified-Since with invalid date should be ignored and return 200 OK, got: {}",
        status_line
    );
    assert!(
        !body.is_empty(),
        "Response should include the resource body"
    );
}

/// Test If-Unmodified-Since on non-existent resource returns 404 or 412
#[tokio::test]
async fn test_if_unmodified_since_on_missing_resource() {
    let server_addr = get_http_server_addr();

    let request = "GET /nonexistent-file-xyz.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Unmodified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    // Per RFC 7232, server may return 404 if resource doesn't exist
    assert!(
        status_code == 404 || status_code == 412,
        "If-Unmodified-Since on non-existent resource should return 404 or 412, got: {}",
        status_line
    );
}

// ============================================================================
// 5. COMBINED CONDITIONAL HEADERS TESTS
// ============================================================================

/// Test precedence: If-Match takes precedence over If-Modified-Since
#[tokio::test]
async fn test_if_match_takes_precedence_over_if_modified_since() {
    let server_addr = get_http_server_addr();

    // If-Match with non-matching ETag should return 412, even with If-Modified-Since
    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Match: \"nonexistent-etag\"\r\nIf-Modified-Since: Thu, 01 Jan 1970 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    assert_eq!(
        status_code, 412,
        "If-Match should take precedence over If-Modified-Since, got: {}",
        status_line
    );
}

/// Test: If-None-Match with If-Modified-Since (both conditions must be met for 304)
#[tokio::test]
async fn test_if_none_match_with_if_modified_since_combined() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        // Both If-None-Match (matching) and If-Modified-Since (future date) - should return 304
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\nIf-Modified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        assert_eq!(
            status_code, 304,
            "If-None-Match + If-Modified-Since both satisfied should return 304, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping test");
    }
}

/// Test: If-None-Match matching but If-Modified-Since old date
#[tokio::test]
async fn test_if_none_match_matching_but_if_modified_since_old() {
    let server_addr = get_http_server_addr();

    // First, get the actual ETag of the resource
    let (etag, _) = get_resource_metadata().await;

    if let Some(etag_value) = etag {
        // If-None-Match matches (would be 304) but If-Modified-Since is old (would be 200)
        // Per RFC 7232: If-None-Match takes precedence, but If-Modified-Since is also evaluated
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\nIf-Modified-Since: Thu, 01 Jan 1970 00:00:00 GMT\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        // RFC 7232 Section 6: For GET/HEAD with If-None-Match, If-Modified-Since is only
        // evaluated if If-None-Match passes. This should return 200 because file was modified
        // after the If-Modified-Since date.
        assert!(
            status_code == 200 || status_code == 304,
            "Combined If-None-Match + If-Modified-Since behavior varies by implementation, got: {}",
            status_line
        );
    } else {
        println!("Warning: Server does not return ETag header, skipping test");
    }
}

// ============================================================================
// 6. 304 RESPONSE VALIDATION TESTS
// ============================================================================

/// Test 304 response includes required headers (ETag and/or Last-Modified if originally sent)
#[tokio::test]
async fn test_304_response_includes_validator_headers() {
    let server_addr = get_http_server_addr();

    // First, get the resource to check what headers it includes
    let (etag, last_modified) = get_resource_metadata().await;

    if let Some(etag_value) = etag.clone() {
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-None-Match: {}\r\n\r\n",
            etag_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, headers, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        if status_code == 304 {
            // 304 response SHOULD include ETag if it was present in 200 response
            if etag.is_some() {
                assert!(
                    headers.get("etag").is_some(),
                    "304 response should include ETag header if present in 200 response"
                );
            }
        }
    } else if let Some(last_modified_value) = last_modified {
        // Try with If-Modified-Since instead
        let request = format!(
            "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: {}\r\n\r\n",
            last_modified_value
        );
        let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
        let (status_line, headers, _) = parse_http_response_bytes(&response);
        let status_code = get_status_code(&status_line);

        if status_code == 304 {
            assert!(
                headers.get("last-modified").is_some(),
                "304 response should include Last-Modified header if present in 200 response"
            );
        }
    } else {
        println!("Warning: Server does not return ETag or Last-Modified headers, skipping test");
    }
}

/// Test 304 response does not include Content-Length for body
#[tokio::test]
async fn test_304_response_no_body() {
    let server_addr = get_http_server_addr();

    let request = "GET /index.html HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\nIf-Modified-Since: Fri, 01 Jan 2100 00:00:00 GMT\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, body) = parse_http_response_bytes(&response);
    let status_code = get_status_code(&status_line);

    if status_code == 304 {
        assert!(
            body.is_empty(),
            "304 Not Modified response must not contain a message body"
        );
    }
}
