use std::{sync::Arc, time::SystemTime};
use tokio::time::Instant;
use dashmap::DashMap;

use crate::file::file_entry::FileEntry;

pub struct FileReaderCache {
    // Normal content cache
    pub(crate) cache: Arc<DashMap<String, Arc<FileEntry>>>,
    pub(crate) cache_max_capacity: u64,

    // 404 cache
    pub(crate) cache_404: Arc<DashMap<String, Instant>>,
    pub(crate) cache_404_max_size: u64,

    // General cache settings
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
