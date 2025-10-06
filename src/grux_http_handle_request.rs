use crate::grux_configuration_struct::*;
use crate::grux_file_cache::get_file_cache;
use crate::grux_file_util::get_full_file_path;
use crate::grux_http_admin::*;
use crate::grux_http_util::*;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::{Request, Response};
use log::debug;
use log::trace;

// Handle the incoming request
pub async fn handle_request(req: Request<hyper::body::Incoming>, binding: Binding, remote_ip: String) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    //  return Ok(empty_response_with_status(hyper::StatusCode::OK));

    // Extract data for the request before we borrow/move
    let method = req.method().clone();
    let uri = req.uri().clone();
    let path = uri.path();
    let query = uri.query().unwrap_or("");
    let body_size = req.body().size_hint().upper().unwrap_or(0);

    // Extract hostname from headers
    let requested_hostname = {
        let headers = req.headers();
        headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("").to_string()
    };

    // Figure out which site we are serving
    let site = find_best_match_site(&binding.sites, &requested_hostname);
    if let None = site {
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }
    let site = site.unwrap();

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
        }
    }

    // Now se determine what the request is, and how to handle it
    let headers = req.headers();
    let headers_map = headers.iter().map(|(k, v)| (k.as_str(), v.to_str().unwrap_or(""))).collect::<Vec<_>>();
    debug!(
        "Received request: method={}, path={}, query={}, body_size={}, headers={:?}",
        method, path, query, body_size, headers_map
    );
    trace!("Matched site with request: {:?}", site);

    // First, check if there is a specific file requested
    let web_root = &site.web_root;

    // Check if if request is for path or file
    let path_cleaned = clean_url_path(path);

    let mut file_path = format!("{}/{}", web_root, path_cleaned);

    // Expand it to full path
    let resolved_path = get_full_file_path(&file_path);
    if let Err(e) = resolved_path {
        trace!("Error resolving file path {}: {}", file_path, e);
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }
    file_path = resolved_path.unwrap();

    trace!("Checking file path: {}", file_path);

    // Check if the file/dir exists using direct tokio::fs calls
    let file_cache = get_file_cache();
    let mut file_data = file_cache.get_file(&file_path).unwrap();

    if !file_data.exists {
        trace!("File does not exist: {}", file_path);
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }

    if file_data.is_directory {
        // If it's a directory, we will try to return the index file
        trace!("File is a directory: {}", file_path);

        let index_file = {
            let mut found_index = None;
            for file in &site.web_root_index_file_list {
                let index_path = format!("{}/{}", file_path, file);
                let index_data = file_cache.get_file(&index_path).unwrap();
                if index_data.exists {
                    trace!("Returning index file: {}", index_path);
                    file_data = index_data;
                    file_path = index_path;
                    found_index = Some(file.clone());
                    break;
                }
            }
            found_index
        };

        if index_file.is_none() {
            trace!("Index files in dir does not exist: {}", file_path);
            return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
        }
    }

    // Extract the information we need before consuming the request for body extraction
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = req.headers().clone();

    // Get HTTP version
    let http_version = match req.version() {
        hyper::Version::HTTP_09 => "HTTP/0.9".to_string(),
        hyper::Version::HTTP_10 => "HTTP/1.0".to_string(),
        hyper::Version::HTTP_11 => "HTTP/1.1".to_string(),
        hyper::Version::HTTP_2 => "HTTP/2.0".to_string(),
        hyper::Version::HTTP_3 => "HTTP/3.0".to_string(),
        _ => "HTTP/1.1".to_string(),
    };

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
    for handler_id in &site.enabled_handlers {
        let handler = crate::grux_external_request_handlers::get_request_handler_by_id(handler_id);
        if let Some(handler) = handler {
            let file_matches = handler.get_file_matches();
            if file_matches.iter().any(|m| file_path.ends_with(m)) {
                trace!("Passing request to external handler {} for file {}", handler_id, file_path);
                handler_response = handler.handle_request(&method, &uri, &headers, body_bytes.clone(), &site, &file_path, &remote_ip, &http_version);
                handler_did_stuff = true;
                break; // Only handle with the first matching handler
            }
        }
    }

    // Create the response
    let mut additional_headers: Vec<(&str, &str)> = vec![];
    let mut response;
    if handler_did_stuff {
        response = handler_response;
        additional_headers.push(("Content-Type", "text/html; charset=utf-8"));
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

    for (key, value) in additional_headers {
        response.headers_mut().insert(key, HeaderValue::from_str(value).unwrap());
    }

    add_standard_headers_to_response(&mut response);

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
        trace!("No matching site found for requested hostname: {}", requested_hostname);
        return None;
    }

    site
}

/*
fn validate_requests(req: &Request<hyper::body::Incoming>) -> Result<(), hyper::Error> {
    // Here we can add any request validation logic if needed
    // For now, we will just return Ok

    /*
      // Protect our server from overly large bodies
      let upper = req.body().size_hint().upper().unwrap_or(u64::MAX);
      if upper > 1024 * 64 {
          let mut resp = Response::new(full("Body too big"));
          *resp.status_mut() = hyper::StatusCode::PAYLOAD_TOO_LARGE;
          return Ok(resp);
      }
    */

    Ok(())
}
*/
