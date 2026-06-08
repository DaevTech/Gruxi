use crate::logging::access_logging::AccessLogBuffer;
use crate::{
    compression::compression_cache::CompressionCache,
    debug,
    external_connections::external_system_handler::ExternalSystemHandler,
    file::file_reader_cache::FileReaderCache,
    http::{
        client::http_client::HttpClient,
        request_handlers::{processors::processor_manager::ProcessorManager, request_handler_manager::RequestHandlerManager},
        site_match::binding_site_cache::BindingSiteCache,
    },
    logging::log_rotation::LogRotation,
};

pub struct RunningState {
    access_log_buffer: AccessLogBuffer,
    file_reader_cache: FileReaderCache,
    request_handler_manager: RequestHandlerManager,
    processor_manager: ProcessorManager,
    external_system_handler: ExternalSystemHandler,
    http_client: HttpClient,
    binding_site_cache: BindingSiteCache,
    log_rotation: LogRotation,
    compression_cache: CompressionCache,
}

impl RunningState {
    pub async fn new() -> Self {
        let access_log_buffer = AccessLogBuffer::new().await;
        access_log_buffer.start_flushing_task();
        debug!("Access log buffers initialized");

        // Start external system handler, which in turns load any defined external handlers, such as PHP-CGI
        let external_system_handler = ExternalSystemHandler::new().await;
        debug!("External system handler initialized");

        // Start file read cache
        let file_reader_cache = FileReaderCache::new().await;
        debug!("File reader cache initialized");

        // Start request handler manager
        let request_handler_manager = RequestHandlerManager::new().await;
        debug!("Request handler manager initialized");

        // Start processor manager
        let processor_manager = ProcessorManager::new().await;
        debug!("Processor manager initialized");

        // Initialize http clients
        let http_client = HttpClient::new();
        debug!("HTTP client initialized");

        // Start binding site cache
        let binding_site_cache = BindingSiteCache::new();
        binding_site_cache.init().await;
        debug!("Binding <> site cache initialized");

        // Start log rotation manager
        let log_rotation = LogRotation::new().await;

        // Start compression cache
        let compression_cache = CompressionCache::new();
        debug!("Compression cache initialized");

        RunningState {
            access_log_buffer,
            file_reader_cache,
            request_handler_manager,
            processor_manager,
            external_system_handler,
            http_client,
            binding_site_cache,
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

    pub fn get_log_rotation(&self) -> &LogRotation {
        &self.log_rotation
    }

    pub fn get_compression_cache(&self) -> &CompressionCache {
        &self.compression_cache
    }
}
