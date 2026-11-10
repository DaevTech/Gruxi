use crate::config::binding::Protocol;
use crate::error;
use crate::error::gruxi_error_enums::{GruxiErrorKind, GruxiRuntimeError};
use crate::http::tcp_listener::{create_tcp_listener_with_retry, tcp_connection_handler};
use crate::{config::binding::Binding, error::gruxi_error::GruxiError, info};
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use std::sync::Arc;
use std::time::Duration;
use std::{net::SocketAddr, sync::OnceLock};
use tokio::net::{TcpListener, UdpSocket};
use tokio_util::sync::CancellationToken;

pub struct BindingManager {
    bindings: Vec<Arc<BindingRuntime>>,
}

#[derive(Debug, Clone)]
pub struct BindingRuntimeConfiguration {
    pub hard_connection_timeout: Duration,
}
#[derive(Debug)]
pub struct BindingRuntime {
    pub binding: Binding,
    pub config: BindingRuntimeConfiguration,
    pub tcp_listener: Option<TcpListener>,
    pub udp_listener: Option<UdpSocket>,
    pub stop_binding_token: CancellationToken,
    pub http_builder: Builder<TokioExecutor>,
}

impl BindingManager {
    pub fn new() -> Self {
        Self { bindings: Vec::new() }
    }

    pub fn add_binding(&mut self, binding: Binding, config: BindingRuntimeConfiguration) -> Result<(), GruxiError> {
        // Check if an existing binding with the same IP and port already exists
        if self.bindings.iter().any(|b| b.binding.ip == binding.ip && b.binding.port == binding.port) {
            error!("Binding with IP {} and port {} already exists", binding.ip, binding.port);
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Runtime(GruxiRuntimeError::BindingError(format!(
                "Binding with IP {} and port {} already exists",
                binding.ip, binding.port
            )))));
        }

        let mut binding_runtime = BindingRuntime {
            binding: binding,
            config: config,
            tcp_listener: None,
            udp_listener: None,
            stop_binding_token: CancellationToken::new(),
            http_builder: Builder::new(TokioExecutor::new()),
        };

        // We start it before adding it to the list, and if fail, we return the error
        match self.start_binding(&mut binding_runtime) {
            Ok(_) => (),
            Err(e) => {
                error!("Failed to start binding {}: {:?}", &binding_runtime.binding.ip, e);
                return Err(e);
            }
        };

        let arc_binding_runtime = Arc::new(binding_runtime);

        // Start connection handler for the binding if it's TCP
        if let Protocol::Tcp = arc_binding_runtime.binding.protocol {
            if let Some(_) = &arc_binding_runtime.tcp_listener {
                let _join_handle = tokio::spawn(tcp_connection_handler(arc_binding_runtime.clone()));
            }
        }

        self.bindings.push(arc_binding_runtime);
        Ok(())
    }

    fn start_binding(&self, binding_runtime: &mut BindingRuntime) -> Result<(), GruxiError> {
        let ip_result = binding_runtime.binding.ip.parse::<std::net::IpAddr>();
        let ip = match ip_result {
            Ok(ip_addr) => ip_addr,
            Err(e) => {
                error!("Invalid IP address for binding {}: {}. Could not start server", binding_runtime.binding.ip, e);
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Runtime(GruxiRuntimeError::BindingError(format!(
                    "Invalid IP address for binding {}: {}",
                    binding_runtime.binding.ip, e
                )))));
            }
        };
        let port = binding_runtime.binding.port;
        let addr = SocketAddr::new(ip, port);

        info!("Starting server on {}", addr);

        match binding_runtime.binding.protocol {
            Protocol::Tcp => {
                // Start TCP listener
                binding_runtime.tcp_listener = Some(create_tcp_listener_with_retry(ip, port)?);
                return Ok(());
            }
            Protocol::Udp => {
                // Start UDP listener (for HTTP3)
                // Not implemented yet
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Runtime(GruxiRuntimeError::BindingError(
                    "UDP protocol not implemented yet".to_string(),
                ))));
            }
        };
    }
}

impl Default for BindingManager {
    fn default() -> Self {
        Self::new()
    }
}

static BINDING_MANAGER_SINGLETON: OnceLock<BindingManager> = OnceLock::new();

pub fn get_binding_manager() -> &'static BindingManager {
    BINDING_MANAGER_SINGLETON.get_or_init(|| BindingManager::new())
}
