use http::HeaderValue;
use http::header::HOST;
use http::request::Parts;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::HeaderMap;
use hyper::Request;
use hyper::body::Body;
use hyper::body::Bytes;
use std::collections::HashMap;
use std::mem;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::GruxiErrorKind;
use crate::error::gruxi_error_enums::GruxiRequestError;
use crate::http::request_response::gruxi_body::GruxiBody;

// Wrapper around hyper Request to add calculated data and serve as a request in Gruxi
#[derive(Debug)]
pub struct GruxiRequest {
    // Parts of the original request
    parts: Parts,
    body: GruxiBody,
    // Additional computed request data, primarily derived from request data
    data: GruxiRequestData,
    // Optional connection semaphore for limiting concurrent requests, such as towards PHP
    pub connection_semaphore: Option<Arc<Semaphore>>,
    // Upgrade future for handling protocol upgrades
    upgrade_future: Option<hyper::upgrade::OnUpgrade>,
}

#[derive(Debug)]
struct GruxiRequestData {
    body_size_hint: u64,
    hostname: String,
    remote_ip: Option<SocketAddr>,
    other: Option<HashMap<String, String>>,
}

impl GruxiRequestData {
    pub fn new(body_size_hint: u64, hostname: String) -> Self {
        GruxiRequestData {
            body_size_hint,
            hostname,
            remote_ip: None,
            other: None,
        }
    }
}

impl GruxiRequest {
    // Created new buffered request from hyper Request<Bytes>
    pub fn new(hyper_request: Request<Bytes>) -> Self {
        let (mut parts, body) = hyper_request.into_parts();

        // Check if this request has the Upgrade header - if so, we need to extract the upgrade extensions
        let upgrade_future = parts.extensions.remove::<hyper::upgrade::OnUpgrade>();

        let body_size_hint = body.len() as u64;
        let hostname = Self::extract_hostname(&parts);
        let data = GruxiRequestData::new(body_size_hint, hostname);

        Self {
            parts,
            body: GruxiBody::Buffered(body),
            data,
            connection_semaphore: None,
            upgrade_future,
        }
    }

    // Created new streaming request from hyper Request<Incoming>
    pub fn from_hyper(hyper_request: Request<hyper::body::Incoming>) -> Self {
        let body_size_hint = hyper_request.body().size_hint().upper().unwrap_or(0);

        let (mut parts, body) = hyper_request.into_parts();
        let body = GruxiBody::Streaming(body);

        // Check if this request has the Upgrade header - if so, we need to extract the upgrade extensions
        let upgrade_future = parts.extensions.remove::<hyper::upgrade::OnUpgrade>();

        let hostname = Self::extract_hostname(&parts);
        let data = GruxiRequestData::new(body_size_hint, hostname);

        Self {
            parts,
            body,
            data,
            connection_semaphore: None,
            upgrade_future,
        }
    }

    pub fn add_calculated_data(&mut self, key: &str, value: &str) {
        if self.data.other.is_none() {
            self.data.other = Some(HashMap::new());
        }
        if let Some(other_map) = &mut self.data.other {
            other_map.insert(key.to_string(), value.to_string());
        }
    }

    pub fn get_calculated_data(&self, key: &str) -> Option<&str> {
        if let Some(other_map) = &self.data.other {
            return other_map.get(key).map(|s| s.as_str());
        }
        None
    }

    // Extract hostname from request parts (Host header or URI authority)
    fn extract_hostname(parts: &Parts) -> String {
        let mut hostname = String::new();

        // Host / :authority header
        if let Some(host) = parts.headers.get(HOST)
            && let Ok(host) = host.to_str() {
                hostname = host.to_string();
            }

        // Absolute-form URI (proxy requests) takes precedence
        if let Some(authority) = parts.uri.authority() {
            hostname = authority.as_str().to_string();
        }

        // Remove any ports if present
        if let Some(colon_index) = hostname.find(':') {
            hostname = hostname[..colon_index].to_string();
        }

        // Make sure to lowercase the hostname for consistent matching
        hostname = hostname.to_lowercase();

        hostname
    }

    pub fn get_headers(&self) -> &HeaderMap {
        &self.parts.headers
    }

    pub fn get_connection_semaphore(&self) -> Option<Arc<Semaphore>> {
        self.connection_semaphore.clone()
    }

    pub fn set_connection_semaphore(&mut self, semaphore: Arc<Semaphore>) {
        self.connection_semaphore = Some(semaphore);
    }

    pub fn set_remote_ip(&mut self, remote_ip: SocketAddr) {
        self.data.remote_ip = Some(remote_ip);
    }

    pub fn get_remote_ip(&self) -> Option<SocketAddr> {
        self.data.remote_ip
    }

    pub fn get_remote_ip_string(&self) -> String {
        if let Some(remote_ip) = self.data.remote_ip {
            remote_ip.to_string()
        } else {
            String::new()
        }
    }

    pub fn get_remote_ip_pretty(&self) -> String {
        if let Some(remote_ip) = self.data.remote_ip {
            remote_ip.to_string()
        } else {
            "<unknown>".to_string()
        }
    }

    pub fn get_hostname(&self) -> &str {
        &self.data.hostname
    }

    pub fn get_scheme(&self) -> &str {
        self.parts.uri.scheme_str().unwrap_or("http")
    }

    pub fn get_http_version(&self) -> &'static str {
        match self.parts.version {
            hyper::Version::HTTP_09 => "HTTP/0.9",
            hyper::Version::HTTP_10 => "HTTP/1.0",
            hyper::Version::HTTP_11 => "HTTP/1.1",
            hyper::Version::HTTP_2 => "HTTP/2.0",
            hyper::Version::HTTP_3 => "HTTP/3.0",
            _ => "UNKNOWN",
        }
    }

    pub fn get_http_method(&self) -> &str {
        self.parts.method.as_str()
    }

    pub fn get_uri(&self) -> String {
        self.parts.uri.to_string()
    }

    pub fn get_uri_struct(&self) -> &http::Uri {
        &self.parts.uri
    }

    pub fn get_path(&self) -> &str {
        self.parts.uri.path()
    }

    pub fn get_query(&self) -> &str {
        self.parts.uri.query().unwrap_or("")
    }

    pub fn get_path_and_query(&self) -> &str {
        self.parts.uri.path_and_query().map(|pq| pq.as_str()).unwrap_or(self.parts.uri.path())
    }

    pub fn get_body_size(&self) -> u64 {
        self.data.body_size_hint
    }

    pub fn is_https(&self) -> bool {
        self.parts.uri.scheme_str().map(|s| s.eq_ignore_ascii_case("https")).unwrap_or(false)
    }

    pub fn get_server_port(&self) -> u16 {
        if let Some(port) = self.parts.uri.port_u16() {
            port
        } else if self.is_https() {
            443
        } else {
            80
        }
    }
    // Returns the full body bytes. Beware this consumes the internal body bytes
    pub async fn get_body_bytes(&mut self) -> Bytes {
        match &mut self.body {
            GruxiBody::Buffered(bytes) => bytes.clone(),
            GruxiBody::Streaming(incoming_body) => {
                let body = incoming_body.collect().await;
                match body {
                    Ok(bytes) => bytes.to_bytes(),
                    Err(_) => Bytes::new(),
                }
            }
            GruxiBody::StreamingBoxed(boxed_body) => {
                let body = boxed_body.collect().await;
                match body {
                    Ok(bytes) => bytes.to_bytes(),
                    Err(_) => Bytes::new(),
                }
            }
        }
    }

    pub fn get_streaming_http_request(&mut self) -> Result<Request<BoxBody<Bytes, hyper::Error>>, GruxiError> {
        match mem::replace(&mut self.body, GruxiBody::Buffered(Bytes::new())) {
            GruxiBody::Streaming(incoming_body) => {
                let request = Request::from_parts(self.parts.clone(), incoming_body.boxed());
                Ok(request)
            }
            other => {
                self.body = other;
                Err(GruxiError::new_with_kind_only(GruxiErrorKind::GruxiRequest(GruxiRequestError::GetStreamingBodyError)))
            }
        }
    }

    pub fn take_upgrade(&mut self) -> Option<hyper::upgrade::OnUpgrade> {
        self.upgrade_future.take()
    }

    pub fn set_new_uri(&mut self, new_uri: &str) {
        if let Ok(uri) = new_uri.parse() {
            self.parts.uri = uri;
        }
    }

    pub fn set_new_hostname(&mut self, new_hostname: &str) {
        self.parts
            .headers
            .insert(HOST, hyper::header::HeaderValue::from_str(new_hostname).unwrap_or(hyper::header::HeaderValue::from_static("")));
        self.data.hostname = new_hostname.to_string();
    }

    pub fn remove_header(&mut self, header_name: &str) {
        self.parts.headers.remove(header_name);
    }

    pub fn clean_hop_by_hop_headers(&mut self) {
        let is_upgrade = self.parts.headers.get("Upgrade").is_some();
        let connection_header_option = self.parts.headers.get("Connection");

        let mut hop_by_hop_headers = crate::http::http_util::get_list_of_hop_by_hop_headers(is_upgrade);

        // Check the connection header for any additional hop-by-hop headers, before we remove the connection header itself
        if !is_upgrade
            && let Some(connection_header) = connection_header_option
                && let Ok(connection_header_str) = connection_header.to_str() {
                    for token in connection_header_str.split(',') {
                        let token_trimmed = token.trim();
                        if !token_trimmed.is_empty() {
                            hop_by_hop_headers.push(token_trimmed.to_string());
                        }
                    }
                }

        for header in &hop_by_hop_headers {
            self.remove_header(header);
        }
    }

    pub fn add_forwarded_headers(&mut self) {
        // Add X-Forwarded-For header
        let remote_ip = self.get_remote_ip_string();
        if !remote_ip.is_empty() {
            let x_forwarded_for_value = if let Some(existing_xff) = self.parts.headers.get("X-Forwarded-For") {
                format!("{}, {}", existing_xff.to_str().unwrap_or(""), remote_ip)
            } else {
                remote_ip.to_string()
            };
            self.parts
                .headers
                .insert("X-Forwarded-For", HeaderValue::from_str(&x_forwarded_for_value).unwrap_or(HeaderValue::from_static("")));
        }

        // Add X-Forwarded-Proto header
        let scheme = self.parts.uri.scheme_str().unwrap_or("http");
        self.parts
            .headers
            .insert("X-Forwarded-Proto", HeaderValue::from_str(scheme).unwrap_or(HeaderValue::from_static("http")));

        // X-Forwarded-Host header
        let hostname = &self.data.hostname;
        self.parts.headers.insert("X-Forwarded-Host", HeaderValue::from_str(hostname).unwrap_or(HeaderValue::from_static("")));
    }

    pub fn check_accepted_encoding(&self, encoding: &str) -> bool {
        if let Some(accept_encoding_header) = self.parts.headers.get("Accept-Encoding") && let Ok(accept_encoding_str) = accept_encoding_header.to_str() {
            return accept_encoding_str.split(',').any(|entry| {
                let mut parts = entry.trim().splitn(2, ';');
                let name = parts.next().unwrap_or("").trim();
                if !name.eq_ignore_ascii_case(encoding) {
                    return false;
                }
                // If there's a quality value of 0, the encoding is explicitly not accepted
                if let Some(params) = parts.next()
                    && let Some(q) = params.trim().strip_prefix("q=").or_else(|| params.trim().strip_prefix("Q="))
                    && let Ok(quality) = q.trim().parse::<f32>() {
                    return quality > 0.0;
                }
                true
            });
        }
        false
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Request;

    fn make_request_with_accept_encoding(value: &str) -> GruxiRequest {
        let req = Request::builder()
            .uri("http://localhost/")
            .header("Accept-Encoding", value)
            .body(Bytes::new())
            .unwrap();
        GruxiRequest::new(req)
    }

    fn make_request_without_accept_encoding() -> GruxiRequest {
        let req = Request::builder()
            .uri("http://localhost/")
            .body(Bytes::new())
            .unwrap();
        GruxiRequest::new(req)
    }

    #[test]
    fn test_check_accepted_encoding_simple_match() {
        let req = make_request_with_accept_encoding("gzip, deflate, br");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(req.check_accepted_encoding("deflate"));
        assert!(req.check_accepted_encoding("br"));
    }

    #[test]
    fn test_check_accepted_encoding_case_insensitive() {
        let req = make_request_with_accept_encoding("Gzip, Deflate");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(req.check_accepted_encoding("GZIP"));
        assert!(req.check_accepted_encoding("Gzip"));
    }

    #[test]
    fn test_check_accepted_encoding_not_present() {
        let req = make_request_with_accept_encoding("gzip, deflate");
        assert!(!req.check_accepted_encoding("br"));
        assert!(!req.check_accepted_encoding("zstd"));
    }

    #[test]
    fn test_check_accepted_encoding_no_header() {
        let req = make_request_without_accept_encoding();
        assert!(!req.check_accepted_encoding("gzip"));
    }

    #[test]
    fn test_check_accepted_encoding_with_quality_one() {
        let req = make_request_with_accept_encoding("gzip;q=1, br;q=1.0");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(req.check_accepted_encoding("br"));
    }

    #[test]
    fn test_check_accepted_encoding_with_quality_partial() {
        let req = make_request_with_accept_encoding("gzip;q=0.5, br;q=0.1");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(req.check_accepted_encoding("br"));
    }

    #[test]
    fn test_check_accepted_encoding_with_quality_zero_rejected() {
        let req = make_request_with_accept_encoding("gzip;q=0, br;q=0.0, deflate;q=0.000");
        assert!(!req.check_accepted_encoding("gzip"));
        assert!(!req.check_accepted_encoding("br"));
        assert!(!req.check_accepted_encoding("deflate"));
    }

    #[test]
    fn test_check_accepted_encoding_mixed_quality() {
        let req = make_request_with_accept_encoding("gzip;q=1, br;q=0, deflate");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(!req.check_accepted_encoding("br"));
        assert!(req.check_accepted_encoding("deflate"));
    }

    #[test]
    fn test_check_accepted_encoding_with_spaces_around_quality() {
        let req = make_request_with_accept_encoding("gzip ; q=0.8 , br ; q=0");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(!req.check_accepted_encoding("br"));
    }

    #[test]
    fn test_check_accepted_encoding_uppercase_q() {
        let req = make_request_with_accept_encoding("gzip;Q=0.5, br;Q=0");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(!req.check_accepted_encoding("br"));
    }

    #[test]
    fn test_check_accepted_encoding_single_encoding() {
        let req = make_request_with_accept_encoding("gzip");
        assert!(req.check_accepted_encoding("gzip"));
        assert!(!req.check_accepted_encoding("br"));
    }

    #[test]
    fn test_check_accepted_encoding_identity() {
        let req = make_request_with_accept_encoding("identity;q=0, gzip");
        assert!(!req.check_accepted_encoding("identity"));
        assert!(req.check_accepted_encoding("gzip"));
    }
}