use crate::grux_external_request_handlers::ExternalRequestHandler;
use crate::grux_external_request_handlers::grux_php_cgi_process::PhpCgiProcess;
use crate::grux_http_util::*;
use crate::grux_file_util::split_path;
use crate::grux_port_manager::PortManager;
use http_body_util::combinators::BoxBody;
use hyper::Request;
use log::{error, trace};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

/// Represents a request to be processed by a PHP worker
///
/// This structure contains all the data needed to process an HTTP request
/// through PHP-CGI, including the method, URI, headers, body, and a channel
/// for sending back the response.
#[derive(Debug)]
struct PHPRequest {
    script_file: String,
    cgi_web_root: String,
    method: String,
    uri: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
    response_tx: oneshot::Sender<hyper::Response<BoxBody<hyper::body::Bytes, hyper::Error>>>,
}

/// PHP handler that manages persistent PHP-CGI processes for handling PHP requests.
///
/// This implementation:
/// - Starts and maintains persistent php-cgi.exe processes on Windows
/// - Monitors process health and automatically restarts dead processes
/// - Provides worker threads that handle requests through the CGI processes
/// - Ensures thread-safe access to the PHP-CGI processes
/// - Uses the singleton port manager to assign unique ports to each process
pub struct PHPHandler {
    request_queue_tx: mpsc::Sender<PHPRequest>,
    request_queue_rx: Arc<Mutex<mpsc::Receiver<PHPRequest>>>,
    tokio_runtime: tokio::runtime::Runtime,
    request_timeout: usize,
    max_concurrent_requests: usize,
    executable: String,
    ip_and_port: String,
    other_webroot: String,
    extra_handler_config: Vec<(String, String)>,
    extra_environment: Vec<(String, String)>,
    php_processes: Arc<Mutex<Vec<Arc<Mutex<PhpCgiProcess>>>>>,
}

impl PHPHandler {
    pub fn new(
        executable: String,
        ip_and_port: String,
        request_timeout: usize,
        max_concurrent_requests: usize,
        other_webroot: String,
        extra_handler_config: Vec<(String, String)>,
        extra_environment: Vec<(String, String)>,
    ) -> Self {
        // Initialize PHP threads
        let (request_queue_tx, rx) = mpsc::channel::<PHPRequest>(1000);
        // Shared receiver
        let request_queue_rx = Arc::new(Mutex::new(rx));
        let tokio_runtime = Runtime::new().expect("Failed to create thread runtime for PHP handler");

        // Get the singleton port manager instance
        let port_manager = PortManager::instance();

        let mut php_processes = Vec::new();

        // Initialize PHP-CGI processes (only on Windows)
        if cfg!(target_os = "windows") {
            // Windows: use persistent php-cgi.exe processes
            for i in 0..max_concurrent_requests {
                let service_id = format!("php-worker-{}", i);
                let process = Arc::new(Mutex::new(PhpCgiProcess::new(executable.clone(), service_id, port_manager.clone())));
                php_processes.push(process);
            }
        }

        PHPHandler {
            request_queue_tx,
            request_queue_rx,
            tokio_runtime,
            request_timeout,
            max_concurrent_requests,
            executable,
            ip_and_port,
            other_webroot,
            extra_handler_config,
            extra_environment,
            php_processes: Arc::new(Mutex::new(php_processes)),
        }
    }

    /// Get the maximum number of concurrent requests this handler supports
    pub fn get_max_concurrent_requests(&self) -> usize {
        self.max_concurrent_requests
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

        // Pass the script file path, to get the directory and filename
        let (directory, filename) = split_path(&php_request.script_file);

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

        // Determine which web root to use for CGI (as it might be different than the site web root used by Grux, such as PHP-FPM in a Docker)
        let mut cgi_web_root = directory.clone();
        if !php_request.cgi_web_root.is_empty() {
            cgi_web_root = php_request.cgi_web_root.clone();
        }

        // Build FastCGI parameters (CGI environment variables)
        let mut params: Vec<(String, String)> = Vec::new();
        params.push(("REQUEST_METHOD".to_string(), php_request.method.clone()));
        params.push(("REQUEST_URI".to_string(), php_request.uri.clone()));
        params.push(("SCRIPT_NAME".to_string(), script_name.to_string()));
        params.push(("SCRIPT_FILENAME".to_string(), filename.to_string()));
        params.push(("DOCUMENT_ROOT".to_string(), cgi_web_root));
        params.push(("QUERY_STRING".to_string(), query_string.to_string()));
        params.push(("CONTENT_LENGTH".to_string(), php_request.body.len().to_string()));
        params.push(("SERVER_SOFTWARE".to_string(), "Grux".to_string()));
//        params.push(("SERVER_NAME".to_string(), "localhost".to_string()));
//        params.push(("SERVER_PORT".to_string(), "80".to_string()));
//        params.push(("HTTPS".to_string(), "".to_string()));
        params.push(("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string()));
        params.push(("SERVER_PROTOCOL".to_string(), "HTTP/1.1".to_string()));
 //       params.push(("REMOTE_ADDR".to_string(), "127.0.0.1".to_string()));
      //  params.push(("REMOTE_HOST".to_string(), "localhost".to_string()));

        // Additional important FastCGI variables for PHP
    //    params.push(("PATH_INFO".to_string(), "".to_string()));
//        params.push(("PATH_TRANSLATED".to_string(), full_file_path.clone()));
   //     params.push(("PATH_TRANSLATED".to_string(), "/var/www/html/index.php".to_string()));
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
        for (key, value) in &params {
            trace!("  {} = {}", key, value);
        }

        // Send a basic FastCGI BEGIN_REQUEST
        let begin_request = Self::create_fastcgi_begin_request();
        stream.write_all(&begin_request).await?;

        // Send parameters
        let params_data = Self::create_fastcgi_params(&params);
        stream.write_all(&params_data).await?;

        // Send empty params to signal end
        let empty_params = Self::create_fastcgi_params(&[]);
        stream.write_all(&empty_params).await?;

        // Send body if present
        if !php_request.body.is_empty() {
            let stdin_data = Self::create_fastcgi_stdin(&php_request.body);
            stream.write_all(&stdin_data).await?;
        }

        // Send empty stdin to signal end
        let empty_stdin = Self::create_fastcgi_stdin(&[]);
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

        trace!("FastCGI response received: {}", http_response);

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

    // Helper functions for FastCGI protocol
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
        // Start PHP worker threads
        let processes = self.php_processes.clone();

        for worker_id in 0..self.max_concurrent_requests {
            let rx = self.request_queue_rx.clone();
            let processes_clone = processes.clone();
            let ip_and_port = self.ip_and_port.clone(); // Clone needed field
            let enter_guard = self.tokio_runtime.enter();

            tokio::spawn(async move {
                trace!("PHP worker thread {} started", worker_id);

                #[cfg(target_os = "windows")]
                // Get the PHP process for this worker
                let process = {
                    let processes_guard = processes_clone.lock().await;
                    processes_guard[worker_id].clone()
                };

                #[cfg(target_os = "windows")]
                // Start the PHP-CGI process
                {
                    let mut process_guard = process.lock().await;
                    if let Err(e) = process_guard.start().await {
                        error!("Failed to start PHP-CGI for worker {}: {}", worker_id, e);
                        return;
                    }
                }

                #[cfg(target_os = "windows")]
                // Process health monitoring task
                let process_monitor = process.clone();

                #[cfg(target_os = "windows")]
                tokio::spawn(async move {
                    loop {
                        {
                            let mut process_guard = process_monitor.lock().await;
                            if let Err(e) = process_guard.ensure_running().await {
                                error!("Failed to ensure PHP-CGI process is running: {}", e);
                            }
                        }
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                });

                // Main request processing loop
                loop {
                    // Lock the receiver and await one job
                    let mut rx_data = rx.lock().await;
                    match rx_data.recv().await {
                        Some(php_request) => {
                            drop(rx_data); // release lock early
                            trace!("PHP Worker {} got request: {} {}", worker_id, php_request.method, php_request.uri);

                            #[cfg(target_os = "windows")]
                            {
                                 // Get the port from the process
                                let port = {
                                    let process_guard = process.lock().await;
                                    process_guard.get_port()
                                };
                                if Some(port).is_none() {
                                    error!("PHP-CGI process for worker {} does not have a valid port", worker_id);
                                    let _ = php_request.response_tx.send(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
                                    continue;
                                }
                                let ip_and_port = format!("127.0.0.1:{}", port.unwrap());
                                match PHPHandler::handle_fastcgi_request(php_request, ip_and_port).await {
                                    Ok(_) => {
                                        trace!("PHP request processed successfully for worker {}", worker_id);
                                    }
                                    Err(e) => {
                                        error!("Failed to process PHP request for worker {}: {}", worker_id, e);
                                    }
                                }
                            }

                            #[cfg(not(target_os = "windows"))]
                            {
                                 // On Linux/Unix, interact with php-fpm directly
                                let ip_and_port = ip_and_port.clone();

                                match PHPHandler::handle_fastcgi_request(php_request, ip_and_port).await {
                                    Ok(_) => {
                                        trace!("PHP request processed successfully for worker {}", worker_id);
                                    }
                                    Err(e) => {
                                        error!("Failed to process PHP request for worker {}: {}", worker_id, e);
                                    }
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

            drop(enter_guard);
        }
    }

    fn stop(&self) {
        trace!("Stopping PHP handler");
        let processes = self.php_processes.clone();
        self.tokio_runtime.spawn(async move {
            let processes_guard = processes.lock().await;
            for process in processes_guard.iter() {
                let mut process_guard = process.lock().await;
                process_guard.stop().await;
            }
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
    fn handle_request(&self, request: &Request<hyper::body::Incoming>, full_file_path: &String) -> hyper::Response<BoxBody<hyper::body::Bytes, hyper::Error>> {
        trace!("PHP request received: {} {}", request.method(), request.uri());

        // Extract request data
        let method = request.method().to_string();
        let uri = request.uri().to_string();

        // Convert headers to HashMap
        let mut headers = HashMap::new();
        for (key, value) in request.headers() {
            if let Ok(value_str) = value.to_str() {
                headers.insert(key.to_string(), value_str.to_string());
            }
        }

        // For now, we'll handle the body extraction as empty
        // In a complete implementation, you would need to handle this asynchronously
        // This is a limitation of the current sync interface
        let body = Vec::new(); // TODO: Extract body properly for POST requests

        // Create a channel for the response
        let (response_tx, response_rx) = oneshot::channel();

        // Create the PHP request
        let php_request = PHPRequest {
            script_file: full_file_path.clone(),
            cgi_web_root: self.other_webroot.clone(),
            method,
            uri,
            headers,
            body,
            response_tx,
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
