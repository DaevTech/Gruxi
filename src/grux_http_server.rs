use crate::grux_configuration_struct::Binding;
use crate::grux_configuration_struct::Server;
use crate::grux_configuration_struct::Sites;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Body;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_content_encoding::Encoding;
use hyper_content_encoding::encode_response;
use hyper_util::rt::TokioIo;
use log::{error, info, trace};
use mime_guess::MimeGuess;
use std::net::SocketAddr;
use tokio::fs;
use tokio::join;
use tokio::net::TcpListener;

#[tokio::main]
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

    for server in servers {
        for binding in server.bindings {
            let ip = binding.ip.parse::<std::net::IpAddr>().map_err(|e| format!("Invalid IP address: {}", e))?;
            let port = binding.port.parse::<u16>().map_err(|e| format!("Invalid port: {}", e))?;
            let addr = SocketAddr::new(ip, port);

            let binds = Box::new(binding);
            let leaked_bindings: &'static Binding = Box::leak(binds);

            // Start listening on the specified address
            let srv = async move {
                let listener = TcpListener::bind(addr).await.unwrap();
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let io = TokioIo::new(stream);

                    let sites = &leaked_bindings.sites;

                    tokio::task::spawn(async move {
                        let svc = service_fn(move |req| handle_request(req, sites));

                        if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                            println!("Error serving connection: {:?}", err);
                        }
                    });
                }
            };

            info!("Starting Grux server on {}", addr);

            started_servers.push(srv);
        }
    }

    // Join threads
    for server in started_servers {
        join!(server);
    }

    Ok(())
}

// Handle the incoming request
async fn handle_request(req: Request<hyper::body::Incoming>, sites: &Vec<Sites>) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Extract data for the request
    let headers = req.headers();
    let headers_map = headers.iter().map(|(k, v)| (k.as_str(), v.to_str().unwrap_or(""))).collect::<Vec<_>>();
    let method = req.method();
    let uri = req.uri();
    let path = uri.path();
    let query = uri.query().unwrap_or("");
    // let version = req.version();
    let body_size = req.body().size_hint().upper().unwrap_or(0);
    let requested_hostname = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");

    // Figure out which site we are serving
    let site = find_best_match_site(sites, requested_hostname);
    if let None = site {
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }
    let site = site.unwrap();

    // Now se determine what the request is, and how to handle it
    trace!(
        "Received request: method={}, path={}, query={}, body_size={}, headers={:?}",
        method, path, query, body_size, headers_map
    );
    trace!("Matched site with request: {:?}", site);

    // First, check if the there is a specific file requested
    let web_root = &site.web_root;

    // Check if path ends with a slash
    if path.ends_with('/') {
        // If the path is just "/", we can return the index file, if it exists
        let index_file = site
            .web_root_index_file_list
            .iter()
            .find(|&file| {
                let file_path = format!("{}{}", web_root, file);
                std::path::Path::new(&file_path).exists()
            })
            .map(|file| file.to_string());
        if index_file.is_none() {
            return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
        }
        let file_path = format!("{}{}", web_root, index_file.unwrap());
        trace!("Returning index file: {}", file_path);

        // Read the index file and return it
        let file_content = match fs::read(&file_path).await {
            Ok(content) => content,
            Err(_) => return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND)),
        };

        let mut resp = Response::new(full(file_content));

        // Check if we should encode the response
        if headers.get("Accept-Encoding").map_or(false, |v| v.to_str().unwrap_or("").contains("gzip")) {
            resp = encode_response(resp, Encoding::Gzip).await.unwrap();
        }

        resp.headers_mut().insert("Content-Type", "text/html; charset=UTF-8".parse().unwrap());

        *resp.status_mut() = hyper::StatusCode::OK;
        add_standard_headers_to_response(&mut resp);

        Ok(resp)
    } else {
        // If the path is not "/" or ends with "/", we will try to serve the requested file
        trace!("Requested filepath: {}", path);

        // Check if file exist
        let file_path = format!("{}{}", web_root, path.trim_start_matches('/'));
        trace!("File exist check: {}", file_path);
        if fs::metadata(&file_path).await.is_err() {
            trace!("File did not exists: {}", file_path);
            return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
        }
        trace!("File exists: {}", file_path);

        // Read the file and return it
        let file_content = match fs::read(&file_path).await {
            Ok(content) => content,
            Err(_) => return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND)),
        };

        let mut resp = Response::new(full(file_content));

        // Attempt to guess the MIME type of the file
        if let Some(mime) = MimeGuess::from_path(&file_path).first() {
            resp.headers_mut().insert("Content-Type", mime.to_string().parse().unwrap());
        } else {
            resp.headers_mut().insert("Content-Type", "application/octet-stream".parse().unwrap());
        }
        add_standard_headers_to_response(&mut resp);

        // Check if we should encode the response
        if headers.get("Accept-Encoding").map_or(false, |v| v.to_str().unwrap_or("").contains("gzip")) {
            // We only encode response for certain content types
            let content_types_to_encode = vec!["text/", "application/json", "application/javascript", "text/css", "application/css"];
            let resp_content_type = resp.headers().get("Content-Type").and_then(|v| v.to_str().ok()).unwrap_or("");
            if content_types_to_encode.iter().any(|ct| resp_content_type.starts_with(ct)) {
                trace!("Encoding file response with gzip");
                resp = encode_response(resp, Encoding::Gzip).await.unwrap();
            } else {
                trace!("Not encoding file response, content type not suitable for gzip");
            }
        }

        Ok(resp)
    }
}

fn empty_response_with_status(status: hyper::StatusCode) -> Response<BoxBody<Bytes, hyper::Error>> {
    let mut resp = Response::new(full(""));
    *resp.status_mut() = status;
    add_standard_headers_to_response(&mut resp);
    resp
}

fn add_standard_headers_to_response(resp: &mut Response<BoxBody<Bytes, hyper::Error>>) {
    for (key, value) in get_standard_headers() {
        resp.headers_mut().insert(key, value.parse().unwrap());
    }
}

fn get_standard_headers() -> Vec<(&'static str, &'static str)> {
    return vec![("Server", "Grux"), ("Vary", "Accept-Encoding")];
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
fn find_best_match_site<'a>(sites: &'a [Sites], requested_hostname: &str) -> Option<&'a Sites> {
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

fn full<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into()).map_err(|never| match never {}).boxed()
}

#[derive(Clone)]
// An Executor that uses the tokio runtime.
pub struct TokioExecutor;

// Implement the `hyper::rt::Executor` trait for `TokioExecutor` so that it can be used to spawn
// tasks in the hyper runtime.
// An Executor allows us to manage execution of tasks which can help us improve the efficiency and
// scalability of the server.
impl<F> hyper::rt::Executor<F> for TokioExecutor
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn(fut);
    }
}
