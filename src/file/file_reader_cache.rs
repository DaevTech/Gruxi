use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::{
    compression::compression::Compression,
    configuration::cached_configuration::get_cached_configuration,
    core::triggers::get_trigger_handler,
    file::file_reader_structs::*,
    http::request_response::{
        body_error::{BodyError, box_err},
        gruxi_request::GruxiRequest,
    },
    logging::syslog::{debug, error, trace, warn},
};

use dashmap::DashMap;
use futures::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::Full;
use http_body_util::{StreamBody, combinators::BoxBody};
use hyper::body::{Bytes, Frame};
use tokio::{
    fs::File,
    select,
    time::{Instant, interval},
};
use tokio_util::io::ReaderStream;

impl FileReaderCache {
    pub async fn new() -> Self {
        // Get configuration
        let cached_configuration = get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        let file_data_config = &config.core.file_cache;

        let is_caching_enabled = file_data_config.is_enabled;
        let max_file_size = file_data_config.cache_max_size_per_file as u64;
        let capacity = file_data_config.cache_item_size;
        let max_item_lifetime = file_data_config.max_item_lifetime;
        let cleanup_thread_interval = file_data_config.cleanup_thread_interval;
        let forced_eviction_threshold = file_data_config.forced_eviction_threshold;

        let compressible_content_types = &config.core.gzip.compressible_content_types;
        let gzip_enabled = &config.core.gzip.is_enabled;

        let cache = Arc::new(DashMap::new());
        let cached_items_last_checked = Arc::new(DashMap::new());

        // Start the cleanup thread
        if is_caching_enabled {
            // Update/cleanup cache thread
            let cache_clone_update = cache.clone();
            let last_checked_clone = cached_items_last_checked.clone();
            let eviction_threshold: f64 = (capacity as f64 * (forced_eviction_threshold as f64 / 100.0)).round();

            tokio::spawn(async move {
                Self::update_cache(
                    cache_clone_update,
                    last_checked_clone,
                    cleanup_thread_interval as u64,
                    max_item_lifetime as u64,
                    eviction_threshold as u64,
                )
                .await;
            });
        }

        FileReaderCache {
            cache: cache,
            is_caching_enabled,
            cached_items_last_checked: cached_items_last_checked,
            max_file_size,
            gzip_enabled: *gzip_enabled,
            compressible_content_types: compressible_content_types.clone(),
        }
    }

    pub fn get_current_item_count(&self) -> u64 {
        self.cache.len() as u64
    }

    // Get file data
    pub async fn get_file(&self, file_path: &str) -> Result<Arc<FileEntry>, std::io::Error> {
        // Check the cache first
        if self.is_caching_enabled {
            if let Some(cached_entry) = self.cache.get(file_path) {
                trace(format!("File found in cache: {}", file_path));
                return Ok(cached_entry.value().clone());
            }
        }

        // Not found in cache, so we populate it, maybe saving it to cache if enabled
        trace(format!("File/dir not found in cache, reading from disk: {}", file_path));
        let (length, exists, is_directory, last_modified) = match std::fs::metadata(file_path) {
            Ok(metadata) => (metadata.len(), true, metadata.is_dir(), metadata.modified().unwrap_or(SystemTime::now())),
            Err(_) => (0, false, false, SystemTime::now()),
        };

        // Determine MIME type, if we have a file
        let mut mime_type = String::new();
        if !is_directory && exists {
            mime_type = mime_guess::from_path(&file_path).first_or_octet_stream().to_string();
            trace(format!("Guessed MIME type for {}: {}", file_path, mime_type));
        }

        let should_compress = self.should_compress(&mime_type, length);

        let mut file_entry = FileEntry {
            meta: FileMeta {
                file_path: file_path.to_string(),
                is_directory,
                exists,
                length,
                is_too_large_to_store: length > self.max_file_size,
                mime_type: mime_type,
            },
            content: ContentCache { raw: None, gzip: None },
        };

        // Pre-fetch content of file if caching is enabled
        if self.is_caching_enabled && !is_directory && exists && length <= self.max_file_size {
            match std::fs::read(file_path) {
                Ok(file_bytes) => {
                    let raw_bytes = Arc::new(Bytes::from(file_bytes));
                    file_entry.content.raw = Some(raw_bytes);

                    if should_compress {
                        let raw_content_result = file_entry.content.raw.as_ref();
                        let mut content_found = false;
                        let raw_content = match raw_content_result {
                            Some(content) => content.as_ref(),
                            None => {
                                warn(format!("Raw content is missing for file: {}", file_path));
                                content_found = false;
                                &Arc::new(Bytes::new())
                            }
                        };

                        // Content should be found, but for safety we check
                        if content_found {
                            let mut gzip_content = Vec::new();

                            match Compression::compress_content(raw_content, &mut gzip_content) {
                                Ok(_) => {}
                                Err(e) => {
                                    warn(format!("Failed to compress file {}: {}", file_path, e));
                                }
                            }
                            let gzip_bytes = Arc::new(Bytes::from(gzip_content));
                            file_entry.content.gzip = Some(gzip_bytes);
                        }
                    }

                    trace(format!("File content cached for file: {}", file_path));
                }
                Err(e) => {
                    trace(format!("Failed to read file {}: {}", file_path, e));
                }
            }
        }

        // Create Arc to return
        let file_entry_arc = Arc::new(file_entry);

        // Add to cache if enabled
        if self.is_caching_enabled {
            // Add to cache and update last checked
            trace(format!("Adding file to cache: {:?}", &file_entry_arc.meta));

            self.cache.insert(file_path.to_string(), file_entry_arc.clone());
            self.cached_items_last_checked.insert(file_path.to_string(), (Instant::now(), Instant::now(), last_modified));
        }

        Ok(file_entry_arc)
    }

    // Check if a MIME type should be compressed
    pub fn should_compress(&self, mime_type: &str, content_length: u64) -> bool {
        if self.gzip_enabled {
            let check_should_compress = content_length > 1000 && content_length < 10485760 && self.compressible_content_types.iter().any(|ct| mime_type.starts_with(ct));
            trace(format!(
                "Should compress check for MIME type {} and content_length: {} - Result: {}",
                mime_type, content_length, check_should_compress
            ));
            return check_should_compress;
        }
        false
    }

    // Handle updating data on the cached items, based on the last modified
    async fn update_cache(
        cache: Arc<DashMap<String, Arc<FileEntry>>>,
        cached_items_last_checked: Arc<DashMap<String, (Instant, Instant, SystemTime)>>,
        lifetime_before_check: u64,
        max_item_lifetime: u64,
        eviction_threshold: u64,
    ) {
        let mut interval = interval(Duration::from_secs(10));

        let max_item_lifetime_duration = Duration::from_secs(max_item_lifetime as u64);
        let lifetime_before_check_duration = Duration::from_secs(lifetime_before_check as u64);

        let triggers = get_trigger_handler();

        let configuration_token_option = triggers.get_token("reload_configuration").await;
        let configuration_token = match configuration_token_option {
            Some(token) => token,
            None => {
                error("Failed to get reload_configuration token - File cache update thread exiting - Please report a bug".to_string());
                return;
            }
        };

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
            let current_cache_size = cache.len() as u64;
            if current_cache_size > eviction_threshold {
                trace("[FileCacheUpdate] Eviction threshold exceeded, triggering cleanup of items older than max".to_string());
                let files_to_remove: Vec<_> = cached_items_last_checked
                    .iter()
                    .filter(|entry| entry.value().1.elapsed() > max_item_lifetime_duration)
                    .map(|entry| entry.key().clone())
                    .collect();

                trace(format!("[FileCacheUpdate] Removing {} files from cache due to eviction threshold", files_to_remove.len()));

                // Remove item from cache
                for path in files_to_remove {
                    cache.remove(&path);
                    cached_items_last_checked.remove(&path);
                }
            } else {
                trace("[FileCacheUpdate] Cache size is below eviction threshold, no action taken".to_string());
            }

            trace("[FileCacheUpdate] Checking for modified timestamps and if known files still exist".to_string());

            // Start by grapping a list of file we want to check on, up to 100
            let files_to_check: Vec<(String, (Instant, Instant, SystemTime))> = cached_items_last_checked
                .iter()
                .filter(|entry| entry.value().1.elapsed() > lifetime_before_check_duration)
                .take(100)
                .map(|entry| (entry.key().clone(), entry.value().clone()))
                .collect();

            trace(format!("[FileCacheUpdate] Files found to check for modified timestamps: {}", files_to_check.len()));

            // Now we go through the list, to check if the file was modified since last known timestamp
            for (path, (added, _last_checked, last_modified)) in files_to_check {
                let metadata = match std::fs::metadata(&path) {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        let mut should_remove_path = false;

                        if let Some(cached_file) = cache.get(&path) {
                            if cached_file.meta.exists {
                                trace(format!("[FileCacheUpdate] File no longer exists: {}", path));
                                should_remove_path = true;
                            } else {
                                trace(format!("[FileCacheUpdate] File is marked as non-existent in cache, which it still is: {}", path));
                            }
                        }

                        if should_remove_path {
                            cache.remove(&path);
                            cached_items_last_checked.remove(&path);
                        }

                        continue;
                    }
                };

                if let Ok(modified_time) = metadata.modified() {
                    if modified_time != last_modified {
                        trace(format!("[FileCacheUpdate] File was changed: {}", path));
                        cache.remove(&path);
                        cached_items_last_checked.remove(&path);
                        continue;
                    }

                    trace(format!("[FileCacheUpdate] File is good and not modified: {}", path));
                    cached_items_last_checked.insert(path, (added, Instant::now(), modified_time));
                }
            }

            let end_time = Instant::now();

            debug(format!("[FileCacheUpdate] Cache update completed in {:?}", end_time.duration_since(start_time)));
        }
    }
}

impl FileEntry {
    pub async fn get_content_stream(&self, gruxi_request: &mut GruxiRequest) -> (BoxBody<Bytes, BodyError>, String) {
        let accept_encoding_headers = gruxi_request.get_accepted_encodings();

        if self.content.raw.is_none() && self.content.gzip.is_none() {
            trace("No cached file data content is present, so we return from the filesystem instead (full if small and stream if big)".to_string());

            // For smaller files (<= 64 KB), return full content, otherwise stream
            if self.meta.length <= 64 * 1024 {
                // Small file, return full
                let file_bytes = match tokio::fs::read(&self.meta.file_path).await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        trace(format!("Failed to read file {} for full content: {}", self.meta.file_path, e));
                        let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                        return (BoxBody::new(empty), String::new());
                    }
                };
                let full_body = Full::new(Bytes::from(file_bytes)).map_err(|never| -> BodyError { match never {} });
                return (BoxBody::new(full_body), String::new());
            }

            // Otherwise we stream, to maintain low memory usage by not loading the full file into memory
            let file = match File::open(&self.meta.file_path).await {
                Ok(f) => f,
                Err(e) => {
                    trace(format!("Failed to open file {} for streaming: {}", self.meta.file_path, e));
                    let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                    return (BoxBody::new(empty), String::new());
                }
            };

            let stream = ReaderStream::new(file).map_ok(Frame::data);
            let streambody = http_body_util::BodyExt::map_err(StreamBody::new(stream), box_err);
            return (BoxBody::new(streambody), String::new());
        }

        // We prefer gzip if the client accepts it
        if accept_encoding_headers.iter().any(|enc| enc.to_lowercase() == "gzip") {
            if let Some(gzip_content) = &self.content.gzip {
                trace("Serving gzipped content from cache".to_string());
                let gzipped_bytes = gzip_content.as_ref().clone();
                let boxbody = BoxBody::new(Full::new(gzipped_bytes).map_err(|never| -> BodyError { match never {} }));
                return (boxbody, "gzip".to_string());
            }
        }

        // Otherwise serve raw content
        if let Some(raw_content) = &self.content.raw {
            trace("Serving raw content from cache".to_string());
            let raw_bytes = raw_content.as_ref().clone();
            let boxbody = BoxBody::new(Full::new(raw_bytes).map_err(|never| -> BodyError { match never {} }));
            return (boxbody, "".to_string());
        }

        // If nothing falls to taste, return empty
        let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
        return (BoxBody::new(empty), String::new());
    }
}
