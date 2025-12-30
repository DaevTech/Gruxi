use std::collections::HashMap;

use crate::http::request_handlers::processors::{php_processor::PHPProcessor, proxy_processor::ProxyProcessor, static_files_processor::StaticFileProcessor};

pub struct ProcessorManager {
    pub static_file_processors: HashMap<String, StaticFileProcessor>,
    pub php_processors: HashMap<String, PHPProcessor>,
    pub proxy_processors: HashMap<String, ProxyProcessor>,
}

impl ProcessorManager {
    pub async fn new() -> Self {
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        let mut processor_manager = ProcessorManager {
            static_file_processors: HashMap::new(),
            php_processors: HashMap::new(),
            proxy_processors: HashMap::new(),
        };

        // Insert the static file processors from config
        config.static_file_processors.iter().for_each(|p| {
            processor_manager.static_file_processors.insert(p.id.clone(), p.clone());
        });

        // Insert the PHP processors from config
        config.php_processors.iter().for_each(|p| {
            processor_manager.php_processors.insert(p.id.clone(), p.clone());
        });

        // Insert the proxy processors from config
        config.proxy_processors.iter().for_each(|p| {
            processor_manager.proxy_processors.insert(p.id.clone(), p.clone());
        });

        processor_manager
    }

    pub fn get_static_file_processor_by_id(&self, processor_id: &String) -> Option<&StaticFileProcessor> {
        self.static_file_processors.get(processor_id)
    }

    pub fn get_php_processor_by_id(&self, processor_id: &String) -> Option<&PHPProcessor> {
        self.php_processors.get(processor_id)
    }

    pub fn get_proxy_processor_by_id(&self, processor_id: &String) -> Option<&ProxyProcessor> {
        self.proxy_processors.get(processor_id)
    }
}
