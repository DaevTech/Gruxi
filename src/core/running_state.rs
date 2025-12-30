use crate::{
    external_connections::external_system_handler::ExternalSystemHandler,
    http::request_handlers::{processors::{load_balancer::load_balancer::LoadBalancer, processor_manager::ProcessorManager}, request_handler_manager::RequestHandlerManager},
    logging::syslog::debug,
};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{file::file_cache::FileCache, logging::access_logging::AccessLogBuffer};

pub struct RunningState {
    pub access_log_buffer: Arc<RwLock<AccessLogBuffer>>,
    pub file_cache: Arc<RwLock<FileCache>>,
    pub request_handler_manager: RequestHandlerManager,
    pub processor_manager: ProcessorManager,
    pub external_system_handler: ExternalSystemHandler,
    pub proxy_processor_load_balancer: LoadBalancer
}

impl RunningState {
    pub async fn new() -> Self {
        let access_log_buffer = AccessLogBuffer::new().await;
        access_log_buffer.start_flushing_task();
        debug("Access log buffers initialized");

        // Start external system handler, which in turns load any defined external handlers, such as PHP-CGI
        let external_system_handler = ExternalSystemHandler::new().await;
        debug("External system handler initialized");

        // Start file cache
        let file_cache = FileCache::new().await;
        debug("File cache initialized");

        // Start request handler manager
        let request_handler_manager = RequestHandlerManager::new().await;
        debug("Request handler manager initialized");

        // Start processor manager
        let processor_manager = ProcessorManager::new().await;
        debug("Processor manager initialized");

        // Start proxy processor load balancer
        let proxy_processor_load_balancer = LoadBalancer::new();


        RunningState {
            access_log_buffer: Arc::new(RwLock::new(access_log_buffer)),
            file_cache: Arc::new(RwLock::new(file_cache)),
            request_handler_manager: request_handler_manager,
            processor_manager: processor_manager,
            external_system_handler: external_system_handler,
            proxy_processor_load_balancer: proxy_processor_load_balancer,
        }
    }

    pub fn get_access_log_buffer(&self) -> Arc<RwLock<AccessLogBuffer>> {
        self.access_log_buffer.clone()
    }

    pub fn get_file_cache(&self) -> Arc<RwLock<FileCache>> {
        self.file_cache.clone()
    }

    pub fn get_request_handler_manager(&self) -> &RequestHandlerManager {
        &self.request_handler_manager
    }

    pub fn get_processor_manager(&self) -> &ProcessorManager {
        &self.processor_manager
    }

    pub fn get_proxy_processor_load_balancer(&self) -> &LoadBalancer {
        &self.proxy_processor_load_balancer
    }

    pub fn get_external_system_handler(&self) -> &ExternalSystemHandler {
        &self.external_system_handler
    }
}
