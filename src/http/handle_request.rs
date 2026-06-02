use crate::admin_portal::http_admin_api::*;
use crate::compression::response_compression::maybe_compress_response;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{AdminApiError, GruxiErrorKind, TelemetryApiError};
use crate::http::http_server::ConnectionContext;
use crate::http::http_util::*;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::http::request_response::request_validation::validate_request;
use crate::http::site_match::site_matcher::find_best_match_site;
use crate::logging::access_log_entry::AccessLogEntry;
use crate::telemetry::http_telemetry_api::handle_telemetry_routes;
use crate::{debug, trace};
use hyper::header::HeaderValue;
use std::sync::Arc;

// Entry point to handle request, as we need to do post-processing, like access logging etc
pub async fn handle_request(mut gruxi_request: GruxiRequest, connection_context: Arc<ConnectionContext>) -> Result<GruxiResponse, GruxiError> {
    // Log the request details
    debug!(
        "Received request: hostname={}, method={}, path={}, query={}, body_size={}, headers={:?}",
        gruxi_request.get_hostname(),
        gruxi_request.get_http_method(),
        gruxi_request.get_path(),
        gruxi_request.get_query(),
        gruxi_request.get_body_size(),
        gruxi_request.get_headers()
    );

    // Get the running state
    let running_state = &connection_context.running_state;

    // Get the sites for this binding
    let binding_site_cache = running_state.get_binding_site_cache();
    let sites = binding_site_cache.get_sites_for_binding(&connection_context.binding.id);

    // Get the hostname and figure out which site matches
    let hostname = gruxi_request.get_hostname();
    let site = match find_best_match_site(&sites, hostname) {
        Some(site) => site,
        None => {
            if hostname.is_empty() {
                trace!("No hostname provided in request on binding ID: '{}'", &connection_context.binding.id);
                return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::BAD_REQUEST.as_u16()));
            } else {
                trace!("No matching site found for hostname: '{}' on binding ID: '{}'", &hostname, &connection_context.binding.id);
                return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
            }
        }
    };
    trace!("Matched site with request: {:?}", &site);

    // Validate the request
    if let Err(gruxi_response) = validate_request(&mut gruxi_request, &connection_context.binding, site, &connection_context.configuration).await {
        return Ok(gruxi_response);
    }

    // Handle special case for OPTIONS * request, which is stupid but valid
    if gruxi_request.get_http_method() == "OPTIONS" && gruxi_request.get_path() == "*" {
        // Special case for OPTIONS * request
        let mut resp = GruxiResponse::new_empty_with_status(hyper::StatusCode::OK.as_u16());
        resp.headers_mut()
            .insert("Allow", HeaderValue::from_static("GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
        return Ok(resp);
    }

    // Handle EXPECT: 100-continue header
    if let Some(expect_header) = gruxi_request.get_headers().get("expect")
        && expect_header.to_str().unwrap_or("").eq_ignore_ascii_case("100-continue")
    {
        // Send 100 Continue response
        let resp = empty_response_with_status(hyper::StatusCode::CONTINUE);
        return Ok(resp);
    }

    // Check if the request is for the telemetry endpoint - handle these first
    let telemetry_response = if connection_context.binding.is_telemetry {
        match handle_telemetry_routes(&mut gruxi_request, site, &connection_context).await {
            Ok(response) => Some(response),
            Err(e) => {
                if let GruxiErrorKind::TelemetryApi(TelemetryApiError::NoRouteMatched) = e.kind {
                    trace!("No matching telemetry API route found, continuing to normal request handling");
                }
                None
            }
        }
    } else {
        None
    };

    // Check if the request is for the admin portal - handle these first
    let admin_response = if connection_context.binding.is_admin {
        match handle_api_routes(&mut gruxi_request, site, &connection_context).await {
            Ok(response) => Some(response),
            Err(e) => {
                // If the error is NoRouteMatched, we continue to normal processing
                match e.kind {
                    GruxiErrorKind::AdminApi(AdminApiError::NoRouteMatched) => {
                        trace!("No matching admin API route found, continuing to normal request handling");
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

    let mut response = if let Some(telemetry_response) = telemetry_response {
        telemetry_response
    } else if let Some(admin_response) = admin_response {
        admin_response
    } else {
        // If no handler wants it, we return 404
        if site.request_handlers.is_empty() {
            return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
        }

        // Now we let the request handler manager process the request in the order defined by the site's request_handlers list.
        let request_handler_manager = running_state.get_request_handler_manager();
        let response_result = request_handler_manager.handle_request(&mut gruxi_request, site, &connection_context).await;
        if response_result.is_err() {
            trace!("No request handler matched for URL path: {}", &gruxi_request.get_path_and_query());
            return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
        }

        match response_result {
            Ok(response) => response,
            Err(_) => {
                trace!("No request handler matched for URL path: {}", &gruxi_request.get_path_and_query());
                return Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
            }
        }
    };

    // Consider gzipping content if not already gzipped
    if running_state.get_file_reader_cache().gzip_enabled {
        maybe_compress_response(&gruxi_request, &mut response, running_state).await;
    }

    // Vector for additional headers to set
    let mut additional_headers: Vec<(&str, &str)> = vec![];

    // If method is OPTIONS, we add the Allow header if not already present
    if gruxi_request.get_http_method() == "OPTIONS" && !response.headers().iter().any(|(k, _)| k.as_str().to_lowercase() == "allow") {
        additional_headers.push(("Allow", "GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
    }

    // Set any additional headers
    for (key, value) in additional_headers {
        let header_value_result = HeaderValue::from_str(value);
        match header_value_result {
            Ok(header_value) => {
                response.headers_mut().insert(key, header_value);
            }
            Err(e) => debug!("Failed to create header value for key '{}', value '{}': {}", key, value, e),
        }
    }

    // Apply site-specific extra headers
    for kv in &site.extra_headers {
        if let Ok(key_name) = hyper::http::HeaderName::from_bytes(kv.key.as_bytes())
            && let Ok(val) = HeaderValue::from_str(kv.value.as_str())
        {
            response.headers_mut().insert(key_name, val);
        }
    }

    // Handle access logging
    if site.access_log_enabled {
        let access_log_entry = AccessLogEntry {
            gruxi_request,
            site_id: site.id.clone(),
            log_time: chrono::Utc::now(),
            status_code: response.get_status(),
            response_size: response.get_body_size(),
        };
        running_state.get_access_log_buffer().add_log(access_log_entry);
    }

    Ok(response)
}
