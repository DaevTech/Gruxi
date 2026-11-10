use crate::{core::monitoring::get_monitoring_state, runtime::binding_manager::BindingRuntime, tls::http_tls::build_unified_tls_acceptor};
use std::{
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{net::TcpListener, select};

use crate::error::{
    gruxi_error::GruxiError,
    gruxi_error_enums::{GruxiErrorKind, GruxiRuntimeError},
};

use crate::http::handle_request::handle_request;
use crate::http::http_util::add_standard_headers_to_response;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::{debug, error, trace};
use futures::FutureExt;
use hyper::Request;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::time::timeout;

pub fn create_tcp_listener_with_retry(ip: IpAddr, port: u16) -> Result<TcpListener, GruxiError> {
    let addr = SocketAddr::new(ip, port);

    // Implement a simple retry mechanism
    let mut attempts = 0;
    let max_attempts = 5;
    let retry_delay = std::time::Duration::from_millis(100);

    loop {
        let listener = std::net::TcpListener::bind(addr).and_then(|l| {
            l.set_nonblocking(true)?;
            TcpListener::from_std(l)
        });

        match listener {
            Ok(listener) => {
                return Ok(listener);
            }
            Err(e) => {
                attempts += 1;
                if attempts >= max_attempts {
                    error!("Failed to bind to {} after {} attempts: {}", addr, attempts, e);
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Runtime(GruxiRuntimeError::BindingError(format!(
                        "Failed to bind to {} after {} attempts: {}",
                        addr, attempts, e
                    )))));
                }
                error!("Failed to bind to {}: {}. Retrying in {:?}...", addr, e, retry_delay);
                std::thread::sleep(retry_delay);
            }
        }
    }
}

pub async fn tcp_connection_handler(binding_runtime: Arc<BindingRuntime>) {
    let should_increment_connections_in_queue = !binding_runtime.binding.is_admin && !binding_runtime.binding.is_telemetry;

    if binding_runtime.binding.is_tls {
        // Build unified TLS acceptor that handles both ACME and manual certificates
        let tls_acceptor = match build_unified_tls_acceptor(&binding_runtime.binding).await {
            Ok(result) => result,
            Err(e) => {
                error!("TLS setup failed for {}:{} => {}", binding_runtime.binding.ip, binding_runtime.binding.port, e);
                return;
            }
        };

        // Unified TLS accept loop
        loop {
            let local_binding_runtime = binding_runtime.clone();

            select! {
                _ = local_binding_runtime.stop_binding_token.cancelled() => {
                    trace!("Binding stop signal received, stopping server on {}:{}", binding_runtime.binding.ip, binding_runtime.binding.port);
                    break;
                },
                result = binding_runtime.tcp_listener.as_ref().unwrap().accept() => {
                    match result {
                        Ok((tcp_stream, _)) => {
                            let remote_socket = tcp_stream.peer_addr();

                            let acceptor = tls_acceptor.clone();
                            let local_binding_runtime = local_binding_runtime.clone();

                            tokio::spawn(async move {
                                match acceptor.accept(tcp_stream).await {
                                    Ok(tls_stream) => {
                                        let io = TokioIo::new(tls_stream);
                                        // Increment connections in queue when connection is ready to be served
                                        if should_increment_connections_in_queue {
                                            get_monitoring_state().increment_connections_in_queue();
                                        }

                                        if let Err(panic) = std::panic::AssertUnwindSafe(serve_connection(io, local_binding_runtime.clone(), remote_socket)).catch_unwind().await {
                                            handle_connection_panic(panic);
                                        }

                                        // Decrement when connection is fully handled
                                        if should_increment_connections_in_queue {
                                            get_monitoring_state().decrement_connections_in_queue();
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
            let local_binding_runtime = binding_runtime.clone();

            select! {
                _ = local_binding_runtime.stop_binding_token.cancelled() => {
                    trace!("Binding stop signal received, stopping server on {}:{}", binding_runtime.binding.ip, binding_runtime.binding.port);
                    break;
                },
                result = binding_runtime.tcp_listener.as_ref().unwrap().accept() => {
                    match result {
                        Ok((tcp_stream, _)) => {
                            let remote_socket = tcp_stream.peer_addr();

                            let io = TokioIo::new(tcp_stream);
                            let local_binding_runtime = local_binding_runtime.clone();

                            tokio::spawn(async move {
                                // Increment connections in queue when connection is ready to be served
                                if should_increment_connections_in_queue {
                                    get_monitoring_state().increment_connections_in_queue();
                                }

                                if let Err(panic) = std::panic::AssertUnwindSafe(serve_connection(io, local_binding_runtime.clone(), remote_socket)).catch_unwind().await {
                                    handle_connection_panic(panic);
                                }

                                // Decrement when connection is fully handled
                                if should_increment_connections_in_queue {
                                    get_monitoring_state().decrement_connections_in_queue();
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
async fn serve_connection<S>(io: TokioIo<S>, binding_runtime: Arc<BindingRuntime>, remote_addr: Result<SocketAddr, std::io::Error>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let local_binding_runtime = binding_runtime.clone();
    let remote_addr_arc = Arc::new(remote_addr);

    let svc = service_fn(move |req: Request<Incoming>| {
        let remote_addr_local = remote_addr_arc.clone();

        let binding_runtime = local_binding_runtime.clone();
        async move {
            // Count the request in monitoring, except for admin bindings
            if !binding_runtime.binding.is_admin && !binding_runtime.binding.is_telemetry {
                get_monitoring_state().increment_requests_served();
            }

            let mut gruxi_request = GruxiRequest::from_hyper(req);
            if let Ok(remote_ip) = &*remote_addr_local {
                gruxi_request.set_remote_ip(*remote_ip);
            }
            let gruxi_response_result = handle_request(gruxi_request, binding_runtime).await;
            let mut response = match gruxi_response_result {
                Err(err) => {
                    error!("Error handling request from {:?}: {:?}", &remote_addr_local, err);

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

    // Serve the connection and listen for shutdown signals
    let binding_runtime = binding_runtime.clone();
    let result = tokio::select! {
        res = timeout(binding_runtime.config.hard_connection_timeout, binding_runtime.http_builder.serve_connection_with_upgrades(io, svc)) => res,
          _ = binding_runtime.stop_binding_token.cancelled() => return,
    };

    if result.is_err() {
        trace!("Connection timed out due to hard timeout");
    }
}
