use crate::grux_configuration::*;
use crate::grux_configuration_struct::*;
use crate::grux_http_handle_request::*;
use crate::grux_http_tls::build_tls_acceptor;
use futures::future::join_all;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, info, trace, warn};
use std::net::SocketAddr;
use tls_listener::rustls as tokio_rustls;
use tokio::net::TcpListener;

// Main function, starting all the Grux magic
#[tokio::main(flavor = "multi_thread")]
pub async fn initialize_server() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Get configuration
    let config = get_configuration();

    // Figure out what we want to start
    let servers: Vec<Server> = config.get("servers").unwrap();
    if servers.is_empty() {
        error!("No servers configured. Please check your configuration.");
        return Err("No servers configured".into());
    }

    let admin_site_config: AdminSite = config.get("admin_site").unwrap();

    let mut started_servers = Vec::new();

    // Starting the defined client servers
    for server in servers {
        for binding in server.bindings {
            let ip = binding.ip.parse::<std::net::IpAddr>().map_err(|e| format!("Invalid IP address: {}", e))?;
            let port = binding.port;
            let addr = SocketAddr::new(ip, port);

            // Enforce admin bindings are TLS-only
            if binding.is_admin && !binding.is_tls {
                warn!("Admin binding requested without TLS on {}:{}. This is not recommended.", binding.ip, binding.port);
            }

            if binding.is_admin {
                if admin_site_config.is_admin_portal_enabled {
                    info!("Starting Grux admin server on {}", addr);
                } else {
                    warn!("Grux admin portal is disabled in the configuration.");
                }
            } else {
                // Non-admin server
                info!("Starting Grux server on {}", addr);
            }

            // Start listening on the specified address
            let server = start_server_binding(binding);
            started_servers.push(server);
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

        if binding.is_tls {
            // TLS path using tokio-rustls so we can inspect ALPN to choose HTTP/2 vs HTTP/1.1
            let acceptor = match build_tls_acceptor(&binding).await {
                Ok(a) => a,
                Err(e) => {
                    error!("TLS setup failed for {}:{} => {}", binding.ip, binding.port, e);
                    return;
                }
            };
            loop {
                let (tcp_stream, _) = listener.accept().await.unwrap();
                let remote_addr = tcp_stream.peer_addr().map(|addr| addr.to_string()).ok();
                let remote_addr_string = remote_addr.unwrap_or_else(|| "<unknown>".to_string());
                let remote_addr_ip = remote_addr_string.split(':').next().unwrap_or("").to_string();

                let acceptor = acceptor.clone();
                tokio::task::spawn({
                    let binding = binding.clone();
                    let remote_addr_ip = remote_addr_ip.clone();

                    async move {
                        match acceptor.accept(tcp_stream).await {
                            Ok(tls_stream) => {
                                // Decide protocol based on ALPN
                                let is_h2 = negotiated_h2(&tls_stream);
                                let io = TokioIo::new(tls_stream);
                                let svc = service_fn(move |req| handle_request(req, binding.clone(), remote_addr_ip.clone()));
                                if is_h2 {
                                    if let Err(err) = http2::Builder::new(TokioExecutor::new()).serve_connection(io, svc).await {
                                        trace!("TLS h2 error serving connection: {:?}", err);
                                    }
                                } else {
                                    if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                                        trace!("TLS http1.1 error serving connection: {:?}", err);
                                    }
                                }
                            }
                            Err(err) => {
                                trace!("TLS handshake error: {:?}", err);
                            }
                        }
                    }
                });
            }
        } else {
            // Non-TLS path
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let remote_addr = stream.peer_addr().map(|addr| addr.to_string()).ok();
                let remote_addr_string = remote_addr.clone().unwrap_or_else(|| "<unknown>".to_string());
                let remote_addr_ip = remote_addr_string.split(':').next().unwrap_or("").to_string();
                let io = TokioIo::new(stream);

                tokio::task::spawn({
                    let binding = binding.clone();
                    let remote_addr_ip = remote_addr_ip.clone();
                    async move {
                        let svc = service_fn(move |req| handle_request(req, binding.clone(), remote_addr_ip.clone()));
                        if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                            trace!("Error serving connection: {:?}", err);
                        }
                    }
                });
            }
        }
    }
}

// Determine if ALPN negotiated h2 for a rustls TLS stream
fn negotiated_h2(stream: &tokio_rustls::server::TlsStream<tokio::net::TcpStream>) -> bool {
    // get_ref returns (IO, Connection)
    let (_io, conn) = stream.get_ref();
    match conn.alpn_protocol() {
        Some(proto) => proto == b"h2",
        None => false,
    }
}
