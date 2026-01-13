use crate::admin_portal::http_admin_api::*;
use crate::compression::compression::Compression;
use crate::configuration::binding::Binding;
use crate::core::monitoring::get_monitoring_state;
use crate::core::running_state_manager::get_running_state_manager;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{AdminApiError, GruxiErrorKind};
use crate::http::http_util::*;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::http::site_match::site_matcher::find_best_match_site;
use crate::logging::syslog::{debug, trace};
use chrono::Local;
use hyper::header::HeaderValue;

// Entry point to handle request, as we need to do post-processing, like access logging etc
pub async fn handle_request(mut gruxi_request: GruxiRequest, binding: Binding) -> Result<GruxiResponse, GruxiError> {
    // Count the request in monitoring
    get_monitoring_state().await.increment_requests_served();

    // Log the request details
    debug(format!(
        "Received request: method={}, path={}, query={}, body_size={}, headers={:?}",
        gruxi_request.get_http_method(),
        gruxi_request.get_path(),
        gruxi_request.get_query(),
        gruxi_request.get_body_size(),
        gruxi_request.get_headers()
    ));

    // Get the running state
    let running_state = get_running_state_manager().await.get_running_state_unlocked().await;

    // Get the sites for this binding
    let binding_site_cache = running_state.get_binding_site_cache();
    let sites = binding_site_cache.get_sites_for_binding(&binding.id);
    if sites.is_empty() {
        trace(format!("No sites configured for binding ID: '{}'", &binding.id));
        return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
    }

    // Get the hostname and figure out which site matches
    let hostname = gruxi_request.get_hostname();
    let site = find_best_match_site(&sites, &hostname);
    if let None = site {
        trace(format!("No matching site found for hostname: '{}' on binding ID: '{}'", &hostname, &binding.id));
        return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
    }
    let site = site.unwrap();
    trace(format!("Matched site with request: {:?}", &site));

    // Validate the request
    if let Err(gruxi_error) = validate_request(&mut gruxi_request).await {
        debug(format!("Request validation failed: {:?}", gruxi_error));
        let status_code = match &gruxi_error.kind {
            GruxiErrorKind::HttpRequestValidation(code) => *code,
            _ => 500, // Default for other errors
        };
        let response = GruxiResponse::new_empty_with_status(status_code);
        return Ok(response);
    }

    // Handle special case for OPTIONS * request, which is stupid but valid
    if gruxi_request.get_http_method() == "OPTIONS" && gruxi_request.get_path() == "*" {
        // Special case for OPTIONS * request
        let mut resp = GruxiResponse::new_empty_with_status(hyper::StatusCode::OK.as_u16());
        resp.headers_mut()
            .insert("Allow", HeaderValue::from_static("GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
        add_standard_headers_to_response(&mut resp);
        return Ok(resp);
    }

    // Handle EXPECT: 100-continue header
    if let Some(expect_header) = gruxi_request.get_headers().get("expect") {
        if expect_header.to_str().unwrap_or("").eq_ignore_ascii_case("100-continue") {
            // Send 100 Continue response
            let mut resp = empty_response_with_status(hyper::StatusCode::CONTINUE);
            add_standard_headers_to_response(&mut resp);
            return Ok(resp);
        }
    }

    // Check if the request is for the admin portal - handle these first
    let admin_response = if binding.is_admin {
        match handle_api_routes(&mut gruxi_request, site).await {
            Ok(response) => Some(response),
            Err(e) => {
                // If the error is NoRouteMatched, we continue to normal processing
                match e.kind {
                    GruxiErrorKind::AdminApi(AdminApiError::NoRouteMatched) => {
                        trace("No matching admin API route found, continuing to normal request handling".to_string());
                    }
                    _ => {
                        // Current no other admin API errors are defined, but in case we add some later, we handle them here
                    }
                }
                None
            }
        }
    } else {
        None
    };

    let mut response = if let Some(admin_response) = admin_response {
        admin_response
    } else {
        // If no handler wants it, we return 404
        if site.request_handlers.is_empty() {
            return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
        }

        // Now we let the request handler manager process the request in the order defined by the site's request_handlers list.
        let request_handler_manager = running_state.get_request_handler_manager();
        let response_result = request_handler_manager.handle_request(&mut gruxi_request, &site).await;
        if response_result.is_err() {
            trace(format!("No request handler matched for URL path: {}", &gruxi_request.get_path_and_query()));
            return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
        }
        response_result.unwrap()
    };

    // Consider gzipping content if not already gzipped
    let content_length = response.get_body_size();
    let content_type_header_option = response.get_header("Content-Type");
    let content_type_header = if let Some(cth) = content_type_header_option {
        cth.to_str().unwrap_or("").to_string()
    } else {
        "".to_string()
    };

    let content_encoding_header_option = response.get_header("Content-Encoding");
    let content_encoding_header = if let Some(ceh) = content_encoding_header_option {
        ceh.to_str().unwrap_or("").to_string()
    } else {
        "".to_string()
    };

    let file_reader_cache = running_state.get_file_reader_cache();

    // Only gzip if not already gzipped and if we should compress based on config and sizes
    if content_encoding_header.to_lowercase() != "gzip" && file_reader_cache.should_compress(&content_type_header, content_length) {
        let accepted_encodings = gruxi_request.get_accepted_encodings();
        let compression = Compression::new();
        compression.compress_response(&mut response, accepted_encodings, content_encoding_header).await;
    }

    // Vector for additional headers to set
    let mut additional_headers: Vec<(&str, &str)> = vec![];

    // If method is OPTIONS, we add the Allow header if not already present
    if gruxi_request.get_http_method() == "OPTIONS" {
        if !response.headers().iter().any(|(k, _)| k.as_str().to_lowercase() == "allow") {
            additional_headers.push(("Allow", "GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
        }
    }

    // Set any additional headers
    for (key, value) in additional_headers {
        response.headers_mut().insert(key, HeaderValue::from_str(value).unwrap());
    }

    // Apply site-specific extra headers
    for kv in &site.extra_headers {
        if let Ok(key_name) = hyper::http::HeaderName::from_bytes(kv.key.as_bytes()) {
            if let Ok(val) = HeaderValue::from_str(kv.value.as_str()) {
                response.headers_mut().insert(key_name, val);
            }
        }
    }

    // Handle access logging
    if site.access_log_enabled {
        // Get current date and time in CLF format, which is like 10/Oct/2000:13:55:36 -0700
        let now = Local::now();
        let clf_date = now.format("%d/%b/%Y:%H:%M:%S %z").to_string();
        let log_entry = format!(
            "{} - - [{}] \"{} {} {}\" {} {}",
            gruxi_request.get_remote_ip(),
            clf_date,
            gruxi_request.get_http_method(),
            gruxi_request.get_path_and_query(),
            gruxi_request.get_http_version(),
            response.get_status(),
            response.get_body_size()
        );

        let access_log_buffer_rwlock = running_state.get_access_log_buffer();
        let access_log_buffer = access_log_buffer_rwlock.read().await;
        access_log_buffer.add_log(site.id.to_string(), log_entry);
    }

    Ok(response)
}

async fn validate_request(gruxi_request: &mut GruxiRequest) -> Result<(), GruxiError> {
    // Here we can add any request validation logic if needed
    let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
    let configuration = cached_configuration.get_configuration().await;

    // Validation for HTTP/1.1 only
    if gruxi_request.get_http_version() == "HTTP/1.1" {
        // [HTTP1.1] Requires a Host header
        if !gruxi_request.get_headers().contains_key("Host") {
            return Err(GruxiError::new(
                GruxiErrorKind::HttpRequestValidation(hyper::StatusCode::BAD_REQUEST.as_u16()),
                format!("Failed to get streaming HTTP request for request: {:?}", gruxi_request),
            ));
        }

        // [HTTP1.1] If there is multiple host headers, we return a 400 error
        if gruxi_request.get_headers().get_all("Host").iter().count() > 1 {
            return Err(GruxiError::new(
                GruxiErrorKind::HttpRequestValidation(hyper::StatusCode::BAD_REQUEST.as_u16()),
                format!("Multiple Host headers for request: {:?}", gruxi_request),
            ));
        }
    }

    // [HTTP1.1 and later] Basic validation: check for valid method
    let http_method = gruxi_request.get_http_method();
    if http_method != "GET"
        && http_method != "POST"
        && http_method != "HEAD"
        && http_method != "PUT"
        && http_method != "DELETE"
        && http_method != "OPTIONS"
        && http_method != "TRACE"
        && http_method != "CONNECT"
        && http_method != "PATCH"
    {
        // Return a error for unsupported method
        return Err(GruxiError::new(
            GruxiErrorKind::HttpRequestValidation(hyper::StatusCode::NOT_IMPLEMENTED.as_u16()),
            format!("Unsupported HTTP method for request: {:?}", gruxi_request),
        ));
    }

    // Protect our server from overly large bodies
    let max_body_size = configuration.core.server_settings.max_body_size;
    if max_body_size > 0 && (http_method == "POST" || http_method == "PUT") {
        // Check Content-Length header if present
        if let Some(content_length_header) = gruxi_request.get_headers().get("Content-Length") {
            if let Ok(content_length_str) = content_length_header.to_str() {
                if let Ok(content_length) = content_length_str.parse::<usize>() {
                    if content_length > max_body_size {
                        return Err(GruxiError::new(
                            GruxiErrorKind::HttpRequestValidation(hyper::StatusCode::PAYLOAD_TOO_LARGE.as_u16()),
                            format!("Payload too large for request, based on content-length header: {:?}", gruxi_request),
                        ));
                    }
                }
            }
        }

        // Also check the expected body size
        if gruxi_request.get_body_size() > max_body_size.try_into().unwrap_or(0) {
            return Err(GruxiError::new(
                GruxiErrorKind::HttpRequestValidation(hyper::StatusCode::PAYLOAD_TOO_LARGE.as_u16()),
                format!("Payload too large for request, based on actual body size: {:?}", gruxi_request),
            ));
        }
    }

    Ok(())
}
