use std::{sync::Arc, time::SystemTime};
use tokio::time::Instant;
use dashmap::DashMap;
use hyper::body::Bytes;

pub struct FileReaderCache {
    pub(crate) cache: Arc<DashMap<String, Arc<FileEntry>>>,
    pub(crate) is_caching_enabled: bool,
    pub(crate) cached_items_last_checked: Arc<DashMap<String, (Instant, Instant, SystemTime)>>, // key:filepath, value:(added time, last checked time, last modified time)
    pub(crate) max_file_size: u64,

    // Compression related
    pub(crate) gzip_enabled: bool,
    pub(crate) compressible_content_types: Vec<String>,
    // Caching related headers
    pub(crate) etag_enabled: bool,
    pub(crate) last_modified_header_enabled: bool,
    pub(crate) expires_header_enabled: bool,
    pub(crate) cache_control_header_enabled: bool,
}

pub struct FileEntry {
    pub meta: FileMeta,
    pub content: ContentCache,
}

pub struct ContentCache {
    pub raw: Option<Arc<Bytes>>,
    pub gzip: Option<Arc<Bytes>>,
}

#[derive(Debug)]
pub struct FileMeta {
    pub file_path: String,
    pub is_directory: bool,
    pub exists: bool,
    pub length: u64,
    pub is_too_large_to_store: bool,
    pub mime_type: String,
    pub last_modified: SystemTime,
    // Response caching headers
    pub etag_header: Option<String>,
    pub last_modified_header: Option<String>,
    pub expires_header: Option<String>,
    pub cache_control_header: Option<String>,
}