use crate::{
    external_connections::external_system_handler::ExternalSystemHandler, file::file_reader_structs::FileReaderCache, http::{
        client::http_client::HttpClient,
        request_handlers::{processors::processor_manager::ProcessorManager, request_handler_manager::RequestHandlerManager}, site_match::binding_site_cache::BindingSiteCache,
    }, logging::syslog::debug, tls::tls_cert_manager::TlsCertManager
};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{logging::access_logging::AccessLogBuffer};

pub struct RunningState {
    pub access_log_buffer: Arc<RwLock<AccessLogBuffer>>,
    pub file_reader_cache: FileReaderCache,
    pub request_handler_manager: RequestHandlerManager,
    pub processor_manager: ProcessorManager,
    pub external_system_handler: ExternalSystemHandler,
    pub http_client: HttpClient,
    pub binding_site_cache: BindingSiteCache,
    pub tls_cert_manager: TlsCertManager,
}

impl RunningState {
    pub async fn new() -> Self {
        let access_log_buffer = AccessLogBuffer::new().await;
        access_log_buffer.start_flushing_task();
        debug("Access log buffers initialized");

        // Start external system handler, which in turns load any defined external handlers, such as PHP-CGI
        let external_system_handler = ExternalSystemHandler::new().await;
        debug("External system handler initialized");

        // Start file read cache
        let file_reader_cache = FileReaderCache::new().await;
        debug("File reader cache initialized");

        // Start request handler manager
        let request_handler_manager = RequestHandlerManager::new().await;
        debug("Request handler manager initialized");

        // Start processor manager
        let processor_manager = ProcessorManager::new().await;
        debug("Processor manager initialized");

        // Initialize http clients
        let http_client = HttpClient::new();
        debug("HTTP client initialized");

        // Start binding site cache
        let binding_site_cache = BindingSiteCache::new();
        binding_site_cache.init().await;
        debug("Binding<>site cache initialized");

        // Start TLS certificate manager
        let tls_cert_manager = TlsCertManager::new().await;
        TlsCertManager::start_certificate_loop().await;
        debug("TLS certificate manager initialized");

        RunningState {
            access_log_buffer: Arc::new(RwLock::new(access_log_buffer)),
            file_reader_cache: file_reader_cache,
            request_handler_manager: request_handler_manager,
            processor_manager: processor_manager,
            external_system_handler: external_system_handler,
            http_client: http_client,
            binding_site_cache: binding_site_cache,
            tls_cert_manager: tls_cert_manager,
        }
    }

    pub fn get_access_log_buffer(&self) -> Arc<RwLock<AccessLogBuffer>> {
        self.access_log_buffer.clone()
    }

    pub fn get_file_reader_cache(&self) -> &FileReaderCache {
        &self.file_reader_cache
    }

    pub fn get_request_handler_manager(&self) -> &RequestHandlerManager {
        &self.request_handler_manager
    }

    pub fn get_processor_manager(&self) -> &ProcessorManager {
        &self.processor_manager
    }

    pub fn get_external_system_handler(&self) -> &ExternalSystemHandler {
        &self.external_system_handler
    }

    pub fn get_http_client(&self) -> &HttpClient {
        &self.http_client
    }

    pub fn get_binding_site_cache(&self) -> &BindingSiteCache {
        &self.binding_site_cache
    }

    pub fn get_tls_cert_manager(&self) -> &TlsCertManager {
        &self.tls_cert_manager
    }
}
