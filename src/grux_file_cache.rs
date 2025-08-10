use log::{debug, trace, warn};
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, RwLock},
};

pub struct FileCache {
    cache: Arc<RwLock<HashMap<String, CachedFile>>>,
    max_file_size: u64,
}

#[derive(Clone, Debug)]
pub struct CachedFile {
    pub last_checked: std::time::Instant,
    pub is_directory: bool,
    pub exists: bool,
    pub length: u64,
    pub is_too_large: bool,
    pub content: Vec<u8>,
}

impl FileCache {
    /// Create a new file cache with specified capacity and max file size
    /// capacity: Maximum number of files to cache
    /// max_file_size: Maximum size of individual files to cache (in bytes)
    pub fn new(capacity: usize, max_file_size: u64) -> Self {
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1000).unwrap());

        Self {
            cache: Arc::new(RwLock::new(HashMap::with_capacity(capacity.get()))),
            max_file_size,
        }
    }

    // Get file data
    pub fn get_file(&self, file_path: &str) -> Result<CachedFile, std::io::Error> {
        if let Some(cached_file) = self.cache.read().unwrap().get(file_path) {
            Ok(cached_file.clone())
        } else {
            // Not found in cache, so we populate it
            let (length, exists, is_directory) = match std::fs::metadata(file_path) {
                Ok(metadata) => (metadata.len(), true, metadata.is_dir()),
                Err(_) => (0, false, false),
            };

            let content = if is_directory || length > self.max_file_size || !exists || length == 0 {
                Vec::new()
            } else {
                let file_content = match std::fs::read(&file_path) {
                    Ok(content) => content,
                    Err(_) => {
                        trace!("Failed to read file content {}", file_path);
                        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "File not found"));
                    }
                };
                file_content
            };

            let new_cached_file = CachedFile {
                last_checked: std::time::Instant::now(),
                is_directory: is_directory,
                exists: exists,
                length: length,
                is_too_large: length > self.max_file_size,
                content: content,
            };

            self.cache.write().unwrap().insert(file_path.to_string(), new_cached_file.clone());
            return Ok(new_cached_file);
        }
    }
}

// Global file cache instance
lazy_static::lazy_static! {
    static ref GLOBAL_FILE_CACHE: FileCache = {
        // Cache up to 1000 files, max 1MB per file
        FileCache::new(1000, 1 * 1024 * 1024)
    };
}

/// Get the global file cache instance
pub fn get_file_cache() -> &'static FileCache {
    &GLOBAL_FILE_CACHE
}
