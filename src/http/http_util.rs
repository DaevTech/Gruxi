use std::sync::Arc;

use http::HeaderValue;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Bytes;

use crate::core::running_state_manager::get_running_state_manager;
use crate::file::file_reader_structs::FileEntry;
use crate::file::normalized_path::NormalizedPath;
use crate::http::request_response::gruxi_response::GruxiResponse;

pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}

/// Combine the web root and path, and resolve to a full path
pub async fn resolve_web_root_and_path_and_get_file(normalized_path: &NormalizedPath) -> Result<Arc<FileEntry>, std::io::Error> {
    let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
    let file_reader_cache = running_state.get_file_reader_cache();
    let file_data = file_reader_cache.get_file(&normalized_path.get_full_path()).await?;
    Ok(file_data)
}

pub fn empty_response_with_status(status: hyper::StatusCode) -> GruxiResponse {
    let mut resp = GruxiResponse::new_empty_with_status(status.as_u16());
    add_standard_headers_to_response(&mut resp);
    resp
}

const VARY_ACCEPT_ENCODING_VALUE: HeaderValue = HeaderValue::from_static("Accept-Encoding");
const SERVER_HEADER_VALUE: HeaderValue = HeaderValue::from_static("Gruxi");
const CONTENT_TYPE_OCTET_STREAM: HeaderValue = HeaderValue::from_static("application/octet-stream");
const CONTENT_TYPE_TEXT_HTML: HeaderValue = HeaderValue::from_static("text/html");

pub fn add_standard_headers_to_response(resp: &mut GruxiResponse) {
    // Default Vary header to Accept-Encoding
    if !resp.headers().contains_key("Vary") {
        resp.headers_mut().insert("Vary", VARY_ACCEPT_ENCODING_VALUE.clone());
    }

    // Always set server header
    resp.headers_mut().insert("Server", SERVER_HEADER_VALUE.clone());

    // Make sure we always a content type header, also when empty, then set octet-stream
    let is_content_type_empty_option = resp.headers().get("Content-Type");
    let is_content_type_empty = match is_content_type_empty_option {
        Some(v) => {
            let to_str_result = v.to_str();
            match to_str_result {
                Ok(s) => s.is_empty(),
                Err(_) => true,
            }
        }
        None => true,
    };

    if !resp.headers().contains_key("Content-Type") || is_content_type_empty {
        if resp.get_status() == hyper::StatusCode::OK {
            resp.headers_mut().insert("Content-Type", CONTENT_TYPE_OCTET_STREAM.clone());
        } else {
            resp.headers_mut().insert("Content-Type", CONTENT_TYPE_TEXT_HTML.clone());
        }
    }
}

pub fn get_list_of_hop_by_hop_headers(is_websocket_upgrade: bool) -> Vec<String> {
    // Remove hop-by-hop headers as per RFC 2616 Section 13.5.1
    let mut hop_by_hop_headers = vec!["Keep-Alive".to_string(), "Proxy-Authenticate".to_string(), "Proxy-Authorization".to_string(), "TE".to_string(), "Trailers".to_string(), "Transfer-Encoding".to_string(), "Content-Length".to_string()];

    if !is_websocket_upgrade {
        // Also remove Connection and Upgrade headers if not a websocket upgrade
        hop_by_hop_headers.push("Connection".to_string());
        hop_by_hop_headers.push("Upgrade".to_string());
    }

    hop_by_hop_headers
}
