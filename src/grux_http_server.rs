use crate::grux_configuration_struct::*;
use crate::grux_file_cache::get_file_cache;
use crate::grux_http_admin::*;
use crate::grux_http_util::*;
use futures::future::join_all;
use http_body_util::combinators::BoxBody;
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use log::{error, info, trace};
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main(flavor = "multi_thread")]
pub async fn initialize_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get configuration
    let config = crate::grux_configuration::get_configuration();

    // Figure out what we want to start
    let servers: Vec<Server> = config.get("servers").unwrap();
    if servers.is_empty() {
        error!("No servers configured. Please check your configuration.");
        return Err("No servers configured".into());
    }

    let mut started_servers = Vec::new();

    // Starting the admin server, if enabled
    let admin_site_config: AdminSite = config.get("admin_site").unwrap();

    if admin_site_config.is_admin_portal_enabled {
        let admin_binding = Binding {
            ip: admin_site_config.admin_portal_ip.clone(),
            port: admin_site_config.admin_portal_port,
            is_admin: true,
            sites: vec![Sites {
                hostnames: vec!["*".to_string()],
                is_default: true,
                is_enabled: true,
                is_ssl: false,
                is_ssl_required: false,
                web_root: admin_site_config.admin_portal_web_root.clone(),
                web_root_index_file_list: vec![admin_site_config.admin_portal_index_file.clone()],
            }],
        };

        let admin_server = start_server_binding(admin_binding);
        started_servers.push(admin_server);

        info!("Starting Grux admin server on {}:{}", admin_site_config.admin_portal_ip, admin_site_config.admin_portal_port);
    } else {
        info!("Grux admin portal is disabled in the configuration.");
    }

    // Starting the defined client servers
    for server in servers {
        for binding in server.bindings {
            let ip = binding.ip.parse::<std::net::IpAddr>().map_err(|e| format!("Invalid IP address: {}", e))?;
            let port = binding.port;
            let addr = SocketAddr::new(ip, port);

            // Start listening on the specified address
            let server = start_server_binding(binding);
            started_servers.push(server);

            info!("Starting Grux server on {}", addr);
        }
    }

    // Wait for all servers to finish (which is never, unless one panics)
    join_all(started_servers).await;

    Ok(())
}

fn start_server_binding(binding: Binding) -> impl std::future::Future<Output = ()> {
    let ip = binding.ip.parse::<std::net::IpAddr>().unwrap();
    let port = binding.port;
    let addr = SocketAddr::new(ip, port);

    async move {
        let listener = TcpListener::bind(addr).await.unwrap();
        trace!("Listening on binding: {:?}", binding);
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);

            tokio::task::spawn({
                let binding = binding.clone();
                async move {
                    let svc = service_fn(move |req| handle_request(req, binding.clone()));
                    if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                        trace!("Error serving connection: {:?}", err);
                    }
                }
            });
        }
    }
}

// Handle the incoming request
async fn handle_request(req: Request<hyper::body::Incoming>, binding: Binding) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
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
    trace!(
        "Received request: method={}, path={}, query={}, body_size={}, headers={:?}",
        method, path, query, body_size, headers_map
    );
    trace!("Matched site with request: {:?}", site);

    // First, check if there is a specific file requested
    let web_root = &site.web_root;

    // Check if if request is for path or file
    let path_cleaned = clean_url_path(path);

    let mut file_path = format!("{}/{}", web_root, path_cleaned);

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
                let index_path = format!("{}{}", file_path, file);
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

    let mut additional_headers: Vec<(&str, &str)> = vec![
        ("Content-Type", &file_data.mime_type)
    ];

    // Gzip body or raw content
    let body_content = if file_data.gzip_content.is_empty() {
        file_data.content
    } else {
        additional_headers.push(("Content-Encoding", "gzip"));
        file_data.gzip_content
    };

    // Create the response
    let mut resp = Response::new(full(body_content));
    *resp.status_mut() = hyper::StatusCode::OK;

    for (key, value) in additional_headers {
        resp.headers_mut().insert(key, HeaderValue::from_str(value).unwrap());
    }

    add_standard_headers_to_response(&mut resp);

    Ok(resp)
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

// Find a best match site for the requested hostname
fn find_best_match_site<'a>(sites: &'a [Sites], requested_hostname: &'a str) -> Option<&'a Sites> {
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
