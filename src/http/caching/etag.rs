use std::time::{SystemTime, UNIX_EPOCH};

use http::HeaderValue;

use crate::http::{http_util::empty_response_with_status, request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse}};

pub enum EtagStrength {
    Strong,
    Weak,
}

pub fn etag_strong_from_metadata(size: u64, last_modified: SystemTime) -> String {
    let mtime_ns = last_modified.duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();

    format!("\"{}-{}\"", size, mtime_ns)
}

// Handle conditional headers like If-Match, If-None-Match, If-Modified-Since, If-Unmodified-Since
// Returns true if response should be returned immediately (304 Not Modified or 412), false otherwise
pub fn handle_conditional_headers(grux_request: &GruxiRequest, grux_response: &mut GruxiResponse, etag: &str, last_modified: &std::time::SystemTime) -> bool {
    // Handle If-Match (RFC 7232 Section 3.1)
    if let Some(if_match_value) = grux_request.get_headers().get(hyper::header::IF_MATCH) {
        let if_match_str = if_match_value.to_str().unwrap_or("");
        // Wildcard "*" matches any existing resource
        if if_match_str.trim() == "*" {
            // Resource exists (we have an etag), so wildcard matches - continue processing
        } else {
            // Split by comma, if any commas are present
            let match_values = split_etag_by_comma(if_match_str);
            // Use strong comparison for If-Match
            if !match_values.iter().any(|v| etag_strong_compare(v, etag)) {
                // ETag does not match, return 412 Precondition Failed
                *grux_response = empty_response_with_status(hyper::StatusCode::PRECONDITION_FAILED);
                return true;
            }
        }
    }

    // Handle If-None-Match (RFC 7232 Section 3.2)
    if let Some(if_none_match_value) = grux_request.get_headers().get(hyper::header::IF_NONE_MATCH) {
        let if_none_match_str = if_none_match_value.to_str().unwrap_or("");
        // Wildcard "*" matches any existing resource
        let matches = if if_none_match_str.trim() == "*" {
            true // Resource exists, so wildcard matches
        } else {
            let match_values = split_etag_by_comma(if_none_match_str);
            // Use weak comparison for If-None-Match (RFC 7232 Section 3.2)
            match_values.iter().any(|v| etag_weak_compare(v, etag))
        };

        if matches {
            // ETag matches, return 304 Not Modified with validator headers
            let mut not_modified_response = empty_response_with_status(hyper::StatusCode::NOT_MODIFIED);
            // Include ETag in 304 response (RFC 7232 Section 4.1)
            if let Ok(etag_header) = HeaderValue::from_str(etag) {
                not_modified_response.headers_mut().insert(hyper::header::ETAG, etag_header);
            }
            // Include Last-Modified in 304 response
            let last_modified_str = httpdate::fmt_http_date(*last_modified);
            if let Ok(last_modified_header) = HeaderValue::from_str(&last_modified_str) {
                not_modified_response.headers_mut().insert(hyper::header::LAST_MODIFIED, last_modified_header);
            }
            *grux_response = not_modified_response;
            return true;
        }
    }

    // Handle If-Modified-Since (RFC 7232 Section 3.3)
    // Only applies to GET and HEAD requests (checked by caller context)
    if let Some(if_modified_since_value) = grux_request.get_headers().get(hyper::header::IF_MODIFIED_SINCE) {
        if let Ok(if_modified_since_str) = if_modified_since_value.to_str() {
            if let Ok(if_modified_since_time) = httpdate::parse_http_date(if_modified_since_str) {
                // HTTP dates have 1-second resolution, so we truncate to seconds for comparison
                // If last_modified <= if_modified_since, the resource has not been modified
                let last_modified_secs = last_modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let if_modified_since_secs = if_modified_since_time.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

                if last_modified_secs <= if_modified_since_secs {
                    // Not modified since, return 304 Not Modified with validator headers
                    let mut not_modified_response = empty_response_with_status(hyper::StatusCode::NOT_MODIFIED);
                    // Include ETag in 304 response
                    if let Ok(etag_header) = HeaderValue::from_str(etag) {
                        not_modified_response.headers_mut().insert(hyper::header::ETAG, etag_header);
                    }
                    // Include Last-Modified in 304 response
                    let last_modified_str = httpdate::fmt_http_date(*last_modified);
                    if let Ok(last_modified_header) = HeaderValue::from_str(&last_modified_str) {
                        not_modified_response.headers_mut().insert(hyper::header::LAST_MODIFIED, last_modified_header);
                    }
                    *grux_response = not_modified_response;
                    return true;
                }
            }
        }
    }

    // Handle If-Unmodified-Since (RFC 7232 Section 3.4)
    if let Some(if_unmodified_since_value) = grux_request.get_headers().get(hyper::header::IF_UNMODIFIED_SINCE) {
        if let Ok(if_unmodified_since_str) = if_unmodified_since_value.to_str() {
            if let Ok(if_unmodified_since_time) = httpdate::parse_http_date(if_unmodified_since_str) {
                // HTTP dates have 1-second resolution, so we truncate to seconds for comparison
                // If last_modified > if_unmodified_since, the resource has been modified
                let last_modified_secs = last_modified.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                let if_unmodified_since_secs = if_unmodified_since_time.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);

                if last_modified_secs > if_unmodified_since_secs {
                    // Modified since, return 412 Precondition Failed
                    *grux_response = empty_response_with_status(hyper::StatusCode::PRECONDITION_FAILED);
                    return true;
                }
            }
        }
    }

    false
}

#[inline]
fn split_etag_by_comma(etag_list: &str) -> Vec<&str> {
    etag_list.split(',').map(|s| s.trim()).collect()
}

/// Strong ETag comparison (RFC 7232 Section 2.3.2)
/// Both ETags must be strong (not prefixed with W/) and identical
#[inline]
fn etag_strong_compare(etag1: &str, etag2: &str) -> bool {
    // Strong comparison: both must not be weak and must be identical
    !etag1.starts_with("W/") && !etag2.starts_with("W/") && etag1 == etag2
}

/// Weak ETag comparison (RFC 7232 Section 2.3.2)
/// Compares the opaque-tag portion, ignoring the W/ prefix if present
#[inline]
fn etag_weak_compare(etag1: &str, etag2: &str) -> bool {
    let tag1 = etag1.strip_prefix("W/").unwrap_or(etag1);
    let tag2 = etag2.strip_prefix("W/").unwrap_or(etag2);
    tag1 == tag2
}
