use crate::config::binding::Protocol;
use crate::config::cached_configuration::get_cached_configuration;
use crate::core::database_connection::get_database_connection;
use crate::error;
use crate::error::gruxi_error_enums::{DatabaseError, GruxiErrorKind, GruxiRuntimeError};
use crate::http::tcp_listener::{create_tcp_listener_with_retry, tcp_connection_handler};
use crate::{config::binding::Binding, error::gruxi_error::GruxiError, info};
use hyper_util::rt::TokioExecutor;
use hyper_util::server::conn::auto::Builder;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::{net::SocketAddr, sync::OnceLock};
use tokio::net::{TcpListener, UdpSocket};
use tokio_util::sync::CancellationToken;

pub struct BindingManager {
    binding_runtimes: Vec<Arc<BindingRuntime>>,
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
    pub tcp_listener: Option<TcpListener>,
    pub udp_listener: Option<UdpSocket>,
    pub stop_binding_token: CancellationToken,
    pub http_builder: Builder<TokioExecutor>,
}

impl BindingManager {
    pub fn new() -> Self {
        Self { binding_runtimes: Vec::new() }
    }

    pub fn remove_binding(&mut self, binding_id: &str) {
        if let Some(b_index) = self.binding_runtimes.iter().position(|b| b.binding.id == binding_id) {
            let binding_to_be_removed = &self.binding_runtimes.remove(b_index);
            binding_to_be_removed.stop_binding_token.cancel();
        }
    }

    pub fn add_binding(&mut self, binding: Binding, config: BindingRuntimeConfiguration) -> Result<(), GruxiError> {
        // Check if an existing binding with that id exist, if so, we stop the current one and start the new like a fresh one
        let _removed = &self.remove_binding(&binding.id);

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
                tokio::spawn(tcp_connection_handler(arc_binding_runtime.clone()));
            }
        }

        self.binding_runtimes.push(arc_binding_runtime);
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
