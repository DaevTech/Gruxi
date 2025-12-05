use flate2::Compression;
use flate2::write::GzEncoder;
use crate::logging::syslog::{debug, trace, warn};
use tokio::select;
use std::io::Write;
use std::time::Instant;
use std::time::SystemTime;
use std::{
    collections::HashMap,
    num::NonZeroUsize,
    sync::{Arc, RwLock},
    time::Duration,
};
use tokio::time::interval;

use crate::configuration::cached_configuration::get_cached_configuration;
use crate::core::triggers::get_trigger_handler;

pub struct FileCache {
    is_enabled: bool,
    cache: Arc<RwLock<HashMap<String, CachedFile>>>,
    cached_items_last_checked: Arc<RwLock<HashMap<String, (Instant, Instant, SystemTime)>>>,
    max_file_size: u64,
    gzip_enabled: bool,
    compressible_content_types: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CachedFile {
    pub file_path: String,
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
    pub async fn new() -> Self {
        // Get configuration
        let cached_configuration = get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        let file_data_config = &config.core.file_cache;

        let is_enabled = file_data_config.is_enabled;
        let max_file_size = file_data_config.cache_max_size_per_file as u64;
        let capacity = file_data_config.cache_item_size;
        let max_item_lifetime = file_data_config.max_item_lifetime;
        let cleanup_thread_interval = file_data_config.cleanup_thread_interval;
        let forced_eviction_threshold = file_data_config.forced_eviction_threshold;

        let compressible_content_types = &config.core.gzip.compressible_content_types;
        let gzip_enabled = &config.core.gzip.is_enabled;

        let mut hashmap = HashMap::with_capacity(0);
        if is_enabled {
            hashmap = HashMap::with_capacity(NonZeroUsize::new(capacity).unwrap().get());
        }

        let cache = Arc::new(RwLock::new(hashmap));
        let cached_items_last_checked = Arc::new(RwLock::new(HashMap::new()));

        // Start the cleanup thread
        if is_enabled {
            // Update/cleanup cache thread
            let cache_clone_update = cache.clone();
            let last_checked_clone = cached_items_last_checked.clone();
            let eviction_threshold: f64 = (capacity as f64 * (forced_eviction_threshold as f64 / 100.0)).round();

            tokio::spawn(async move {
                Self::update_cache(cache_clone_update, last_checked_clone, cleanup_thread_interval, max_item_lifetime, eviction_threshold as usize).await;
            });
        }

        Self {
            is_enabled,
            cache,
            max_file_size,
            compressible_content_types: compressible_content_types.to_vec(),
            cached_items_last_checked,
            gzip_enabled: *gzip_enabled,
        }
    }

    pub fn get_current_item_count(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    // Get file data
    pub fn get_file(&self, file_path: &str) -> Result<CachedFile, std::io::Error> {
        if let Some(cached_file) = self.cache.read().unwrap().get(file_path) {
            Ok(cached_file.clone())
        } else {
            // Not found in cache, so we populate it
            trace(format!("File/dir not found in cache, reading from disk: {}", file_path));
            let (length, exists, is_directory, last_modified) = match std::fs::metadata(file_path) {
                Ok(metadata) => (metadata.len(), true, metadata.is_dir(), metadata.modified().unwrap_or(SystemTime::now())),
                Err(_) => (0, false, false, std::time::SystemTime::now()),
            };

            // If its a file and has content, read it
            let content = if is_directory || !exists || length == 0 {
                Vec::new()
            } else {
                let file_content = match std::fs::read(&file_path) {
                    Ok(content) => content,
                    Err(_) => {
                        trace(format!("Failed to read file content {}", file_path));
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
                trace(format!("Guessed MIME type for {}: {}", file_path, mime_type));

                // Create gzip version if content type is compressible
                if self.should_compress(&mime_type, length) {
                    match self.compress_content(&content, &mut gzip_content) {
                        Ok(_) => {
                            // Only keep compressed version if it's significantly smaller
                            if gzip_content.len() as f64 > content.len() as f64 * 0.8 {
                                trace(format!("Compressed version not significantly smaller, skipping for: {}", file_path));
                                gzip_content.clear();
                            }
                        }
                        Err(e) => {
                            warn(format!("Failed to compress file {}: {}", file_path, e));
                        }
                    }
                }
            }

            let new_cached_file = CachedFile {
                file_path: file_path.to_string(),
                is_directory: is_directory,
                exists: exists,
                length: length,
                is_too_large: length > self.max_file_size,
                content: content,
                mime_type: mime_type,
                gzip_content: gzip_content,
            };

            if self.is_enabled && (length < self.max_file_size) {
                trace(format!("New cached file/dir: path={}, is_directory={}, exists={}, length={}, is_too_large={}, mime_type={}", new_cached_file.file_path, new_cached_file.is_directory, new_cached_file.exists, new_cached_file.length, new_cached_file.is_too_large, new_cached_file.mime_type));
                self.cache.write().unwrap().insert(file_path.to_string(), new_cached_file.clone());
                self.cached_items_last_checked
                    .write()
                    .unwrap()
                    .insert(file_path.to_string(), (Instant::now(), Instant::now(), last_modified));
            }

            return Ok(new_cached_file);
        }
    }

    // Check if a MIME type should be compressed
    pub fn should_compress(&self, mime_type: &str, content_length: u64) -> bool {
        if self.gzip_enabled {
            let check_should_compress = content_length > 1000 && content_length < (10 * 1024 * 1024) && self.compressible_content_types.iter().any(|ct| mime_type.starts_with(ct));
            trace(format!("Should compress check for MIME type {} and content_length: {} - Result: {}", mime_type, content_length, check_should_compress));
            return check_should_compress;
        }
        false
    }

    // Handle updating data on the cached items, based on the last modified
    async fn update_cache(
        cache: Arc<RwLock<HashMap<String, CachedFile>>>,
        cached_items_last_checked: Arc<RwLock<HashMap<String, (Instant, Instant, SystemTime)>>>,
        lifetime_before_check: usize,
        max_item_lifetime: usize,
        eviction_threshold: usize,
    ) {
        let mut interval = interval(Duration::from_secs(10));

        let max_item_lifetime_duration = Duration::from_secs(max_item_lifetime as u64);
        let lifetime_before_check_duration = Duration::from_secs(lifetime_before_check as u64);

        let triggers = get_trigger_handler();
        let configuration_trigger = triggers.get_trigger("reload_configuration").expect("Failed to get reload_configuration trigger");
        let configuration_token = configuration_trigger.read().await.clone();

        loop {
            select! {
                _ = configuration_token.cancelled() => {
                    trace("[FileCacheUpdate] Configuration reload trigger received, so stopping update thread".to_string());
                    break;
                }
                _ = interval.tick() => {}
            }

            let start_time = Instant::now();

            trace("[FileCacheUpdate] Checking if we are above the eviction threshold, so we can delete files in cache that have been in cache for too long".to_string());
            let current_cache_size = cache.read().unwrap().len();
            if current_cache_size > eviction_threshold {
                trace("[FileCacheUpdate] Eviction threshold exceeded, triggering cleanup of items older than max".to_string());
                let files_to_remove: Vec<_> = cached_items_last_checked
                    .read()
                    .unwrap()
                    .iter()
                    .filter(|(_, (added, _last_checked, _last_modified))| added.elapsed() > max_item_lifetime_duration)
                    .map(|(path, _)| path.clone())
                    .collect();

                trace(format!("[FileCacheUpdate] Removing {} files from cache due to eviction threshold", files_to_remove.len()));

                // Remove item from cache
                for path in files_to_remove {
                    cache.write().unwrap().remove(&path);
                    cached_items_last_checked.write().unwrap().remove(&path);
                }
            } else {
                trace("[FileCacheUpdate] Cache size is below eviction threshold, no action taken".to_string());
            }

            trace("[FileCacheUpdate] Checking for modified timestamps and if known files still exist".to_string());

            // Start by grapping a list of file we want to check on, up to 100
            let files_to_check: Vec<_> = cached_items_last_checked
                .read()
                .unwrap()
                .iter()
                .filter(|(_, (_added, last_checked, _last_modified))| last_checked.elapsed() > lifetime_before_check_duration)
                .take(100)
                .map(|(path, (added, last_checked, last_modified))| (path.clone(), (added.clone(), last_checked.clone(), last_modified.clone())))
                .collect();

            trace(format!("[FileCacheUpdate] Files found to check for modified timestamps: {}", files_to_check.len()));

            // Now we go through the list, to check if the file was modified since last known timestamp
            for (path, (added, _last_checked, last_modified)) in files_to_check {
                let metadata = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        // We try to load that cache entry, so figure out if we already have it as non-existent
                        let mut should_remove_path = false;
                        if let Some(cached_file) = cache.read().unwrap().get(&path) {
                            if cached_file.exists {
                                // File no longer exists, so we just remove it from the cache
                                trace(format!("[FileCacheUpdate] File no longer exists: {}", path));
                                should_remove_path = true;
                            } else {
                                trace(format!("[FileCacheUpdate] File is marked as non-existent in cache, which it still is: {}", path));
                            }
                        }

                        if should_remove_path {
                            cache.write().unwrap().remove(&path);
                            cached_items_last_checked.write().unwrap().remove(&path);
                        }

                        continue;
                    }
                };

                if let Ok(modified_time) = metadata.modified() {
                    if modified_time != last_modified {
                        // File was changed, so we remove it from cache
                        trace(format!("[FileCacheUpdate] File was changed: {}", path));
                        cache.write().unwrap().remove(&path);
                        cached_items_last_checked.write().unwrap().remove(&path);
                        continue;
                    }
                    // If all is good, we update the last_checked
                    trace(format!("[FileCacheUpdate] File is good and not modified: {}", path));
                    cached_items_last_checked.write().unwrap().insert(path, (added, Instant::now(), modified_time));
                }
            }

            let end_time = Instant::now();

            debug(format!("[FileCacheUpdate] Cache update completed in {:?}", end_time.duration_since(start_time)));
        }
    }

    /// Compress content using gzip
    pub fn compress_content(&self, content: &[u8], gzip_content: &mut Vec<u8>) -> Result<(), std::io::Error> {
        let mut encoder = GzEncoder::new(gzip_content, Compression::default());
        encoder.write_all(content)?;
        encoder.finish()?;
        Ok(())
    }
}
