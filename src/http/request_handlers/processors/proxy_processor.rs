use crate::{
    configuration::site::Site,
    core::running_state_manager,
    http::{
        http_util::empty_response_with_status,
        request_handlers::{
            processor_trait::ProcessorTrait,
            processors::{load_balancer::round_robin::RoundRobin},
        },
        requests::grux_request::GruxRequest,
    },
    logging::syslog::{error, trace},
};
use http::HeaderValue;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::Response;
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyProcessorUrlRewrite {
    pub from: String,
    pub to: String,
    pub is_case_insensitive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyProcessor {
    pub id: String,         // Unique identifier for the processor
    pub proxy_type: String, // e.g., "http", for further extension
    // HTTP Proxy specific settings
    pub upstream_servers: Vec<String>,               // List of upstream servers e.g., ["http://server1:8080", "https://server2:8080"]
    pub load_balancing_strategy: String,             // e.g., "round_robin" only for now
    pub timeout_seconds: u16,                        // Timeout for upstream requests, in seconds
    pub health_check_path: String,                   // Path to use for health checks
    pub url_rewrites: Vec<ProxyProcessorUrlRewrite>, // URL rewrite rules - Rewrites on entire URL
    // Host header handling
    pub preserve_host_header: bool, // Whether to preserve the Host header to match the original request, normally not recommended for upstream servers
    pub forced_host_header: String, // If set, this host header will be used instead of the original request's Host header, disregarding preserve_host_header - normally not recommended for normal use
    // SSL/TLS settings
    pub verify_tls_certificates: bool, // Whether to verify TLS certificates (set to false for self-signed certs)
}

impl ProxyProcessor {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            proxy_type: "http".to_string(),
            upstream_servers: Vec::new(),
            load_balancing_strategy: "round_robin".to_string(),
            timeout_seconds: 30,
            health_check_path: "/health".to_string(),
            url_rewrites: Vec::new(),
            preserve_host_header: false,
            forced_host_header: "".to_string(),
            verify_tls_certificates: true,
        }
    }

    pub fn apply_url_rewrites(&self, original_url: &str) -> String {
        // Process the URI through the rewrite rules
        let mut url = original_url.to_string();

        for rewrite in &self.url_rewrites {
            if rewrite.is_case_insensitive {
                url = Self::replace_case_insensitive(&url, &rewrite.from, &rewrite.to);
            } else {
                url = url.replace(&rewrite.from, &rewrite.to);
            }
        }

        url
    }

    // Case-insensitive replacement
    fn replace_case_insensitive(s: &str, from: &str, to: &str) -> String {
        if from.is_empty() {
            return s.to_string();
        }

        let mut result = String::with_capacity(s.len());
        let mut i = 0;
        let s_lower = s.to_lowercase();
        let from_lower = from.to_lowercase();
        let from_len = from.len();

        while i < s.len() {
            // Check if from matches at this position
            if i + from_len <= s.len() && &s_lower[i..i + from_len] == from_lower.as_str() {
                result.push_str(to);
                i += from_len;
            } else {
                // Push the next character (handle UTF-8 properly)
                let ch = s[i..].chars().next().unwrap();
                result.push(ch);
                i += ch.len_utf8();
            }
        }

        result
    }

    fn clean_hop_by_hop_headers_in_response(response: &mut Response<hyper::body::Incoming>, is_websocket_upgrade: bool) {
        let hop_by_hop_headers = crate::http::http_util::get_list_of_hop_by_hop_headers(is_websocket_upgrade);
        for header in &hop_by_hop_headers {
            response.headers_mut().remove(header);
        }
    }
}

impl ProcessorTrait for ProxyProcessor {
    fn sanitize(&mut self) {
        // Clean up upstream server URLs
        self.upstream_servers = self.upstream_servers.iter().map(|url| url.trim().to_string()).filter(|url| !url.is_empty()).collect();

        // Load balancing strategy trim
        self.load_balancing_strategy = self.load_balancing_strategy.trim().to_string();

        // Health check path trim
        self.health_check_path = self.health_check_path.trim().to_string();

        // URL rewrites cleanup
        for rewrite in &mut self.url_rewrites {
            rewrite.from = rewrite.from.trim().to_string();
            rewrite.to = rewrite.to.trim().to_string();
        }
    }

    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.proxy_type != "http" {
            errors.push("Unsupported proxy type. Only 'http' is supported.".to_string());
        }

        // There needs to be at least one upstream server
        if self.upstream_servers.is_empty() {
            errors.push("At least one upstream server must be specified.".to_string());
        }

        // All upstream servers must be valid URLs, starting with http:// or https://
        for server in &self.upstream_servers {
            if !server.starts_with("http://") && !server.starts_with("https://") {
                errors.push(format!("Upstream server '{}' is not a valid upstream URL. It must start with 'http://' or 'https://'.", server));
            }
        }

        if self.load_balancing_strategy != "round_robin" {
            errors.push("Unsupported load balancing strategy. Only 'Round Robin' is supported.".to_string());
        }

        if self.timeout_seconds < 1 {
            errors.push("Timeout seconds must be greater than zero.".to_string());
        }

        if !self.health_check_path.is_empty() && !self.health_check_path.starts_with('/') {
            errors.push("Health check path must start with '/'.".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn handle_request(&self, grux_request: &mut GruxRequest, _site: &Site) -> Result<Response<BoxBody<hyper::body::Bytes, hyper::Error>>, ()> {
        trace(format!("ProxyProcessor handling request - {:?}", &self));

        // We determine which upstream server to use based on the load balancing strategy.
        let running_state_manager = running_state_manager::get_running_state_manager().await;
        let running_state = running_state_manager.get_running_state();
        let running_state_read_lock = running_state.read().await;
        let load_balancer = running_state_read_lock.get_proxy_processor_load_balancer();

        if !load_balancer.check_load_balancer_exists(&self.id) {
            // Create load balancer instance
            let lb_instance = match self.load_balancing_strategy.as_str() {
                "round_robin" => {
                    let rr = RoundRobin::new(self.upstream_servers.clone());
                    rr
                }
                _ => {
                    return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
                }
            };

            // Register the load balancer
            load_balancer.create_load_balancer(&self.id, lb_instance);
        }

        let server_to_handle_request = {
            let lb = load_balancer.get_load_balancer(&self.id).unwrap();
            lb.read().unwrap().get_next_server()
        };
        if server_to_handle_request.is_none() {
            return Ok(empty_response_with_status(hyper::StatusCode::BAD_GATEWAY));
        }
        let server_to_handle_request = server_to_handle_request.unwrap();

        // Rewrite the request URL to point to the upstream server
        let original_uri = grux_request.get_uri();
        let new_uri = format!("{}{}", server_to_handle_request, original_uri);

        // Apply any URL rewrites
        let rewritten_url = self.apply_url_rewrites(&new_uri);

        // Parse the full upstream URL
        let upstream_uri: hyper::Uri = match rewritten_url.parse() {
            Ok(uri) => uri,
            Err(e) => {
                error(format!("Failed to parse upstream URL '{}': {}", rewritten_url, e));
                return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
            }
        };

        // Get the client appropriate for TLS verification settings
        let client = running_state_read_lock.get_http_client().get_client(self.verify_tls_certificates);

        // Get the client-side upgrade on the request side
        let client_upgrade = grux_request.take_upgrade();

        // Clean any hop by hop headers from the request and add forwarded headers
        grux_request.clean_hop_by_hop_headers();
        grux_request.add_forwarded_headers();

        // Get the original request to extract headers and body
        let mut proxy_request = match grux_request.get_streaming_http_request() {
            Ok(req) => req,
            Err(_) => {
                error("Failed to get HTTP request from GruxRequest");
                return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
            }
        };

        // Update the URI to point to the upstream server (with full URL including scheme/host/port)
        *proxy_request.uri_mut() = upstream_uri;

        // Check if we should preserve the host header or remote it to let hyper set it
        if self.forced_host_header.is_empty() {
            // Header is there already, so we only remove it if we are not preserving it
            if !self.preserve_host_header {
                proxy_request.headers_mut().remove(hyper::header::HOST);
                trace("Not preserving original Host header for upstream request");
            }
        } else {
            trace("Using forced Host header for upstream request");
            if let Ok(header_value) = HeaderValue::from_str(&self.forced_host_header) {
                proxy_request.headers_mut().insert(hyper::header::HOST, header_value);
            }
        }

        trace(format!("Forwarding request to upstream server: {:?}", proxy_request));

        match client.request(proxy_request).await {
            Ok(mut resp) => {
                // Check if this is a protocol upgrade
                let mut is_websocket_upgrade = false;
                if resp.status() == hyper::StatusCode::SWITCHING_PROTOCOLS {
                    trace("Detected WebSocket/protocol upgrade (HTTP 101)");

                    // Get the upstream upgrade from the response extensions
                    let upstream_upgrade = resp.extensions_mut().remove::<hyper::upgrade::OnUpgrade>();

                    if let (Some(client_upgrade), Some(upstream_upgrade)) = (client_upgrade, upstream_upgrade) {
                        // Spawn task to bridge the connections
                        tokio::spawn(async move {
                            match tokio::try_join!(client_upgrade, upstream_upgrade) {
                                Ok((client, upstream)) => {
                                    trace("WebSocket upgrade successful, bridging connections");
                                    // Wrap the upgraded connections with TokioIo to make them compatible with tokio::io
                                    let mut client = TokioIo::new(client);
                                    let mut upstream = TokioIo::new(upstream);
                                    match tokio::io::copy_bidirectional(&mut client, &mut upstream).await {
                                        Ok((from_client, from_server)) => {
                                            trace(format!("WebSocket closed. Client→Server: {} bytes, Server→Client: {} bytes", from_client, from_server));
                                        }
                                        Err(e) => {
                                            error(format!("WebSocket proxy error: {}", e));
                                        }
                                    }
                                }
                                Err(e) => {
                                    error(format!("Failed to upgrade connections: {}", e));
                                }
                            }
                        });
                        is_websocket_upgrade = true;
                    }
                }

                // In the response, we make sure to update/clean the headers as needed
                Self::clean_hop_by_hop_headers_in_response(&mut resp, is_websocket_upgrade);

                return Ok(resp.map(|body| body.boxed()));
            }
            Err(e) => {
                error(format!("Failed to send request to upstream server: {:?} (can be certificate error, if using TLS)", e));
                return Ok(empty_response_with_status(hyper::StatusCode::BAD_GATEWAY));
            }
        }
    }

    fn get_type(&self) -> String {
        "proxy".to_string()
    }

    fn get_default_pretty_name(&self) -> String {
        "Proxy Processor".to_string()
    }
}
