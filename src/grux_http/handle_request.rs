use crate::configuration::binding::Binding;
use crate::configuration::load_configuration::get_configuration;
use crate::configuration::site::Site;
use crate::external_request_handlers::external_request_handlers;
use crate::grux_admin::http_admin_api::*;
use crate::grux_core::monitoring::get_monitoring_state;
use crate::grux_file_cache::CachedFile;
use crate::grux_file_cache::get_file_cache;
use crate::grux_file_util::check_path_secure;
use crate::grux_file_util::get_full_file_path;
use crate::grux_http::http_util::*;
use crate::logging::access_logging::get_access_log_buffer;
use chrono::Local;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::HeaderMap;
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::{Request, Response};
use log::debug;
use log::trace;
use std::collections::HashMap;

// Entry point to handle request, as we need to do post-processing, like access logging etc
pub async fn handle_request_entry(req: Request<hyper::body::Incoming>, binding: Binding, remote_ip: String) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Hashmap that holds calculated request data
    let mut request_data: HashMap<String, String> = HashMap::new();

    // Extract hostname from headers
    let headers = req.headers();
    let requested_hostname = headers.get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("")
        .to_string();

    request_data.insert("remote_ip".to_string(), remote_ip.clone());
    request_data.insert("requested_hostname".to_string(), requested_hostname.clone());

    // Get HTTP version
    let http_version = match req.version() {
        hyper::Version::HTTP_09 => "HTTP/0.9".to_string(),
        hyper::Version::HTTP_10 => "HTTP/1.0".to_string(),
        hyper::Version::HTTP_11 => "HTTP/1.1".to_string(),
        hyper::Version::HTTP_2 => "HTTP/2.0".to_string(),
        hyper::Version::HTTP_3 => "HTTP/3.0".to_string(),
        _ => "HTTP/1.1".to_string(),
    };
    request_data.insert("http_version".to_string(), http_version);

    let sites = binding.get_sites();

    // Figure out which site we are serving
    let site = find_best_match_site(&sites, &requested_hostname);
    if let None = site {
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }
    let site = site.unwrap();
    trace!("Matched site with request: {:?}", &site);

    let method = req.method().clone();

    let uri = req.uri().clone();
    let path_and_query = uri.path_and_query().map(|pq| pq.as_str()).unwrap_or("");

    let response = handle_request(req, &binding, &site, &request_data).await;

    // Handle access logging
    if site.access_log_enabled {
        let access_log = get_access_log_buffer();
        // Get current date and time in CLF format, which is like 10/Oct/2000:13:55:36 -0700
        let now = Local::now();
        let clf_date = now.format("%d/%b/%Y:%H:%M:%S %z").to_string();
        let log_entry = format!(
            "{} - - [{}] \"{} {} {}\" {} {}",
            &remote_ip,
            clf_date,
            method,
            path_and_query,
            request_data.get("http_version").cloned().unwrap_or_default(),
            response.as_ref().map(|resp| resp.status().as_u16()).unwrap_or(0),
            response.as_ref().map(|resp| resp.size_hint().upper().unwrap_or(0)).unwrap_or(0)
        );
        access_log.add_log(site.id.to_string(), log_entry);
    }

    response
}

// Handle the incoming request
async fn handle_request(req: Request<hyper::body::Incoming>, binding: &Binding, site: &Site, request_data: &HashMap<String, String>) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Count the request in monitoring
    get_monitoring_state().increment_requests_served();

    // Extract data for the request before we borrow/move
    let method = req.method().clone();
    let uri = req.uri().clone();
    let mut path = uri.path();
    let query = uri.query().unwrap_or("");
    let body_size = req.body().size_hint().upper().unwrap_or(0);
    let headers = req.headers();
    let http_version = request_data.get("http_version").cloned().unwrap_or_default();
    let remote_ip = request_data.get("remote_ip").cloned().unwrap_or_default();

    // Validate the request
    if let Err(resp) = validate_request(
        &http_version,
        &headers,
        &method.to_string(),
        &uri.to_string(),
        &path.to_string(),
        &query.to_string(),
        body_size.try_into().unwrap_or(0),
    )
    .await
    {
        return Ok(resp);
    }

    // Put the rewrite functions in a hashmap, so we can easily check them
    let rewrite_functions = {
        let mut map = HashMap::new();
        for rewrite in &site.rewrite_functions {
            map.insert(rewrite.clone(), ());
        }
        map
    };

    // Check if the request is for the admin portal - handle these first
    if binding.is_admin {
        let path_cleaned = clean_url_path(path);
        trace!("Handling request for admin portal with path: {}", path_cleaned);

        // We only want to handle a few paths in the admin portal
        if path_cleaned == "login" && method == hyper::Method::POST {
            return handle_login_request(req, site).await;
        } else if path_cleaned == "logout" && method == hyper::Method::POST {
            return handle_logout_request(req, site).await;
        } else if path_cleaned == "config" && method == hyper::Method::GET {
            return admin_get_configuration_endpoint(&req, site).await;
        } else if path_cleaned == "config" && method == hyper::Method::POST {
            return admin_post_configuration_endpoint(req, site).await;
        } else if path_cleaned == "monitoring" && method == hyper::Method::GET {
            return admin_monitoring_endpoint(&req, site).await;
        } else if path_cleaned == "healthcheck" && method == hyper::Method::GET {
            return admin_healthcheck_endpoint(&req, site).await;
        } else if (path_cleaned == "logs" || path_cleaned.starts_with("logs/")) && method == hyper::Method::GET {
            return admin_logs_endpoint(&req, site).await;
        }
    }

    // Now se determine what the request is, and how to handle it
    let headers = req.headers();
    let headers_map = headers.iter().map(|(k, v)| (k.as_str(), v.to_str().unwrap_or(""))).collect::<Vec<_>>();
    debug!(
        "Received request: method={}, path={}, query={}, body_size={}, headers={:?}",
        method, path, query, body_size, headers_map
    );

    // Handle special case for OPTIONS * request, which is stupid but valid
    if method == hyper::Method::OPTIONS && path == "*" {
        // Special case for OPTIONS * request
        let mut resp = Response::new(full(""));
        *resp.status_mut() = hyper::StatusCode::OK;
        resp.headers_mut()
            .insert("Allow", HeaderValue::from_static("GET, HEAD, POST, PUT, DELETE, OPTIONS, TRACE, CONNECT, PATCH"));
        add_standard_headers_to_response(&mut resp);
        return Ok(resp);
    }
    // Handle EXPECT: 100-continue header
    if let Some(expect_header) = headers.get("expect") {
        if expect_header.to_str().unwrap_or("").eq_ignore_ascii_case("100-continue") {
            // Send 100 Continue response
            let mut resp = empty_response_with_status(hyper::StatusCode::CONTINUE);
            add_standard_headers_to_response(&mut resp);
            return Ok(resp);
        }
    }

    // First, check if there is a specific file requested
    let web_root_result = get_full_file_path(&site.web_root);
    if let Err(e) = web_root_result {
        debug!("Failed to get full web root path: {}", e);
        return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR));
    }
    let web_root = web_root_result.unwrap();

    // Get the cached file, if it exists
    let file_data_result = resolve_web_root_and_path_and_get_file(web_root.clone(), path.to_string());
    if let Err(_) = file_data_result {
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }
    let mut file_data = file_data_result.unwrap();
    let mut file_path = file_data.file_path.clone();

    if !file_data.exists {
        trace!("File does not exist: {}", file_path);
        if rewrite_functions.contains_key("OnlyWebRootIndexForSubdirs") {
            trace!("[OnlyWebRootIndexForSubdirs] Rewriting request path {} to root dir due to rewrite function", path);
            // We rewrite the path to just "/" which will make it serve the index file
            path = "/";

            // Get the cached file, if it exists
            let file_data_result = resolve_web_root_and_path_and_get_file(web_root.clone(), path.to_string());
            if let Err(_) = file_data_result {
                return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
            }
            file_data = file_data_result.unwrap();
            file_path = file_data.file_path.clone();
        } else {
            return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
        }
    }

    if file_data.is_directory {
        // If it's a directory, we will try to return the index file
        trace!("File is a directory: {}", file_path);

        // Check if we can find a index file in the directory
        let mut found_index = false;
        for file in &site.web_root_index_file_list {
            // Get the cached file, if it exists
            let file_data_result = resolve_web_root_and_path_and_get_file(file_path.clone(), file.to_string());
            if let Err(_) = file_data_result {
                trace!("Index files in dir does not exist: {}", file_path);
                continue;
            }
            file_data = file_data_result.unwrap();
            file_path = file_data.file_path.clone();
            trace!("Found index file: {}", file_path);
            found_index = true;
            break;
        }

        if !found_index {
            trace!("Did not find index file: {}", file_path);
            return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
        }
    }

    // Extract the information we need before consuming the request for body extraction
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // Extract body for POST/PUT requests
    let body_bytes = if method == hyper::Method::POST || method == hyper::Method::PUT {
        match req.collect().await {
            Ok(collected) => {
                let bytes = collected.to_bytes();
                bytes.to_vec()
            }
            Err(e) => {
                debug!("Failed to collect request body: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // We check if is a request we need to handle another way, such as PHP intepreter
    // We only go through the handlers that are active for this site
    let mut handler_response = Response::new(full(""));
    let mut handler_did_stuff = false;
    let mut handler_had_error = false;
    let external_request_handlers = external_request_handlers::get_request_handlers();

    for handler_id in &site.enabled_handlers {
        let handler_response_result = external_request_handlers
            .handle_external_request(handler_id, &method, &uri, &headers, &body_bytes, &site, &file_path, &remote_ip, &http_version)
            .await;
        handler_response = match handler_response_result {
            Ok(resp) => {
                handler_did_stuff = true;
                resp
            }
            Err(e) => {
                debug!("Error from external handler {}: {}", handler_id, e);
                handler_did_stuff = true;
                handler_had_error = true;
                empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR)
            }
        };
        if handler_did_stuff {
            break;
        }
    }

    // If the handler had an error, we return the error response
    if handler_had_error {
        return Ok(handler_response);
    }

    // Do a safety check of the path, make sure it's still under the web root and not blocked
    if !handler_did_stuff {
        if !check_path_secure(&web_root, &file_path) {
            trace!("File path is not secure: {}", file_path);
            // We should probably not reveal that the file is blocked, so we return a 404
            return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
        }
    }

    // Create the response
    let mut additional_headers: Vec<(&str, &str)> = vec![];
    let mut response;
    if handler_did_stuff {
        response = handler_response;
        // We do not set a default Content-Type here, as the handler should do that
    } else {
        additional_headers.push(("Content-Type", &file_data.mime_type));

        // Gzip body or raw content
        let body_content = if file_data.gzip_content.is_empty() {
            file_data.content
        } else {
            additional_headers.push(("Content-Encoding", "gzip"));
            file_data.gzip_content
        };

        response = Response::new(full(body_content));
        *response.status_mut() = hyper::StatusCode::OK;
    }

    // If method is OPTIONS, we add the Allow header if not already present
    if method == hyper::Method::OPTIONS {
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

    trace!("Responding with: {:?}", response);

    Ok(response)
}

// Combine the web root and path, and resolve to a full path
fn resolve_web_root_and_path_and_get_file(web_root: String, path: String) -> Result<CachedFile, std::io::Error> {
    let path_cleaned = clean_url_path(&path);
    let mut file_path = format!("{}/{}", web_root, path_cleaned);
    trace!("Resolved file path for resolving: {}", file_path);
    file_path = get_full_file_path(&file_path)?;
    let file_cache = get_file_cache();
    let file_data = file_cache.get_file(&file_path).unwrap();
    Ok(file_data)
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
        trace!("No matching site found for requested hostname: {}", requested_hostname);
        return None;
    }

    site
}

async fn validate_request(http_version: &str, headers: &HeaderMap, method: &str, _uri: &str, _path: &str, _query: &str, body_size: usize) -> Result<(), Response<BoxBody<Bytes, hyper::Error>>> {
    // Here we can add any request validation logic if needed
    let configuration = get_configuration();

    // Validation for HTTP/1.1 only
    if http_version == "HTTP/1.1" {
        // [HTTP1.1] Requires a Host header
        if !headers.contains_key("Host") {
            trace!("Missing Host header, return HTTP 400");
            return Err(empty_response_with_status(hyper::StatusCode::BAD_REQUEST));
        }

        // [HTTP1.1] If there is multiple host headers, we return a 400 error
        if headers.get_all("Host").iter().count() > 1 {
            trace!("Multiple Host headers, return HTTP 400");
            return Err(empty_response_with_status(hyper::StatusCode::BAD_REQUEST));
        }
    }

    // [HTTP1.1 and later] Basic validation: check for valid method
    if method != "GET" && method != "POST" && method != "HEAD" && method != "PUT" && method != "DELETE" && method != "OPTIONS" && method != "TRACE" && method != "CONNECT" && method != "PATCH" {
        // Return a error for unsupported method
        trace!("Unsupported HTTP method, return HTTP 501: {}", method);
        return Err(empty_response_with_status(hyper::StatusCode::NOT_IMPLEMENTED));
    }

    // Protect our server from overly large bodies
    let max_body_size = configuration.core.server_settings.max_body_size;
    if max_body_size > 0 && (method == "POST" || method == "PUT") {
        // Check Content-Length header if present
        if let Some(content_length_header) = headers.get("Content-Length") {
            if let Ok(content_length_str) = content_length_header.to_str() {
                if let Ok(content_length) = content_length_str.parse::<usize>() {
                    if content_length > max_body_size {
                        return Err(empty_response_with_status(hyper::StatusCode::PAYLOAD_TOO_LARGE));
                    }
                }
            }
        }

        // Also check the actual body size
        if body_size > max_body_size {
            return Err(empty_response_with_status(hyper::StatusCode::PAYLOAD_TOO_LARGE));
        }
    }

    Ok(())
}
