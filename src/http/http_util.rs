use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::Response;
use hyper::body::Bytes;

use crate::core::running_state_manager::get_running_state_manager;
use crate::file::file_cache::CachedFile;
use crate::file::file_util::get_full_file_path;
use crate::logging::syslog::trace;

pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}

pub fn clean_url_path(path: &str) -> String {
    let mut buf = String::with_capacity(path.len());
    let mut chars = path.trim_start_matches('/').chars().peekable();
    let mut prev_was_slash = false;

    while let Some(c) = chars.next() {
        let decoded = if c == '%' {
            let code: String = chars.by_ref().take(2).collect();
            match code.as_str() {
                "20" => Some(' '),
                "2F" | "5C" => Some('/'),
                _ => {
                    buf.push('%');
                    buf.push_str(&code);
                    None
                }
            }
        } else if c == '\\' {
            Some('/')
        } else {
            Some(c)
        };

        if let Some(ch) = decoded {
            if ch == '/' {
                if prev_was_slash {
                    continue; // skip duplicate slashes
                }
                prev_was_slash = true;
            } else {
                prev_was_slash = false;
            }
            buf.push(ch);
        }
    }

    // Remove trailing slash
    while buf.ends_with('/') {
        buf.pop();
    }

    // Remove "." and ".." segments
    let mut parts = Vec::new();
    for part in buf.split('/') {
        match part {
            "" | "." | ".." => continue,
            _ => parts.push(part),
        }
    }

    // Join parts and ensure no trailing slash
    let result = parts.join("/");

    // Final safety check - ensure we never return a trailing slash
    if result.ends_with('/') { result[..result.len() - 1].to_string() } else { result }
}


// Combine the web root and path, and resolve to a full path
pub async fn resolve_web_root_and_path_and_get_file(web_root: &str, path: &str) -> Result<CachedFile, std::io::Error> {
    let path_cleaned = clean_url_path(&path);
    let mut file_path = format!("{}/{}", web_root, path_cleaned);
    trace(format!("Resolved file path for resolving: {}", file_path));
    file_path = get_full_file_path(&file_path)?;

    let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
    let file_cache_rwlock = running_state.get_file_cache();
    let file_cache = file_cache_rwlock.read().await;
    let file_data = file_cache.get_file(&file_path).unwrap();
    Ok(file_data)
}

pub fn empty_response_with_status(status: hyper::StatusCode) -> Response<BoxBody<Bytes, hyper::Error>> {
    let mut resp = Response::new(full(""));
    *resp.status_mut() = status;
    add_standard_headers_to_response(&mut resp);
    resp
}

pub fn add_standard_headers_to_response(resp: &mut Response<BoxBody<Bytes, hyper::Error>>) {
    // Set our standard headers, if not already set
    for (key, value) in get_standard_headers() {
        if resp.headers().contains_key(key) {
            continue;
        }
        resp.headers_mut().insert(key, value.parse().unwrap());
    }

    // Make sure we always a content type header, also when empty, then set octet-stream

    if !resp.headers().contains_key("Content-Type") || resp.headers().get("Content-Type").unwrap().to_str().unwrap().is_empty() {
        if resp.status() == hyper::StatusCode::OK {
            resp.headers_mut().insert("Content-Type", "application/octet-stream".parse().unwrap());
        } else {
            resp.headers_mut().insert("Content-Type", "text/html".parse().unwrap());
        }
    }
}

fn get_standard_headers() -> Vec<(&'static str, &'static str)> {
    return vec![("Server", "Grux"), ("Vary", "Accept-Encoding")];
}
