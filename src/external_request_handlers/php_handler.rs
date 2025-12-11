use crate::configuration::site::Site;
use crate::core::triggers::get_trigger_handler;
use crate::external_request_handlers::external_request_handlers::ExternalRequestHandler;
use crate::file::file_util::{get_full_file_path, replace_web_root_in_path, split_path};
use crate::http::http_util::*;
use crate::logging::syslog::{error, trace, warn};
use crate::network::port_manager::{PortManager, get_port_manager};
use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use hyper::{HeaderMap, Response};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicU16, Ordering},
};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::select;
use tokio::sync::{Mutex, Semaphore};

/// Structure to manage a single persistent PHP-CGI process with FastCGI children.
///
/// This handles:
/// - Starting php-cgi.exe with PHP_FCGI_CHILDREN environment variable
/// - Monitoring process health with keep-alive FastCGI requests
/// - Automatic restart when the process dies or doesn't respond
/// - Port management through the PortManager
struct PhpCgiProcess {
    process: Option<Child>,
    executable_path: String,
    restart_count: u32,
    assigned_port: Option<u16>,
    port_manager: PortManager,
    concurrent_threads: usize,
    last_activity: Instant,
    extra_environment: Vec<(String, String)>,
}

impl PhpCgiProcess {
    fn new(executable_path: String, port_manager: PortManager, concurrent_threads: usize, extra_environment: Vec<(String, String)>) -> Self {
        PhpCgiProcess {
            process: None,
            executable_path,
            restart_count: 0,
            assigned_port: None,
            port_manager,
            concurrent_threads,
            last_activity: Instant::now(),
            extra_environment,
        }
    }

    async fn start(&mut self) -> Result<(), String> {
        trace(format!("Starting PHP-CGI process with FastCGI children: {}", self.executable_path));

        // Allocate a port if we don't have one
        if self.assigned_port.is_none() {
            self.assigned_port = self.port_manager.allocate_port("php-main-process".to_string()).await;
            if self.assigned_port.is_none() {
                return Err("Failed to allocate port for PHP-CGI process".to_string());
            }
        }

        let port = self.assigned_port.unwrap();
        let mut cmd = Command::new(&self.executable_path);

        if cfg!(target_os = "windows") {
            // For Windows, use php-cgi.exe in FastCGI mode with children
            cmd.arg("-b").arg(format!("127.0.0.1:{}", port));

            // Set environment variable for FastCGI children
            cmd.env("PHP_FCGI_CHILDREN", self.concurrent_threads.to_string());
            cmd.env("PHP_FCGI_MAX_REQUESTS", "10000");

            // Set any extra environment variables
            for (key, value) in &self.extra_environment {
                cmd.env(key, value);
            }
        }

        match cmd.spawn() {
            Ok(child) => {
                self.process = Some(child);
                self.restart_count += 1;
                self.last_activity = Instant::now();
                trace(format!("PHP-CGI process started successfully on port {} (restart count: {})", port, self.restart_count));
                Ok(())
            }
            Err(e) => {
                error(format!("Failed to start PHP-CGI process: {}", e));
                // Release the port if process failed to start
                if let Some(port) = self.assigned_port {
                    self.port_manager.release_port(port).await;
                    self.assigned_port = None;
                }
                Err(format!("Failed to start PHP-CGI: {}", e))
            }
        }
    }

    async fn is_alive(&mut self) -> bool {
        if let Some(ref mut process) = self.process {
            match process.try_wait() {
                Ok(Some(_)) => {
                    warn("PHP-CGI process has exited".to_string());
                    self.process = None;
                    false
                }
                Ok(None) => true, // Process is still running
                Err(e) => {
                    error(format!("Error checking PHP-CGI process status: {}", e));
                    self.process = None;
                    false
                }
            }
        } else {
            false
        }
    }

    async fn send_keep_alive(&mut self) -> bool {
        if let Some(port) = self.assigned_port {
            let ip_and_port = format!("127.0.0.1:{}", port);
            match self.send_fastcgi_keep_alive(&ip_and_port).await {
                Ok(_) => {
                    self.last_activity = Instant::now();
                    true
                }
                Err(e) => {
                    error(format!("Keep-alive FastCGI request failed: {}", e));
                    false
                }
            }
        } else {
            false
        }
    }

    async fn send_fastcgi_keep_alive(&self, ip_and_port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Connect to the FastCGI server with a short timeout
        let stream = tokio::time::timeout(Duration::from_secs(2), tokio::net::TcpStream::connect(ip_and_port)).await??;

        // Send a minimal FastCGI request just to test connectivity
        let mut stream = stream;
        let begin_request = Self::create_fastcgi_begin_request();
        stream.write_all(&begin_request).await?;

        // Send empty params to signal end
        let empty_params = Self::create_fastcgi_params(&HashMap::new());
        stream.write_all(&empty_params).await?;

        // Send empty stdin to signal end
        let empty_stdin = Self::create_fastcgi_stdin(&[]);
        stream.write_all(&empty_stdin).await?;

        // Try to read a small response (don't need to parse it fully)
        let mut buffer = [0u8; 64];
        tokio::time::timeout(Duration::from_secs(1), stream.read(&mut buffer)).await??;

        Ok(())
    }

    async fn ensure_running(&mut self) -> Result<(), String> {
        if !self.is_alive().await {
            warn("PHP-CGI process is not running, restarting...".to_string());
            // Wait a bit before restarting to avoid rapid restart loops
            tokio::time::sleep(Duration::from_millis(1000)).await;
            self.start().await?;
        } else {
            // Check if we need to send a keep-alive
            let time_since_activity = self.last_activity.elapsed();
            if time_since_activity >= Duration::from_secs(10) {
                if !self.send_keep_alive().await {
                    warn("Keep-alive failed, restarting PHP-CGI process".to_string());
                    self.stop().await;
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    self.start().await?;
                }
            }
        }
        Ok(())
    }

    async fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            trace("Stopping PHP-CGI process".to_string());
            if let Err(e) = process.kill().await {
                error(format!("Failed to kill PHP-CGI process: {}", e));
            }
        }

        // Release the assigned port
        if let Some(port) = self.assigned_port.take() {
            self.port_manager.release_port(port).await;
        }
    }

    fn get_port(&self) -> Option<u16> {
        self.assigned_port
    }

    fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    // Helper functions for FastCGI protocol (moved from main impl)
    fn create_fastcgi_begin_request() -> Vec<u8> {
        let mut packet = Vec::new();
        packet.push(1); // version
        packet.push(1); // type: FCGI_BEGIN_REQUEST
        packet.extend(&1u16.to_be_bytes()); // request_id
        packet.extend(&8u16.to_be_bytes()); // content_length
        packet.push(0); // padding_length
        packet.push(0); // reserved

        // FCGI_BEGIN_REQUEST body
        packet.extend(&1u16.to_be_bytes()); // role: FCGI_RESPONDER
        packet.push(0); // flags
        packet.extend(&[0; 5]); // reserved

        packet
    }

    fn create_fastcgi_params(params: &HashMap<String, String>) -> Vec<u8> {
        let mut content = Vec::new();

        for (key, value) in params {
            let key_bytes = key.as_bytes();
            let value_bytes = value.as_bytes();

            // Length of key
            if key_bytes.len() < 128 {
                content.push(key_bytes.len() as u8);
            } else {
                content.extend(&((key_bytes.len() as u32) | 0x80000000).to_be_bytes());
            }

            // Length of value
            if value_bytes.len() < 128 {
                content.push(value_bytes.len() as u8);
            } else {
                content.extend(&((value_bytes.len() as u32) | 0x80000000).to_be_bytes());
            }

            content.extend(key_bytes);
            content.extend(value_bytes);
        }

        let mut packet = Vec::new();
        packet.push(1); // version
        packet.push(4); // type: FCGI_PARAMS
        packet.extend(&1u16.to_be_bytes()); // request_id
        packet.extend(&(content.len() as u16).to_be_bytes()); // content_length
        packet.push(0); // padding_length
        packet.push(0); // reserved
        packet.extend(content);

        packet
    }

    fn create_fastcgi_stdin(data: &[u8]) -> Vec<u8> {
        let mut packet = Vec::new();
        packet.push(1); // version
        packet.push(5); // type: FCGI_STDIN
        packet.extend(&1u16.to_be_bytes()); // request_id
        packet.extend(&(data.len() as u16).to_be_bytes()); // content_length
        packet.push(0); // padding_length
        packet.push(0); // reserved
        packet.extend(data);

        packet
    }
}

/// PHP handler that manages a single persistent PHP-CGI process with FastCGI children.
///
/// This implementation:
/// - Starts and maintains a single php-cgi.exe process with PHP_FCGI_CHILDREN=10
/// - Monitors process health with keep-alive FastCGI requests every 10 seconds
/// - Automatically restarts the process if it doesn't respond to keep-alive
/// - Provides worker threads that handle requests through the single CGI process
/// - Uses the singleton port manager to assign a single port starting from 9000
/// - Limits concurrent connections to php-fmp based on concurrent_threads to prevent timeouts
pub struct PHPHandler {
    request_timeout: usize,
    ip_and_port: String,
    other_webroot: String,
    php_process: Arc<Mutex<PhpCgiProcess>>,
    cached_port: Arc<AtomicU16>,
    is_using_external_fastcgi: bool,
    connection_semaphore: Arc<Semaphore>,
}

impl PHPHandler {
    pub fn new(
        executable: String,
        ip_and_port: String,
        request_timeout: usize,
        concurrent_threads: usize,
        other_webroot: String,
        _extra_handler_config: Vec<(String, String)>,
        extra_environment: Vec<(String, String)>,
    ) -> Self {
        // Get the singleton port manager instance
        let port_manager = get_port_manager();

        // Initialize single PHP-CGI process (only used on Windows)
        let php_process = Arc::new(Mutex::new(PhpCgiProcess::new(executable, port_manager.clone(), concurrent_threads.clone(), extra_environment)));

        // On Windows, we can use internal php-cgi.exe processes
        // unless the user has specified an external FastCGI server
        // we prefer the external fastcgi, as it is more efficient than maintaining our own process
        let mut is_using_external_fastcgi = true;
        if cfg!(target_os = "windows") {
            if ip_and_port.is_empty() {
                is_using_external_fastcgi = false;
            }
        }

        trace(format!("PHP handler initialized with {} concurrent connection limit", concurrent_threads));

        PHPHandler {
            request_timeout,
            ip_and_port,
            other_webroot,
            php_process,
            cached_port: Arc::new(AtomicU16::new(0)),
            is_using_external_fastcgi,
            connection_semaphore: Arc::new(Semaphore::new(concurrent_threads)),
        }
    }

    fn is_fastcgi_response_complete(buffer: &[u8]) -> bool {
        // Check if we have received a complete FastCGI response stream:
        // 1. Find an FCGI_STDOUT record with contentLength = 0 (stream terminator)
        // 2. Followed by an FCGI_END_REQUEST record (type 3)
        let mut i = 0;
        let mut found_empty_stdout = false;

        while i + 8 <= buffer.len() {
            let version = buffer[i];
            let record_type = buffer[i + 1];
            let content_length = u16::from_be_bytes([buffer[i + 4], buffer[i + 5]]) as usize;
            let padding_length = buffer[i + 6] as usize;

            if version != 1 {
                break;
            }

            // Check for empty FCGI_STDOUT (type 6, length 0)
            if record_type == 6 && content_length == 0 {
                found_empty_stdout = true;
            }

            // Check for FCGI_END_REQUEST (type 3)
            if record_type == 3 && found_empty_stdout {
                return true;
            }

            // Move to next record
            i += 8 + content_length + padding_length;
        }

        false
    }

    pub fn parse_fastcgi_response(buffer: &[u8]) -> Vec<u8> {
        let mut response = Vec::new();
        let mut i = 0;
        let mut stdout_records = 0;

        while i + 8 <= buffer.len() {
            let version = buffer[i];
            let record_type = buffer[i + 1];
            let content_length = u16::from_be_bytes([buffer[i + 4], buffer[i + 5]]) as usize;
            let padding_length = buffer[i + 6] as usize;

            if version != 1 {
                trace(format!("Unexpected FastCGI version {} at offset {}, stopping parse", version, i));
                break;
            }

            let content_start = i + 8;
            let content_end = content_start + content_length;

            if content_end > buffer.len() {
                trace(format!(
                    "Incomplete FastCGI record at offset {}, expected {} bytes but only {} available",
                    i,
                    content_end - i,
                    buffer.len() - i
                ));
                break;
            }

            if record_type == 6 {
                // FCGI_STDOUT
                if content_length > 0 {
                    let content = &buffer[content_start..content_end];
                    response.extend_from_slice(content);
                    stdout_records += 1;
                    trace(format!(
                        "Parsed FCGI_STDOUT record #{} with {} bytes (total response: {} bytes)",
                        stdout_records,
                        content_length,
                        response.len()
                    ));
                } else {
                    trace("Received empty FCGI_STDOUT record (stream terminator)".to_string());
                }
            } else if record_type == 7 {
                // FCGI_STDERR - log errors
                if content_length > 0 {
                    let stderr_content = String::from_utf8_lossy(&buffer[content_start..content_end]);
                    error(format!("FastCGI STDERR: {}", stderr_content));
                }
            } else if record_type == 3 {
                // FCGI_END_REQUEST
                trace(format!("Received FCGI_END_REQUEST, parsed {} STDOUT records with total {} bytes", stdout_records, response.len()));
                break;
            }

            // Move to next record (header + content + padding)
            i = content_end + padding_length;
        }

        response
    }
}

impl ExternalRequestHandler for PHPHandler {
    fn start(&self) {
        // Start the single PHP-CGI process and monitoring
        let php_process = self.php_process.clone();

        let is_using_external_fastcgi = self.is_using_external_fastcgi;

        // Start the local PHP-CGI process if not using a external FastCGI
        if !is_using_external_fastcgi {
            let process_clone = php_process.clone();
            let cached_port_clone = self.cached_port.clone();

            tokio::spawn(async move {
                let mut process_guard = process_clone.lock().await;
                if let Err(e) = process_guard.start().await {
                    error(format!("Failed to start PHP-CGI process: {}", e));
                    return;
                }
                // Cache the port after successful start to avoid future mutex contention
                if let Some(port) = process_guard.get_port() {
                    cached_port_clone.store(port, Ordering::Relaxed);
                    trace(format!("PHP-CGI process started successfully on port {}", port));
                } else {
                    trace("PHP-CGI process started successfully".to_string());
                }
            });

            // Start the keep-alive monitoring task
            let process_clone = php_process.clone();
            tokio::spawn(async move {
                let triggers = get_trigger_handler();
                let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
                let stop_services_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

                loop {
                    select! {
                        _ = shutdown_token.cancelled() => {
                            trace("Shutdown signal received, stopping PHP processes if running".to_string());
                            let mut process_guard = process_clone.lock().await;
                            process_guard.stop().await;
                            break;
                        },
                        _ = stop_services_token.cancelled() => {
                            trace("Stop services signal received, stopping PHP processes if running".to_string());
                            let mut process_guard = process_clone.lock().await;
                            process_guard.stop().await;
                            break;
                        },
                        _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                            let mut process_guard = process_clone.lock().await;
                            if let Err(e) = process_guard.ensure_running().await {
                                error(format!("Failed to ensure PHP-CGI process is running: {}", e));
                            }
                        }
                    }
                }
            });
        }
    }

    fn stop(&self) {
        trace("Stopping PHP handler".to_string());
        let php_process = self.php_process.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                let mut process_guard = php_process.lock().await;
                process_guard.stop().await;
            });
        } else {
            // If no runtime available, attempt blocking stop (less ideal but functional)
            trace("No Tokio runtime available for async stop, attempting synchronous stop".to_string());
        }
    }

    fn get_file_matches(&self) -> Vec<String> {
        vec![".php".to_string()]
    }

    /// Handle an incoming HTTP request using a fully async approach.
    ///
    /// This method processes the request directly in the current async context:
    /// 1. Extract request data
    /// 2. Process the FastCGI request asynchronously
    /// 3. Return the HTTP response
    ///
    /// This eliminates all complex channel/spawning logic for maximum concurrency.
    async fn handle_request(
        &self,
        method: &hyper::Method,
        uri: &hyper::Uri,
        headers: &hyper::HeaderMap,
        body: &Vec<u8>,
        site: &Site,
        full_file_path: &String,
        uri_is_a_dir_with_index_file_inside: bool,
        remote_ip: &str,
        http_version: &String,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        // Clone the necessary data to avoid lifetime issues
        let method = method.clone();
        let uri = uri.clone();
        let site = site.clone();
        let full_file_path = full_file_path.clone();
        let http_version = http_version.clone();

        // Extract request data
        let method_str = method.to_string();
        let uri_str = uri.path_and_query().unwrap().as_str().to_string();
        let path = uri.path();

        trace(format!("PHP request body length: {} bytes", body.len()));

        // Make sure the web root is full path
        let full_web_root_result = get_full_file_path(&site.web_root);
        if let Err(e) = full_web_root_result {
            trace(format!("Error resolving file path for web root {}: {}", site.web_root, e));
            return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
        }
        let full_web_root = full_web_root_result.unwrap();

        // Get some info needed for the fastcgi params
        let is_https = if let Some(scheme) = uri.scheme_str() { scheme.eq_ignore_ascii_case("https") } else { false };

        // Get server port from ip_and_port if possible
        let server_port = if let Some(colon_pos) = self.ip_and_port.rfind(':') {
            if let Ok(port) = self.ip_and_port[colon_pos + 1..].parse::<u16>() { port } else { 80 }
        } else {
            80
        };

        // Get the IP and port for FastCGI
        let mut ip_and_port = self.ip_and_port.clone();

        // If we are not using external FastCGI, get the port from atomic cache
        if !self.is_using_external_fastcgi {
            let port = self.cached_port.load(Ordering::Relaxed);

            if port != 0 {
                ip_and_port = format!("127.0.0.1:{}", port);

                // Update activity in a separate non-blocking task (optional, non-critical)
                let process_clone_for_activity = self.php_process.clone();
                if let Ok(handle) = tokio::runtime::Handle::try_current() {
                    handle.spawn(async move {
                        if let Ok(mut process_guard) = process_clone_for_activity.try_lock() {
                            process_guard.update_activity();
                        }
                    });
                }
                // If no runtime, skip activity update (it's optional)
            } else {
                // Port not cached yet, we need to get it from the process (this should be rare)
                // This also handles the case where the process wasn't started during initialization
                let port = {
                    let mut process_guard = self.php_process.lock().await;

                    // Check if process is running, if not, start it
                    let mut port = process_guard.get_port();
                    if port.is_none() {
                        trace("PHP-CGI process not running, starting it now...".to_string());
                        if let Err(e) = process_guard.start().await {
                            error(format!("Failed to start PHP-CGI process on first request: {}", e));
                            return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
                        }
                        port = process_guard.get_port();
                    }

                    // Cache the port for future requests
                    if let Some(p) = port {
                        self.cached_port.store(p, Ordering::Relaxed);
                        trace(format!("PHP-CGI process port {} cached for future requests", p));
                    }
                    port
                };

                if let Some(port) = port {
                    ip_and_port = format!("127.0.0.1:{}", port);
                } else {
                    error("PHP-CGI process does not have a valid port even after attempting to start it".to_string());
                    return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
                }
            }
        }

        // Process the FastCGI request with timeout
        match tokio::time::timeout(
            Duration::from_secs(self.request_timeout as u64),
            self.process_fastcgi_request_direct(
                method_str,
                uri_str,
                path.to_string(),
                &headers,
                body,
                full_file_path.clone(),
                full_web_root,
                uri_is_a_dir_with_index_file_inside,
                is_https,
                remote_ip,
                server_port,
                http_version.clone(),
                ip_and_port,
                self.other_webroot.clone(),
            ),
        )
        .await
        {
            Ok(response) => {
                trace("PHP Request completed successfully".to_string());
                Ok(response)
            }
            Err(_) => {
                error("PHP Request timed out".to_string());
                Ok(empty_response_with_status(hyper::StatusCode::GATEWAY_TIMEOUT))
            }
        }
    }

    // Return the handle type identifier
    fn get_handler_type(&self) -> String {
        "php".to_string()
    }
}

impl PHPHandler {
    /// Process FastCGI request directly without any channels or complex async spawning
    async fn process_fastcgi_request_direct(
        &self,
        method: String,
        uri: String,
        _path: String,
        headers: &HeaderMap,
        body: &Vec<u8>,
        script_file: String,
        local_web_root: String,
        uri_is_a_dir_with_index_file_inside: bool,
        is_https: bool,
        remote_ip: &str,
        server_port: u16,
        http_version: String,
        ip_and_port: String,
        other_webroot: String,
    ) -> Response<BoxBody<Bytes, hyper::Error>> {
        // Check if the script file actually exists
        if let Ok(metadata) = std::fs::metadata(&script_file) {
            if !metadata.is_file() {
                error(format!("PHP script path exists but is not a file: {}", script_file));
                return empty_response_with_status(hyper::StatusCode::NOT_FOUND);
            }
        } else {
            error(format!("PHP script file does not exist: {}", script_file));
            return empty_response_with_status(hyper::StatusCode::NOT_FOUND);
        }

        // Generate the fastcgi params, so we are ready to send the request
        let params_result = generate_fast_cgi_params(
            &method,
            &uri,
            headers,
            &body,
            &script_file,
            &local_web_root,
            uri_is_a_dir_with_index_file_inside,
            &other_webroot,
            is_https,
            &remote_ip,
            &server_port,
            &http_version,
        );
        let params = match params_result {
            Ok(p) => p,
            Err(_) => {
                return empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR);
            }
        };

        let available_permits = self.connection_semaphore.available_permits();
        trace(format!("Acquiring connection permit for FastCGI server at {} (available permits: {})", ip_and_port, available_permits));

        // Acquire a connection permit to limit concurrent connections to php-fmp
        let _permit = match self.connection_semaphore.acquire().await {
            Ok(permit) => {
                trace(format!(
                    "Connection permit acquired for FastCGI server (remaining permits: {})",
                    self.connection_semaphore.available_permits()
                ));
                permit
            }
            Err(e) => {
                error(format!("Failed to acquire connection permit for FastCGI server: {}", e));
                return empty_response_with_status(hyper::StatusCode::SERVICE_UNAVAILABLE);
            }
        };

        trace(format!("Connecting to FastCGI server at {}", ip_and_port));

        // Connect to the FastCGI server
        let mut stream = match tokio::net::TcpStream::connect(&ip_and_port).await {
            Ok(stream) => stream,
            Err(e) => {
                error(format!("Failed to connect to FastCGI server {}: {}", ip_and_port, e));
                return empty_response_with_status(hyper::StatusCode::BAD_GATEWAY);
            }
        };

        // Send FastCGI request
        trace(format!("Sending FastCGI request... with parameters: {:?}", params));
        let start_time = Instant::now();

        // Send BEGIN_REQUEST
        let begin_request = PhpCgiProcess::create_fastcgi_begin_request();
        if let Err(e) = stream.write_all(&begin_request).await {
            error(format!("Failed to send BEGIN_REQUEST: {}", e));
            return empty_response_with_status(hyper::StatusCode::BAD_GATEWAY);
        }

        // Send parameters
        let params_data = PhpCgiProcess::create_fastcgi_params(&params);
        if let Err(e) = stream.write_all(&params_data).await {
            error(format!("Failed to send PARAMS: {}", e));
            return empty_response_with_status(hyper::StatusCode::BAD_GATEWAY);
        }

        // Send empty params to signal end
        let empty_params = PhpCgiProcess::create_fastcgi_params(&HashMap::new());
        if let Err(e) = stream.write_all(&empty_params).await {
            error(format!("Failed to send empty params: {}", e));
            return empty_response_with_status(hyper::StatusCode::BAD_GATEWAY);
        }

        // Send body if present
        if !body.is_empty() {
            let stdin_data = PhpCgiProcess::create_fastcgi_stdin(&body);
            if let Err(e) = stream.write_all(&stdin_data).await {
                error(format!("Failed to send STDIN: {}", e));
                return empty_response_with_status(hyper::StatusCode::BAD_GATEWAY);
            }
        }

        // Send empty stdin to signal end
        let empty_stdin = PhpCgiProcess::create_fastcgi_stdin(&[]);
        if let Err(e) = stream.write_all(&empty_stdin).await {
            error(format!("Failed to send empty stdin: {}", e));
            return empty_response_with_status(hyper::StatusCode::BAD_GATEWAY);
        }

        // Read response
        trace("Reading FastCGI response...".to_string());
        let mut response_buffer = Vec::new();
        // Use 65535 byte buffer to match FastCGI max record size (FCGI_MAX_LENGTH)
        let mut buffer = vec![0u8; 65535];

        // Read with timeout
        let timeout_duration = Duration::from_secs(30);
        match tokio::time::timeout(timeout_duration, async {
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => {
                        trace("FastCGI connection closed by server".to_string());
                        break; // Connection closed
                    }
                    Ok(n) => {
                        trace(format!("Read {} bytes from FastCGI stream (total: {} bytes)", n, response_buffer.len() + n));
                        response_buffer.extend_from_slice(&buffer[..n]);

                        // Check for complete response (empty STDOUT + END_REQUEST)
                        if Self::is_fastcgi_response_complete(&response_buffer) {
                            trace(format!("FastCGI response complete, total size: {} bytes", response_buffer.len()));
                            break;
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            Ok::<(), std::io::Error>(())
        })
        .await
        {
            Ok(_) => {}
            Err(_) => {
                error(format!("FastCGI response timeout after reading {} bytes", response_buffer.len()));
                return empty_response_with_status(hyper::StatusCode::GATEWAY_TIMEOUT);
            }
        }

        // Parse FastCGI response and extract HTTP response
        let http_response_bytes = Self::parse_fastcgi_response(&response_buffer);

        if http_response_bytes.is_empty() {
            error("Empty response from PHP-CGI process".to_string());
            return empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR);
        }

        // Find the end of headers to separate headers from body
        let (headers_bytes, body_bytes) = if let Some(pos) = http_response_bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            let split_pos = pos + 4;
            (&http_response_bytes[..pos], &http_response_bytes[split_pos..])
        } else if let Some(pos) = http_response_bytes.windows(2).position(|w| w == b"\n\n") {
            let split_pos = pos + 2;
            (&http_response_bytes[..pos], &http_response_bytes[split_pos..])
        } else {
            // No headers separator found, treat entire response as body
            (&[][..], &http_response_bytes[..])
        };

        // Convert headers to string for parsing (headers should always be valid UTF-8)
        let headers_part = String::from_utf8_lossy(headers_bytes).to_string();

        // Build HTTP response
        let mut response_builder = hyper::Response::builder();
        let mut status_code = hyper::StatusCode::OK;

        // Parse headers
        for line in headers_part.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(colon_pos) = line.find(':') {
                let (key, value) = line.split_at(colon_pos);
                let value = value[1..].trim(); // Remove colon and trim

                if key.eq_ignore_ascii_case("status") {
                    // Parse status code
                    if let Some(space_pos) = value.find(' ') {
                        if let Ok(code) = value[..space_pos].parse::<u16>() {
                            if let Ok(status) = hyper::StatusCode::from_u16(code) {
                                status_code = status;
                            }
                        }
                    }
                } else {
                    // Add other headers
                    if let Ok(header_name) = hyper::header::HeaderName::from_bytes(key.as_bytes()) {
                        if let Ok(header_value) = hyper::header::HeaderValue::from_str(&value) {
                            response_builder = response_builder.header(header_name, header_value);
                        }
                    }
                }
            }
        }

        // Build the final response with binary body
        match response_builder.status(status_code).body(full(body_bytes.to_vec())) {
            Ok(response) => {
                let end_time = Instant::now();
                let duration = end_time - start_time;
                trace(format!("FastCGI response parsed successfully in {:?}", duration));

                // _permit will be automatically dropped here, releasing the semaphore permit
                trace(format!(
                    "Connection permit will be released (available permits after release: {})",
                    self.connection_semaphore.available_permits() + 1
                ));
                response
            }
            Err(e) => {
                error(format!("Failed to build HTTP response: {}", e));
                empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
            }
        }
    }
}

fn generate_fast_cgi_params(
    method: &String,
    uri: &String,
    headers: &HeaderMap,
    body: &Vec<u8>,
    script_file: &String,
    local_web_root: &String,
    uri_is_a_dir_with_index_file_inside: bool,
    other_webroot: &String,
    is_https: bool,
    remote_ip: &str,
    server_port: &u16,
    http_version: &String,
) -> Result<HashMap<String, String>, ()> {
    let mut params: HashMap<String, String> = HashMap::new();

    // Add HTTP headers as CGI variables, prefixed with HTTP_ and uppercased
    for (key, value) in headers.iter() {
        let key_str = key.to_string();

        // Try converting the value to a &str
        if let Ok(value_str) = value.to_str() {
            let key_str = format!("HTTP_{}", key_str.replace("-", "_").to_uppercase());
            params.insert(key_str, value_str.to_string());
        }
    }

    // Set content type and length if present
    if let Some(content_type) = headers.get("content-type") {
        if let Ok(content_type) = content_type.to_str() {
            params.insert("CONTENT_TYPE".to_string(), content_type.to_string());
        }
    }
    if let Some(content_length) = headers.get("content-length") {
        if let Ok(content_length) = content_length.to_str() {
            params.insert("CONTENT_LENGTH".to_string(), content_length.to_string());
        }
    }

    // Parse the URI to get script name and query string
    let uri_parts: Vec<&str> = uri.splitn(2, '?').collect();
    let query_string = if uri_parts.len() > 1 { uri_parts[1] } else { "" };

    // Get the hostname that is requested
    let requested_hostname = headers.get(":authority").or_else(|| headers.get("Host")).and_then(|h| h.to_str().ok()).unwrap_or("").to_string();

    // Handle web root mapping
    let mut full_script_path = script_file.clone();
    let mut script_web_root = local_web_root.clone();

    if !other_webroot.is_empty() {
        let full_local_web_root_result = get_full_file_path(&local_web_root);
        if let Err(e) = full_local_web_root_result {
            trace(format!("Error resolving file path for local web root {}: {}", local_web_root, e));
            return Err(());
        }
        let full_local_web_root = full_local_web_root_result.unwrap();
        full_script_path = replace_web_root_in_path(&full_script_path, &full_local_web_root, &other_webroot);
        script_web_root = other_webroot.clone();
    }

    let (directory, filename) = split_path(&script_web_root, &full_script_path);

    // Request uri
    let mut request_uri = uri.clone();
    if uri_is_a_dir_with_index_file_inside {
        // Split off any query string first
        let (path_only_str, query_part) = if let Some(pos) = uri.find('?') { (&uri[..pos], &uri[pos..]) } else { (uri.as_str(), "") };
        // Add forward slash to the end if missing, but before any query string
        let path_only = if !path_only_str.ends_with('/') {
            format!("{}/", path_only_str)
        } else {
            path_only_str.to_string()
        };
        request_uri = format!("{}{}", path_only, query_part);
    }

    // Figure out PATH_INFO
    let path_info = compute_path_info(&request_uri, &filename);

    trace(format!("PHP FastCGI - Directory: {}, Filename: {}", directory, filename));

    // Build FastCGI parameters (CGI environment variables)
    params.insert("REQUEST_METHOD".to_string(), method.clone());
    params.insert("REQUEST_URI".to_string(), request_uri.clone());
    params.insert("SCRIPT_NAME".to_string(), request_uri);
    params.insert("SCRIPT_FILENAME".to_string(), full_script_path);
    params.insert("DOCUMENT_ROOT".to_string(), script_web_root);
    params.insert("QUERY_STRING".to_string(), query_string.to_string());
    params.insert("CONTENT_LENGTH".to_string(), body.len().to_string());
    params.insert("SERVER_SOFTWARE".to_string(), "Grux".to_string());
    params.insert("SERVER_NAME".to_string(), requested_hostname.clone());
    params.insert("SERVER_PORT".to_string(), server_port.to_string());
    params.insert("HTTPS".to_string(), if is_https { "on" } else { "off" }.to_string());
    params.insert("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string());
    params.insert("SERVER_PROTOCOL".to_string(), http_version.clone());
    params.insert("REMOTE_ADDR".to_string(), remote_ip.to_string());
    params.insert("REMOTE_HOST".to_string(), "".to_string());
    params.insert("PATH_INFO".to_string(), path_info);
    params.insert("REDIRECT_STATUS".to_string(), "200".to_string());
    params.insert("HTTP_HOST".to_string(), requested_hostname);

    Ok(params)
}

/// Compute PATH_INFO for a request given REQUEST_URI and SCRIPT_NAME
///
/// # Arguments
/// * `request_uri` - full URI path from the client, e.g., "/wp-admin/foo/bar"
/// * `script_name` - the SCRIPT_NAME being executed, e.g., "/wp-admin/index.php"
///
/// # Returns
/// PATH_INFO string (empty if no extra path)
fn compute_path_info(request_uri: &str, script_name: &str) -> String {
    // Strip query string if present
    let path_only = match request_uri.find('?') {
        Some(pos) => &request_uri[..pos],
        None => request_uri,
    };

    if path_only.starts_with(script_name) {
        let path_info = &path_only[script_name.len()..];
        if path_info.is_empty() {
            "".to_string()
        } else {
            if path_info.starts_with('/') { path_info.to_string() } else { format!("/{}", path_info) }
        }
    } else {
        // If REQUEST_URI does not start with SCRIPT_NAME, PATH_INFO is empty
        "".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_info() {
        assert_eq!(compute_path_info("/wp-admin", "/wp-admin/index.php"), "");
        assert_eq!(compute_path_info("/wp-admin/", "/wp-admin/index.php"), "");
        assert_eq!(compute_path_info("/wp-admin/index.php", "/wp-admin/index.php"), "");
        assert_eq!(compute_path_info("/wp-admin/index.php/foo", "/wp-admin/index.php"), "/foo");
        assert_eq!(compute_path_info("/wp-admin/index.php/foo/bar", "/wp-admin/index.php"), "/foo/bar");
        assert_eq!(compute_path_info("/wp-admin/abc/def", "/wp-admin/index.php"), ""); // Does not start with script
        assert_eq!(compute_path_info("/wp-admin/index.phpfoo", "/wp-admin/index.php"), "/foo");
        assert_eq!(compute_path_info("/wp-admin/index.php?x=1", "/wp-admin/index.php"), "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_generate_fastcgi_params() {
        // Try with scenario where user requests the root
        let params_result = generate_fast_cgi_params(
            &"GET".to_string(),
            &"/".to_string(),
            &HeaderMap::new(),
            &vec![],
            &"D:/old-d/websites/wpsynchro1/public/index.php".to_string(),
            &"D:/old-d/websites/wpsynchro1/public".to_string(),
            false,
            &"".to_string(),
            false,
            "127.0.0.1",
            &80,
            &"HTTP/1.1".to_string(),
        );

        assert!(params_result.is_ok());
        let params = params_result.unwrap();

        assert_eq!(params.get("REQUEST_METHOD").unwrap(), "GET");
        assert_eq!(params.get("REQUEST_URI").unwrap(), "/");
        assert_eq!(params.get("SCRIPT_NAME").unwrap(), "/");
        assert_eq!(params.get("SCRIPT_FILENAME").unwrap(), "D:/old-d/websites/wpsynchro1/public/index.php");
        assert_eq!(params.get("DOCUMENT_ROOT").unwrap(), "D:/old-d/websites/wpsynchro1/public");
        assert_eq!(params.get("PATH_INFO").unwrap(), "");

        // Try with scenario where user requests a sub-path /wp-admin/
        let params_result = generate_fast_cgi_params(
            &"GET".to_string(),
            &"/wp-admin/".to_string(),
            &HeaderMap::new(),
            &vec![],
            &"D:/old-d/websites/wpsynchro1/public/wp-admin/index.php".to_string(),
            &"D:/old-d/websites/wpsynchro1/public".to_string(),
            true,
            &"".to_string(),
            false,
            "127.0.0.1",
            &80,
            &"HTTP/1.1".to_string(),
        );

        assert!(params_result.is_ok());
        let params = params_result.unwrap();

        assert_eq!(params.get("REQUEST_METHOD").unwrap(), "GET");
        assert_eq!(params.get("REQUEST_URI").unwrap(), "/wp-admin/");
        assert_eq!(params.get("SCRIPT_NAME").unwrap(), "/wp-admin/");
        assert_eq!(params.get("SCRIPT_FILENAME").unwrap(), "D:/old-d/websites/wpsynchro1/public/wp-admin/index.php");
        assert_eq!(params.get("DOCUMENT_ROOT").unwrap(), "D:/old-d/websites/wpsynchro1/public");
        assert_eq!(params.get("PATH_INFO").unwrap(), "");

        // Try with scenario where user requests a sub-path and specific file /wp-admin/index.php
        let params_result = generate_fast_cgi_params(
            &"GET".to_string(),
            &"/wp-admin/index.php".to_string(),
            &HeaderMap::new(),
            &vec![],
            &"D:/old-d/websites/wpsynchro1/public/wp-admin/index.php".to_string(),
            &"D:/old-d/websites/wpsynchro1/public".to_string(),
            false,
            &"".to_string(),
            false,
            "127.0.0.1",
            &80,
            &"HTTP/1.1".to_string(),
        );

        assert!(params_result.is_ok());
        let params = params_result.unwrap();

        assert_eq!(params.get("REQUEST_METHOD").unwrap(), "GET");
        assert_eq!(params.get("REQUEST_URI").unwrap(), "/wp-admin/index.php");
        assert_eq!(params.get("SCRIPT_NAME").unwrap(), "/wp-admin/index.php");
        assert_eq!(params.get("SCRIPT_FILENAME").unwrap(), "D:/old-d/websites/wpsynchro1/public/wp-admin/index.php");
        assert_eq!(params.get("DOCUMENT_ROOT").unwrap(), "D:/old-d/websites/wpsynchro1/public");
        assert_eq!(params.get("PATH_INFO").unwrap(), "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_php_handler_creation() {
        let handler = PHPHandler::new("php-cgi.exe".to_string(), "127.0.0.1:9000".to_string(), 30, 2, "./www-default".to_string(), vec![], vec![]);

        assert_eq!(handler.get_handler_type(), "php");
        assert_eq!(handler.get_file_matches(), vec![".php".to_string()]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_php_handler_with_single_process() {
        let handler = PHPHandler::new(
            "echo".to_string(), // Use 'echo' as a test executable
            "".to_string(),     // Empty string means use internal PHP-CGI process
            30,
            2,
            "./www-default".to_string(),
            vec![],
            vec![],
        );

        // Test that handler can be created and will use single internal process
        assert_eq!(handler.get_handler_type(), "php");
        assert_eq!(handler.get_file_matches(), vec![".php".to_string()]);

        // Start and stop the handler (internal process management is now hidden)
        handler.start();
        handler.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_php_handler_lifecycle() {
        let handler = PHPHandler::new(
            "echo".to_string(), // Use 'echo' as a test executable
            "127.0.0.1:9000".to_string(),
            30,
            1,
            "./www-default".to_string(),
            vec![],
            vec![],
        );

        // Test that we can call start and stop methods
        handler.start();
        handler.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_php_handler_concurrent_processing() {
        let handler = PHPHandler::new("echo".to_string(), "127.0.0.1:9000".to_string(), 30, 3, "./www-default".to_string(), vec![], vec![]);

        // Start and stop the handler
        handler.start();
        handler.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_fastcgi_binary_response_parsing() {
        // Test that the parse_fastcgi_response function correctly handles binary data
        // This simulates a FastCGI response with binary content in the body

        // Create a mock FastCGI STDOUT record with binary data
        let mut fastcgi_response = Vec::new();

        // FastCGI header: version=1, type=6 (STDOUT), request_id=1, content_length, padding=0, reserved=0
        let binary_content = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD]; // Some binary data
        let headers = b"Content-Type: application/octet-stream\r\n\r\n";
        let full_content = [headers.as_slice(), &binary_content].concat();

        fastcgi_response.push(1); // version
        fastcgi_response.push(6); // type: FCGI_STDOUT
        fastcgi_response.extend(&1u16.to_be_bytes()); // request_id
        fastcgi_response.extend(&(full_content.len() as u16).to_be_bytes()); // content_length
        fastcgi_response.push(0); // padding_length
        fastcgi_response.push(0); // reserved
        fastcgi_response.extend(&full_content);

        // Add FCGI_END_REQUEST record
        fastcgi_response.push(1); // version
        fastcgi_response.push(3); // type: FCGI_END_REQUEST
        fastcgi_response.extend(&1u16.to_be_bytes()); // request_id
        fastcgi_response.extend(&8u16.to_be_bytes()); // content_length (8 bytes for end request)
        fastcgi_response.push(0); // padding_length
        fastcgi_response.push(0); // reserved
        fastcgi_response.extend(&[0u8; 8]); // end request body

        // Parse the response using our updated function
        let parsed_response = PHPHandler::parse_fastcgi_response(&fastcgi_response);

        // Verify the binary data is preserved
        assert!(parsed_response.len() > 0);
        assert!(parsed_response.windows(binary_content.len()).any(|w| w == binary_content.as_slice()));
    }
}
