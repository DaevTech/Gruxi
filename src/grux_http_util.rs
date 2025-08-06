use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::{Response};
use hyper::body::Bytes;



pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}

pub fn clean_url_path(path: &str) -> String {
    let mut cleaned_path = path.trim_start_matches('/').to_string();

    cleaned_path = cleaned_path.replace("%20", " "); // Decode spaces
    cleaned_path = cleaned_path.replace("%2F", "/"); // Decode forward slashes
    cleaned_path = cleaned_path.replace("%5C", "/"); // Decode backslashes
    cleaned_path = cleaned_path.replace("\\", "/"); // Replace backslashes with forward slashes
    cleaned_path = cleaned_path.trim_end_matches('/').to_string(); // Remove trailing slashes
    cleaned_path = cleaned_path.replace("..", ""); // Remove any parent directory references
    cleaned_path = cleaned_path.replace("./", ""); // Remove current directory references
    cleaned_path = cleaned_path.replace("//", "/"); // Ensure no double slashes remain
    cleaned_path
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