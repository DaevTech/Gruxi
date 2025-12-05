use std::sync::Arc;
use crate::logging::syslog::info;
use tokio::sync::RwLock;

use crate::{
    external_request_handlers::external_request_handlers::ExternalRequestHandlers, file::file_cache::FileCache, logging::access_logging::AccessLogBuffer
};

pub struct RunningState {
    pub access_log_buffer: Arc<RwLock<AccessLogBuffer>>,
    pub external_request_handlers: Arc<RwLock<ExternalRequestHandlers>>,
    pub file_cache: Arc<RwLock<FileCache>>,
}

impl RunningState {
    pub async fn new() -> Self {
        let access_log_buffer = AccessLogBuffer::new().await;
        access_log_buffer.start_flushing_task();
        info("Access log buffers initialized");

        // Start external request handlers
        let external_request_handlers = ExternalRequestHandlers::new().await;
        info("External request handlers initialized");

        // Start file cache
        let file_cache = FileCache::new().await;

        RunningState {
            access_log_buffer: Arc::new(RwLock::new(access_log_buffer)),
            external_request_handlers: Arc::new(RwLock::new(external_request_handlers)),
            file_cache: Arc::new(RwLock::new(file_cache)),
        }
    }

    pub fn get_external_request_handlers(&self) -> Arc<RwLock<ExternalRequestHandlers>> {
        self.external_request_handlers.clone()
    }

    pub fn get_access_log_buffer(&self) -> Arc<RwLock<AccessLogBuffer>> {
        self.access_log_buffer.clone()
    }

    pub fn get_file_cache(&self) -> Arc<RwLock<FileCache>> {
        self.file_cache.clone()
    }
}
