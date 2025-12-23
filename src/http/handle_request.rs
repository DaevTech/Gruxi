use crate::admin_portal::http_admin_api::*;
use crate::configuration::binding::Binding;
use crate::configuration::site::Site;
use crate::core::monitoring::get_monitoring_state;
use crate::core::running_state_manager::get_running_state_manager;
use crate::http::http_util::*;
use crate::http::requests::grux_request::GruxRequest;
use crate::logging::syslog::{debug, trace};
use chrono::Local;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::Response;
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use tokio_util::sync::CancellationToken;

// Entry point to handle request, as we need to do post-processing, like access logging etc
pub async fn handle_request(
    mut grux_request: GruxRequest,
    binding: Binding,
    shutdown_token: CancellationToken,
    stop_services_token: CancellationToken,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Count the request in monitoring
    get_monitoring_state().await.increment_requests_served();

    // Log the request details
    debug(format!(
        "Received request: method={}, path={}, query={}, body_size={}, headers={:?}",
        grux_request.get_http_method(),
        grux_request.get_path(),
        grux_request.get_query(),
        grux_request.get_body_size(),
        grux_request.get_headers()
    ));

    // Figure out which site we are serving
    let sites = binding.get_sites();
    let hostname = grux_request.get_hostname();
    let site = find_best_match_site(&sites, &hostname);
    if let None = site {
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }

    let site = site.unwrap();
    trace(format!("Matched site with request: {:?}", &site));

    // Validate the request pre-body extraction, so if any body is sent, we dont waste time processing it
    if let Err(resp) = validate_request(&mut grux_request).await {
        return Ok(resp);
    }

    // Check if the request is for the admin portal - handle these first
    if binding.is_admin {
        let path = grux_request.get_path();
        let path_cleaned = clean_url_path(&path);
        let method = grux_request.get_http_method();

        trace(format!("Handling request for admin portal with path: {}", path_cleaned));

        // We only want to handle a few paths in the admin portal
        if path_cleaned == "login" && method == "POST" {
            return handle_login_request(grux_request, site).await;
        } else if path_cleaned == "logout" && method == "POST" {
            return handle_logout_request(grux_request, site).await;
        } else if path_cleaned == "config" && method == "GET" {
            return admin_get_configuration_endpoint(grux_request, site).await;
        } else if path_cleaned == "config" && method == "POST" {
            return admin_post_configuration_endpoint(grux_request, site).await;
        } else if path_cleaned == "monitoring" && method == "GET" {
            return admin_monitoring_endpoint(grux_request, site).await;
        } else if path_cleaned == "healthcheck" && method == "GET" {
            return admin_healthcheck_endpoint(grux_request, site).await;
        } else if (path_cleaned == "logs" || path_cleaned.starts_with("logs/")) && method == "GET" {
            return admin_logs_endpoint(grux_request, site).await;
        } else if (path_cleaned == "configuration/reload") && method == "POST" {
            return admin_post_configuration_reload(grux_request, site).await;
        } else if path_cleaned == "operation-mode" && method == "GET" {
            return admin_get_operation_mode_endpoint(grux_request, site).await;
        } else if path_cleaned == "operation-mode" && method == "POST" {
            return admin_post_operation_mode_endpoint(grux_request, site).await;
        }
    }

    // Handle special case for OPTIONS * request, which is stupid but valid
    if grux_request.get_http_method() == "OPTIONS" && grux_request.get_path() == "*" {
        // Special case for OPTIONS * request
        let mut resp = Response::new(full(""));
        *resp.status_mut() = hyper::StatusCode::OK;
        resp.headers_mut()
            .insert("Allow", HeaderValue::from_static("GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
        add_standard_headers_to_response(&mut resp);
        return Ok(resp);
    }

    /* DOES NOT CURRENTLY WORK DUE TO HYPER BUG

    // Handle EXPECT: 100-continue header
    if let Some(expect_header) = grux_request.get_headers().get("expect") {
        if expect_header.to_str().unwrap_or("").eq_ignore_ascii_case("100-continue") {
            // Send 100 Continue response
            let mut resp = empty_response_with_status(hyper::StatusCode::CONTINUE);
            add_standard_headers_to_response(&mut resp);
            return Ok(resp);
        }
    }
     */

    // Now we check the request handlers to see if any of them want to handle this request
    // If no handler wants it, we return 404
    if site.request_handlers.is_empty() {
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }

    // Get the running state
    let running_state = get_running_state_manager().await.get_running_state_unlocked().await;

    // Now we let the request handler manager process the request, with the request handlers in order of priority, which should already be sorted
    let request_handler_manager = running_state.get_request_handler_manager();
    let response_result = request_handler_manager.handle_request(&mut grux_request, &site).await;
    if response_result.is_err() {
        trace(format!("No request handler matched for URL path: {}", &grux_request.get_path_and_query()));
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }
    let mut response = response_result.unwrap();

    // If this is kept alive and we have shut down, we need to inform the client we are shutting down
    if shutdown_token.is_cancelled() || stop_services_token.is_cancelled() {
        response.headers_mut().insert("Connection", "close".parse().unwrap());
    }

    // Consider gzipping content if not already gzipped
    let content_type_header = response.headers().get("Content-Type").and_then(|v| v.to_str().ok()).unwrap_or("");
    let content_encoding_header = response.headers().get("Content-Encoding").and_then(|v| v.to_str().ok()).unwrap_or("");
    let content_length = response.size_hint().upper().unwrap_or(0);

    let file_cache_rwlock = running_state.get_file_cache();
    let file_cache = file_cache_rwlock.read().await;

    // Only gzip if not already gzipped and if we should compress based on config and sizes
    if content_encoding_header.to_lowercase() != "gzip" && file_cache.should_compress(content_type_header, content_length) {
        // Gzip the body
        // First, preserve the original headers and status
        let original_headers = response.headers().clone();
        let original_status = response.status();

        // Collect the body data
        let body_bytes = match response.collect().await {
            Ok(collected) => collected.to_bytes().to_vec(),
            Err(_) => {
                debug("Failed to collect response body for compression".to_string());
                Vec::new()
            }
        };

        if !body_bytes.is_empty() {
            // Compress the content
            let mut gzip_content = Vec::new();
            if file_cache.compress_content(&body_bytes, &mut gzip_content).is_ok() {
                // Create new response with compressed content
                response = Response::new(full(gzip_content));
                *response.status_mut() = original_status;

                // Copy over the original headers (except Content-Length which will be wrong)
                for (key, value) in original_headers.iter() {
                    if key != "content-length" {
                        response.headers_mut().insert(key, value.clone());
                    }
                }
                response.headers_mut().insert("Content-Encoding", HeaderValue::from_static("gzip"));
            } else {
                // If compression failed, recreate response with original body
                response = Response::new(full(body_bytes));
                *response.status_mut() = original_status;

                // Copy over the original headers
                for (key, value) in original_headers.iter() {
                    response.headers_mut().insert(key, value.clone());
                }
            }
        } else {
            // If body is empty, recreate response to avoid moved value issues
            response = Response::new(full(""));
            *response.status_mut() = original_status;

            // Copy over the original headers
            for (key, value) in original_headers.iter() {
                response.headers_mut().insert(key, value.clone());
            }
        }
    }

    // Vector for additional headers to set
    let mut additional_headers: Vec<(&str, &str)> = vec![];

    // If method is OPTIONS, we add the Allow header if not already present
    if grux_request.get_http_method() == "OPTIONS" {
        if !response.headers().iter().any(|(k, _)| k.as_str().to_lowercase() == "allow") {
            additional_headers.push(("Allow", "GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
        }
    }

    // Set any additional headers
    for (key, value) in additional_headers {
        response.headers_mut().insert(key, HeaderValue::from_str(value).unwrap());
    }

    // Add standard headers
    add_standard_headers_to_response(&mut response);

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
            grux_request.get_remote_ip(),
            clf_date,
            grux_request.get_http_method(),
            grux_request.get_path_and_query(),
            grux_request.get_http_version(),
            response.status().as_u16(),
            response.size_hint().upper().unwrap_or(0)
        );

        let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
        let access_log_buffer_rwlock = running_state.get_access_log_buffer();
        let access_log_buffer = access_log_buffer_rwlock.read().await;
        access_log_buffer.add_log(site.id.to_string(), log_entry);
    }

    Ok(response)
}

// Find a best match site for the requested hostname
fn find_best_match_site<'a>(sites: &'a [Site], requested_hostname: &'a str) -> Option<&'a Site> {
    let mut site = sites.iter().find(|s| s.hostnames.contains(&requested_hostname.to_string()) && s.is_enabled);

    // We check for star hostnames
    if site.is_none() {
        site = sites.iter().find(|s| s.hostnames.contains(&"*".to_string()) && s.is_enabled);
    }

    // If we cant find a matching site, we see if there is a default one
    if site.is_none() {
        site = sites.iter().find(|s| s.is_default && s.is_enabled);
    }

    // If we still cant find a proper site, we return None
    if site.is_none() {
        trace(format!("No matching site found for requested hostname: {}", requested_hostname));
        return None;
    }

    site
}

async fn validate_request(grux_request: &mut GruxRequest) -> Result<(), Response<BoxBody<Bytes, hyper::Error>>> {
    // Here we can add any request validation logic if needed
    let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
    let configuration = cached_configuration.get_configuration().await;

    // Validation for HTTP/1.1 only
    if grux_request.get_http_version() == "HTTP/1.1" {
        // [HTTP1.1] Requires a Host header
        if !grux_request.get_headers().contains_key("Host") {
            trace("Missing Host header, return HTTP 400".to_string());
            return Err(empty_response_with_status(hyper::StatusCode::BAD_REQUEST));
        }

        // [HTTP1.1] If there is multiple host headers, we return a 400 error
        if grux_request.get_headers().get_all("Host").iter().count() > 1 {
            trace("Multiple Host headers, return HTTP 400".to_string());
            return Err(empty_response_with_status(hyper::StatusCode::BAD_REQUEST));
        }
    }

    // [HTTP1.1 and later] Basic validation: check for valid method
    let http_method = grux_request.get_http_method();
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
        trace(format!("Unsupported HTTP method, return HTTP 501: {}", http_method));
        return Err(empty_response_with_status(hyper::StatusCode::NOT_IMPLEMENTED));
    }

    // Protect our server from overly large bodies
    let max_body_size = configuration.core.server_settings.max_body_size;
    if max_body_size > 0 && (http_method == "POST" || http_method == "PUT") {
        // Check Content-Length header if present
        if let Some(content_length_header) = grux_request.get_headers().get("Content-Length") {
            if let Ok(content_length_str) = content_length_header.to_str() {
                if let Ok(content_length) = content_length_str.parse::<usize>() {
                    if content_length > max_body_size {
                        return Err(empty_response_with_status(hyper::StatusCode::PAYLOAD_TOO_LARGE));
                    }
                }
            }
        }

        // Also check the expected body size
        if grux_request.get_body_size() > max_body_size.try_into().unwrap_or(0) {
            return Err(empty_response_with_status(hyper::StatusCode::PAYLOAD_TOO_LARGE));
        }
    }

    Ok(())
}
