use http_body_util::BodyExt;
use hyper::HeaderMap;
use hyper::Request;
use hyper::body::Body;
use hyper::body::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

// Wrapper around hyper Request to add calculated data and serve as a request in Grux
#[derive(Debug)]
pub struct GruxRequest {
    request: Request<Bytes>,
    body_bytes: Vec<u8>,
    pub calculated_data: HashMap<String, String>,
    pub connection_semaphore: Option<Arc<Semaphore>>,
}

impl GruxRequest {
    pub fn new(request: Request<Bytes>, body_bytes: Vec<u8>) -> Self {
        Self {
            request,
            body_bytes,
            calculated_data: HashMap::new(),
            connection_semaphore: None,
        }
    }

    pub async fn from_hyper<B>(req: Request<B>) -> Result<Self, hyper::Error>
    where
        B: Body + Send + 'static
    {
        // Split parts and body
        let (parts, body) = req.into_parts();

        // Collect the body into Bytes
        let body_bytes_result = body.collect().await;
        let body_bytes = match body_bytes_result {
            Ok(bytes) => bytes.to_bytes(),
            Err(_) => Bytes::new(),
        };

        // Rebuild a Request<Bytes>
        let req = Request::from_parts(parts, Bytes::new());
        let grux_request = GruxRequest::new(req, body_bytes.to_vec());
        Ok(grux_request)
    }

    pub fn get_headers(&self) -> &HeaderMap {
        self.request.headers()
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
        if let Some(host_header) = self.calculated_data.get("hostname") {
            return host_header.to_string();
        }
        let requested_hostname = self
            .request
            .headers()
            .get(":authority")
            .or_else(|| self.request.headers().get("Host"))
            .and_then(|h| h.to_str().ok())
            .unwrap_or("")
            .to_string();
        self.add_calculated_data("hostname", &requested_hostname);
        requested_hostname
    }

    pub fn get_http_version(&mut self) -> String {
        if let Some(http_version) = self.calculated_data.get("http_version") {
            return http_version.to_string();
        }
        let http_version = match self.request.version() {
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
        let http_method = self.request.method().to_string();
        self.add_calculated_data("http_method", &http_method);
        http_method
    }

    pub fn get_uri(&mut self) -> String {
        if let Some(http_method) = self.calculated_data.get("uri") {
            return http_method.to_string();
        }
        let uri = self.request.uri().to_string();
        self.add_calculated_data("uri", &uri);
        uri
    }

    pub fn get_path(&mut self) -> String {
        if let Some(path) = self.calculated_data.get("path") {
            return path.to_string();
        }
        let path = self.request.uri().path().to_string();
        self.add_calculated_data("path", &path);
        path
    }

    pub fn get_query(&mut self) -> String {
        if let Some(query) = self.calculated_data.get("query") {
            return query.to_string();
        }
        let query = self.request.uri().query().unwrap_or("").to_string();
        self.add_calculated_data("query", &query);
        query
    }

    pub fn get_path_and_query(&mut self) -> String {
        if let Some(path_and_query) = self.calculated_data.get("path_and_query") {
            return path_and_query.to_string();
        }
        let path_and_query = match self.request.uri().query() {
            Some(query) => format!("{}?{}", self.request.uri().path(), query),
            None => self.request.uri().path().to_string(),
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

    pub fn get_body_size(&self) -> u64 {
        self.body_bytes.len() as u64
    }

    pub fn get_body_bytes(&self) -> &Vec<u8> {
        self.body_bytes.as_ref()
    }

    pub fn is_https(&mut self) -> bool {
        if let Some(is_https) = self.calculated_data.get("is_https") {
            return is_https == "true";
        }

        let is_https = if let Some(scheme) = self.request.uri().scheme_str() {
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

        let server_port = if let Some(port) = self.request.uri().port_u16() {
            port
        } else if self.is_https() {
            443
        } else {
            80
        };
        self.calculated_data.insert("server_port".to_string(), server_port.to_string());
        server_port
    }
}
