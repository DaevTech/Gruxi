use crate::configuration::binding::Binding;
use crate::core::monitoring::get_monitoring_state;
use crate::http::handle_request::handle_request;
use crate::http::http_tls::build_unified_tls_acceptor;
use crate::http::http_util::add_standard_headers_to_response;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::logging::syslog::{debug, error, info, trace, warn};
use crate::tls::shared_acme_manager::initialize_shared_acme_manager;
use hyper::Request;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HttpAutoBuilder;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::select;
use tokio_util::sync::CancellationToken;

// Starting all the Gruxi magic
pub async fn initialize_server() {
    // Get configuration from the current configuration
    let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration().await;

    // Initialize shared ACME manager ONCE before starting any bindings.
    // This ensures all TLS bindings share a single ACME client, resolver, and polling task.
    if let Err(e) = initialize_shared_acme_manager().await {
        error(format!("Failed to initialize shared ACME manager: {}. ACME certificates will not be available.", e));
    }

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

        info(format!("Starting server on {}", addr));

        // Start listening on the specified address - spawn each binding as a separate task
        let binding_clone = binding.clone();
        tokio::spawn(start_server_binding(binding_clone));
    }
}

async fn start_listener_with_retry(addr: SocketAddr) -> TcpListener {
    // Implement a simple retry mechanism
    let mut attempts = 0;
    let max_attempts = 5;
    let retry_delay = std::time::Duration::from_millis(100);

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
    let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
    let stop_services_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

    if binding.is_tls {
        // Build unified TLS acceptor that handles both ACME and manual certificates
        // Note: ACME polling is handled by the shared manager, no per-binding task needed
        let tls_acceptor = match build_unified_tls_acceptor(&binding).await {
            Ok(result) => result,
            Err(e) => {
                error(format!("TLS setup failed for {}:{} => {}", binding.ip, binding.port, e));
                return;
            }
        };

        // Unified TLS accept loop
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
                            let remote_addr_ip = tcp_stream.peer_addr()
                                .map(|addr| addr.ip().to_string())
                                .unwrap_or_else(|_| "<unknown>".to_string());

                            let acceptor = tls_acceptor.clone();
                            let binding = binding.clone();
                            let shutdown_token = shutdown_token.clone();
                            let stop_services_token = stop_services_token.clone();

                            tokio::spawn(async move {
                                match acceptor.accept(tcp_stream).await {
                                    Ok(tls_stream) => {
                                        let io = TokioIo::new(tls_stream);
                                        serve_connection(io, binding, remote_addr_ip, shutdown_token, stop_services_token).await;
                                    }
                                    Err(err) => {
                                        trace(format!("TLS handshake error: {:?}", err));
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
                            let remote_addr_ip = tcp_stream.peer_addr()
                                .map(|addr| addr.ip().to_string())
                                .unwrap_or_else(|_| "<unknown>".to_string());

                            let io = TokioIo::new(tcp_stream);
                            let binding = binding.clone();
                            let shutdown_token = shutdown_token.clone();
                            let stop_services_token = stop_services_token.clone();

                            tokio::spawn(async move {
                                serve_connection(io, binding, remote_addr_ip, shutdown_token, stop_services_token).await;
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

// Helper function to serve a connection (works for both TLS and non-TLS)
async fn serve_connection<S>(io: TokioIo<S>, binding: Binding, remote_addr_ip: String, shutdown_token: CancellationToken, stop_services_token: CancellationToken)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let shutdown_token_conn = shutdown_token.clone();
    let stop_services_token_conn = stop_services_token.clone();

    let svc = service_fn(move |req: Request<Incoming>| {
        let binding = binding.clone();
        let remote_ip = remote_addr_ip.clone();

        async move {
            // Count the request in monitoring
            get_monitoring_state().await.increment_requests_served();

            let mut gruxi_request = GruxiRequest::from_hyper(req);
            gruxi_request.add_calculated_data("remote_ip", &remote_ip);
            let gruxi_response_result = handle_request(gruxi_request, binding).await;
            let mut response = match gruxi_response_result {
                Err(err) => {
                    error(format!("Error handling request from {}: {:?}", &remote_ip, err));
                    let response = GruxiResponse::new_empty_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16());
                    response
                }
                Ok(response) => response,
            };

            // Add standard headers
            add_standard_headers_to_response(&mut response);

            debug(format!("Responding with: {:?}", response));

            get_monitoring_state().await.decrement_requests_in_progress();

            // Convert gruxi_response to hyper response
            Ok::<_, std::convert::Infallible>(response.into_hyper())
        }
    });

    let connection = HttpAutoBuilder::new(TokioExecutor::new());

    // Serve the connection and listen for shutdown signals
    let result = tokio::select! {
        res = connection.serve_connection_with_upgrades(io, svc) => res,
        _ = shutdown_token_conn.cancelled() => Ok(()),
        _ = stop_services_token_conn.cancelled() => Ok(()),
    };

    if let Err(err) = result {
        trace(format!("Connection error: {:?}", err));
    }
}
