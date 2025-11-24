use crate::configuration::binding::Binding;
use crate::configuration::load_configuration::get_configuration;
use crate::core::shutdown_manager::get_shutdown_manager;
use crate::http::handle_request::handle_request_entry;
use crate::http::http_tls::build_tls_acceptor;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use log::{error, info, trace, warn};
use std::future::Future;
use std::net::SocketAddr;
use tls_listener::rustls as tokio_rustls;
use tokio::net::TcpListener;
use tokio::select;

// Starting all the Grux magic
pub  fn initialize_server() {
    // Get configuration
    let config = get_configuration();

    // Collect all server tasks
    let mut server_tasks = Vec::new();

    // Starting listening on all configured bindings
    for binding in &config.bindings {
        let ip_result = binding.ip.parse::<std::net::IpAddr>();
        let ip = match ip_result {
            Ok(ip_addr) => ip_addr,
            Err(e) => {
                error!("Invalid IP address for binding {}: {}. Skipping this binding.", binding.ip, e);
                continue;
            }
        };
        let port = binding.port;
        let addr = SocketAddr::new(ip, port);

        // Enforce admin bindings are TLS-only
        if binding.is_admin && !binding.is_tls {
            warn!("Admin binding requested without TLS on {}:{}. This is not recommended.", binding.ip, binding.port);
        }

        info!("Starting Grux server on {}", addr);

        // Start listening on the specified address - spawn each binding as a separate task
        let binding_clone = binding.clone();
        let task = tokio::task::spawn(start_server_binding(binding_clone));
        server_tasks.push(task);
    }

    // Wait for the first task to complete (which should only happen on shutdown or error)
//    if let Some(first_task) = server_tasks.into_iter().next() {
//        let _ = first_task.await;
//    }

}

fn start_server_binding(binding: Binding) -> impl Future<Output = ()> {
    async move {
        let ip = binding.ip.parse::<std::net::IpAddr>().unwrap();
        let port = binding.port;
        let addr = SocketAddr::new(ip, port);

        let listener = TcpListener::bind(addr).await.unwrap();
        trace!("Listening on binding: {:?}", binding);

        let shutdown_manager = get_shutdown_manager();
        if binding.is_tls {
            // TLS path using tokio-rustls so we can inspect ALPN to choose HTTP/2 vs HTTP/1.1
            let acceptor = match build_tls_acceptor(&binding).await {
                Ok(a) => a,
                Err(e) => {
                    error!("TLS setup failed for {}:{} => {}", binding.ip, binding.port, e);
                    return;
                }
            };
            let cancellation_token = shutdown_manager.get_cancellation_token();

            loop {
                select! {
                    _ = cancellation_token.cancelled() => {
                        trace!("Termination signal received, stopping server on {}:{}", binding.ip, binding.port);
                        return;
                    },
                    result = listener.accept() => {
                        match result {
                            Ok((tcp_stream, _)) => {
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
                                                let svc = service_fn(move |req| handle_request_entry(req, binding.clone(), remote_addr_ip.clone()));
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
                            Err(err) => {
                                error!("Failed to accept connection: {:?}", err);
                            }
                        }
                    }

                };
            }
        } else {
            // Non-TLS path
            let cancellation_token = shutdown_manager.get_cancellation_token();
            loop {
                select! {
                    _ = cancellation_token.cancelled() => {
                        trace!("Termination signal received, stopping server on {}:{}", binding.ip, binding.port);
                        return;
                    },
                    result = listener.accept() => {
                        match result {
                            Ok((tcp_stream, _)) => {
                                let remote_addr = tcp_stream.peer_addr().map(|addr| addr.to_string()).ok();
                                let remote_addr_string = remote_addr.clone().unwrap_or_else(|| "<unknown>".to_string());
                                let remote_addr_ip = remote_addr_string.split(':').next().unwrap_or("").to_string();
                                let io = TokioIo::new(tcp_stream);

                                tokio::task::spawn({
                                    let binding = binding.clone();
                                    let remote_addr_ip = remote_addr_ip.clone();
                                    async move {
                                        let svc = service_fn(move |req| handle_request_entry(req, binding.clone(), remote_addr_ip.clone()));
                                        if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                                            trace!("Error serving connection: {:?}", err);
                                        }
                                    }
                                });
                            }
                            Err(err) => {
                                error!("Failed to accept connection: {:?}", err);
                            }
                        }
                    }
                };
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
