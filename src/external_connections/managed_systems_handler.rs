use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{GruxiErrorKind, ManagedSystemErrorKind};
use crate::external_connections::managed_system::php_cgi::PhpCgi;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct ManagedSystemsHandler {
    php_cgi_id_to_port: HashMap<String, Arc<AtomicU16>>,
    connection_semaphore: HashMap<String, Arc<Semaphore>>,
    stop_tokens: HashMap<String, CancellationToken>,
    process_tasks: HashMap<String, JoinHandle<()>>,
}

impl ManagedSystemsHandler {
    pub fn new() -> Self {
        Self {
            php_cgi_id_to_port: HashMap::new(),
            connection_semaphore: HashMap::new(),
            stop_tokens: HashMap::new(),
            process_tasks: HashMap::new(),
        }
    }

    pub async fn start_managed_system(&mut self, config: &PhpCgi) -> Result<(), GruxiError> {
        if self.stop_tokens.contains_key(&config.id) {
            self.stop_managed_system(&config.id).await?;
        }

        let mut process = PhpCgi::new(config.id.clone(), config.name.clone(), config.request_timeout, config.concurrent_threads, config.executable.clone());
        let shared_port = process.get_shared_port();
        process.start().await.map_err(|error| {
            GruxiError::new(
                GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::StartFailed),
                format!("Failed to start managed system {}: {}", config.id, error),
            )
        })?;

        let id = config.id.clone();
        let stop_token = CancellationToken::new();
        let process_task = tokio::spawn(PhpCgi::start_monitoring_thread(process, stop_token.clone()));
        self.php_cgi_id_to_port.insert(id.clone(), shared_port);
        self.connection_semaphore.insert(id.clone(), Arc::new(Semaphore::new(config.get_max_children_processes() as usize)));
        self.stop_tokens.insert(id.clone(), stop_token);
        self.process_tasks.insert(id, process_task);
        Ok(())
    }

    pub async fn stop_managed_system(&mut self, id: &str) -> Result<(), GruxiError> {
        if let Some(stop_token) = self.stop_tokens.remove(id) {
            stop_token.cancel();
        }
        let task_result = match self.process_tasks.remove(id) {
            Some(task) => task.await,
            None => Ok(()),
        };
        self.php_cgi_id_to_port.remove(id);
        self.connection_semaphore.remove(id);
        task_result.map_err(|error| {
            GruxiError::new(
                GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::StopFailed),
                format!("Failed while stopping managed system {}: {}", id, error),
            )
        })
    }

    pub fn get_port_for_php_cgi(&self, id: &str) -> Result<u16, GruxiError> {
        self.php_cgi_id_to_port
            .get(id)
            .map(|port| port.load(Ordering::SeqCst))
            .filter(|port| *port != 0)
            .ok_or(GruxiError::new_with_kind_only(GruxiErrorKind::Internal("managed system port unavailable")))
    }

    pub fn get_connection_semaphore(&self, id: &str) -> Option<Arc<Semaphore>> {
        self.connection_semaphore.get(id).cloned()
    }
}
