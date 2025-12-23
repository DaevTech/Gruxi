use crate::configuration::binding::Binding;
use crate::http::handle_request::handle_request;
use crate::http::http_tls::build_tls_acceptor;
use crate::http::requests::grux_request::GruxRequest;
use crate::logging::syslog::{debug, error, info, trace, warn};
use hyper::Request;
use hyper::body::Incoming;
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::net::SocketAddr;
use tls_listener::rustls as tokio_rustls;
use tokio::net::TcpListener;
use tokio::select;

// Starting all the Grux magic
pub async fn initialize_server() {
    // Get configuration from the current configuration
    let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration().await;

    // Starting listening on all configured bindings
    for binding in &config.bindings {
        let ip_result = binding.ip.parse::<std::net::IpAddr>();
        let ip = match ip_result {
            Ok(ip_addr) => ip_addr,
            Err(e) => {
                error(format!("Invalid IP address for binding {}: {}. Skipping this binding.", binding.ip, e));
                continue;
            }
        };
        let port = binding.port;
        let addr = SocketAddr::new(ip, port);

        // Enforce admin bindings are TLS-only
        if binding.is_admin && !binding.is_tls {
            warn(format!("Admin binding requested without TLS on {}:{}. This is not recommended.", binding.ip, binding.port));
        }

        info(format!("Starting Grux server on {}", addr));

        // Start listening on the specified address - spawn each binding as a separate task
        let binding_clone = binding.clone();
        tokio::spawn(start_server_binding(binding_clone));
    }
}

async fn start_listener_with_retry(addr: SocketAddr) -> TcpListener {
    // Implement a simple retry mechanism
    let mut attempts = 0;
    let max_attempts = 5;
    let retry_delay = std::time::Duration::from_millis(500);

    loop {
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                return listener;
            }
            Err(e) => {
                attempts += 1;
                if attempts >= max_attempts {
                    panic!("Failed to bind to {} after {} attempts: {}", addr, attempts, e);
                }
                error(format!("Failed to bind to {}: {}. Retrying in {:?}...", addr, e, retry_delay));
                tokio::time::sleep(retry_delay).await;
            }
        }
    }
}

async fn start_server_binding(binding: Binding) {
    let ip = binding.ip.parse::<std::net::IpAddr>().unwrap();
    let port = binding.port;
    let addr = SocketAddr::new(ip, port);

    let listener = start_listener_with_retry(addr).await;
    trace(format!("Listening on binding: {:?}", binding));

    let triggers = crate::core::triggers::get_trigger_handler();

    if binding.is_tls {
        // TLS path using tokio-rustls so we can inspect ALPN to choose HTTP/2 vs HTTP/1.1
        let acceptor = match build_tls_acceptor(&binding).await {
            Ok(a) => a,
            Err(e) => {
                error(format!("TLS setup failed for {}:{} => {}", binding.ip, binding.port, e));
                return;
            }
        };
        let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
        let stop_services_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

        loop {
            select! {
                            _ = shutdown_token.cancelled() => {
                                trace(format!("Shutdown signal received, stopping server on {}:{}", binding.ip, binding.port));
                                break;
                            },
                            _ = stop_services_token.cancelled() => {
                                trace(format!("Service cancellation signal received, stopping server on {}:{}", binding.ip, binding.port));
                                break;
                            },
                            result = listener.accept() => {
                                match result {
                                    Ok((tcp_stream, _)) => {
                                        let remote_addr = tcp_stream.peer_addr().map(|addr| addr.to_string()).ok();
                                        let remote_addr_string = remote_addr.unwrap_or_else(|| "<unknown>".to_string());
                                        let remote_addr_ip = remote_addr_string.split(':').next().unwrap_or("").to_string();

                                        let acceptor = acceptor.clone();
                                        let shutdown_token = shutdown_token.clone();
                                        let stop_services_token = stop_services_token.clone();
                                        tokio::task::spawn({
                                            let binding = binding.clone();
                                            let remote_addr_ip = remote_addr_ip.clone();

                                            async move {
                                                match acceptor.accept(tcp_stream).await {
                                                    Ok(tls_stream) => {
                                                        // Decide protocol based on ALPN
                                                        let is_h2 = negotiated_h2(&tls_stream);
                                                        let io = TokioIo::new(tls_stream);

                                                        let svc = service_fn(move |req: Request<Incoming>| {
                                                            let binding = binding.clone();
                                                            let remote_ip = remote_addr_ip.clone();
                                                            let shutdown_token = shutdown_token.clone();
                                                            let stop_services_token = stop_services_token.clone();

                                                            async move {
                                                                // Convert hyper Request<Incoming> to GruxRequest
                                                                let mut grux_req = GruxRequest::from_hyper(req).await?;
                                                                grux_req.add_calculated_data("remote_ip", &remote_ip);

                                                                // Call your application handler
                                                                let response = handle_request(grux_req, binding, shutdown_token, stop_services_token).await;
                                                                debug(format!("Responding with: {:?}", response));
                                                                response
                                                            }
                                                        });

                                                        if is_h2 {
                                                            if let Err(err) = http2::Builder::new(TokioExecutor::new()).serve_connection(io, svc).await {
                                                                trace(format!("TLS h2 error serving connection: {:?}", err));
                                                            }
                                                        } else {
                                                            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                                                                trace(format!("TLS http1.1 error serving connection: {:?}", err));
                                                            }
                                                        }
                                                    }
                                                    Err(err) => {
                                                        trace(format!("TLS handshake error: {:?}", err));
                                                    }
                                                }
                                            }
                                        });
                                    }
                                    Err(err) => {
                                        error(format!("Failed to accept connection: {:?}", err));
                                    }
                                }
                            }

                        };
        }
    } else {
        // Non-TLS path
        let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
        let stop_services_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

        loop {
            select! {
                _ = shutdown_token.cancelled() => {
                    trace(format!("Termination signal received, stopping server on {}:{}", binding.ip, binding.port));
                    break;
                },
                _ = stop_services_token.cancelled() => {
                    trace(format!("Service stop signal received, stopping server on {}:{}", binding.ip, binding.port));
                    break;
                },
                result = listener.accept() => {

                    match result {
                        Ok((tcp_stream, _)) => {
                            let remote_addr = tcp_stream.peer_addr().map(|addr| addr.to_string()).ok();
                            let remote_addr_string = remote_addr.clone().unwrap_or_else(|| "<unknown>".to_string());
                            let remote_addr_ip = remote_addr_string.split(':').next().unwrap_or("").to_string();
                            let io = TokioIo::new(tcp_stream);

                            let shutdown_token = shutdown_token.clone();
                            let stop_services_token = stop_services_token.clone();

                            tokio::task::spawn({
                                let binding = binding.clone();
                                let remote_addr_ip = remote_addr_ip.clone();
                                async move {
                                     let svc = service_fn(move |req: Request<Incoming>| {
                                        let binding = binding.clone();
                                        let remote_ip = remote_addr_ip.clone();
                                        let shutdown_token = shutdown_token.clone();
                                        let stop_services_token = stop_services_token.clone();

                                        async move {
                                            // Convert hyper Request<Incoming> to GruxRequest
                                            let mut grux_req = GruxRequest::from_hyper(req).await?;
                                            grux_req.add_calculated_data("remote_ip", &remote_ip);

                                            // Call your application handler
                                            let response = handle_request(grux_req, binding, shutdown_token, stop_services_token).await;
                                            debug(format!("Responding with: {:?}", response));
                                            response
                                        }
                                    });

                                    if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                                        trace(format!("Error serving connection: {:?}", err));
                                    }
                                }
                            });
                        }
                        Err(err) => {
                            error(format!("Failed to accept connection: {:?}", err));
                        }
                    }
                }
            };
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
