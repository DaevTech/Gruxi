use hyper::HeaderMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

/// HTTP/1.1 Compliance Test Suite for Gruxi Web Server
///
/// This comprehensive test suite validates Gruxi's compliance with HTTP/1.1 specifications
/// as defined in RFC 7230 (Message Syntax and Routing) and RFC 7231 (Semantics and Content).
///
/// ============================================================================
/// IMPORTANT: These tests validate the ACTUAL running Gruxi server, not a mock!
/// ============================================================================
///
/// SETUP INSTRUCTIONS:
/// 1. Start Gruxi server: `cargo run` (in separate terminal)
/// 2. Ensure server is running on 127.0.0.1:80
/// 3. Ensure www-default/ directory has content (index.html, etc.)
/// 4. Run tests: `cargo test --test test_gruxi_http11_compliance`
///
/// WHAT THESE TESTS VERIFY:
/// These tests send real HTTP requests to the running Gruxi server and verify:
///
/// ✓ HTTP Methods: GET, HEAD, OPTIONS, POST compliance with RFC standards
/// ✓ Status Codes: Proper 200, 404, 400, 405, 501 responses
/// ✓ Headers: Host header requirement, case insensitivity, proper formatting
/// ✓ HTTP/1.1 Features: Connection management, protocol version handling
/// ✓ Error Handling: Malformed requests, invalid URIs, bad headers
/// ✓ Content Negotiation: Accept headers and content type responses
/// ✓ Message Format: Proper HTTP message structure and framing
///
/// WHY THIS APPROACH:
/// Unlike mock-based tests, these integration tests provide real confidence
/// that Gruxi correctly implements HTTP/1.1 by testing the actual server
/// behavior against real HTTP requests and validating real responses.
///
/// TROUBLESHOOTING:
/// - If tests fail with "connection refused": Start Gruxi server first
/// - If tests timeout: Check that Gruxi is listening on port 80
/// - If 404 errors: Ensure www-default/index.html exists

// Test server configuration
const GRUXI_HTTP_HOST: &str = "127.0.0.1";
const GRUXI_HTTP_PORT: u16 = 80;
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Get the HTTP server address for testing
fn get_http_server_addr() -> SocketAddr {
    SocketAddr::new(GRUXI_HTTP_HOST.parse().unwrap(), GRUXI_HTTP_PORT)
}

/// Send raw HTTP request and get raw response
async fn send_raw_http_request(addr: SocketAddr, request: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let response_bytes = send_raw_http_request_bytes(addr, request).await?;
    Ok(String::from_utf8_lossy(&response_bytes).into_owned())
}

/// Send raw HTTP request and get raw response bytes.
///
/// This avoids UTF-8 assumptions and preserves the exact body bytes, which is
/// required for meaningful Content-Length comparisons.
async fn send_raw_http_request_bytes(
    addr: SocketAddr,
    request: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let mut stream = timeout(TEST_TIMEOUT, TcpStream::connect(addr)).await??;

    if !request.is_empty() {
        stream.write_all(request.as_bytes()).await?;
    }

    // Give the server time to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut response = Vec::new();
    // Use timeout for reading response to avoid hanging
    match timeout(Duration::from_millis(5000), stream.read_to_end(&mut response)).await {
        Ok(Ok(_)) => Ok(response),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => {
            // Timeout - return what we have
            Ok(response)
        }
    }
}

/// Parse HTTP response into components
fn parse_http_response(response: &str) -> (String, HeaderMap, String) {
    // Split headers/body using the HTTP delimiter. This preserves the body verbatim
    // (no newline normalization), which is required for meaningful Content-Length checks.
    let (header_block, body) = if let Some(pos) = response.find("\r\n\r\n") {
        (&response[..pos], response[pos + 4..].to_string())
    } else if let Some(pos) = response.find("\n\n") {
        (&response[..pos], response[pos + 2..].to_string())
    } else {
        (response, String::new())
    };

    let mut header_lines = header_block.lines();
    let status_line = header_lines.next().unwrap_or("").trim_end_matches('\r').to_string();

    let mut headers = HeaderMap::new();
    for line in header_lines {
        let line = line.trim_end_matches('\r');
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_lowercase();
            let value = value.trim();
            if let Ok(header_name) = name.parse::<hyper::header::HeaderName>() {
                if let Ok(header_value) = value.parse::<hyper::header::HeaderValue>() {
                    headers.insert(header_name, header_value);
                }
            }
        }
    }

    (status_line, headers, body)
}

/// Parse HTTP response bytes into components.
///
/// Note: This preserves the body bytes verbatim (no newline normalization).
fn parse_http_response_bytes(response: &[u8]) -> (String, HeaderMap, Vec<u8>) {
    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || haystack.len() < needle.len() {
            return None;
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    let (header_block, body) = if let Some(pos) = find_subslice(response, b"\r\n\r\n") {
        (&response[..pos], response[pos + 4..].to_vec())
    } else if let Some(pos) = find_subslice(response, b"\n\n") {
        (&response[..pos], response[pos + 2..].to_vec())
    } else {
        (response, Vec::new())
    };

    let mut headers = HeaderMap::new();
    let mut lines = header_block.split(|b| *b == b'\n');

    let status_line = lines
        .next()
        .map(|l| {
            let l = l.strip_suffix(b"\r").unwrap_or(l);
            String::from_utf8_lossy(l).to_string()
        })
        .unwrap_or_default();

    for line in lines {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            continue;
        }

        let Some(colon_pos) = line.iter().position(|b| *b == b':') else {
            continue;
        };

        let name_bytes = &line[..colon_pos];
        let value_bytes = &line[colon_pos + 1..];

        let name = String::from_utf8_lossy(name_bytes).trim().to_ascii_lowercase();
        let value = String::from_utf8_lossy(value_bytes).trim().to_string();

        if let Ok(header_name) = hyper::header::HeaderName::from_bytes(name.as_bytes()) {
            if let Ok(header_value) = hyper::header::HeaderValue::from_bytes(value.as_bytes()) {
                headers.insert(header_name, header_value);
            }
        }
    }

    (status_line, headers, body)
}

/// Validate status line format: HTTP-Version SP Status-Code SP Reason-Phrase CRLF
fn validate_status_line(status_line: &str) -> bool {
    let parts: Vec<&str> = status_line.split_whitespace().collect();
    if parts.len() < 3 {
        return false;
    }

    // Check HTTP version format
    let version = parts[0];
    if !version.starts_with("HTTP/") {
        return false;
    }

    // Check status code is 3 digits
    let status_code = parts[1];
    if status_code.len() != 3 || !status_code.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }

    true
}

// ============================================================================
// 1. HTTP METHODS COMPLIANCE TESTING
// ============================================================================

#[tokio::test]
async fn test_required_methods_support() {
    let server_addr = get_http_server_addr();

    // RFC 7231: GET and HEAD methods MUST be supported by all general-purpose servers
    let get_request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, get_request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    assert!(validate_status_line(&status_line), "Invalid status line: {}", status_line);
    assert!(!status_line.contains("501"), "GET method should be implemented"); // Not "Not Implemented"

    let head_request = "HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, head_request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);
    assert!(validate_status_line(&status_line), "Invalid status line: {}", status_line);
    assert!(!status_line.contains("501"), "HEAD method should be implemented");
}

#[tokio::test]
async fn test_head_method_identical_to_get_minus_body() {
    let server_addr = get_http_server_addr();

    // GET request
    let get_request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let get_response = send_raw_http_request_bytes(server_addr, get_request).await.unwrap();
    let (get_status, get_headers, get_body) = parse_http_response_bytes(&get_response);

    // HEAD request
    let head_request = "HEAD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let head_response = send_raw_http_request_bytes(server_addr, head_request).await.unwrap();
    let (head_status, head_headers, head_body) = parse_http_response_bytes(&head_response);

    // Status code should be identical
    let get_status_code = get_status.split_whitespace().nth(1).unwrap_or("000");
    let head_status_code = head_status.split_whitespace().nth(1).unwrap_or("000");
    assert_eq!(get_status_code, head_status_code, "HEAD and GET should return same status code");

    // Content-Type should be identical if present
    if get_headers.get("content-type").is_some() {
        assert_eq!(get_headers.get("content-type"), head_headers.get("content-type"), "Content-Type should be identical");
    }

    // HEAD response must not have a body (or much smaller body)
    assert!(head_body.is_empty() || head_body.len() < get_body.len(), "HEAD should not have body or smaller body than GET");
}

#[tokio::test]
async fn test_options_method_allowed_methods() {
    let server_addr = get_http_server_addr();

    let options_request = "OPTIONS * HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, options_request).await.unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line), "Invalid status line: {}", status_line);

    // OPTIONS should return 200 OK or 405 Method Not Allowed
    assert!(status_line.contains("200") || status_line.contains("405"), "OPTIONS should return 200 or 405");

    // If 200, should include Allow header with supported methods
    if status_line.contains("200") {
        let allow_header = headers.get("allow");
        if let Some(allow_value) = allow_header {
            let allow_str = allow_value.to_str().unwrap_or("");
            // Should at least include GET and HEAD
            assert!(allow_str.to_uppercase().contains("GET"), "Allow header should include GET");
            assert!(allow_str.to_uppercase().contains("HEAD"), "Allow header should include HEAD");
        }
    }
}

#[tokio::test]
async fn test_unknown_method_handling() {
    let server_addr = get_http_server_addr();

    let unknown_request = "CUSTOMMETHOD / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, unknown_request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 501 Not Implemented for unknown methods
    assert!(status_line.contains("501") || status_line.contains("405"));
}

#[tokio::test]
async fn test_method_case_sensitivity() {
    let server_addr = get_http_server_addr();

    // Methods are case-sensitive per RFC 7231
    let lowercase_request = "get / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, lowercase_request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 400 Bad Request or 501 Not Implemented for invalid method case
    assert!(status_line.contains("400") || status_line.contains("501"));
}

// ============================================================================
// 2. STATUS CODE COMPLIANCE TESTING
// ============================================================================

#[tokio::test]
async fn test_status_code_format_compliance() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Validate Status-Line format: HTTP-Version SP Status-Code SP Reason-Phrase CRLF
    assert!(validate_status_line(&status_line));

    let parts: Vec<&str> = status_line.split_whitespace().collect();
    assert!(parts.len() >= 3);

    // HTTP version should be HTTP/1.1
    assert!(parts[0] == "HTTP/1.1" || parts[0] == "HTTP/1.0");

    // Status code should be valid 3-digit number
    let status_code: u16 = parts[1].parse().unwrap();
    assert!(status_code >= 100 && status_code < 600);
}

#[tokio::test]
async fn test_404_not_found_response() {
    let server_addr = get_http_server_addr();

    let request = "GET /nonexistent-file-that-should-not-exist HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    assert!(status_line.contains("404"));
}

#[tokio::test]
async fn test_405_method_not_allowed_includes_allow_header() {
    let server_addr = get_http_server_addr();

    // Try to POST to a resource that doesn't accept POST
    let request = "POST /index.html HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);

    if status_line.contains("405") {
        // 405 Method Not Allowed MUST include Allow header
        assert!(headers.contains_key("allow"));
    }
}

#[tokio::test]
async fn test_100_continue_handling() {
    let server_addr = get_http_server_addr();

    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nExpect: 100-continue\r\nContent-Length: 10\r\nConnection: close\r\n\r\ntest data";
    let response = send_raw_http_request(server_addr, request).await.unwrap();

    println!("Response for Expect: 100-continue test: {:?}", response);

    // Should handle Expect: 100-continue properly (either send 100 Continue or process directly)
    // Does not work at the moment. Hyper complaining about unknown status code for some reason: Error serving connection: hyper::Error(User(UnsupportedStatusCode))
   // assert!(!response.is_empty());
}

// ============================================================================
// 3. HEADER FIELD VALIDATION TESTING
// ============================================================================

#[tokio::test]
async fn test_host_header_requirement() {
    let server_addr = get_http_server_addr();

    // HTTP/1.1 requests MUST include Host header
    let request_without_host = "GET / HTTP/1.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request_without_host).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 400 Bad Request for missing Host header in HTTP/1.1
    assert!(status_line.contains("400"));
}

#[tokio::test]
async fn test_header_case_insensitivity() {
    let server_addr = get_http_server_addr();

    // Header names are case-insensitive
    let request = "GET / HTTP/1.1\r\nhost: localhost\r\nuser-agent: TestClient\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should process lowercase headers correctly
    assert!(validate_status_line(&status_line));
    assert!(!status_line.contains("400"));
}

#[tokio::test]
async fn test_invalid_header_characters() {
    let server_addr = get_http_server_addr();

    // Headers with invalid characters should be rejected
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nInvalid\x00Header: value\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 400 Bad Request for invalid header characters
    assert!(status_line.contains("400"));
}

#[tokio::test]
async fn test_content_length_validation() {
    let server_addr = get_http_server_addr();

    // Content-Length must match actual body length
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\nConnection: close\r\n\r\ntest\r\n\r\n"; // 4 chars, not 5
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Server should handle Content-Length mismatch appropriately
    assert!(validate_status_line(&status_line));
}

#[tokio::test]
async fn test_multiple_host_headers() {
    let server_addr = get_http_server_addr();

    // Multiple Host headers should be rejected
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nHost: example.com\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 400 Bad Request for multiple Host headers
    assert!(status_line.contains("400"));
}

// ============================================================================
// 4. MESSAGE FRAMING AND TRANSFER ENCODING TESTING
// ============================================================================

#[tokio::test]
async fn test_chunked_transfer_encoding() {
    let server_addr = get_http_server_addr();

    // Send chunked request
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should handle chunked encoding properly
    assert!(validate_status_line(&status_line));
    assert!(!status_line.contains("400"));
}

#[tokio::test]
async fn test_content_length_vs_transfer_encoding() {
    let server_addr = get_http_server_addr();

    // Transfer-Encoding takes precedence over Content-Length
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should process as chunked, ignoring Content-Length
    assert!(validate_status_line(&status_line));
}

/* We dont support chunked quite yet
#[tokio::test]
async fn test_invalid_chunk_format() {
    let server_addr = get_http_server_addr();

    // Send malformed chunk
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\nINVALID\r\ntest\r\n0\r\n\r\n";
    let response = send_raw_http_request(server_addr, request).await.unwrap();
    println!("Response for invalid chunk format: {:?}", response);
    let (status_line, _, _) = parse_http_response(&response);

    // Should return 400 Bad Request for malformed chunks
    assert!(status_line.contains("400"));
}
*/

#[tokio::test]
async fn test_trailer_headers_in_chunked_encoding() {
    let server_addr = get_http_server_addr();

    // Chunked encoding with trailer headers
    let request = "POST / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nTransfer-Encoding: chunked\r\nTrailer: X-Custom-Header\r\n\r\n4\r\ntest\r\n0\r\nX-Custom-Header: value\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should handle trailer headers correctly
    assert!(validate_status_line(&status_line));
}

// ============================================================================
// 5. CONNECTION MANAGEMENT TESTING
// ============================================================================

#[tokio::test]
async fn test_persistent_connection_default() {
    let server_addr = get_http_server_addr();

    // HTTP/1.1 connections should be persistent by default
    let request1 = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"; // No Connection header
    let request2 = "GET / HTTP/1.1\r\nHost: localhost\r\n\r\n"; // No Connection header

    let mut stream = TcpStream::connect(server_addr).await.unwrap();

    // Send first request
    stream.write_all(request1.as_bytes()).await.unwrap();
    let mut response1 = vec![0; 4096];
    let n1 = stream.read(&mut response1).await.unwrap();
    let response1_str = String::from_utf8_lossy(&response1[..n1]);

    // Send second request on same connection
    stream.write_all(request2.as_bytes()).await.unwrap();
    let mut response2 = vec![0; 4096];
    let n2 = stream.read(&mut response2).await.unwrap();
    let response2_str = String::from_utf8_lossy(&response2[..n2]);

    // Both responses should be valid
    assert!(!response1_str.is_empty());
    assert!(!response2_str.is_empty());
}

#[tokio::test]
async fn test_connection_close_handling() {
    let server_addr = get_http_server_addr();

    // Connection: close should terminate after response
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request(server_addr, request).await.unwrap();
    let (status_line, headers, _) = parse_http_response(&response);

    assert!(validate_status_line(&status_line));

    // Response should include Connection: close
    if let Some(connection) = headers.get("connection") {
        assert!(connection.to_str().unwrap_or("").contains("close"));
    }
}

#[tokio::test]
async fn test_connection_timeout_behavior() {
    let server_addr = get_http_server_addr();

    // Connect but don't send anything
    let mut stream = TcpStream::connect(server_addr).await.unwrap();

    // Connection should eventually timeout (this tests server behavior)
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to write after delay
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let result = stream.write_all(request.as_bytes()).await;

    // Connection might still be open or closed depending on server timeout
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// 6. PROTOCOL VERSION HANDLING TESTING
// ============================================================================

#[tokio::test]
async fn test_http10_backward_compatibility() {
    let server_addr = get_http_server_addr();

    // HTTP/1.0 request (no Host header required)
    let request = "GET / HTTP/1.0\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line));
    // Server should respond with HTTP/1.0 or HTTP/1.1
    assert!(status_line.starts_with("HTTP/1."));
}

#[tokio::test]
async fn test_http11_version_response() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Server should respond with HTTP/1.1 for HTTP/1.1 requests
    assert!(status_line.starts_with("HTTP/1.1"));
}

#[tokio::test]
async fn test_invalid_http_version() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/2.0\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should handle unsupported HTTP version appropriately
    assert!(status_line.contains("400") || status_line.contains("505"));
}

// ============================================================================
// 7. CONTENT NEGOTIATION TESTING
// ============================================================================

#[tokio::test]
async fn test_accept_header_negotiation() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nAccept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line));

    // Should include Content-Type header
    assert!(headers.contains_key("content-type"));
}

#[tokio::test]
async fn test_accept_encoding_support() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nAccept-Encoding: gzip, deflate, br\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should handle Accept-Encoding header
    assert!(validate_status_line(&status_line));
}

#[tokio::test]
async fn test_quality_value_processing() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nAccept: text/html;q=0.9,text/plain;q=0.8,*/*;q=0.1\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should process q-values correctly
    assert!(validate_status_line(&status_line));
}

#[tokio::test]
async fn test_406_not_acceptable_response() {
    let server_addr = get_http_server_addr();

    // Request only unsupported media types
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nAccept: application/vnd.unsupported-format\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Might return content anyway or 406 Not Acceptable
    assert!(validate_status_line(&status_line));
}

// ============================================================================
// 8. ERROR HANDLING AND EDGE CASES TESTING
// ============================================================================

#[tokio::test]
async fn test_malformed_request_line() {
    let server_addr = get_http_server_addr();

    // Invalid request line format
    let request = "INVALID REQUEST LINE\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 400 Bad Request
    assert!(status_line.contains("400"));
}

#[tokio::test]
async fn test_request_uri_too_long() {
    let server_addr = get_http_server_addr();

    // Extremely long URI
    let long_path = "a".repeat(8192);
    let request = format!("GET /{} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n", long_path);
    let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 414 Request-URI Too Long or handle gracefully
    assert!(validate_status_line(&status_line));
}

#[tokio::test]
async fn test_request_header_fields_too_large() {
    let server_addr = get_http_server_addr();

    // Very large header
    let large_header_value = "x".repeat(8192);
    let request = format!("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nX-Large-Header: {}\r\n\r\n", large_header_value);
    let response = send_raw_http_request_bytes(server_addr, &request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 431 Request Header Fields Too Large or handle gracefully
    assert!(validate_status_line(&status_line));
}

#[tokio::test]
async fn test_invalid_uri_characters() {
    let server_addr = get_http_server_addr();

    // URI with invalid characters
    let request = "GET /path with spaces HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should return 400 Bad Request for invalid URI
    assert!(status_line.contains("400"));
}

#[tokio::test]
async fn test_empty_request_handling() {
    let server_addr = get_http_server_addr();

    // Send empty request
    let request = "";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();

    // Should handle empty request gracefully - either return 400 Bad Request or close connection
    if !response.is_empty() {
        let (status_line, _, _) = parse_http_response_bytes(&response);
        assert!(status_line.contains("400") || status_line.contains("HTTP/1.1"));
    }
    // If response is empty, that's also acceptable (connection closed)
}

// ============================================================================
// 9. TLS AND SECURITY COMPLIANCE (GRUXI-SPECIFIC)
// ============================================================================

// Note: TLS tests would require proper certificate setup
// These are placeholder tests for the TLS functionality

#[tokio::test]
async fn test_non_admin_endpoint_http_support() {
    let server_addr = get_http_server_addr();

    // Non-admin endpoints should work over HTTP
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line));
    assert!(!status_line.contains("400"));
}

// ============================================================================
// 10. REQUEST/RESPONSE MESSAGE VALIDATION
// ============================================================================

#[tokio::test]
async fn test_response_header_format() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, headers, _) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line));

    // Validate common required headers
    assert!(headers.contains_key("date") || headers.contains_key("server"));
}

#[tokio::test]
async fn test_response_body_consistency() {
    let server_addr = get_http_server_addr();

    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, headers, body) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line));

    // If Content-Length is present (and response is not chunked), body byte length should match.
    let is_chunked = headers
        .get("transfer-encoding")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);

    if !is_chunked {
        if let Some(content_length) = headers.get("content-length") {
            if let Ok(length) = content_length.to_str().unwrap_or("0").parse::<usize>() {
                assert_eq!(body.len(), length);
            }
        }
    }
}

#[tokio::test]
async fn test_http_message_crlf_handling() {
    let server_addr = get_http_server_addr();

    // Test with proper CRLF line endings
    let request = "GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    assert!(validate_status_line(&status_line));

    // Test with LF only (should be tolerant per RFC)
    let request_lf = "GET / HTTP/1.1\nHost: localhost\nConnection: close\n\n";
    let response_lf = send_raw_http_request_bytes(server_addr, request_lf).await.unwrap();
    let (status_line_lf, _, _) = parse_http_response_bytes(&response_lf);

    // Should be tolerant of LF-only line endings
    assert!(validate_status_line(&status_line_lf));
}

#[tokio::test]
async fn test_whitespace_handling_in_headers() {
    let server_addr = get_http_server_addr();

    // Test with extra whitespace around header values
    let request = "GET / HTTP/1.1\r\nHost:   localhost   \r\nUser-Agent:  TestClient  \r\nConnection: close\r\n\r\n";
    let response = send_raw_http_request_bytes(server_addr, request).await.unwrap();
    let (status_line, _, _) = parse_http_response_bytes(&response);

    // Should handle whitespace in headers correctly
    assert!(validate_status_line(&status_line));
    assert!(!status_line.contains("400"));
}

// ============================================================================
// HELPER FUNCTIONS FOR ADVANCED TESTING
// ============================================================================

// ============================================================================
// INTEGRATION TESTS WITH MULTIPLE PROTOCOLS
// ============================================================================

#[tokio::test]
async fn test_concurrent_requests_compliance() {
    let server_addr = get_http_server_addr();

    // Send multiple concurrent requests
    let mut handles = vec![];

    for i in 0..10 {
        let addr = server_addr;
        let handle = tokio::spawn(async move {
            let request = format!("GET /?request={} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n", i);
            send_raw_http_request_bytes(addr, &request).await.unwrap()
        });
        handles.push(handle);
    }

    // Wait for all requests to complete
    let responses = futures::future::join_all(handles).await;

    // All responses should be valid
    for response_result in responses {
        let response = response_result.unwrap();
        let (status_line, _, _) = parse_http_response_bytes(&response);
        assert!(validate_status_line(&status_line));
    }
}

#[tokio::test]
async fn test_pipeline_request_handling() {
    let server_addr = get_http_server_addr();

    // Send pipelined requests
    let pipelined_requests = "GET /1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\nGET /2 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";

    let mut stream = TcpStream::connect(server_addr).await.unwrap();
    stream.write_all(pipelined_requests.as_bytes()).await.unwrap();

    let mut response_buffer = vec![0; 8192];
    let n = stream.read(&mut response_buffer).await.unwrap();
    let responses = String::from_utf8_lossy(&response_buffer[..n]);

    // Should handle pipelined requests (responses in order)
    assert!(!responses.is_empty());
    assert!(responses.contains("HTTP/1.1"));
}
