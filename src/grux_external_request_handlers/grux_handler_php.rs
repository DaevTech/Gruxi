use crate::grux_configuration_struct::Site;
use crate::grux_external_request_handlers::ExternalRequestHandler;
use crate::grux_file_util::{get_full_file_path, replace_web_root_in_path, split_path};
use crate::grux_http_util::*;
use crate::grux_port_manager::PortManager;
use http_body_util::combinators::BoxBody;
use log::{error, trace, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

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
    last_activity: Instant,
    extra_environment: Vec<(String, String)>,
}

impl PhpCgiProcess {
    fn new(executable_path: String, port_manager: PortManager, extra_environment: Vec<(String, String)>) -> Self {
        PhpCgiProcess {
            process: None,
            executable_path,
            restart_count: 0,
            assigned_port: None,
            port_manager,
            last_activity: Instant::now(),
            extra_environment,
        }
    }

    async fn start(&mut self) -> Result<(), String> {
        trace!("Starting PHP-CGI process with FastCGI children: {}", self.executable_path);

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

            // Fetch the CPU count to set children accordingly
            let mut cpus = num_cpus::get_physical();
            if cpus > 10 {
                cpus = 10;
            }
            if cpus < 1 {
                cpus = 1;
            }

            // Set environment variable for FastCGI children
            cmd.env("PHP_FCGI_CHILDREN", cpus.to_string());
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
                trace!("PHP-CGI process started successfully on port {} (restart count: {})", port, self.restart_count);
                Ok(())
            }
            Err(e) => {
                error!("Failed to start PHP-CGI process: {}", e);
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
                    warn!("PHP-CGI process has exited");
                    self.process = None;
                    false
                }
                Ok(None) => true, // Process is still running
                Err(e) => {
                    error!("Error checking PHP-CGI process status: {}", e);
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
                    error!("Keep-alive FastCGI request failed: {}", e);
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
        let empty_params = Self::create_fastcgi_params(&[]);
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
            warn!("PHP-CGI process is not running, restarting...");
            // Wait a bit before restarting to avoid rapid restart loops
            tokio::time::sleep(Duration::from_millis(1000)).await;
            self.start().await?;
        } else {
            // Check if we need to send a keep-alive
            let time_since_activity = self.last_activity.elapsed();
            if time_since_activity >= Duration::from_secs(10) {
                if !self.send_keep_alive().await {
                    warn!("Keep-alive failed, restarting PHP-CGI process");
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
            trace!("Stopping PHP-CGI process");
            if let Err(e) = process.kill().await {
                error!("Failed to kill PHP-CGI process: {}", e);
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

    fn create_fastcgi_params(params: &[(String, String)]) -> Vec<u8> {
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

/// Represents a request to be processed by a PHP worker
///
/// This structure contains all the data needed to process an HTTP request
/// through PHP-CGI, including the method, URI, headers, body, and a channel
/// for sending back the response.
#[derive(Debug)]
struct PHPRequest {
    script_file: String,
    local_web_root: String,
    cgi_web_root: String,
    method: String,
    uri: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    response_tx: oneshot::Sender<hyper::Response<BoxBody<hyper::body::Bytes, hyper::Error>>>,
    is_https: bool,
    remote_ip: String,
    server_port: u16,
    http_version: String,
}

/// PHP handler that manages a single persistent PHP-CGI process with FastCGI children.
///
/// This implementation:
/// - Starts and maintains a single php-cgi.exe process with PHP_FCGI_CHILDREN=10
/// - Monitors process health with keep-alive FastCGI requests every 10 seconds
/// - Automatically restarts the process if it doesn't respond to keep-alive
/// - Provides worker threads that handle requests through the single CGI process
/// - Uses the singleton port manager to assign a single port starting from 9000
pub struct PHPHandler {
    request_queue_tx: mpsc::Sender<PHPRequest>,
    request_queue_rx: Arc<Mutex<mpsc::Receiver<PHPRequest>>>,
    tokio_runtime: tokio::runtime::Runtime,
    request_timeout: usize,
    concurrent_threads: usize,
    ip_and_port: String,
    other_webroot: String,
    php_process: Arc<Mutex<PhpCgiProcess>>,
    is_using_external_fastcgi: bool,
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
        // Initialize PHP threads
        let (request_queue_tx, rx) = mpsc::channel::<PHPRequest>(1000);

        // Shared receiver
        let request_queue_rx = Arc::new(Mutex::new(rx));
        let tokio_runtime = Runtime::new().expect("Failed to create thread runtime for PHP handler");

        // Get the singleton port manager instance
        let port_manager = PortManager::instance();

        // Initialize single PHP-CGI process (only used on Windows)
        let php_process = Arc::new(Mutex::new(PhpCgiProcess::new(executable.clone(), port_manager.clone(), extra_environment.clone())));

        // On Windows, we can use internal php-cgi.exe processes
        // unless the user has specified an external FastCGI server
        // we prefer the external fastcgi, as it is more efficient than maintaining our own process
        let mut is_using_external_fastcgi = true;
        if cfg!(target_os = "windows") {
            if ip_and_port.is_empty() {
                is_using_external_fastcgi = false;
            }
        }

        // Determine the concurrent threads we want to spawn to handle requests
        let concurrent_threads = if concurrent_threads == 0 {
            let cpus = num_cpus::get_physical();
            cpus
        } else if concurrent_threads < 1 {
            1
        } else {
            concurrent_threads
        };

        PHPHandler {
            request_queue_tx,
            request_queue_rx,
            tokio_runtime,
            request_timeout,
            concurrent_threads,
            ip_and_port,
            other_webroot,
            php_process,
            is_using_external_fastcgi,
        }
    }

    /// Handle a FastCGI request to the PHP process
    ///
    /// This method implements a manual FastCGI protocol client to communicate
    /// with the php-cgi.exe process. It:
    /// 1. Connects to the FastCGI server via TCP
    /// 2. Sends BEGIN_REQUEST, PARAMS, and STDIN records
    /// 3. Reads the response containing STDOUT records
    /// 4. Parses the HTTP response and converts it to a Hyper response
    /// 5. Sends the response back through the oneshot channel
    async fn handle_fastcgi_request(php_request: PHPRequest, ip_and_port: String) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Connect to the FastCGI server
        let mut stream = tokio::net::TcpStream::connect(&ip_and_port).await?;

        // Parse the URI to get script name and query string
        let uri_parts: Vec<&str> = php_request.uri.splitn(2, '?').collect();
        let script_name = uri_parts[0];
        let query_string = if uri_parts.len() > 1 { uri_parts[1] } else { "" };

        // Pass the script file path, to get the directory and filename, and get the cgi web root
        let mut full_script_path = php_request.script_file.clone();
        let mut script_web_root = php_request.local_web_root.clone();

        if !php_request.cgi_web_root.is_empty() {
            // we need to full local web root, so we can replace to full path
            let full_local_web_root_result = get_full_file_path(&php_request.local_web_root);

            if let Err(e) = full_local_web_root_result {
                trace!("Error resolving file path for local web root {}: {}", php_request.local_web_root, e);
                // Return error
                let _ = php_request.response_tx.send(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
                return Ok(());
            }
            let full_local_web_root = full_local_web_root_result.unwrap();

            full_script_path = replace_web_root_in_path(&full_script_path, &full_local_web_root, &php_request.cgi_web_root);
            script_web_root = php_request.cgi_web_root.clone();
        }

        let (directory, filename) = split_path(&script_web_root, &full_script_path);

        trace!("PHP FastCGI - Directory: {}, Filename: {}", directory, filename);

        // Check if the script file actually exists
        if let Ok(metadata) = std::fs::metadata(&php_request.script_file) {
            if metadata.is_file() {
                trace!("PHP script file exists: {}", php_request.script_file);
            } else {
                error!("PHP script path exists but is not a file: {}", php_request.script_file);
                let _ = php_request.response_tx.send(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
                return Ok(());
            }
        } else {
            error!("PHP script file does not exist: {}", php_request.script_file);
            let _ = php_request.response_tx.send(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
            return Ok(());
        }

        // Build FastCGI parameters (CGI environment variables)
        let mut params: Vec<(String, String)> = Vec::new();
        params.push(("REQUEST_METHOD".to_string(), php_request.method.clone()));
        params.push(("REQUEST_URI".to_string(), php_request.uri.clone()));
        params.push(("SCRIPT_NAME".to_string(), script_name.to_string()));
        params.push(("SCRIPT_FILENAME".to_string(), full_script_path.to_string()));
        params.push(("DOCUMENT_ROOT".to_string(), script_web_root));
        params.push(("QUERY_STRING".to_string(), query_string.to_string()));
        params.push(("CONTENT_LENGTH".to_string(), php_request.body.len().to_string()));
        params.push(("SERVER_SOFTWARE".to_string(), "Grux".to_string()));
        params.push(("SERVER_NAME".to_string(), "".to_string()));
        params.push(("SERVER_PORT".to_string(), php_request.server_port.to_string()));
        params.push(("HTTPS".to_string(), if php_request.is_https { "on" } else { "off" }.to_string()));
        params.push(("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string()));
        params.push(("SERVER_PROTOCOL".to_string(), php_request.http_version.to_string()));
        params.push(("REMOTE_ADDR".to_string(), php_request.remote_ip));
        params.push(("REMOTE_HOST".to_string(), "".to_string()));

        // Additional important FastCGI variables for PHP
        params.push(("PATH_INFO".to_string(), php_request.path));
        params.push(("REDIRECT_STATUS".to_string(), "200".to_string())); // Important for PHP-CGI security

        // Add HTTP headers as CGI variables
        for (key, value) in &php_request.headers {
            let cgi_key = format!("HTTP_{}", key.to_uppercase().replace('-', "_"));
            params.push((cgi_key, value.clone()));
        }

        // Set content type if present
        if let Some(content_type) = php_request.headers.get("content-type") {
            params.push(("CONTENT_TYPE".to_string(), content_type.clone()));
        }

        // Log all FastCGI parameters for debugging
        trace!("FastCGI parameters being sent:");
        let mut fastcgi_params = String::new();
        for (key, value) in &params {
            fastcgi_params.push_str(&format!("{}={}  ", key, value));
        }
        trace!("{}", fastcgi_params);

        // Send a basic FastCGI BEGIN_REQUEST
        let begin_request = PhpCgiProcess::create_fastcgi_begin_request();
        stream.write_all(&begin_request).await?;

        // Send parameters
        let params_data = PhpCgiProcess::create_fastcgi_params(&params);
        stream.write_all(&params_data).await?;

        // Send empty params to signal end
        let empty_params = PhpCgiProcess::create_fastcgi_params(&[]);
        stream.write_all(&empty_params).await?;

        // Send body if present
        if !php_request.body.is_empty() {
            let stdin_data = PhpCgiProcess::create_fastcgi_stdin(&php_request.body);
            stream.write_all(&stdin_data).await?;
        }

        // Send empty stdin to signal end
        let empty_stdin = PhpCgiProcess::create_fastcgi_stdin(&[]);
        stream.write_all(&empty_stdin).await?;

        // Read response
        let mut response_buffer = Vec::new();
        let mut buffer = [0u8; 4096];

        // Read with timeout
        let timeout_duration = Duration::from_secs(30);
        match tokio::time::timeout(timeout_duration, async {
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => break, // Connection closed
                    Ok(n) => response_buffer.extend_from_slice(&buffer[..n]),
                    Err(e) => return Err(e),
                }
                // Simple check for end of FastCGI response
                if response_buffer.len() > 8 && Self::is_fastcgi_response_complete(&response_buffer) {
                    break;
                }
            }
            Ok::<(), std::io::Error>(())
        })
        .await
        {
            Ok(_) => {}
            Err(_) => return Err("FastCGI request timeout".into()),
        }

        // Parse FastCGI response and extract HTTP response
        let http_response = Self::parse_fastcgi_response(&response_buffer);

        if http_response.trim().is_empty() {
            error!("Empty response from PHP-CGI process");
            let _ = php_request.response_tx.send(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
            return Ok(());
        }

        // Parse the HTTP response
        let (headers_part, body_part) = if let Some(pos) = http_response.find("\r\n\r\n") {
            let (h, b) = http_response.split_at(pos + 4);
            (h.to_string(), b.to_string())
        } else if let Some(pos) = http_response.find("\n\n") {
            let (h, b) = http_response.split_at(pos + 2);
            (h.to_string(), b.to_string())
        } else {
            ("".to_string(), http_response)
        };

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
                            status_code = hyper::StatusCode::from_u16(code).unwrap_or(hyper::StatusCode::OK);
                        }
                    }
                } else {
                    response_builder = response_builder.header(key, value);
                }
            }
        }

        // Build the final response
        let body_bytes = body_part.into_bytes();
        let response = response_builder
            .status(status_code)
            .body(full(body_bytes))
            .unwrap_or_else(|_| empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));

        // Send the response back through the channel
        let _ = php_request.response_tx.send(response);

        Ok(())
    }

    fn is_fastcgi_response_complete(buffer: &[u8]) -> bool {
        // Simple check: look for FCGI_END_REQUEST packet (type 3)
        let mut i = 0;
        while i + 8 <= buffer.len() {
            if buffer[i] == 1 && buffer[i + 1] == 3 {
                // version 1, type FCGI_END_REQUEST
                return true;
            }
            i += 1;
        }
        false
    }

    fn parse_fastcgi_response(buffer: &[u8]) -> String {
        let mut response = String::new();
        let mut i = 0;

        while i + 8 <= buffer.len() {
            let version = buffer[i];
            let record_type = buffer[i + 1];
            let content_length = u16::from_be_bytes([buffer[i + 4], buffer[i + 5]]) as usize;
            let padding_length = buffer[i + 6] as usize;

            if version != 1 {
                break;
            }

            let content_start = i + 8;
            let content_end = content_start + content_length;

            if content_end > buffer.len() {
                break;
            }

            if record_type == 6 {
                // FCGI_STDOUT
                let content = &buffer[content_start..content_end];
                response.push_str(&String::from_utf8_lossy(content));
            } else if record_type == 3 {
                // FCGI_END_REQUEST
                break;
            }

            i = content_end + padding_length;
        }

        response
    }
}

impl ExternalRequestHandler for PHPHandler {
    fn start(&self) {
        // Start the single PHP-CGI process and monitoring
        let php_process = self.php_process.clone();
        let enter_guard = self.tokio_runtime.enter();
        let is_using_external_fastcgi = self.is_using_external_fastcgi;
        let ip_and_port = self.ip_and_port.clone();

        // Start the local PHP-CGI process if not using a external FastCGI
        if !is_using_external_fastcgi {
            let process_clone = php_process.clone();
            tokio::spawn(async move {
                let mut process_guard = process_clone.lock().await;
                if let Err(e) = process_guard.start().await {
                    error!("Failed to start PHP-CGI process: {}", e);
                    return;
                }
                trace!("PHP-CGI process started successfully");
            });

            // Start the keep-alive monitoring task
            let process_clone = php_process.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    let mut process_guard = process_clone.lock().await;
                    if let Err(e) = process_guard.ensure_running().await {
                        error!("Failed to ensure PHP-CGI process is running: {}", e);
                    }
                }
            });
        }

        // Start PHP worker threads
        for worker_id in 0..self.concurrent_threads {
            let rx = self.request_queue_rx.clone();
            let process_clone = php_process.clone();
            let mut ip_and_port = ip_and_port.clone();

            tokio::spawn(async move {
                trace!("PHP worker thread {} started", worker_id);

                // Main request processing loop (no longer restart the process after max requests)
                loop {
                    // Lock the receiver and await one job
                    let mut rx_data = rx.lock().await;
                    match rx_data.recv().await {
                        Some(php_request) => {
                            drop(rx_data); // release lock early
                            trace!("PHP Worker {} got request: {} {}", worker_id, php_request.method, php_request.uri);

                            // If we are not using the external FastCGI, get the current port from the process
                            if !is_using_external_fastcgi {
                                let port = {
                                    let mut process_guard = process_clone.lock().await;
                                    let port = process_guard.get_port();
                                    process_guard.update_activity();
                                    port
                                };

                                if let Some(port) = port {
                                    ip_and_port = format!("127.0.0.1:{}", port);
                                } else {
                                    error!("PHP-CGI process does not have a valid port");
                                    let _ = php_request.response_tx.send(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
                                    continue;
                                }
                            }

                            match PHPHandler::handle_fastcgi_request(php_request, ip_and_port.clone()).await {
                                Ok(_) => {
                                    trace!("PHP request processed successfully for worker {}", worker_id);
                                }
                                Err(e) => {
                                    error!("Failed to process PHP request for worker {}: {}", worker_id, e);
                                }
                            }
                        }
                        None => {
                            // Channel closed, exit the loop
                            trace!("PHP worker {} request channel closed, exiting", worker_id);
                            break;
                        }
                    }
                }
            });
        }

        drop(enter_guard);
    }

    fn stop(&self) {
        trace!("Stopping PHP handler");
        let php_process = self.php_process.clone();
        self.tokio_runtime.spawn(async move {
            let mut process_guard = php_process.lock().await;
            process_guard.stop().await;
        });
    }

    fn get_file_matches(&self) -> Vec<String> {
        vec![".php".to_string()]
    }

    /// Handle an incoming HTTP request synchronously.
    ///
    /// This method bridges the gap between the synchronous ExternalRequestHandler trait
    /// and our asynchronous FastCGI implementation. Since we can't use `block_on` within
    /// an existing tokio runtime (which would cause a "runtime within runtime" panic),
    /// we use std::sync channels to communicate between the sync and async worlds:
    ///
    /// 1. Extract request data synchronously
    /// 2. Send the request to async worker threads via tokio channels
    /// 3. Spawn an async task in the existing runtime to wait for the response
    /// 4. Use std::sync channels to get the result back to the sync context
    /// 5. Return the HTTP response synchronously
    fn handle_request(
        &self,
        method: &hyper::Method,
        uri: &hyper::Uri,
        headers: &hyper::HeaderMap,
        body: Vec<u8>,
        site: &Site,
        full_file_path: &String,
        remote_ip: &String,
        http_version: &String,
    ) -> hyper::Response<BoxBody<hyper::body::Bytes, hyper::Error>> {
        trace!("PHP request received: {} {}", method, uri);

        // Extract request data
        let method_str = method.to_string();
        let uri_str = uri.to_string();
        let path = uri.path();

        // Convert headers to HashMap
        let mut headers_map = HashMap::new();
        for (key, value) in headers {
            if let Ok(value_str) = value.to_str() {
                headers_map.insert(key.to_string(), value_str.to_string());
            }
        }

        // Body is now provided as a parameter, extracted by the main request handler
        trace!("PHP handler received body of {} bytes", body.len());

        // Create a channel for the response
        let (response_tx, response_rx) = oneshot::channel();

        // Make sure the web root is full path
        let full_web_root_result = get_full_file_path(&site.web_root);
        if let Err(e) = full_web_root_result {
            trace!("Error resolving file path for web root {}: {}", site.web_root, e);
            return empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR);
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

        // Create the PHP request
        let php_request = PHPRequest {
            script_file: full_file_path.clone(),
            local_web_root: full_web_root,
            cgi_web_root: self.other_webroot.clone(),
            method: method_str,
            uri: uri_str,
            path: path.to_string(),
            headers: headers_map.clone(),
            body,
            response_tx,
            is_https,
            remote_ip: remote_ip.clone(),
            server_port,
            http_version: http_version.clone(),
        };

        // Send the request to the worker queue
        let sender = self.request_queue_tx.clone();
        if let Err(e) = sender.try_send(php_request) {
            error!("Failed to queue PHP request: {}", e);
            return empty_response_with_status(hyper::StatusCode::SERVICE_UNAVAILABLE);
        }

        // Use a blocking approach with a separate thread pool
        let timeout_duration = Duration::from_secs(self.request_timeout as u64);

        // Use std::sync channels to bridge async and sync worlds
        let (sync_tx, sync_rx) = std::sync::mpsc::channel();

        // Spawn the async work in the existing runtime using spawn_blocking
        let _handle = self.tokio_runtime.spawn(async move {
            let result = tokio::time::timeout(timeout_duration, response_rx).await;
            let _ = sync_tx.send(result);
        });

        // Wait for the result synchronously
        match sync_rx.recv_timeout(timeout_duration + Duration::from_secs(1)) {
            Ok(Ok(Ok(response))) => {
                trace!("PHP request processed successfully");
                response
            }
            Ok(Ok(Err(_))) => {
                error!("PHP request processing channel closed unexpectedly");
                empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
            }
            Ok(Err(_)) => {
                error!("PHP request processing timed out");
                empty_response_with_status(hyper::StatusCode::GATEWAY_TIMEOUT)
            }
            Err(_) => {
                error!("PHP request processing thread communication timed out");
                empty_response_with_status(hyper::StatusCode::GATEWAY_TIMEOUT)
            }
        }
    }
    fn get_handler_type(&self) -> String {
        "php".to_string()
    }
}
