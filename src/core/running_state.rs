use crate::config::cached_configuration::get_cached_configuration;
use crate::config::request_handler::RequestHandler;
use crate::http::request_handlers::processors::static_files_processor::StaticFileProcessor;
use arc_swap::{ArcSwap, Guard};
use std::{collections::HashSet, sync::Arc};
use crate::logging::access_logging::AccessLogBuffer;
use crate::{
    compression::compression_cache::CompressionCache,
    debug,
    external_connections::external_system_handler::ExternalSystemHandler,
    file::file_reader_cache::FileReaderCache,
    http::{
        client::http_client::HttpClient,
        request_handlers::{processors::processor_manager::ProcessorManager, request_handler_manager::RequestHandlerManager},
    },
    logging::log_rotation::LogRotation,
};

pub struct RunningState {
    access_log_buffer: std::sync::Arc<AccessLogBuffer>,
    file_reader_cache: FileReaderCache,
    site_processing: ArcSwap<SiteProcessing>,
    processing_refresh: tokio::sync::Mutex<()>,
    generated_handlers: Vec<RequestHandler>,
    generated_static: Vec<StaticFileProcessor>,
    external_system_handler: ExternalSystemHandler,
    http_client: HttpClient,
    log_rotation: LogRotation,
    compression_cache: CompressionCache,
}

pub struct SiteProcessing {
    pub request_handlers: RequestHandlerManager,
    pub processors: ProcessorManager,
}

impl RunningState {
    pub async fn new() -> Self {
        // Get configuration
        let cached_configuration = get_cached_configuration();
        let config = cached_configuration.get_configuration();

        // Access log buffer
        let access_log_buffer = std::sync::Arc::new(AccessLogBuffer::new().await);
        access_log_buffer.start_flushing_task();
        debug!("Access log buffers initialized");

        // Start external system handler, which in turns load any defined external handlers, such as PHP-CGI
        let external_system_handler = ExternalSystemHandler::new().await;
        debug!("External system handler initialized");

        // Start file read cache
        let file_reader_cache = FileReaderCache::new(&config);
        debug!("File reader cache initialized");

        // Start request handler manager
        let request_handler_manager = RequestHandlerManager::new().await;
        debug!("Request handler manager initialized");

        // Start processor manager
        let processor_manager = ProcessorManager::new().await;
        debug!("Processor manager initialized");

        let generated_binding_ids: HashSet<_> = config.bindings.iter().filter(|binding| binding.is_admin || binding.is_telemetry).map(|binding| &binding.id).collect();
        let generated_site_ids: HashSet<_> = config.binding_sites.iter().filter(|relation| generated_binding_ids.contains(&relation.binding_id)).map(|relation| &relation.site_id).collect();
        let generated_handler_ids: HashSet<_> = config.sites.iter().filter(|site| generated_site_ids.contains(&site.id)).flat_map(|site| site.request_handlers.iter()).collect();
        let generated_handlers: Vec<_> = config.request_handlers.iter().filter(|handler| generated_handler_ids.contains(&handler.id)).cloned().collect();
        let generated_static_ids: HashSet<_> = generated_handlers.iter().filter(|handler| handler.processor_type == "static").map(|handler| &handler.processor_id).collect();
        let generated_static = config.static_file_processors.iter().filter(|processor| generated_static_ids.contains(&processor.id)).cloned().collect();

        // Initialize http clients
        let http_client = HttpClient::new();
        debug!("HTTP client initialized");

        // Start log rotation manager
        let log_rotation = LogRotation::new().await;

        // Start compression cache
        let compression_cache = CompressionCache::new();
        debug!("Compression cache initialized");

        RunningState {
            access_log_buffer,
            file_reader_cache,
            site_processing: ArcSwap::from_pointee(SiteProcessing { request_handlers: request_handler_manager, processors: processor_manager }),
            processing_refresh: tokio::sync::Mutex::new(()),
            generated_handlers,
            generated_static,
            external_system_handler,
            http_client,
            log_rotation,
            compression_cache,
        }
    }

    pub fn get_access_log_buffer(&self) -> &AccessLogBuffer {
        &self.access_log_buffer
    }

    pub fn get_file_reader_cache(&self) -> &FileReaderCache {
        &self.file_reader_cache
    }

    pub fn get_site_processing(&self) -> Guard<Arc<SiteProcessing>> {
        self.site_processing.load()
    }

    pub async fn refresh_site_processing(&self) -> Result<(), String> {
        let _refresh = self.processing_refresh.lock().await;
        let config = crate::config::load_configuration::fetch_configuration_in_db()?;
        let request_handlers = RequestHandlerManager::from_handlers(config.request_handlers.iter().chain(&self.generated_handlers));
        let processors = ProcessorManager::from_configuration(&config, &self.generated_static).await;
        self.site_processing.store(Arc::new(SiteProcessing { request_handlers, processors }));
        Ok(())
    }

    pub fn get_external_system_handler(&self) -> &ExternalSystemHandler {
        &self.external_system_handler
    }

    pub fn get_http_client(&self) -> &HttpClient {
        &self.http_client
    }

    pub fn get_log_rotation(&self) -> &LogRotation {
        &self.log_rotation
    }

    pub fn get_compression_cache(&self) -> &CompressionCache {
        &self.compression_cache
    }
}
