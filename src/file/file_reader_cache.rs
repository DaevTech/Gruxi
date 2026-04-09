use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::{
    compression::response_compression::compress_content,
    config::cached_configuration::get_cached_configuration,
    core::triggers::get_trigger_handler,
    debug, error,
    file::{
        file_entry::{ContentCache, FileEntry, FileMeta},
        file_reader_structs::*,
    },
    http::caching::etag::etag_strong_from_metadata,
    trace, warn,
};

use dashmap::DashMap;

use hyper::body::Bytes;
use tokio::{
    select,
    time::{Instant, interval},
};

impl FileReaderCache {
    pub async fn new() -> Self {
        // Get configuration
        let cached_configuration = get_cached_configuration();
        let config = cached_configuration.get_configuration();

        let file_data_config = &config.core.file_cache;

        let is_caching_enabled = file_data_config.is_enabled;
        let max_file_size = file_data_config.cache_max_size_per_file;
        let capacity = file_data_config.cache_item_size;
        let max_item_lifetime = file_data_config.max_item_lifetime;
        let cache_update_thread_interval = file_data_config.cache_update_thread_interval;
        let forced_eviction_threshold = file_data_config.forced_eviction_threshold;

        // Gzip enabled
        let compressible_content_types = &config.core.gzip.compressible_content_types;
        let gzip_enabled = &config.core.gzip.is_enabled;

        // Etag enabled
        let etag_enabled = config.core.http_caching.enable_header_etag;

        // Last modified header enabled
        let last_modified_header_enabled = config.core.http_caching.enable_header_last_modified;

        // Expires header enabled
        let expires_header_enabled = config.core.http_caching.enable_header_expires;

        // Cache-control header enabled
        let cache_control_header_enabled = config.core.http_caching.enable_header_cache_control;

        // Create the actual caches
        let cache = Arc::new(DashMap::new());
        let cached_items_last_checked = Arc::new(DashMap::new());

        // 404 cache
        let cache_404 = Arc::new(DashMap::new());
        let cache_404_max_size = 10000; // Max size of cache is currently hardcoded, but we may need to let it scale with X sites in some clever way, instead of global max

        // Start the cache update thread
        if is_caching_enabled {
            // Update/cache cache thread
            let cache_clone_update = cache.clone();
            let cache_404_clone_update = cache_404.clone();
            let last_checked_clone = cached_items_last_checked.clone();
            let eviction_threshold: f64 = (capacity as f64 * (forced_eviction_threshold as f64 / 100.0)).round();

            tokio::spawn(async move {
                Self::update_cache(
                    cache_clone_update,
                    cache_404_clone_update,
                    last_checked_clone,
                    cache_update_thread_interval,
                    max_item_lifetime,
                    eviction_threshold as u64,
                )
                .await;
            });
        }

        FileReaderCache {
            cache,
            cache_max_capacity: capacity,
            cache_404,
            cache_404_max_size,
            is_caching_enabled,
            cached_items_last_checked,
            max_file_size,
            gzip_enabled: *gzip_enabled,
            compressible_content_types: compressible_content_types.clone(),
            etag_enabled,
            last_modified_header_enabled,
            expires_header_enabled,
            cache_control_header_enabled,
        }
    }

    pub fn get_current_item_count(&self) -> u64 {
        self.cache.len() as u64
    }

    pub fn clear_cache(&self) {
        self.cache.clear();
        self.cached_items_last_checked.clear();
        self.cache_404.clear();
        trace!("File reader cache cleared");
    }

    fn get_empty_file_with_path(&self, file_path: &str) -> Arc<FileEntry> {
        Arc::new(FileEntry {
            meta: FileMeta {
                file_path: file_path.to_string(),
                is_directory: false,
                exists: false,
                length: 0,
                is_too_large_to_store: false,
                mime_type: String::new(),
                last_modified: SystemTime::now(),
                etag_header: None,
                last_modified_header: None,
                expires_header: None,
                cache_control_header: None,
            },
            content: ContentCache { raw: None, gzip: None },
        })
    }

    // Get file data
    pub async fn get_file(&self, file_path: &str) -> Result<Arc<FileEntry>, std::io::Error> {
        // Check the positive cache first
        if self.is_caching_enabled
            && let Some(cached_entry) = self.cache.get(file_path)
        {
            trace!("File found in cache: {}", file_path);
            return Ok(cached_entry.value().clone());
        }

        // Also check the 404 cache to short circuit if we know the file doesn't exist, to avoid unnecessary disk reads
        if self.is_caching_enabled && self.cache_404.get(file_path).is_some() {
            trace!("File found in 404 cache, so we short circuit and return empty result for file: '{}'", file_path);
            return Ok(self.get_empty_file_with_path(file_path));
        }

        // Not found in caches, so we populate it, maybe saving it to cache if enabled
        trace!("File/dir not found in cache, reading from disk: {}", file_path);
        let metadata_result = tokio::fs::metadata(file_path).await;
        let (length, exists, is_directory, last_modified) = match metadata_result {
            Ok(metadata) => (metadata.len(), true, metadata.is_dir(), metadata.modified().unwrap_or(SystemTime::now())),
            Err(_) => (0, false, false, SystemTime::now()),
        };

        // We check for a quick disconnect if the file/path was not found, to avoid unnecessary work for non-existent files
        if !exists {
            if self.is_caching_enabled && self.cache_404.len() < self.cache_404_max_size as usize {
                self.cache_404.insert(file_path.to_string(), Instant::now());
            }
            return Ok(self.get_empty_file_with_path(file_path));
        }

        // Determine MIME type, if we have a file
        let mut mime_type = String::new();
        if !is_directory && exists {
            mime_type = mime_guess::from_path(file_path).first_or_octet_stream().to_string();
            trace!("Guessed MIME type for {}: {}", file_path, mime_type);
        }

        let should_compress = self.should_compress(&mime_type, length);

        // Calculate ETag if enabled
        let etag_header = if self.etag_enabled && !is_directory && exists {
            let etag_value = etag_strong_from_metadata(length, last_modified);
            Some(etag_value)
        } else {
            None
        };

        // Prepare last modified header if enabled
        let last_modified_header_value = if self.last_modified_header_enabled {
            // String based on syntax from last modified: Last-Modified: <day-name>, <day> <month> <year> <hour>:<minute>:<second> GMT
            Some(httpdate::fmt_http_date(last_modified))
        } else {
            None
        };

        // Prepare expires header if enabled
        let expires_header_value = if self.expires_header_enabled {
            // Expires one year from now
            Some(httpdate::fmt_http_date(SystemTime::now() + std::time::Duration::from_secs(31557600)))
        } else {
            None
        };

        // Cache control header if enabled
        let cache_control_header_value = if self.cache_control_header_enabled {
            let control_value = if mime_type == "text/css"
                || mime_type == "text/javascript"
                || mime_type == "application/javascript"
                || mime_type == "application/wasm"
                || mime_type.starts_with("font/")
                || mime_type.starts_with("image/")
                || mime_type.starts_with("video/")
                || mime_type.starts_with("audio/")
            {
                "public, max-age=31536000, immutable" // 1 year for static files
            } else if mime_type == "text/html" {
                "no-cache" // Always revalidate HTML files
            } else {
                "public, max-age=86400" // 1 day for other files
            };
            Some(control_value.to_string())
        } else {
            None
        };

        let file_path_string = file_path.to_string();

        let mut file_entry = FileEntry {
            meta: FileMeta {
                file_path: file_path_string.clone(),
                is_directory,
                exists,
                length,
                is_too_large_to_store: length > self.max_file_size,
                mime_type,
                last_modified,
                etag_header,
                last_modified_header: last_modified_header_value,
                expires_header: expires_header_value,
                cache_control_header: cache_control_header_value,
            },
            content: ContentCache { raw: None, gzip: None },
        };

        // Pre-fetch content of file if caching is enabled
        if self.is_caching_enabled && !is_directory && exists && length <= self.max_file_size {
            match tokio::fs::read(file_path).await {
                Ok(file_bytes) => {
                    let raw_bytes = Bytes::from(file_bytes);
                    file_entry.content.raw = Some(raw_bytes);

                    if should_compress {
                        let raw_content = file_entry.content.raw.as_ref().unwrap();
                        let mut gzip_content = Vec::new();
                        match compress_content(raw_content, &mut gzip_content) {
                            Ok(_) => {
                                file_entry.content.gzip = Some(Bytes::from(gzip_content));
                            }
                            Err(e) => {
                                warn!("Failed to compress file {}: {}", file_path, e);
                            }
                        }
                    }

                    trace!("File content cached for file: {}", file_path);
                }
                Err(e) => {
                    trace!("Failed to read file {}: {}", file_path, e);
                }
            }
        }

        // Create Arc to return
        let file_entry_arc = Arc::new(file_entry);

        // Add to cache if enabled and we are within the limits we want to enforce
        // Idea for the future is to log when we hit the cache ceiling, so the user can adjust if needed
        if self.is_caching_enabled && self.cache.len() < self.cache_max_capacity as usize {
            // Add to cache and update last checked
            trace!("Adding file to cache: {:?}", &file_entry_arc.meta);

            self.cache.insert(file_path_string.clone(), file_entry_arc.clone());
            self.cached_items_last_checked.insert(file_path_string, (Instant::now(), Instant::now(), last_modified));
        }

        Ok(file_entry_arc)
    }

    // Check if a MIME type should be compressed
    pub fn should_compress(&self, mime_type: &str, content_length: u64) -> bool {
        if self.gzip_enabled {
            let check_should_compress = content_length > 1024 && content_length < 10 * 1024 * 1024 && self.compressible_content_types.iter().any(|ct| mime_type.starts_with(ct));
            trace!(
                "Should compress check for MIME type {} and content_length: {} - Result: {}",
                mime_type, content_length, check_should_compress
            );
            return check_should_compress;
        }
        false
    }

    // Handle updating data on the cached items, based on the last modified
    async fn update_cache(
        cache: Arc<DashMap<String, Arc<FileEntry>>>,
        cache_404: Arc<DashMap<String, Instant>>,
        cached_items_last_checked: Arc<DashMap<String, (Instant, Instant, SystemTime)>>,
        cache_update_thread_interval: u64,
        max_item_lifetime: u64,
        eviction_threshold: u64,
    ) {
        // Time interval for checking cache
        let mut interval = interval(Duration::from_secs(cache_update_thread_interval));
        // Each items max lifetime
        let max_item_lifetime_duration = Duration::from_secs(max_item_lifetime);

        // Get configuration reload trigger
        let triggers = get_trigger_handler();
        let configuration_token_option = triggers.get_token("reload_configuration").await;
        let configuration_token = match configuration_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get reload_configuration token - File cache update thread exiting - Please report a bug");
                return;
            }
        };

        loop {
            select! {
                _ = configuration_token.cancelled() => {
                    trace!("[FileCacheUpdate] Configuration reload trigger received, so stopping update thread");
                    break;
                }
                _ = interval.tick() => {}
            }

            let start_time = Instant::now();

            // Clear out the 404 cache for entries that are older than the max item lifetime, to allow re-checking of previously not found files
            trace!("[FileCacheUpdate] Cleaning up 404 cache for entries older than max item lifetime");
            cache_404.retain(|_, instant| instant.elapsed() <= max_item_lifetime_duration);

            trace!("[FileCacheUpdate] Checking if we are above the eviction threshold, so we can delete files in cache that have been in cache for too long");
            let current_cache_size = cache.len() as u64;
            if current_cache_size > eviction_threshold {
                trace!("[FileCacheUpdate] Eviction threshold exceeded, triggering clean-up of items older than max item lifetime");
                let files_to_remove: Vec<_> = cached_items_last_checked
                    .iter()
                    .filter(|entry| entry.value().0.elapsed() > max_item_lifetime_duration)
                    .map(|entry| entry.key().clone())
                    .collect();

                trace!("[FileCacheUpdate] Removing {} files from cache due to eviction threshold", files_to_remove.len());

                // Remove item from cache
                let files_to_remove_len = files_to_remove.len();
                for path in files_to_remove {
                    cache.remove(&path);
                    cached_items_last_checked.remove(&path);
                }
                trace!("[FileCacheUpdate] Eviction cleanup completed - Removed {} files", files_to_remove_len);
            } else {
                trace!("[FileCacheUpdate] Cache size is below eviction threshold - No delete action taken");
            }

            // Get a list of files to check for modified timestamps
            trace!("[FileCacheUpdate] Checking for modified timestamps and if known files still exist - Count: {}", cache.len());

            let local_cache = cache.clone();
            let local_cached_items_last_checked = cached_items_last_checked.clone();

            let task_result = tokio::task::spawn_blocking(move || {
                // We collect the entries to check into a vector first to avoid holding locks across await points, which can lead to deadlocks
                let entries: Vec<_> = local_cached_items_last_checked.iter().map(|item| (item.key().clone(), *item.value())).collect();

                // Loopy loppy
                for (path, (added, _last_checked, last_modified)) in entries {
                    // Get the data from the disk and if the file doesn't exist anymore, remove from cache
                    let metadata = match std::fs::metadata(&path) {
                        Ok(metadata) => metadata,
                        Err(_) => {
                            // Clear out the cache
                            local_cache.remove(&path);
                            local_cached_items_last_checked.remove(&path);

                            continue;
                        }
                    };

                    // If file still exist, but was modified, we also remove it from cache to allow it to be re-cached with the new content on next request
                    if let Ok(modified_time) = metadata.modified() {
                        if modified_time != last_modified {
                            trace!("[FileCacheUpdate] File was changed: {}", &path);
                            local_cache.remove(&path);
                            local_cached_items_last_checked.remove(&path);
                            continue;
                        }

                        trace!("[FileCacheUpdate] File is good and not modified: {}", &path);
                        local_cached_items_last_checked.insert(path, (added, Instant::now(), modified_time));
                    }
                }
            })
            .await;

            if let Err(e) = task_result {
                error!("[FileCacheUpdate] Blocking metadata check task failed: {}", e);
            }

            let end_time = Instant::now();

            debug!("[FileCacheUpdate] Cache update completed in {:?}", end_time.duration_since(start_time));
        }
    }
}
