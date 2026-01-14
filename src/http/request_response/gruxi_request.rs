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
use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::http::request_response::gruxi_body::GruxiBody;

// Wrapper around hyper Request to add calculated data and serve as a request in Gruxi
#[derive(Debug)]
pub struct GruxiRequest {
    // Parts of the original request
    parts: Parts,
    body: GruxiBody,
    // Calculated data cache, such as remote_ip, hostname etc
    pub calculated_data: HashMap<String, String>,
    // Optional connection semaphore for limiting concurrent requests
    pub connection_semaphore: Option<Arc<Semaphore>>,
    // Upgrade future for handling protocol upgrades
    upgrade_future: Option<hyper::upgrade::OnUpgrade>,
}

impl GruxiRequest {
    // Created new buffered request from hyper Request<Bytes>
    pub fn new(hyper_request: Request<Bytes>) -> Self {
        let (mut parts, body) = hyper_request.into_parts();

        // Check if this request has the Upgrade header - if so, we need to extract the upgrade extensions
        let upgrade_future = parts.extensions.remove::<hyper::upgrade::OnUpgrade>();

        // Calculated data cache, such as remote_ip, hostname etc
        let mut calculated_data = HashMap::new();
        calculated_data.insert("body_size_hint".to_string(), body.len().to_string());

        Self {
            parts,
            body: GruxiBody::Buffered(body),
            calculated_data,
            connection_semaphore: None,
            upgrade_future,
        }
    }

    // Created new streaming request from hyper Request<Incoming>
    pub fn from_hyper(hyper_request: Request<hyper::body::Incoming>) -> Self {
        // Calculated data cache, such as remote_ip, hostname etc
        let mut calculated_data = HashMap::new();
        let body_size_hint = hyper_request.body().size_hint().upper().unwrap_or(0);
        calculated_data.insert("body_size_hint".to_string(), body_size_hint.to_string());

        let (mut parts, body) = hyper_request.into_parts();
        let body = GruxiBody::Streaming(body);

        // Check if this request has the Upgrade header - if so, we need to extract the upgrade extensions
        let upgrade_future = parts.extensions.remove::<hyper::upgrade::OnUpgrade>();

        Self {
            parts,
            body,
            calculated_data,
            connection_semaphore: None,
            upgrade_future,
        }
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

    pub fn add_calculated_data(&mut self, key: &str, value: &str) {
        self.calculated_data.insert(key.to_string(), value.to_string());
    }

    pub fn get_calculated_data(&self, key: &str) -> Option<String> {
        self.calculated_data.get(key).cloned()
    }

    pub fn get_hostname(&mut self) -> String {
        if let Some(hostname) = self.calculated_data.get("hostname") {
            return hostname.clone();
        }

        // Default to empty string
        let mut hostname = String::new();

        // Host / :authority
        if let Some(host) = self.parts.headers.get(HOST) {
            if let Ok(host) = host.to_str() {
                hostname = host.to_string();
            }
        }

        // Absolute-form URI (proxy requests)
        if let Some(authority) = self.parts.uri.authority() {
            hostname = authority.as_str().to_string();
        }

        // Remove any ports if present
        if let Some(colon_index) = hostname.find(':') {
            hostname = hostname[..colon_index].to_string();
        }

        self.add_calculated_data("hostname", &hostname);
        hostname
    }

    pub fn get_scheme(&mut self) -> String {
        if let Some(scheme) = self.calculated_data.get("scheme") {
            return scheme.to_string();
        }
        let scheme = if let Some(scheme_str) = self.parts.uri.scheme_str() {
            scheme_str.to_string()
        } else {
            "http".to_string()
        };
        self.add_calculated_data("scheme", &scheme);
        scheme
    }

    pub fn get_http_version(&mut self) -> String {
        if let Some(http_version) = self.calculated_data.get("http_version") {
            return http_version.to_string();
        }
        let http_version = match self.parts.version {
            hyper::Version::HTTP_09 => "HTTP/0.9".to_string(),
            hyper::Version::HTTP_10 => "HTTP/1.0".to_string(),
            hyper::Version::HTTP_11 => "HTTP/1.1".to_string(),
            hyper::Version::HTTP_2 => "HTTP/2.0".to_string(),
            hyper::Version::HTTP_3 => "HTTP/3.0".to_string(),
            _ => "UNKNOWN".to_string(),
        };
        self.add_calculated_data("http_version", &http_version);
        http_version
    }

    pub fn get_http_method(&mut self) -> String {
        if let Some(http_method) = self.calculated_data.get("http_method") {
            return http_method.to_string();
        }
        let http_method = self.parts.method.to_string();
        self.add_calculated_data("http_method", &http_method);
        http_method
    }

    pub fn get_uri(&mut self) -> String {
        if let Some(uri) = self.calculated_data.get("uri") {
            return uri.to_string();
        }
        let uri = self.parts.uri.to_string();
        self.add_calculated_data("uri", &uri);
        uri
    }

    pub fn get_uri_struct(&self) -> &http::Uri {
        &self.parts.uri
    }

    pub fn get_path(&mut self) -> String {
        if let Some(path) = self.calculated_data.get("path") {
            return path.to_string();
        }
        let path = self.parts.uri.path().to_string();
        self.add_calculated_data("path", &path);
        path
    }

    pub fn get_query(&mut self) -> String {
        if let Some(query) = self.calculated_data.get("query") {
            return query.to_string();
        }
        let query = self.parts.uri.query().unwrap_or("").to_string();
        self.add_calculated_data("query", &query);
        query
    }

    pub fn get_path_and_query(&mut self) -> String {
        if let Some(path_and_query) = self.calculated_data.get("path_and_query") {
            return path_and_query.to_string();
        }
        let path_and_query = match self.parts.uri.query() {
            Some(query) => format!("{}?{}", self.parts.uri.path(), query),
            None => self.parts.uri.path().to_string(),
        };
        self.add_calculated_data("path_and_query", &path_and_query);
        path_and_query
    }

    pub fn get_remote_ip(&mut self) -> String {
        if let Some(remote_ip) = self.calculated_data.get("remote_ip") {
            return remote_ip.to_string();
        }
        return "".to_string();
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

    pub fn get_streaming_http_request(&mut self) -> Result<Request<BoxBody<Bytes, hyper::Error>>, ()> {
        match mem::replace(&mut self.body, GruxiBody::Buffered(Bytes::new())) {
            GruxiBody::Streaming(incoming_body) => {
                let request = Request::from_parts(self.parts.clone(), incoming_body.boxed());
                Ok(request)
            }
            other => {
                self.body = other;
                Err(())
            }
        }
    }

    pub fn get_body_size(&mut self) -> u64 {
        if let Some(body_size_hint) = self.calculated_data.get("body_size_hint") {
            return body_size_hint.parse().unwrap_or(0);
        }
        0
    }

    pub fn is_https(&mut self) -> bool {
        if let Some(is_https) = self.calculated_data.get("is_https") {
            return is_https == "true";
        }

        let is_https = if let Some(scheme) = self.parts.uri.scheme_str() {
            scheme.eq_ignore_ascii_case("https")
        } else {
            false
        };
        self.calculated_data.insert("is_https".to_string(), if is_https { "true" } else { "false" }.to_string());
        is_https
    }

    pub fn get_server_port(&mut self) -> u16 {
        if let Some(server_port) = self.calculated_data.get("server_port") {
            return server_port.parse().unwrap_or(80);
        }

        let server_port = if let Some(port) = self.parts.uri.port_u16() {
            port
        } else if self.is_https() {
            443
        } else {
            80
        };
        self.calculated_data.insert("server_port".to_string(), server_port.to_string());
        server_port
    }

    pub fn take_upgrade(&mut self) -> Option<hyper::upgrade::OnUpgrade> {
        self.upgrade_future.take()
    }

    pub fn set_new_uri(&mut self, new_uri: &str) {
        let uri = new_uri.parse().unwrap_or(self.parts.uri.clone());
        self.parts.uri = uri;
        self.add_calculated_data("uri", new_uri);
    }

    pub fn set_new_hostname(&mut self, new_hostname: &str) {
        self.parts
            .headers
            .insert("Host", hyper::header::HeaderValue::from_str(new_hostname).unwrap_or(hyper::header::HeaderValue::from_static("")));
        self.add_calculated_data("hostname", new_hostname);
    }

    pub fn remove_header(&mut self, header_name: &str) {
        self.parts.headers.remove(header_name);
    }

    pub fn clean_hop_by_hop_headers(&mut self) {
        let is_upgrade = self.parts.headers.get("Upgrade").is_some();
        let connection_header_option = self.parts.headers.get("Connection");

        let mut hop_by_hop_headers = crate::http::http_util::get_list_of_hop_by_hop_headers(is_upgrade);

        // Check the connection header for any additional hop-by-hop headers, before we remove the connection header itself
        if !is_upgrade {
            if let Some(connection_header) = connection_header_option {
                if let Ok(connection_header_str) = connection_header.to_str() {
                    for token in connection_header_str.split(',') {
                        let token_trimmed = token.trim();
                        if !token_trimmed.is_empty() {
                            hop_by_hop_headers.push(token_trimmed.to_string());
                        }
                    }
                }
            }
        }

        for header in &hop_by_hop_headers {
            self.remove_header(header);
        }
    }

    pub fn add_forwarded_headers(&mut self) {
        // Add X-Forwarded-For header
        if let Some(remote_ip) = self.get_calculated_data("remote_ip") {
            let x_forwarded_for_value = if let Some(existing_xff) = self.parts.headers.get("X-Forwarded-For") {
                format!("{}, {}", existing_xff.to_str().unwrap_or(""), remote_ip)
            } else {
                remote_ip
            };
            self.parts
                .headers
                .insert("X-Forwarded-For", HeaderValue::from_str(&x_forwarded_for_value).unwrap_or(HeaderValue::from_static("")));
        }

        // Add X-Forwarded-Proto header
        let scheme = self.get_scheme();
        self.parts
            .headers
            .insert("X-Forwarded-Proto", HeaderValue::from_str(&scheme).unwrap_or(HeaderValue::from_static("http")));

        // X-Forwarded-Host header
        let hostname = self.get_hostname();
        self.parts.headers.insert("X-Forwarded-Host", HeaderValue::from_str(&hostname).unwrap_or(HeaderValue::from_static("")));
    }

    pub fn get_accepted_encodings(&self) -> Vec<String> {
        if let Some(accept_encoding_header) = self.parts.headers.get("Accept-Encoding") {
            if let Ok(accept_encoding_str) = accept_encoding_header.to_str() {
                return accept_encoding_str.split(',').map(|s| s.trim().to_string()).collect();
            }
        }
        Vec::new()
    }
}
