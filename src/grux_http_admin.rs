use http_body_util::combinators::BoxBody;
use http_body_util::BodyExt;
use hyper::{Request, Response};
use hyper::body::Bytes;
use crate::grux_configuration_struct::Sites;
use crate::grux_http_util::{full};
use crate::grux_database::{LoginRequest, authenticate_user, create_session, verify_session_token, invalidate_session};
use log::{info, error, debug};
use serde_json;


pub async fn handle_login_request(req: Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Check if this is a POST request
    if req.method() != hyper::Method::POST {
        let mut resp = Response::new(full("Method not allowed"));
        *resp.status_mut() = hyper::StatusCode::METHOD_NOT_ALLOWED;
        return Ok(resp);
    }

    // Read the request body
    let body_bytes = match req.collect().await {
        Ok(body) => body.to_bytes(),
        Err(_) => {
            let mut resp = Response::new(full("Failed to read request body"));
            *resp.status_mut() = hyper::StatusCode::BAD_REQUEST;
            return Ok(resp);
        }
    };

    // Parse JSON body
    let login_request: LoginRequest = match serde_json::from_slice(&body_bytes) {
        Ok(req) => req,
        Err(e) => {
            error!("Failed to parse login request: {}", e);
            let mut resp = Response::new(full("Invalid JSON format"));
            *resp.status_mut() = hyper::StatusCode::BAD_REQUEST;
            return Ok(resp);
        }
    };

    debug!("Login attempt for username: {}", login_request.username);

    // Authenticate user
    let user = match authenticate_user(&login_request.username, &login_request.password) {
        Ok(Some(user)) => user,
        Ok(None) => {
            info!("Failed login attempt for username: {}", login_request.username);
            let mut resp = Response::new(full(r#"{"error": "Invalid username or password"}"#));
            *resp.status_mut() = hyper::StatusCode::UNAUTHORIZED;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
        Err(e) => {
            error!("Database error during authentication: {}", e);
            let mut resp = Response::new(full(r#"{"error": "Internal server error"}"#));
            *resp.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
    };

    // Create session
    let session = match create_session(&user) {
        Ok(session) => session,
        Err(e) => {
            error!("Failed to create session: {}", e);
            let mut resp = Response::new(full(r#"{"error": "Failed to create session"}"#));
            *resp.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
    };

    info!("Successful login for user: {}", user.username);

    // Return success response with session token
    let response_json = serde_json::json!({
        "success": true,
        "message": "Login successful",
        "session_token": session.token,
        "username": session.username,
        "expires_at": session.expires_at.to_rfc3339()
    });

    let mut resp = Response::new(full(response_json.to_string()));
    *resp.status_mut() = hyper::StatusCode::OK;
    resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
    Ok(resp)
}

pub async fn handle_logout_request(req: Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Check if this is a POST request
    if req.method() != hyper::Method::POST {
        let mut resp = Response::new(full("Method not allowed"));
        *resp.status_mut() = hyper::StatusCode::METHOD_NOT_ALLOWED;
        return Ok(resp);
    }

    // Get the session token from Authorization header or request body
    let token = get_session_token_from_request(&req).await;

    if let Some(token) = token {
        match invalidate_session(&token) {
            Ok(true) => {
                info!("Successfully logged out session");
                let response_json = serde_json::json!({
                    "success": true,
                    "message": "Logout successful"
                });
                let mut resp = Response::new(full(response_json.to_string()));
                *resp.status_mut() = hyper::StatusCode::OK;
                resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Ok(resp)
            }
            Ok(false) => {
                let mut resp = Response::new(full(r#"{"error": "Session not found"}"#));
                *resp.status_mut() = hyper::StatusCode::NOT_FOUND;
                resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Ok(resp)
            }
            Err(e) => {
                error!("Failed to logout session: {}", e);
                let mut resp = Response::new(full(r#"{"error": "Internal server error"}"#));
                *resp.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Ok(resp)
            }
        }
    } else {
        let mut resp = Response::new(full(r#"{"error": "No session token provided"}"#));
        *resp.status_mut() = hyper::StatusCode::BAD_REQUEST;
        resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        Ok(resp)
    }
}

pub async fn admin_get_configuration_endpoint(req: &Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Check authentication first
    match require_authentication(req).await {
        Ok(Some(_session)) => {
            // User is authenticated, proceed with getting configuration
            debug!("User authenticated, retrieving configuration");
        }
        Ok(None) => {
            // This shouldn't happen as require_authentication returns error for None
            let mut resp = Response::new(full(r#"{"error": "Authentication required"}"#));
            *resp.status_mut() = hyper::StatusCode::UNAUTHORIZED;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
        Err(auth_response) => {
            // Authentication failed, return the auth error response
            return Ok(auth_response);
        }
    }

    // Get the current configuration
    let config = crate::grux_configuration::get_newest_configuration();

    // Try to deserialize the configuration to ensure it's valid
    match config.clone().try_deserialize::<crate::grux_configuration_struct::Configuration>() {
        Ok(configuration) => {
            // Serialize the configuration to JSON
            match serde_json::to_string_pretty(&configuration) {
                Ok(json_string) => {
                    let mut resp = Response::new(full(json_string));
                    *resp.status_mut() = hyper::StatusCode::OK;
                    resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                    Ok(resp)
                }
                Err(e) => {
                    error!("Failed to serialize configuration to JSON: {}", e);
                    let mut resp = Response::new(full(r#"{"error": "Failed to serialize configuration"}"#));
                    *resp.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                    resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                    Ok(resp)
                }
            }
        }
        Err(e) => {
            error!("Failed to deserialize configuration: {}", e);
            let mut resp = Response::new(full(r#"{"error": "Invalid configuration format"}"#));
            *resp.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            Ok(resp)
        }
    }
}

pub async fn admin_post_configuration_endpoint(req: Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Check if this is a POST request
    if req.method() != hyper::Method::POST {
        let mut resp = Response::new(full(r#"{"error": "Method not allowed"}"#));
        *resp.status_mut() = hyper::StatusCode::METHOD_NOT_ALLOWED;
        resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        return Ok(resp);
    }

    // Check authentication first
    match require_authentication(&req).await {
        Ok(Some(_session)) => {
            debug!("User authenticated for configuration update");
        }
        Ok(None) => {
            let mut resp = Response::new(full(r#"{"error": "Authentication required"}"#));
            *resp.status_mut() = hyper::StatusCode::UNAUTHORIZED;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
        Err(auth_response) => {
            return Ok(auth_response);
        }
    }

    // Read the request body
    let body_bytes = match req.collect().await {
        Ok(body) => body.to_bytes(),
        Err(e) => {
            error!("Failed to read request body: {}", e);
            let mut resp = Response::new(full(r#"{"error": "Failed to read request body"}"#));
            *resp.status_mut() = hyper::StatusCode::BAD_REQUEST;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
    };

    // Parse JSON body into Configuration struct
    let configuration: crate::grux_configuration_struct::Configuration = match serde_json::from_slice(&body_bytes) {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to parse configuration JSON: {}", e);
            let error_response = serde_json::json!({
                "error": "Invalid JSON format",
                "details": e.to_string()
            });
            let mut resp = Response::new(full(error_response.to_string()));
            *resp.status_mut() = hyper::StatusCode::BAD_REQUEST;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            return Ok(resp);
        }
    };

    // Save the configuration
    match crate::grux_configuration::save_configuration(&configuration) {
        Ok(true) => {
            info!("Configuration updated successfully");
            let success_response = serde_json::json!({
                "success": true,
                "message": "Configuration updated successfully. Please restart the server for changes to take effect."
            });
            let mut resp = Response::new(full(success_response.to_string()));
            *resp.status_mut() = hyper::StatusCode::OK;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            Ok(resp)
        }
        Ok(false) => {
            info!("Configuration save requested, but no changes detected");
            let success_response = serde_json::json!({
                "success": true,
                "message": "Configuration is up to date. No changes were needed."
            });
            let mut resp = Response::new(full(success_response.to_string()));
            *resp.status_mut() = hyper::StatusCode::OK;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            Ok(resp)
        }
        Err(validation_errors) => {
            error!("Configuration validation failed: {}", validation_errors);
            let error_response = serde_json::json!({
                "error": "Configuration validation failed",
                "details": validation_errors
            });
            let mut resp = Response::new(full(error_response.to_string()));
            *resp.status_mut() = hyper::StatusCode::BAD_REQUEST;
            resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
            Ok(resp)
        }
    }
}

// Helper function to extract session token from request
async fn get_session_token_from_request(req: &Request<hyper::body::Incoming>) -> Option<String> {
    // First, check for Authorization header (Bearer token)
    if let Some(auth_header) = req.headers().get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Some(auth_str[7..].to_string());
            }
        }
    }

    None
}

// Helper function to verify session token and return session info
pub fn verify_session(token: &str) -> Result<Option<crate::grux_database::Session>, String> {
    verify_session_token(token)
}

// Middleware-like function to check if request is authenticated
pub async fn require_authentication(req: &Request<hyper::body::Incoming>) -> Result<Option<crate::grux_database::Session>, Response<BoxBody<Bytes, hyper::Error>>> {
    let token = get_session_token_from_request(req).await;

    if let Some(token) = token {
        match verify_session(&token) {
            Ok(Some(session)) => Ok(Some(session)),
            Ok(None) => {
                let mut resp = Response::new(full(r#"{"error": "Invalid or expired session"}"#));
                *resp.status_mut() = hyper::StatusCode::UNAUTHORIZED;
                resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Err(resp)
            }
            Err(e) => {
                error!("Failed to verify session: {}", e);
                let mut resp = Response::new(full(r#"{"error": "Internal server error"}"#));
                *resp.status_mut() = hyper::StatusCode::INTERNAL_SERVER_ERROR;
                resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
                Err(resp)
            }
        }
    } else {
        let mut resp = Response::new(full(r#"{"error": "Authentication required"}"#));
        *resp.status_mut() = hyper::StatusCode::UNAUTHORIZED;
        resp.headers_mut().insert("Content-Type", "application/json".parse().unwrap());
        Err(resp)
    }
}
