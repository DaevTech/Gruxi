use crate::error;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::FastCgiError;
use crate::http::http_util::full;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_request_processor_data::GruxiRequestProcessorData::PhpProcessorFastCgi;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::trace;
use std::time::Instant;
use std::{collections::HashMap, time::Duration};
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub struct FastCgi;

impl Default for FastCgi {
    fn default() -> Self {
        Self::new()
    }
}

impl FastCgi {
    pub fn new() -> Self {
        FastCgi
    }

    pub async fn send_fastcgi_keep_alive(ip_and_port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    // Helper functions for FastCGI protocol (moved from main impl)
    pub fn create_fastcgi_begin_request() -> Vec<u8> {
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

    /// Maximum content length per FastCGI record (FCGI_MAX_LENGTH)
    const FCGI_MAX_CONTENT_LEN: usize = 65535;

    pub fn create_fastcgi_params(params: &HashMap<String, String>) -> Vec<u8> {
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

        // Chunk content into records of at most FCGI_MAX_CONTENT_LEN bytes
        Self::create_fastcgi_records(4, &content)
    }

    pub fn create_fastcgi_stdin(data: &[u8]) -> Vec<u8> {
        // Chunk data into records of at most FCGI_MAX_CONTENT_LEN bytes
        Self::create_fastcgi_records(5, data)
    }

    /// Create one or more FastCGI records of the given type, chunking data
    /// into records of at most FCGI_MAX_CONTENT_LEN (65535) bytes each.
    fn create_fastcgi_records(record_type: u8, data: &[u8]) -> Vec<u8> {
        let mut packet = Vec::new();

        if data.is_empty() {
            // Empty record (used as stream terminator)
            packet.push(1); // version
            packet.push(record_type);
            packet.extend(&1u16.to_be_bytes()); // request_id
            packet.extend(&0u16.to_be_bytes()); // content_length = 0
            packet.push(0); // padding_length
            packet.push(0); // reserved
            return packet;
        }

        for chunk in data.chunks(Self::FCGI_MAX_CONTENT_LEN) {
            packet.push(1); // version
            packet.push(record_type);
            packet.extend(&1u16.to_be_bytes()); // request_id
            packet.extend(&(chunk.len() as u16).to_be_bytes()); // content_length
            packet.push(0); // padding_length
            packet.push(0); // reserved
            packet.extend(chunk);
        }

        packet
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
                trace!("Unexpected FastCGI version {} at offset {}, stopping parse", version, i);
                break;
            }

            let content_start = i + 8;
            let content_end = content_start + content_length;

            if content_end > buffer.len() {
                trace!("Incomplete FastCGI record at offset {}, expected {} bytes but only {} available", i, content_end - i, buffer.len() - i);
                break;
            }

            if record_type == 6 {
                // FCGI_STDOUT
                if content_length > 0 {
                    let content = &buffer[content_start..content_end];
                    response.extend_from_slice(content);
                    stdout_records += 1;
                    trace!("Parsed FCGI_STDOUT record #{} with {} bytes (total response: {} bytes)", stdout_records, content_length, response.len());
                } else {
                    trace!("Received empty FCGI_STDOUT record (stream terminator)");
                }
            } else if record_type == 7 {
                // FCGI_STDERR - log errors
                if content_length > 0 {
                    let stderr_content = String::from_utf8_lossy(&buffer[content_start..content_end]);
                    error!("FastCGI STDERR: {}", stderr_content);
                }
            } else if record_type == 3 {
                // FCGI_END_REQUEST
                trace!("Received FCGI_END_REQUEST, parsed {} STDOUT records with total {} bytes", stdout_records, response.len());
                break;
            }

            // Move to next record (header + content + padding)
            i = content_end + padding_length;
        }

        response
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

    pub async fn process_fastcgi_request(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, FastCgiError> {
        // Generate FastCGI parameters
        let params_result = Self::generate_fast_cgi_params(gruxi_request);
        let params = match params_result {
            Ok(p) => p,
            Err(_) => {
                error!("Failed to generate FastCGI parameters from request {:?}", gruxi_request);
                return Err(FastCgiError::Initialization);
            }
        };
        trace!("Generated FastCGI parameters: {:?}", params);

        let connect_ip_and_port = match gruxi_request.get_processor_data().expect("Processor data should be available for FastCgi processing") {
            PhpProcessorFastCgi(data) => data.connect_ip_and_port.clone(),
        };

        // Now we work on getting a semaphore permit for the connection, if relevant
        let connection_semaphore_option = gruxi_request.get_connection_semaphore();

        match connection_semaphore_option {
            Some(connection_semaphore) => {
                // We only need a permit, if a connection semaphore is set
                let available_permits = connection_semaphore.available_permits();
                trace!("Acquiring connection permit for FastCGI server at {} (available permits: {})", &connect_ip_and_port, available_permits);

                // Acquire a connection permit to limit concurrent connections to php-fpm
                let _permit = match connection_semaphore.acquire().await {
                    Ok(permit) => {
                        trace!("Connection permit acquired for FastCGI server (remaining permits: {})", connection_semaphore.available_permits());
                        permit
                    }
                    Err(e) => {
                        error!("Failed to acquire connection permit for FastCGI server: {}", e);
                        return Err(FastCgiError::ConnectionPermitAcquisition);
                    }
                };
                Self::do_fastcgi_request_and_response(gruxi_request, &connect_ip_and_port, &params).await
            }
            None => Self::do_fastcgi_request_and_response(gruxi_request, &connect_ip_and_port, &params).await,
        }
    }

    pub async fn do_fastcgi_request_and_response(gruxi_request: &mut GruxiRequest, ip_and_port: &str, params: &HashMap<String, String>) -> Result<GruxiResponse, FastCgiError> {
        trace!("Connecting to FastCGI server at {}", ip_and_port);

        // Connect to the FastCGI server
        let mut stream = match tokio::net::TcpStream::connect(&ip_and_port).await {
            Ok(stream) => stream,
            Err(e) => {
                error!("FastCGI Error: Failed to connect to FastCGI server {}: {}", ip_and_port, e);
                return Err(FastCgiError::Connection(e));
            }
        };

        // Send FastCGI request
        trace!("Sending FastCGI request... with parameters: {:?}", params);
        let start_time = Instant::now();

        // Send BEGIN_REQUEST
        let begin_request = Self::create_fastcgi_begin_request();
        if let Err(e) = stream.write_all(&begin_request).await {
            error!("FastCGI Error: Failed to send BEGIN_REQUEST: {}", e);
            return Err(FastCgiError::Communication(e));
        }

        // Send parameters
        let params_data = Self::create_fastcgi_params(params);
        if let Err(e) = stream.write_all(&params_data).await {
            error!("FastCGI Error: Failed to send PARAMS: {}", e);
            return Err(FastCgiError::Communication(e));
        }

        // Send empty params to signal end
        let empty_params = Self::create_fastcgi_params(&HashMap::new());
        if let Err(e) = stream.write_all(&empty_params).await {
            error!("FastCGI Error: Failed to send empty params: {}", e);
            return Err(FastCgiError::Communication(e));
        }

        // Send body if present
        let body_bytes = gruxi_request.get_body_bytes().await;
        if !body_bytes.is_empty() {
            let stdin_data = Self::create_fastcgi_stdin(&body_bytes);
            if let Err(e) = stream.write_all(&stdin_data).await {
                error!("FastCGI Error: Failed to send STDIN: {}", e);
                return Err(FastCgiError::Communication(e));
            }
        }

        // Send empty stdin to signal end
        let empty_stdin = Self::create_fastcgi_stdin(&[]);
        if let Err(e) = stream.write_all(&empty_stdin).await {
            error!("FastCGI Error: Failed to send empty stdin: {}", e);
            return Err(FastCgiError::Communication(e));
        }

        // Read response
        trace!("Reading FastCGI response...");
        let mut response_buffer = Vec::new();
        // Use 65535 byte buffer to match FastCGI max record size (FCGI_MAX_LENGTH)
        let mut buffer = vec![0u8; 65535];

        // Read with timeout
        let timeout_duration = Duration::from_secs(30);
        match tokio::time::timeout(timeout_duration, async {
            loop {
                match stream.read(&mut buffer).await {
                    Ok(0) => {
                        trace!("FastCGI connection closed by server");
                        break; // Connection closed
                    }
                    Ok(n) => {
                        trace!("Read {} bytes from FastCGI stream (total: {} bytes)", n, response_buffer.len() + n);
                        response_buffer.extend_from_slice(&buffer[..n]);

                        // Check for complete response (empty STDOUT + END_REQUEST)
                        if Self::is_fastcgi_response_complete(&response_buffer) {
                            trace!("FastCGI response complete, total size: {} bytes", response_buffer.len());
                            break;
                        }
                    }
                    Err(e) => {
                        return Err(FastCgiError::Communication(e));
                    }
                }
            }
            Ok::<(), FastCgiError>(())
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return Err(e);
            }
            Err(_) => {
                error!("FastCGI response timeout after reading {} bytes", response_buffer.len());
                return Err(FastCgiError::Timeout);
            }
        }

        // Parse FastCGI response and extract HTTP response
        let http_response_bytes = Self::parse_fastcgi_response(&response_buffer);
        if http_response_bytes.is_empty() {
            error!("FastCGI - Empty response from PHP-CGI process");
            return Err(FastCgiError::InvalidResponse);
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
                    // Parse status code (handles both "404 Not Found" and "404")
                    let code_str = match value.find(' ') {
                        Some(space_pos) => &value[..space_pos],
                        None => value,
                    };
                    if let Ok(code) = code_str.parse::<u16>()
                        && let Ok(status) = hyper::StatusCode::from_u16(code)
                    {
                        status_code = status;
                    }
                } else {
                    // Add other headers
                    if let Ok(header_name) = hyper::header::HeaderName::from_bytes(key.as_bytes())
                        && let Ok(header_value) = hyper::header::HeaderValue::from_str(value)
                    {
                        response_builder = response_builder.header(header_name, header_value);
                    }
                }
            }
        }

        // Build the final response with binary body
        match response_builder.status(status_code).body(full(body_bytes.to_vec())) {
            Ok(response) => {
                let end_time = Instant::now();
                let duration = end_time - start_time;
                trace!("FastCGI response parsed successfully in {:?}", duration);
                Ok(GruxiResponse::from_hyper_bytes(response).await)
            }
            Err(e) => {
                error!("FastCGI - Failed to build HTTP response: {}", e);
                Err(FastCgiError::InvalidResponse)
            }
        }
    }

    pub fn generate_fast_cgi_params(gruxi_request: &mut GruxiRequest) -> Result<HashMap<String, String>, GruxiError> {
        let mut params: HashMap<String, String> = HashMap::new();

        let path = gruxi_request.get_path().to_string();
        let query = gruxi_request.get_query().to_string();
        let uri = if query.is_empty() { path.clone() } else { format!("{}?{}", path, query) };
        let headers = gruxi_request.get_headers();

        // Add HTTP headers as CGI variables, prefixed with HTTP_ and uppercased
        for (key, value) in headers.iter() {
            let key_str = key.to_string();
            // Skip content-type and content-length as they have special handling in CGI
            if key_str.eq_ignore_ascii_case("content-type") || key_str.eq_ignore_ascii_case("content-length") {
                continue;
            }

            // Try converting the value to a &str
            if let Ok(value_str) = value.to_str() {
                let key_str = format!("HTTP_{}", key_str.replace("-", "_").to_uppercase());
                params.insert(key_str, value_str.to_string());
            }
        }

        // Set content type if present (from header)
        if let Some(content_type) = headers.get("content-type")
            && let Ok(content_type) = content_type.to_str()
        {
            params.insert("CONTENT_TYPE".to_string(), content_type.to_string());
        }

        // Handle web root mapping
        let (mut full_script_path, fastcgi_web_root, uri_is_a_dir_with_index_file_inside, server_software_spoof) =
            match gruxi_request.get_processor_data().expect("Processor data should be available for generating FastCgi parameters") {
                PhpProcessorFastCgi(data) => (data.script_file.clone(), data.fastcgi_web_root.clone(), data.uri_is_a_dir_with_index_file_inside, data.server_software_spoof.clone()),
            };

        // If the fastcgi web root is set, we need to replace the local web root in the script path with the fastcgi web root
        if !fastcgi_web_root.is_empty() {
            full_script_path.set_web_root(fastcgi_web_root.get_web_root())?;
        }

        // Request uri (must include query string for PHP's $_SERVER['REQUEST_URI'])
        let (path_only_str, query_part) = if let Some(pos) = uri.find('?') { (&uri[..pos], &uri[pos..]) } else { (uri.as_str(), "") };
        let mut request_uri = uri.clone();
        if uri_is_a_dir_with_index_file_inside {
            // Add forward slash to the end if missing, but before any query string
            let path_only = if !path_only_str.ends_with('/') { &format!("{}/", path_only_str) } else { path_only_str };
            request_uri = format!("{}{}", path_only, query_part);
        }

        let mut server_software = server_software_spoof;
        if server_software.is_empty() {
            server_software = "Gruxi".to_string();
        }

        // Figure out PATH_INFO
        let path_info = Self::compute_path_info(&request_uri, full_script_path.get_path());

        trace!("FastCGI - Directory: {}, Filename: {}", full_script_path.get_web_root(), full_script_path.get_path());

        // Build FastCGI parameters (CGI environment variables)
        let script_name = full_script_path.get_path().to_string();

        // REMOTE_ADDR must be IP only (no port)
        let remote_addr = match gruxi_request.get_remote_ip() {
            Some(addr) => addr.ip().to_string(),
            None => String::new(),
        };
        let remote_port = match gruxi_request.get_remote_ip() {
            Some(addr) => addr.port().to_string(),
            None => String::new(),
        };

        params.insert("REQUEST_METHOD".to_string(), gruxi_request.get_http_method().to_string());
        params.insert("DOCUMENT_URI".to_string(), path_only_str.to_string());
        params.insert("REQUEST_URI".to_string(), request_uri);
        params.insert("SCRIPT_NAME".to_string(), script_name);
        params.insert("SCRIPT_FILENAME".to_string(), full_script_path.get_full_path().to_string());
        params.insert("DOCUMENT_ROOT".to_string(), full_script_path.get_web_root().to_string());
        params.insert("QUERY_STRING".to_string(), gruxi_request.get_query().to_string());
        params.insert("CONTENT_LENGTH".to_string(), gruxi_request.get_body_size().to_string());
        params.insert("SERVER_SOFTWARE".to_string(), server_software);
        params.insert("SERVER_NAME".to_string(), gruxi_request.get_hostname().to_string());
        params.insert("SERVER_PORT".to_string(), gruxi_request.get_server_port().to_string());
        params.insert("HTTPS".to_string(), if gruxi_request.is_https() { "on" } else { "off" }.to_string());
        params.insert("GATEWAY_INTERFACE".to_string(), "CGI/1.1".to_string());
        params.insert("SERVER_PROTOCOL".to_string(), gruxi_request.get_http_version().to_string());
        params.insert("REMOTE_ADDR".to_string(), remote_addr);
        params.insert("REMOTE_PORT".to_string(), remote_port);
        params.insert("REMOTE_HOST".to_string(), "".to_string());
        params.insert("PATH_INFO".to_string(), path_info);
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

        if let Some(path_info) = path_only.strip_prefix(script_name) {
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
}

#[cfg(test)]
mod tests {
    use hyper::body::Bytes;

    use crate::{file::normalized_path::NormalizedPath, http::request_response::{gruxi_request::GruxiRequest, gruxi_request_processor_data::{GruxiRequestProcessorData, PhpProcessorFastCgiData}}};

    use super::FastCgi;

    #[test]
    fn test_path_info() {
        assert_eq!(FastCgi::compute_path_info("/wp-admin", "/wp-admin/index.php"), "");
        assert_eq!(FastCgi::compute_path_info("/wp-admin/", "/wp-admin/index.php"), "");
        assert_eq!(FastCgi::compute_path_info("/wp-admin/index.php", "/wp-admin/index.php"), "");
        assert_eq!(FastCgi::compute_path_info("/wp-admin/index.php/foo", "/wp-admin/index.php"), "/foo");
        assert_eq!(FastCgi::compute_path_info("/wp-admin/index.php/foo/bar", "/wp-admin/index.php"), "/foo/bar");
        assert_eq!(FastCgi::compute_path_info("/wp-admin/abc/def", "/wp-admin/index.php"), ""); // Does not start with script
        assert_eq!(FastCgi::compute_path_info("/wp-admin/index.phpfoo", "/wp-admin/index.php"), "/foo");
        assert_eq!(FastCgi::compute_path_info("/wp-admin/index.php?x=1", "/wp-admin/index.php"), "");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_generate_fastcgi_params() {
        // Try with scenario where user requests the root
        let request = hyper::Request::builder().method("GET").uri("/").header("Host", "localhost").body(Bytes::new()).unwrap();
        let mut gruxi_request = GruxiRequest::new(request);

        let processor_data = PhpProcessorFastCgiData {
            connect_ip_and_port: String::new(),
            script_file: NormalizedPath::new("D:/websites/test1/public", "/index.php", true).unwrap(),
            uri_is_a_dir_with_index_file_inside: false,
            local_web_root: NormalizedPath::new("D:/websites/test1/public", "", true).unwrap(),
            fastcgi_web_root: NormalizedPath::new("", "", true).unwrap(),
            server_software_spoof: String::new(),
        };

        gruxi_request.set_processor_data(GruxiRequestProcessorData::PhpProcessorFastCgi(processor_data));

        let params_result = FastCgi::generate_fast_cgi_params(&mut gruxi_request);

        assert!(params_result.is_ok());
        let params = params_result.unwrap();

        assert_eq!(params.get("REQUEST_METHOD").unwrap(), "GET");
        assert_eq!(params.get("REQUEST_URI").unwrap(), "/");
        assert_eq!(params.get("DOCUMENT_URI").unwrap(), "/");
        assert_eq!(params.get("SCRIPT_NAME").unwrap(), "/index.php");
        assert_eq!(params.get("SCRIPT_FILENAME").unwrap(), "D:/websites/test1/public/index.php");
        assert_eq!(params.get("DOCUMENT_ROOT").unwrap(), "D:/websites/test1/public");
        assert_eq!(params.get("PATH_INFO").unwrap(), "");
        // Verify HTTP_CONTENT_TYPE and HTTP_CONTENT_LENGTH are NOT present
        assert!(params.get("HTTP_CONTENT_TYPE").is_none());
        assert!(params.get("HTTP_CONTENT_LENGTH").is_none());
        // Verify REMOTE_ADDR has no port
        let remote_addr = params.get("REMOTE_ADDR").unwrap();
        assert!(!remote_addr.contains(':') || remote_addr.contains("::")); // IPv4 has no colon, IPv6 is fine
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_generate_fastcgi_params_with_path_and_query_string() {
        // Try with scenario where user requests a path with query string
        let request = hyper::Request::builder().method("GET").uri("/my-site/index.php?x=1").header("Host", "localhost").body(Bytes::new()).unwrap();
        let mut gruxi_request = GruxiRequest::new(request);

        let processor_data = PhpProcessorFastCgiData {
            connect_ip_and_port: String::new(),
            script_file: NormalizedPath::new("D:/websites/test1/public", "/my-site/index.php", true).unwrap(),
            uri_is_a_dir_with_index_file_inside: false,
            local_web_root: NormalizedPath::new("D:/websites/test1/public", "", true).unwrap(),
            fastcgi_web_root: NormalizedPath::new("", "", true).unwrap(),
            server_software_spoof: String::new(),
        };

        gruxi_request.set_processor_data(GruxiRequestProcessorData::PhpProcessorFastCgi(processor_data));

        let params_result = FastCgi::generate_fast_cgi_params(&mut gruxi_request);

        assert!(params_result.is_ok());
        let params = params_result.unwrap();

        assert_eq!(params.get("REQUEST_METHOD").unwrap(), "GET");
        assert_eq!(params.get("REQUEST_URI").unwrap(), "/my-site/index.php?x=1");
        assert_eq!(params.get("DOCUMENT_URI").unwrap(), "/my-site/index.php");
        assert_eq!(params.get("SCRIPT_NAME").unwrap(), "/my-site/index.php");
        assert_eq!(params.get("SCRIPT_FILENAME").unwrap(), "D:/websites/test1/public/my-site/index.php");
        assert_eq!(params.get("DOCUMENT_ROOT").unwrap(), "D:/websites/test1/public");
        assert_eq!(params.get("PATH_INFO").unwrap(), "");
        // Verify HTTP_CONTENT_TYPE and HTTP_CONTENT_LENGTH are NOT present
        assert!(params.get("HTTP_CONTENT_TYPE").is_none());
        assert!(params.get("HTTP_CONTENT_LENGTH").is_none());
        // Verify REMOTE_ADDR has no port
        let remote_addr = params.get("REMOTE_ADDR").unwrap();
        assert!(!remote_addr.contains(':') || remote_addr.contains("::")); // IPv4 has no colon, IPv6 is fine
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
        let parsed_response = FastCgi::parse_fastcgi_response(&fastcgi_response);

        // Verify the binary data is preserved
        assert!(parsed_response.len() > 0);
        assert!(parsed_response.windows(binary_content.len()).any(|w| w == binary_content.as_slice()));
    }
}
