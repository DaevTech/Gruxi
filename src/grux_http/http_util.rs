use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::{Response};
use hyper::body::Bytes;



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
    if result.ends_with('/') {
        result[..result.len() - 1].to_string()
    } else {
        result
    }
}

pub fn empty_response_with_status(status: hyper::StatusCode) -> Response<BoxBody<Bytes, hyper::Error>> {
    let mut resp = Response::new(full(""));
    *resp.status_mut() = status;
    add_standard_headers_to_response(&mut resp);
    resp
}

pub fn add_standard_headers_to_response(resp: &mut Response<BoxBody<Bytes, hyper::Error>>) {
    for (key, value) in get_standard_headers() {
        resp.headers_mut().insert(key, value.parse().unwrap());
    }
}

fn get_standard_headers() -> Vec<(&'static str, &'static str)> {
    return vec![("Server", "Grux"), ("Vary", "Accept-Encoding")];
}