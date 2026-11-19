use std::collections::HashMap;
use crate::config::configuration::Configuration;
use crate::config::{request_handler::Processor, site::Site};

use crate::http::request_handlers::processors::{
    load_balancers::load_balancer::LoadBalancerRegistry, php_processor::PHPProcessor, proxy_processor::ProxyProcessor, static_files_processor::StaticFileProcessor,
};

pub struct ProcessorManager {
    // Processors by their IDs
    pub static_file_processors: HashMap<String, StaticFileProcessor>,
    pub php_processors: HashMap<String, PHPProcessor>,
    pub proxy_processors: HashMap<String, ProxyProcessor>,
    // Helpers for processors
    pub load_balancer_registry: LoadBalancerRegistry,
}

impl ProcessorManager {
    pub async fn new() -> Self {
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();
        Self::from_configuration(&config, &[]).await
    }

    pub async fn from_configuration(config: &Configuration, generated_static: &[StaticFileProcessor]) -> Self {
        let mut processor_manager = ProcessorManager {
            static_file_processors: HashMap::new(),
            php_processors: HashMap::new(),
            proxy_processors: HashMap::new(),
            load_balancer_registry: LoadBalancerRegistry::new(),
        };

        // Insert the static file processors from config
        for handler in config.sites.iter().flat_map(|site| &site.request_handlers) {
            processor_manager.insert_processor(&handler.processor);
        }
        for p in generated_static {
            processor_manager.static_file_processors.insert(p.id.clone(), p.clone());
        }

        processor_manager
    }

    fn insert_processor(&mut self, processor: &Processor) {
        match processor {
            Processor::Static(p) => { self.static_file_processors.insert(p.id.clone(), p.clone()); }
            Processor::Php(p) => { self.php_processors.insert(p.id.clone(), p.clone()); }
            Processor::Proxy(p) => { self.proxy_processors.insert(p.id.clone(), p.clone()); }
        }
    }

    pub fn from_site(site: &Site) -> Self {
        let mut manager = Self {
            static_file_processors: HashMap::new(),
            php_processors: HashMap::new(),
            proxy_processors: HashMap::new(),
            load_balancer_registry: LoadBalancerRegistry::new(),
        };
        for handler in &site.request_handlers {
            manager.insert_processor(&handler.processor);
        }
        for processor in manager.proxy_processors.values() {
            manager.load_balancer_registry.create_initial(processor.id.clone(), processor.get_load_balancer_service());
        }
        manager
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
