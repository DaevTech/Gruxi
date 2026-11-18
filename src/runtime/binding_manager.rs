use crate::config::binding::Protocol;
use crate::config::cached_configuration::get_cached_configuration;
use crate::core::database_connection::get_database_connection;
use crate::error;
use crate::error::gruxi_error_enums::{DatabaseError, GruxiErrorKind, GruxiRuntimeError};
use crate::http::tcp_listener::{create_tcp_listener_with_retry, tcp_connection_handler};
use crate::{config::binding::Binding, error::gruxi_error::GruxiError, info};
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{net::SocketAddr, sync::OnceLock};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub struct BindingManager {
    binding_runtimes: Vec<Arc<BindingRuntime>>,
    tcp_listeners: HashMap<String, Arc<TcpListener>>,
}

#[derive(Debug, Clone)]
pub struct BindingRuntimeConfiguration {
    pub hard_connection_timeout: Duration,
}

impl Default for BindingRuntimeConfiguration {
    fn default() -> Self {
        // Fetch our current configuration for init
        let configuration = get_cached_configuration().get_configuration();

        let runtime_binding_configuration = Self {
            hard_connection_timeout: Duration::from_secs(configuration.core.server_settings.max_connection_duration_seconds),
        };
        runtime_binding_configuration
    }
}

impl BindingRuntimeConfiguration {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug)]
pub struct BindingRuntime {
    pub binding: Binding,
    pub config: BindingRuntimeConfiguration,
    pub stop_binding_token: CancellationToken,
    pub http_builder: Builder<TokioExecutor>,
}

impl BindingManager {
    pub fn new() -> Self {
        Self { binding_runtimes: Vec::new(), tcp_listeners: HashMap::new() }
    }

    pub fn remove_binding(&mut self, binding_id: &str) {
        if let Some(b_index) = self.binding_runtimes.iter().position(|b| b.binding.id == binding_id) {
            let binding_to_be_removed = &self.binding_runtimes.remove(b_index);
            binding_to_be_removed.stop_binding_token.cancel();
            self.tcp_listeners.remove(binding_id);
        }
    }

    pub fn add_binding(&mut self, binding: Binding, config: BindingRuntimeConfiguration) -> Result<(), GruxiError> {
        let existing = self.binding_runtimes.iter().find(|runtime| runtime.binding.id == binding.id);
        let listener = if existing.is_some_and(|runtime| {
            runtime.binding.ip == binding.ip && runtime.binding.port == binding.port && runtime.binding.protocol == binding.protocol
        }) {
            self.tcp_listeners.get(&binding.id).cloned()
        } else {
            None
        };
        let listener = match listener {
            Some(listener) => listener,
            None => Arc::new(self.start_binding(&binding)?),
        };

        self.remove_binding(&binding.id);
        let binding_runtime = BindingRuntime {
            binding: binding,
            config: config,
            stop_binding_token: CancellationToken::new(),
            http_builder: Builder::new(TokioExecutor::new()),
        };
        let arc_binding_runtime = Arc::new(binding_runtime);
        tokio::spawn(tcp_connection_handler(arc_binding_runtime.clone(), listener.clone()));
        self.tcp_listeners.insert(arc_binding_runtime.binding.id.clone(), listener);
        self.binding_runtimes.push(arc_binding_runtime);
        Ok(())
    }

    fn start_binding(&self, binding: &Binding) -> Result<TcpListener, GruxiError> {
        let ip_result = binding.ip.parse::<std::net::IpAddr>();
        let ip = match ip_result {
            Ok(ip_addr) => ip_addr,
            Err(e) => {
                error!("Invalid IP address for binding {}: {}. Could not start server", binding.ip, e);
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Runtime(GruxiRuntimeError::BindingError(format!(
                    "Invalid IP address for binding {}: {}",
                    binding.ip, e
                )))));
            }
        };
        let port = binding.port;
        let addr = SocketAddr::new(ip, port);

        info!("Starting server on {}", addr);

        match binding.protocol {
            Protocol::Tcp => {
                // Start TCP listener
                return create_tcp_listener_with_retry(ip, port);
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

    pub fn get_binding_runtimes(&self) -> &Vec<Arc<BindingRuntime>> {
        &self.binding_runtimes
    }

    pub fn get_configuration_bindings(&self) -> Result<Vec<Binding>, GruxiError> {
        // Load all bindings from the db
        let connection = match get_database_connection() {
            Ok(conn) => conn,
            Err(_) => {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Database(DatabaseError::ConnectionFailed)));
            }
        };

        let bindings = match self.load_bindings_from_db(&connection) {
            Ok(bindings) => bindings,
            Err(e) => {
                return Err(GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), format!("Failed to load bindings from database: {}", e)));
            }
        };

        Ok(bindings)
    }

    fn load_bindings_from_db(&self, connection: &sqlite::Connection) -> Result<Vec<Binding>, sqlite::Error> {
        let mut statement = connection.prepare("SELECT id, ip, port, is_tls, protocol FROM bindings")?;

        let mut bindings = Vec::new();
        while let sqlite::State::Row = statement.next()? {
            let binding_id: String = statement.read(0)?;
            let ip: String = statement.read(1)?;
            let port: i64 = statement.read(2)?;
            let is_tls: i64 = statement.read(3)?;
            let protocol: String = statement.read(4)?;

            bindings.push(Binding {
                id: binding_id,
                ip,
                port: port as u16,
                is_admin: false,
                is_telemetry: false,
                is_tls: is_tls != 0,
                protocol: match protocol.as_str() {
                    "tcp" => Protocol::Tcp,
                    "udp" => Protocol::Udp,
                    _ => Protocol::Tcp,
                },
            });
        }
        Ok(bindings)
    }
}

impl Default for BindingManager {
    fn default() -> Self {
        Self::new()
    }
}

static BINDING_MANAGER_SINGLETON: OnceLock<Mutex<BindingManager>> = OnceLock::new();

pub fn get_binding_manager() -> &'static Mutex<BindingManager> {
    BINDING_MANAGER_SINGLETON.get_or_init(|| Mutex::new(BindingManager::new()))
}
