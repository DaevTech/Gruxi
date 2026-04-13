use std::{collections::HashMap, sync::Arc};
use std::sync::atomic::{AtomicU16, Ordering};
use crate::error;
use tokio::sync::Semaphore;

use crate::{
    error::{ gruxi_error::GruxiError, gruxi_error_enums::GruxiErrorKind}, external_connections::managed_system::php_cgi::PhpCgi, trace
};

pub struct ExternalSystemHandler {
    pub php_cgi_id_to_port: HashMap<String, Arc<AtomicU16>>,
    pub connection_semaphore: HashMap<String, Arc<Semaphore>>,
}

impl ExternalSystemHandler {
    pub async fn new() -> Self {
        let mut connection_semaphore = HashMap::new();

        // Get the config, to determine what we need
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();

        let mut php_cgi_id_to_port = HashMap::new();

        // Load PHP-CGI handlers from configuration
        for php_cgi_config in &config.php_cgi_handlers {
            let mut new_php_cgi = PhpCgi::new(
                php_cgi_config.id.clone(),
                php_cgi_config.name.clone(),
                php_cgi_config.request_timeout,
                php_cgi_config.concurrent_threads,
                php_cgi_config.executable.clone(),
            );

            // Get a shared reference to the port before starting — this Arc stays
            // in sync even if the monitoring thread restarts the process on a new port.
            let shared_port = new_php_cgi.get_shared_port();

            if let Err(e) = new_php_cgi.start().await {
                error!("Failed to start PHP-CGI handler with ID: {}: {}", php_cgi_config.id, e);
                continue;
            }

            // We save the shared port reference for this PHP-CGI instance
            php_cgi_id_to_port.insert(php_cgi_config.id.clone(), shared_port);

            // Create a connection semaphore for this PHP-CGI instance
            let connection_semaphore_value = Arc::new(Semaphore::new(php_cgi_config.get_max_children_processes() as usize));
            connection_semaphore.insert(php_cgi_config.id.clone(), connection_semaphore_value);

            // Start monitoring thread for this PHP-CGI instance
            tokio::spawn(PhpCgi::start_monitoring_thread(new_php_cgi));

            trace!("Initialized PHP-CGI handler with ID: {}", php_cgi_config.id);
        }

        ExternalSystemHandler {
            php_cgi_id_to_port,
            connection_semaphore,
        }
    }

    pub fn get_port_for_php_cgi(&self, php_cgi_id: &str) -> Result<u16, GruxiError> {
        self.php_cgi_id_to_port
            .get(php_cgi_id)
            .map(|p| p.load(Ordering::SeqCst))
            .filter(|&p| p != 0)
            .ok_or(GruxiError::new_with_kind_only(GruxiErrorKind::Internal("")))
    }

    pub fn get_connection_semaphore(&self, external_system_id: &str) -> Option<Arc<Semaphore>> {
        self.connection_semaphore.get(external_system_id).cloned()
    }
}
