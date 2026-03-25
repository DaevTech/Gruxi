use crate::config::binding::Binding;
use crate::core::monitoring::get_monitoring_state;
use crate::core::running_state::RunningState;
use crate::core::running_state_manager::get_running_state_manager;
use crate::http::handle_request::handle_request;
use crate::http::http_util::add_standard_headers_to_response;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::tls::http_tls::build_unified_tls_acceptor;
use crate::tls::shared_acme_manager::initialize_shared_acme_manager;
use crate::{debug, error, info, trace};
use arc_swap::Guard;
use futures::FutureExt;
use hyper::Request;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HttpAutoBuilder;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::select;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

pub struct ConnectionContext {
    pub binding: Binding,
    pub hard_connection_timeout: Duration,
    pub shutdown_token: CancellationToken,
    pub stop_services_token: CancellationToken,
    pub running_state: Guard<Arc<RunningState>>,
}

// Starting all the Gruxi magic
pub async fn initialize_server() {
    // Get configuration from the current configuration
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration().await;

    // Initialize shared ACME manager ONCE before starting any bindings.
    // This ensures all TLS bindings share a single ACME client, resolver, and polling task.
    if let Err(e) = initialize_shared_acme_manager().await {
        panic!("Failed to initialize shared ACME TLS Certificate manager: {}.", e);
    }

    // Starting listening on all configured bindings
    for binding in &config.bindings {
        let ip_result = binding.ip.parse::<std::net::IpAddr>();
        let ip = match ip_result {
            Ok(ip_addr) => ip_addr,
            Err(e) => {
                panic!("Invalid IP address for binding {}: {}. Could not start server", binding.ip, e);
            }
        };
        let port = binding.port;
        let addr = SocketAddr::new(ip, port);

        info!("Starting server on {}", addr);

        // Create the context for this binding to be used in the server task
        let triggers = crate::core::triggers::get_trigger_handler();

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get shutdown token - Could not start server binding. Please report a bug");
                return;
            }
        };

        let stop_services_token_option = triggers.get_token("stop_services").await;
        let stop_services_token = match stop_services_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get stop_services token - Could not start server binding. Please report a bug");
                return;
            }
        };

        // Get the running state
        let running_state = get_running_state_manager().await.get_running_state();

        let context = ConnectionContext {
            binding: binding.clone(),
            hard_connection_timeout: Duration::from_secs(config.core.server_settings.max_connection_duration_seconds),
            shutdown_token: shutdown_token.clone(),
            stop_services_token: stop_services_token.clone(),
            running_state,
        };

        // Start listening on the specified address - spawn each binding as a separate task
        tokio::spawn(start_server_binding(Arc::new(context)));
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
                error!("Failed to bind to {}: {}. Retrying in {:?}...", addr, e, retry_delay);
                tokio::time::sleep(retry_delay).await;
            }
        }
    }
}

async fn start_server_binding(connection_context: Arc<ConnectionContext>) {
    let ip_result = connection_context.binding.ip.parse::<std::net::IpAddr>();
    let ip = match ip_result {
        Ok(ip_addr) => ip_addr,
        Err(e) => {
            panic!("Invalid IP address for binding {}: {}. Could not start server", connection_context.binding.ip, e);
        }
    };
    let port = connection_context.binding.port;
    let addr = SocketAddr::new(ip, port);

    let listener = start_listener_with_retry(addr).await;
    trace!("Listening on binding: {:?}", connection_context.binding);

    // Get the monitoring state to update active connections
    let monitoring_state = get_monitoring_state().await;
    let should_increment_connections_in_queue = !connection_context.binding.is_admin && !connection_context.binding.is_telemetry;

    if connection_context.binding.is_tls {
        // Build unified TLS acceptor that handles both ACME and manual certificates
        let tls_acceptor = match build_unified_tls_acceptor(&connection_context.binding).await {
            Ok(result) => result,
            Err(e) => {
                error!("TLS setup failed for {}:{} => {}", connection_context.binding.ip, connection_context.binding.port, e);
                return;
            }
        };

        // Unified TLS accept loop
        loop {
            select! {
                _ = connection_context.shutdown_token.cancelled() => {
                    trace!("Shutdown signal received, stopping server on {}:{}", connection_context.binding.ip, connection_context.binding.port);
                    break;
                },
                _ = connection_context.stop_services_token.cancelled() => {
                    trace!("Service cancellation signal received, stopping server on {}:{}", connection_context.binding.ip, connection_context.binding.port);
                    break;
                },
                result = listener.accept() => {
                    match result {
                        Ok((tcp_stream, _)) => {
                            let remote_addr_ip = tcp_stream.peer_addr()
                                .map(|addr| addr.ip().to_string())
                                .unwrap_or_else(|_| "<unknown>".to_string());

                            let acceptor = tls_acceptor.clone();
                            let local_connection_context = connection_context.clone();

                            tokio::spawn(async move {
                                match acceptor.accept(tcp_stream).await {
                                    Ok(tls_stream) => {
                                        let io = TokioIo::new(tls_stream);
                                        // Increment connections in queue when connection is ready to be served
                                        if should_increment_connections_in_queue {
                                            monitoring_state.increment_connections_in_queue();
                                        }

                                        if let Err(panic) = std::panic::AssertUnwindSafe(serve_connection(io, local_connection_context, remote_addr_ip)).catch_unwind().await {
                                            handle_connection_panic(panic);
                                        }

                                        // Decrement when connection is fully handled
                                        if should_increment_connections_in_queue {
                                            monitoring_state.decrement_connections_in_queue();
                                        }
                                    }
                                    Err(err) => {
                                        debug!("TLS handshake error: {:?}", err);
                                    }
                                }
                            });
                        }
                        Err(err) => {
                            debug!("Failed to accept connection: {:?}", err);
                        }
                    }
                }
            };
        }
    } else {
        loop {
            select! {
                _ = connection_context.shutdown_token.cancelled() => {
                    trace!("Termination signal received, stopping server on {}:{}", connection_context.binding.ip, connection_context.binding.port);
                    break;
                },
                _ = connection_context.stop_services_token.cancelled() => {
                    trace!("Service stop signal received, stopping server on {}:{}", connection_context.binding.ip, connection_context.binding.port);
                    break;
                },
                result = listener.accept() => {
                    match result {
                        Ok((tcp_stream, _)) => {
                            let remote_addr_ip = tcp_stream.peer_addr()
                                .map(|addr| addr.ip().to_string())
                                .unwrap_or_else(|_| "<unknown>".to_string());

                            let io = TokioIo::new(tcp_stream);
                            let local_connection_context = connection_context.clone();

                            tokio::spawn(async move {
                                // Increment connections in queue when connection is ready to be served
                                if should_increment_connections_in_queue {
                                    monitoring_state.increment_connections_in_queue();
                                }

                                if let Err(panic) = std::panic::AssertUnwindSafe(serve_connection(io, local_connection_context, remote_addr_ip)).catch_unwind().await {
                                    handle_connection_panic(panic);
                                }

                                // Decrement when connection is fully handled
                                if should_increment_connections_in_queue {
                                    monitoring_state.decrement_connections_in_queue();
                                }
                            });
                        }
                        Err(err) => {
                            debug!("Failed to accept connection: {:?}", err);
                        }
                    }
                }
            };
        }
    }
}

fn handle_connection_panic(panic: Box<dyn std::any::Any + Send>) {
    let message = if let Some(s) = panic.downcast_ref::<&str>() {
        *s
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.as_str()
    } else {
        "Panic occured but payload is not a string"
    };
    error!(
        "Panic occurred in request handling while serving connection: {:?} - Please submit a bug with this information and message",
        message
    );
}

// Helper function to serve a connection (works for both TLS and non-TLS)
async fn serve_connection<S>(io: TokioIo<S>, connection_context: Arc<ConnectionContext>, remote_addr_ip: String)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let local_connection_context = connection_context.clone();

    let svc = service_fn(move |req: Request<Incoming>| {
        let remote_ip = remote_addr_ip.clone();

        let conn_context = local_connection_context.clone();
        async move {
            // Count the request in monitoring, except for admin bindings
            if !conn_context.binding.is_admin {
                get_monitoring_state().await.increment_requests_served();
            }

            let mut gruxi_request = GruxiRequest::from_hyper(req);
            gruxi_request.set_remote_ip(remote_ip.clone());
            let gruxi_response_result = handle_request(gruxi_request, conn_context).await;
            let mut response = match gruxi_response_result {
                Err(err) => {
                    error!("Error handling request from {}: {:?}", &remote_ip, err);

                    GruxiResponse::new_empty_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16())
                }
                Ok(response) => response,
            };

            // Add standard headers
            add_standard_headers_to_response(&mut response);

            debug!("Responding with: {:?}", response);

            // Convert gruxi_response to hyper response
            Ok::<_, std::convert::Infallible>(response.into_hyper())
        }
    });

    let connection = HttpAutoBuilder::new(TokioExecutor::new());

    // Serve the connection and listen for shutdown signals
    let conn_context = connection_context.clone();
    let result = tokio::select! {
        res = timeout(conn_context.hard_connection_timeout, connection.serve_connection_with_upgrades(io, svc)) => res,
        _ = conn_context.shutdown_token.cancelled() => return,
        _ = conn_context.stop_services_token.cancelled() => return,
    };

    if result.is_err() {
        trace!("Connection timed out due to hard timeout");
    }
}
