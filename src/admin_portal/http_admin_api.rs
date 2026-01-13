use crate::configuration::configuration::Configuration;
use crate::configuration::save_configuration::save_configuration;
use crate::configuration::site::Site;
use crate::core::admin_user::{LoginRequest, authenticate_user, create_session, invalidate_session, verify_session_token};
use crate::core::monitoring::get_monitoring_state;
use crate::core::operation_mode::{get_operation_mode_as_string, is_valid_operation_mode, set_new_operation_mode};
use crate::core::triggers::get_trigger_handler;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{AdminApiError, GruxiErrorKind};
use crate::http::http_util::{clean_url_path};
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::logging::syslog::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;
use std::path::Path;
use tokio_util::bytes;

pub async fn handle_api_routes(gruxi_request: &mut GruxiRequest, site: &Site) -> Result<GruxiResponse, GruxiError> {
    let path = gruxi_request.get_path();
    let path_cleaned = clean_url_path(&path);
    let method = gruxi_request.get_http_method();

    trace(format!("Handling request for admin portal with path: {}", path_cleaned));

    // We only want to handle a few paths in the admin portal
    let response_result = if path_cleaned == "login" && method == "POST" {
        handle_login_request(gruxi_request, site).await
    } else if path_cleaned == "logout" && method == "POST" {
        handle_logout_request(gruxi_request, site).await
    } else if path_cleaned == "config" && method == "GET" {
        admin_get_configuration_endpoint(gruxi_request, site).await
    } else if path_cleaned == "config" && method == "POST" {
        admin_post_configuration_endpoint(gruxi_request, site).await
    } else if path_cleaned == "monitoring" && method == "GET" {
        admin_monitoring_endpoint(gruxi_request, site).await
    } else if path_cleaned == "healthcheck" && method == "GET" {
        admin_healthcheck_endpoint(gruxi_request, site).await
    } else if (path_cleaned == "logs" || path_cleaned.starts_with("logs/")) && method == "GET" {
        admin_logs_endpoint(gruxi_request, site).await
    } else if (path_cleaned == "configuration/reload") && method == "POST" {
        admin_post_configuration_reload(gruxi_request, site).await
    } else if path_cleaned == "operation-mode" && method == "GET" {
        admin_get_operation_mode_endpoint(gruxi_request, site).await
    } else if path_cleaned == "operation-mode" && method == "POST" {
        admin_post_operation_mode_endpoint(gruxi_request, site).await
    } else {
        // If we reach here, no matching admin API route was found
        trace(format!("No matching admin API route found for path: {}", path_cleaned));
        Err(GruxiError::new_with_kind_only(GruxiErrorKind::AdminApi(AdminApiError::NoRouteMatched)))
    };

    response_result
}

pub async fn handle_login_request(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check if this is a POST request
    if gruxi_request.get_http_method() != "POST" {
        trace(format!("Login request attempted with invalid method: {}", gruxi_request.get_http_method()));
        let response = GruxiResponse::new_empty_with_status(hyper::StatusCode::METHOD_NOT_ALLOWED.as_u16());
        return Ok(response);
    }

    // Read the request body
    let body_bytes = gruxi_request.get_body_bytes().await;

    // Parse JSON body
    let login_request: LoginRequest = match serde_json::from_slice(&body_bytes) {
        Ok(req) => req,
        Err(e) => {
            error(format!("Failed to parse login request: {}", e));
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Invalid JSON format for login"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    debug(format!("Login attempt for username: {}", login_request.username));

    // Authenticate user
    let user = match authenticate_user(&login_request.username, &login_request.password) {
        Ok(Some(user)) => user,
        Ok(None) => {
            info(format!("Failed login attempt for username: {}", login_request.username));
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Invalid username or password"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(e) => {
            error(format!("Database error during authentication: {}", e));
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(r#"{"error": "Internal server error"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    // Create session
    let session = match create_session(&user) {
        Ok(session) => session,
        Err(e) => {
            error(format!("Failed to create session: {}", e));
            let return_message = r#"{"error": "Failed to create session"}"#;
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(return_message));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    info(format!("Successful login for user: {}", user.username));

    // Return success response with session token
    let response_json = serde_json::json!({
        "success": true,
        "message": "Login successful",
        "session_token": session.token,
        "username": session.username,
        "expires_at": session.expires_at.to_rfc3339()
    });

    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(response_json.to_string()));
    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
    return Ok(response);
}

pub async fn handle_logout_request(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check if this is a POST request
    if gruxi_request.get_http_method() != "POST" {
        trace(format!("Logout request with invalid method: {}", gruxi_request.get_http_method()));
        let response = GruxiResponse::new_empty_with_status(hyper::StatusCode::METHOD_NOT_ALLOWED.as_u16());
        return Ok(response);
    }

    // Get the session token from Authorization header or request body
    let token = get_session_token_from_request(&gruxi_request).await;

    if let Some(token) = token {
        match invalidate_session(&token) {
            Ok(true) => {
                info("Successfully logged out session".to_string());
                let response_json = serde_json::json!({
                    "success": true,
                    "message": "Logout successful"
                });
                let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(response_json.to_string()));
                response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Ok(response)
            }
            Ok(false) => {
                let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::NOT_FOUND.as_u16(), bytes::Bytes::from(r#"{"error": "Session not found"}"#));
                response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Ok(response)
            }
            Err(e) => {
                error(format!("Failed to logout session: {}", e));
                let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(r#"{"error": "Internal server error"}"#));
                response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Ok(response)
            }
        }
    } else {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(r#"{"error": "No session token provided"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        Ok(response)
    }
}

pub async fn admin_get_configuration_endpoint(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            // User is authenticated, proceed with getting configuration
            debug("User authenticated, retrieving configuration".to_string());
        }
        Ok(None) => {
            // This shouldn't happen as require_authentication returns error for None
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            // Authentication failed, return the auth error response
            return Ok(auth_response);
        }
    }

    // Get configuration
    let config = crate::configuration::load_configuration::fetch_configuration_in_db().expect("Expected to be able to load configuration");

    let json_config = match serde_json::to_string_pretty(&config) {
        Ok(json) => json,
        Err(e) => {
            error(format!("Failed to serialize configuration: {}", e));
            let mut response = GruxiResponse::new_with_bytes(
                hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                bytes::Bytes::from(r#"{"error": "Failed to serialize configuration"}"#),
            );
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(json_config));
    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
    return Ok(response);
}

pub async fn admin_post_configuration_reload(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            // User is authenticated, proceed with reloading configuration
            debug("User authenticated, reloading configuration".to_string());
        }
        Ok(None) => {
            // This shouldn't happen as require_authentication returns error for None
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            // Authentication failed, return the auth error response
            return Ok(auth_response);
        }
    }

    // Trigger the configuration cache reload
    let triggers = get_trigger_handler();
    triggers.run_trigger("refresh_cached_configuration").await;
    triggers.run_trigger("reload_configuration").await;

    info("Configuration reload triggered by admin user".to_string());

    let success_response = serde_json::json!({
        "success": true,
        "message": "Configuration reload initiated. Server is restarting..."
    });

    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(success_response.to_string()));
    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
    return Ok(response);
}

pub async fn admin_post_configuration_endpoint(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check if this is a POST request
    if gruxi_request.get_http_method() != "POST" {
        trace(format!("Request with invalid method: {}", gruxi_request.get_http_method()));
        let response = GruxiResponse::new_empty_with_status(hyper::StatusCode::METHOD_NOT_ALLOWED.as_u16());
        return Ok(response);
    }

    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            debug("User authenticated for configuration update".to_string());
        }
        Ok(None) => {
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            return Ok(auth_response);
        }
    }

    // Read the request body
    if gruxi_request.get_body_size() == 0 {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(r#"{"error": "Empty request body"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }
    let body_bytes = gruxi_request.get_body_bytes().await;

    // Parse JSON body into Configuration struct
    let mut configuration: Configuration = match serde_json::from_slice(&body_bytes) {
        Ok(config) => config,
        Err(e) => {
            error(format!("Failed to parse configuration JSON: {}", e));
            let error_response = serde_json::json!({
                "error": "Invalid JSON format",
                "details": e.to_string()
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(error_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    // Save the configuration
    match save_configuration(&mut configuration, false) {
        Ok(true) => {
            info("Configuration updated successfully".to_string());

            // Serialize the sanitized configuration to return to the client
            let config_json = match serde_json::to_value(&configuration) {
                Ok(json) => json,
                Err(e) => {
                    error(format!("Failed to serialize updated configuration: {}", e));

                    let mut response = GruxiResponse::new_with_bytes(
                        hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        bytes::Bytes::from(r#"{"error": "Configuration saved but failed to serialize response"}"#),
                    );
                    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                    return Ok(response);
                }
            };

            let success_response = serde_json::json!({
                "success": true,
                "message": "Configuration updated successfully. Please restart the server for changes to take effect.",
                "configuration": config_json
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(success_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Ok(false) => {
            info("Configuration save requested, but no changes detected".to_string());

            // Even if no changes were made, return the current configuration
            let config_json = match serde_json::to_value(&configuration) {
                Ok(json) => json,
                Err(e) => {
                    error(format!("Failed to serialize configuration: {}", e));
                    let mut response = GruxiResponse::new_with_bytes(
                        hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                        bytes::Bytes::from(r#"{"error": "Failed to serialize configuration response"}"#),
                    );
                    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                    return Ok(response);
                }
            };

            let success_response = serde_json::json!({
                "success": true,
                "message": "Configuration is up to date. No changes were needed.",
                "configuration": config_json
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(success_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(validation_errors) => {
            info(format!("Configuration validation failed: {}", validation_errors.join("; ")));
            let error_response = serde_json::json!({
                "errors": validation_errors
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(error_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    }
}

// Helper function to extract session token from request
async fn get_session_token_from_request(gruxi_request: &GruxiRequest) -> Option<String> {
    // First, check for Authorization header (Bearer token)
    if let Some(auth_header) = gruxi_request.get_headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Some(auth_str[7..].to_string());
            }
        }
    }

    None
}

// Helper function to verify session token and return session info
pub fn verify_session(token: &str) -> Result<Option<crate::core::admin_user::Session>, String> {
    verify_session_token(token)
}

// Middleware-like function to check if request is authenticated
pub async fn require_authentication(gruxi_request: &GruxiRequest) -> Result<Option<crate::core::admin_user::Session>, GruxiResponse> {
    let token = get_session_token_from_request(gruxi_request).await;

    if let Some(token) = token {
        match verify_session(&token) {
            Ok(Some(session)) => Ok(Some(session)),
            Ok(None) => {
                let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Invalid or expired session"}"#));
                response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Err(response)
            }
            Err(e) => {
                error(format!("Failed to verify session: {}", e));
                let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(r#"{"error": "Internal server error"}"#));
                response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Err(response)
            }
        }
    } else {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        Err(response)
    }
}

// Admin monitoring endpoint - returns monitoring data as JSON
pub async fn admin_monitoring_endpoint(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            debug("User authenticated, retrieving monitoring data".to_string());
        }
        Ok(None) => {
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            return Ok(auth_response);
        }
    }

    // Get monitoring data
    let monitoring_data = get_monitoring_state().await.get_json().await;

    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(monitoring_data.to_string()));
    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
    return Ok(response);
}

// Admin healthcheck endpoint - returns simple status without authentication
pub async fn admin_healthcheck_endpoint(_gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from("The server is healthy"));
    response.headers_mut().insert("Content-Type", "text/plain".parse().unwrap());
    return Ok(response);
}

// Admin logs endpoint - lists available log files or returns specific log content
pub async fn admin_logs_endpoint(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            debug("User authenticated, retrieving logs".to_string());
        }
        Ok(None) => {
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            return Ok(auth_response);
        }
    }

    let path = gruxi_request.get_path();
    let path_parts: Vec<&str> = path.split('/').collect();

    // Parse the request path: /logs or /logs/{filename}
    if path_parts.len() == 2 && path_parts[1] == "logs" {
        // List all available log files
        list_log_files().await
    } else if path_parts.len() == 3 && path_parts[1] == "logs" {
        // Return specific log file content
        let filename = path_parts[2];
        get_log_file_content(filename).await
    } else {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(r#"{"error": "Invalid logs endpoint path"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }
}

// Helper function to list all .log files in the logs directory
async fn list_log_files() -> Result<GruxiResponse, GruxiError> {
    let logs_dir = Path::new("logs");

    match fs::read_dir(logs_dir) {
        Ok(entries) => {
            let mut log_files = Vec::new();

            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if let Some(extension) = path.extension() {
                        if extension == "log" {
                            if let Some(filename) = path.file_name() {
                                if let Some(filename_str) = filename.to_str() {
                                    let metadata = fs::metadata(&path);
                                    let file_size = metadata.map(|m| m.len()).unwrap_or(0);

                                    log_files.push(serde_json::json!({
                                        "filename": filename_str,
                                        "size": file_size,
                                        "path": path.to_string_lossy()
                                    }));
                                }
                            }
                        }
                    }
                }
            }

            let response_json = serde_json::json!({
                "success": true,
                "files": log_files
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(response_json.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(e) => {
            error(format!("Failed to read logs directory: {}", e));
            let error_response = serde_json::json!({
                "error": "Failed to read logs directory",
                "details": e.to_string()
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(error_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    }
}

// Helper function to get log file content with 1MB limit
async fn get_log_file_content(filename: &str) -> Result<GruxiResponse, GruxiError> {
    // Validate filename to prevent directory traversal
    if filename.contains("..") || filename.contains("/") || filename.contains("\\") {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(r#"{"error": "Invalid filename"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }

    // Ensure filename ends with .log
    if !filename.ends_with(".log") {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(r#"{"error": "Only .log files are allowed"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }

    let log_path = Path::new("logs").join(filename);

    if !log_path.exists() {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::NOT_FOUND.as_u16(), bytes::Bytes::from(r#"{"error": "Log file not found"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }

    match fs::metadata(&log_path) {
        Ok(metadata) => {
            let file_size = metadata.len();
            let max_size = 1024 * 1024; // 1MB limit

            match fs::read_to_string(&log_path) {
                Ok(content) => {
                    let (log_content, is_truncated) = if file_size > max_size {
                        // If file is larger than 1MB, return only the last 1MB
                        let bytes = content.as_bytes();
                        let start_pos = if bytes.len() > max_size as usize { bytes.len() - max_size as usize } else { 0 };

                        // Try to start from a newline to avoid cutting mid-line
                        let start_pos = if start_pos > 0 {
                            match bytes[start_pos..].iter().position(|&b| b == b'\n') {
                                Some(newline_pos) => start_pos + newline_pos + 1,
                                None => start_pos,
                            }
                        } else {
                            start_pos
                        };

                        let truncated_content = String::from_utf8_lossy(&bytes[start_pos..]).to_string();
                        (truncated_content, true)
                    } else {
                        (content, false)
                    };

                    let response_json = serde_json::json!({
                        "success": true,
                        "filename": filename,
                        "content": log_content,
                        "file_size": file_size,
                        "is_truncated": is_truncated,
                        "full_path": log_path.to_string_lossy(),
                        "message": if is_truncated {
                            format!("File is larger than 1MB. Showing last ~1MB. Full file is available at: {}", log_path.to_string_lossy())
                        } else {
                            "".to_string()
                        }
                    });

                    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(response_json.to_string()));
                    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                    return Ok(response);
                }
                Err(e) => {
                    error(format!("Failed to read log file {}: {}", filename, e));
                    let error_response = serde_json::json!({
                        "error": "Failed to read log file",
                        "details": e.to_string()
                    });

                    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(error_response.to_string()));
                    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                    return Ok(response);
                }
            }
        }
        Err(e) => {
            error(format!("Failed to get metadata for log file {}: {}", filename, e));
            let error_response = serde_json::json!({
                "error": "Failed to access log file",
                "details": e.to_string()
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(error_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    }
}

// Request/Response structures for operation mode
#[derive(Serialize, Deserialize)]
struct OperationModeResponse {
    mode: String,
}

#[derive(Serialize, Deserialize)]
struct OperationModeRequest {
    mode: String,
}

// Admin operation mode GET endpoint - returns current operation mode
pub async fn admin_get_operation_mode_endpoint(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            debug("User authenticated, retrieving operation mode".to_string());
        }
        Ok(None) => {
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            return Ok(auth_response);
        }
    }

    // Get current operation mode
    let current_mode = get_operation_mode_as_string();

    let response = OperationModeResponse { mode: current_mode };

    let json_response = match serde_json::to_string(&response) {
        Ok(json) => json,
        Err(e) => {
            error(format!("Failed to serialize operation mode response: {}", e));
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(), bytes::Bytes::from(r#"{"error": "Failed to serialize response"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    let response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(json_response));
    return Ok(response);
}

// Admin operation mode POST endpoint - changes operation mode
pub async fn admin_post_operation_mode_endpoint(gruxi_request: &mut GruxiRequest, _admin_site: &Site) -> Result<GruxiResponse, GruxiError> {
    // Check if this is a POST request
    if gruxi_request.get_http_method() != "POST" {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::METHOD_NOT_ALLOWED.as_u16(), bytes::Bytes::from(r#"{"error": "Method not allowed"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }

    // Check authentication first
    match require_authentication(&gruxi_request).await {
        Ok(Some(_session)) => {
            debug("User authenticated for operation mode update".to_string());
        }
        Ok(None) => {
            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::UNAUTHORIZED.as_u16(), bytes::Bytes::from(r#"{"error": "Authentication required"}"#));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
        Err(auth_response) => {
            return Ok(auth_response);
        }
    }

    // Read the request body
    if gruxi_request.get_body_size() == 0 {
        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(r#"{"error": "Empty request body"}"#));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }
    let body_bytes = gruxi_request.get_body_bytes().await;

    // Parse JSON body
    let mode_request: OperationModeRequest = match serde_json::from_slice(&body_bytes) {
        Ok(req) => req,
        Err(e) => {
            error(format!("Failed to parse operation mode request: {}", e));
            let error_response = serde_json::json!({
                "error": "Invalid JSON format",
                "details": e.to_string()
            });

            let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(error_response.to_string()));
            response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(response);
        }
    };

    // Validate the mode
    if is_valid_operation_mode(&mode_request.mode) == false {
        let error_response = serde_json::json!({
            "error": "Invalid operation mode",
            "details": format!("Mode '{}' is not recognized as a valid operation mode", mode_request.mode)
        });

        let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::BAD_REQUEST.as_u16(), bytes::Bytes::from(error_response.to_string()));
        response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(response);
    }

    // Change the operation mode
    let was_changed = set_new_operation_mode(mode_request.mode.clone());

    let return_message = if was_changed {
        format!("Operation mode changed to {}", mode_request.mode)
    } else {
        format!("Operation mode was already set to {}", mode_request.mode)
    };

    let success_response = serde_json::json!({
        "success": was_changed,
        "message": return_message,
        "mode": mode_request.mode
    });

    let mut response = GruxiResponse::new_with_bytes(hyper::StatusCode::OK.as_u16(), bytes::Bytes::from(success_response.to_string()));
    response.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
    return Ok(response);
}
