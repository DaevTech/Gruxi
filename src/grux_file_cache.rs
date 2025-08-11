use crate::grux_configuration_struct::FileCache as GruxFileCacheConfig;
use flate2::Compression;
use flate2::write::GzEncoder;
use log::{info, trace, warn};
use std::io::Write;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::time::interval;

pub struct FileCache {
    is_enabled: bool,
    cache: Arc<RwLock<HashMap<String, CachedFile>>>,
    max_file_size: u64,
    gzip_enabled: bool,
    compressible_content_types: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CachedFile {
    pub last_checked: std::time::Instant,
    pub is_directory: bool,
    pub exists: bool,
    pub length: u64,
    pub is_too_large: bool,
    pub content: Vec<u8>,
    pub mime_type: String,
    pub gzip_content: Vec<u8>,
}

impl FileCache {
    /// Create a new file cache with specified capacity and max file size
    /// capacity: Maximum number of files to cache
    /// max_file_size: Maximum size of individual files to cache (in bytes)
    pub fn new() -> Self {
        // Get configuration
        let config = crate::grux_configuration::get_configuration();
        let file_data_config: GruxFileCacheConfig = config.get("core.file_cache").unwrap();

        let is_enabled = file_data_config.is_enabled;
        let max_file_size = file_data_config.cache_max_size_per_file as u64;
        let capacity = file_data_config.cache_size;
        let max_item_lifetime = file_data_config.cache_max_item_lifetime;
        let cleanup_thread_interval = file_data_config.cleanup_thread_interval;

        let compressible_content_types = config.get::<Vec<String>>("core.gzip.compressible_content_types").unwrap_or(vec![]);
        let gzip_enabled = config.get_bool("core.gzip.is_enabled").unwrap_or(false);

        let mut hashmap = HashMap::with_capacity(0);
        if is_enabled {
            hashmap = HashMap::with_capacity(NonZeroUsize::new(capacity).unwrap().get());
        }

        let cache = Arc::new(RwLock::new(hashmap));

        // Start the cleanup thread
        let cache_clone_clean = cache.clone();
        tokio::spawn(async move {
            Self::cleanup_thread_static(cache_clone_clean, max_item_lifetime, cleanup_thread_interval).await;
        });

        Self {
            is_enabled,
            cache,
            max_file_size,
            compressible_content_types,
            gzip_enabled,
        }
    }

    // Get file data
    pub fn get_file(&self, file_path: &str) -> Result<CachedFile, std::io::Error> {
        if let Some(cached_file) = self.cache.read().unwrap().get(file_path) {
            Ok(cached_file.clone())
        } else {
            // Not found in cache, so we populate it
            trace!("File/dir not found in cache, reading from disk: {}", file_path);
            let (length, exists, is_directory) = match std::fs::metadata(file_path) {
                Ok(metadata) => (metadata.len(), true, metadata.is_dir()),
                Err(_) => (0, false, false),
            };

            // If its a file and has content, read it
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

            let mut mime_type = String::new();
            let mut gzip_content = Vec::new();

            if !is_directory && length > 0 && exists && !content.is_empty() {
                // Handle the mime guessing
                mime_type = mime_guess::from_path(&file_path).first_or_octet_stream().to_string();
                trace!("Guessed MIME type for {}: {}", file_path, mime_type);

                // Create gzip version if content type is compressible
                if self.should_compress(&mime_type, length) {
                    match self.compress_content(&content, &mut gzip_content) {
                        Ok(_) => {
                            // Only keep compressed version if it's significantly smaller
                            if gzip_content.len() > (content.len() * (90 / 100)) {
                                trace!("Compressed version not significantly smaller, skipping for: {}", file_path);
                                gzip_content.clear();
                            }
                        }
                        Err(e) => {
                            warn!("Failed to compress file {}: {}", file_path, e);
                        }
                    }
                };
            }

            let new_cached_file = CachedFile {
                last_checked: std::time::Instant::now(),
                is_directory: is_directory,
                exists: exists,
                length: length,
                is_too_large: length > self.max_file_size,
                content: content,
                mime_type: mime_type,
                gzip_content: gzip_content,
            };

            if self.is_enabled {
                trace!("New cached file/dir: {:?}", new_cached_file);
                self.cache.write().unwrap().insert(file_path.to_string(), new_cached_file.clone());
            }

            return Ok(new_cached_file);
        }
    }

    /// Check if a MIME type should be compressed
    fn should_compress(&self, mime_type: &str, content_length: u64) -> bool {
        if self.gzip_enabled {
            return content_length > 1000 && content_length < (10 * 1024 * 1024) && self.compressible_content_types.iter().any(|ct| mime_type.starts_with(ct));
        }
        false
    }

    /// Background cleanup thread that periodically removes old cache entries
    async fn cleanup_thread_static(cache: Arc<RwLock<HashMap<String, CachedFile>>>, max_item_lifetime: usize, cleanup_thread_interval: usize) {
        let mut interval = interval(Duration::from_secs(cleanup_thread_interval as u64));

        loop {
            interval.tick().await;

            let cache_read = cache.read().unwrap();

            // Figure out how many bytes are currently used and how many items are in the cache
            let cache_bytes_used: usize = cache_read.values().map(|f| f.content.len() + f.gzip_content.len()).sum();
            drop(cache_read);

            let now = std::time::Instant::now();
            let max_age = Duration::from_secs(max_item_lifetime as u64);

            // Get write lock and remove old entries
            if let Ok(mut cache_map) = cache.write() {
                let initial_count = cache_map.len();

                cache_map.retain(|_path, cached_file| {
                    let age = now.duration_since(cached_file.last_checked);
                    if age > max_age { false } else { true }
                });

                let final_count = cache_map.len();
                let removed_count = initial_count.saturating_sub(final_count);

                let cache_bytes_used_after: usize = cache_map.values().map(|f| f.content.len() + f.gzip_content.len()).sum();

                info!(
                    "Memory file data cache cleanup: removed {} expired, {} remaining. Cache size: {} bytes -> {} bytes",
                    removed_count, final_count, cache_bytes_used, cache_bytes_used_after
                );
            }
        }
    }

    /// Compress content using gzip
    fn compress_content(&self, content: &[u8], gzip_content: &mut Vec<u8>) -> Result<(), std::io::Error> {
        let mut encoder = GzEncoder::new(gzip_content, Compression::default());
        encoder.write_all(content)?;
        encoder.finish()?;
        Ok(())
    }
}

// Global file cache instance
lazy_static::lazy_static! {
    static ref GLOBAL_FILE_CACHE: FileCache = {
        FileCache::new()
    };
}

/// Get the global file cache instance
pub fn get_file_cache() -> &'static FileCache {
    &GLOBAL_FILE_CACHE
}
