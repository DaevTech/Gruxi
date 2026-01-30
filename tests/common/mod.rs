//! Common test utilities for Gruxi HTTP integration tests.
//!
//! This module provides shared helper functions for sending raw HTTP requests
//! and parsing responses, avoiding code duplication across test files.

#![allow(dead_code)]

use hyper::HeaderMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};

// Test server configuration
pub const GRUXI_HTTP_HOST: &str = "127.0.0.1";
pub const GRUXI_HTTP_PORT: u16 = 80;
pub const TEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Get the HTTP server address for testing
pub fn get_http_server_addr() -> SocketAddr {
    SocketAddr::new(GRUXI_HTTP_HOST.parse().unwrap(), GRUXI_HTTP_PORT)
}

/// Send raw HTTP request and get raw response as a String.
pub async fn send_raw_http_request(
    addr: SocketAddr,
    request: &str,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let response_bytes = send_raw_http_request_bytes(addr, request).await?;
    Ok(String::from_utf8_lossy(&response_bytes).into_owned())
}

/// Send raw HTTP request and get raw response bytes.
///
/// This avoids UTF-8 assumptions and preserves the exact body bytes, which is
/// required for meaningful Content-Length comparisons.
pub async fn send_raw_http_request_bytes(
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
    match timeout(Duration::from_millis(500), stream.read_to_end(&mut response)).await {
        Ok(Ok(_)) => Ok(response),
        Ok(Err(e)) => Err(e.into()),
        Err(_) => {
            // Timeout - return what we have
            Ok(response)
        }
    }
}

/// Parse HTTP response string into components (status line, headers, body).
pub fn parse_http_response(response: &str) -> (String, HeaderMap, String) {
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
    let status_line = header_lines
        .next()
        .unwrap_or("")
        .trim_end_matches('\r')
        .to_string();

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

/// Parse HTTP response bytes into components (status line, headers, body).
///
/// Note: This preserves the body bytes verbatim (no newline normalization).
pub fn parse_http_response_bytes(response: &[u8]) -> (String, HeaderMap, Vec<u8>) {
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

        let name = String::from_utf8_lossy(name_bytes)
            .trim()
            .to_ascii_lowercase();
        let value = String::from_utf8_lossy(value_bytes).trim().to_string();

        if let Ok(header_name) = hyper::header::HeaderName::from_bytes(name.as_bytes()) {
            if let Ok(header_value) = hyper::header::HeaderValue::from_bytes(value.as_bytes()) {
                headers.insert(header_name, header_value);
            }
        }
    }

    (status_line, headers, body)
}

/// Extract status code from an HTTP status line.
pub fn get_status_code(status_line: &str) -> u16 {
    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

/// Validate status line format: HTTP-Version SP Status-Code SP Reason-Phrase CRLF
pub fn validate_status_line(status_line: &str) -> bool {
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
