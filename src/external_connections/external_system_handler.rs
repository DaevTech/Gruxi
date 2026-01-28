use std::{collections::HashMap, sync::Arc};

use tokio::sync::Semaphore;

use crate::{
    external_connections::managed_system::php_cgi::PhpCgi,
    logging::syslog::{error, trace},
};

pub struct ExternalSystemHandler {
    pub php_cgi_id_to_port: HashMap<String, u16>,
    pub connection_semaphore: HashMap<String, Arc<Semaphore>>,
}

impl ExternalSystemHandler {
    pub async fn new() -> Self {
        let mut connection_semaphore = HashMap::new();

        // Get the config, to determine what we need
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

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

            let port_result = new_php_cgi.start().await;
            let port = match port_result {
                Ok(p) => p,
                Err(e) => {
                    error(format!("Failed to start PHP-CGI handler with ID: {}: {}", php_cgi_config.id, e));
                    0
                }
            };

            // If we couldn't start, skip it
            if port == 0 {
                continue;
            }

            // We save the id matched to port for reference
            php_cgi_id_to_port.insert(php_cgi_config.id.clone(), port);

            // Create a connection semaphore for this PHP-CGI instance
            let connection_semaphore_value = Arc::new(Semaphore::new(php_cgi_config.get_max_children_processes() as usize));
            connection_semaphore.insert(php_cgi_config.id.clone(), connection_semaphore_value);

            // Start monitoring thread for this PHP-CGI instance
            tokio::spawn(PhpCgi::start_monitoring_thread(new_php_cgi));

            trace(format!("Initialized PHP-CGI handler with ID: {}", php_cgi_config.id));
        }

        ExternalSystemHandler {
            php_cgi_id_to_port,
            connection_semaphore,
        }
    }

    pub fn get_port_for_php_cgi(&self, php_cgi_id: &str) -> Result<u16, ()> {
        self.php_cgi_id_to_port.get(php_cgi_id).cloned().ok_or(())
    }

    pub fn get_connection_semaphore(&self, external_system_id: &str) -> Option<Arc<Semaphore>> {
        self.connection_semaphore.get(external_system_id).cloned()
    }
}
