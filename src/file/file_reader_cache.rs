use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::{
    compression::response_compression::compress_content, config::cached_configuration::get_cached_configuration, core::triggers::get_trigger_handler, debug, error, file::file_reader_structs::*, http::{
        caching::{
            etag::etag_strong_from_metadata,
            range::{RangeParseResult, build_multipart_body, build_multipart_body_from_parts, format_content_range, get_range_header, parse_range_header, should_process_range},
        },
        request_response::{
            body_error::{BodyError, box_err},
            gruxi_request::GruxiRequest,
        },
    }, trace, warn
};

use dashmap::DashMap;
use futures::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::Full;
use http_body_util::{StreamBody, combinators::BoxBody};
use hyper::body::{Bytes, Frame};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
    select,
    time::{Instant, interval},
};
use tokio_util::io::ReaderStream;

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

        let cache = Arc::new(DashMap::new());
        let cached_items_last_checked = Arc::new(DashMap::new());

        // Start the cache update thread
        if is_caching_enabled {
            // Update/cache cache thread
            let cache_clone_update = cache.clone();
            let last_checked_clone = cached_items_last_checked.clone();
            let eviction_threshold: f64 = (capacity as f64 * (forced_eviction_threshold as f64 / 100.0)).round();

            tokio::spawn(async move {
                Self::update_cache(cache_clone_update, last_checked_clone, cache_update_thread_interval, max_item_lifetime, eviction_threshold as u64).await;
            });
        }

        FileReaderCache {
            cache,
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
        trace!("File reader cache cleared");
    }

    // Get file data
    pub async fn get_file(&self, file_path: &str) -> Result<Arc<FileEntry>, std::io::Error> {
        // Check the cache first
        if self.is_caching_enabled
            && let Some(cached_entry) = self.cache.get(file_path) {
                trace!("File found in cache: {}", file_path);
                return Ok(cached_entry.value().clone());
            }

        // Not found in cache, so we populate it, maybe saving it to cache if enabled
        trace!("File/dir not found in cache, reading from disk: {}", file_path);
        let metadata_result = tokio::fs::metadata(file_path).await;
        let (length, exists, is_directory, last_modified) = match metadata_result {
            Ok(metadata) => (metadata.len(), true, metadata.is_dir(), metadata.modified().unwrap_or(SystemTime::now())),
            Err(_) => (0, false, false, SystemTime::now()),
        };

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

        let mut file_entry = FileEntry {
            meta: FileMeta {
                file_path: file_path.to_string(),
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
                    let raw_bytes = Arc::new(Bytes::from(file_bytes));
                    file_entry.content.raw = Some(raw_bytes);

                    if should_compress {
                        let raw_content_result = file_entry.content.raw.as_ref();
                        let mut content_found = true;
                        let raw_content = match raw_content_result {
                            Some(content) => content.as_ref(),
                            None => {
                                warn!("Raw content is missing for file: {}", file_path);
                                content_found = false;
                                &Arc::new(Bytes::new())
                            }
                        };

                        // Content should be found, but for safety we check
                        if content_found {
                            let mut gzip_content = Vec::new();

                            match compress_content(raw_content, &mut gzip_content) {
                                Ok(_) => {}
                                Err(e) => {
                                    warn!("Failed to compress file {}: {}", file_path, e);
                                }
                            }
                            let gzip_bytes = Arc::new(Bytes::from(gzip_content));
                            file_entry.content.gzip = Some(gzip_bytes);
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

        // Add to cache if enabled
        if self.is_caching_enabled {
            // Add to cache and update last checked
            trace!("Adding file to cache: {:?}", &file_entry_arc.meta);

            self.cache.insert(file_path.to_string(), file_entry_arc.clone());
            self.cached_items_last_checked.insert(file_path.to_string(), (Instant::now(), Instant::now(), last_modified));
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

            trace!("[FileCacheUpdate] Checking for modified timestamps and if known files still exist");
            // Get a list of files to check
            let files_to_check: Vec<(String, (Instant, Instant, SystemTime))> = cached_items_last_checked.iter().map(|entry| (entry.key().clone(), *entry.value())).collect();

            trace!("[FileCacheUpdate] Files found to check for modified timestamps: {}", files_to_check.len());
            // Now we go through the list, to check if the file was modified since last known timestamp
            for (path, (added, _last_checked, last_modified)) in files_to_check {
                let metadata = match tokio::fs::metadata(&path).await {
                    Ok(metadata) => metadata,
                    Err(_) => {
                        let mut should_remove_path = false;

                        if let Some(cached_file) = cache.get(&path) {
                            if cached_file.meta.exists {
                                trace!("[FileCacheUpdate] File no longer exists: {}", path);
                                should_remove_path = true;
                            } else {
                                trace!("[FileCacheUpdate] File is marked as non-existent in cache, which it still is: {}", path);
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
                        trace!("[FileCacheUpdate] File was changed: {}", path);
                        cache.remove(&path);
                        cached_items_last_checked.remove(&path);
                        continue;
                    }

                    trace!("[FileCacheUpdate] File is good and not modified: {}", path);
                    cached_items_last_checked.insert(path, (added, Instant::now(), modified_time));
                }
            }

            let end_time = Instant::now();

            debug!("[FileCacheUpdate] Cache update completed in {:?}", end_time.duration_since(start_time));
        }
    }
}

impl FileEntry {
    /// Result type for range request handling
    /// Contains the body, encoding, optional content-range header, and status code
    pub async fn get_content_stream(&self, gruxi_request: &mut GruxiRequest) -> (BoxBody<Bytes, BodyError>, String) {
        // Check if this is a range request - clone the header value to avoid borrow issues
        let range_header_value = get_range_header(gruxi_request).and_then(|h| h.to_str().ok()).map(|s| s.to_string());

        if let Some(range_str) = range_header_value {
            // Check If-Range precondition before processing range
            if should_process_range(gruxi_request, self.meta.etag_header.as_deref(), &self.meta.last_modified) {
                let range_result = self.handle_range_request(&range_str).await;
                if let Some(result) = range_result {
                    return result;
                }
                // If range_result is None, fall through to serve full content
            }
        }

        // Serve full content (no range request or range not applicable)
        self.get_full_content_stream(gruxi_request).await
    }

    /// Handle a range request, returning None if we should serve full content instead
    async fn handle_range_request(&self, range_str: &str) -> Option<(BoxBody<Bytes, BodyError>, String)> {
        let content_length = self.meta.length;

        match parse_range_header(range_str) {
            RangeParseResult::NoRangeHeader | RangeParseResult::InvalidSyntax | RangeParseResult::UnsupportedUnit => {
                // Serve full content
                None
            }
            RangeParseResult::Ranges(ranges) => {
                // Resolve all ranges against content length
                let resolved_ranges: Vec<(u64, u64)> = ranges.iter().filter_map(|r| r.resolve(content_length)).collect();

                if resolved_ranges.is_empty() {
                    // No satisfiable ranges - this will be handled by the caller with 416 response
                    // Return a special marker (empty body with encoding indicating unsatisfiable)
                    trace!("No satisfiable ranges found");
                    let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                    return Some((BoxBody::new(empty), "RANGE_NOT_SATISFIABLE".to_string()));
                }

                if resolved_ranges.len() == 1 {
                    // Single range - use optimized path that avoids reading entire file
                    let (start, end) = resolved_ranges[0];
                    return Some(self.get_single_range_content(start, end).await);
                } else {
                    // Multiple ranges - need to build multipart response
                    // For cached content, use zero-copy slicing; for uncached, read efficiently
                    return Some(self.get_multipart_range_content(&resolved_ranges).await);
                }
            }
        }
    }

    /// Get content for a single range request - optimized to avoid reading entire file
    async fn get_single_range_content(&self, start: u64, end: u64) -> (BoxBody<Bytes, BodyError>, String) {
        let content_length = self.meta.length;
        let content_range = format_content_range(start, end, content_length);

        // If content is cached, slice directly without copying
        if let Some(raw_content) = &self.content.raw {
            let start_idx = start as usize;
            let end_idx = (end + 1) as usize;
            let end_idx = end_idx.min(raw_content.len());

            if start_idx < raw_content.len() {
                // Use Bytes::slice for zero-copy
                let range_bytes = raw_content.slice(start_idx..end_idx);
                let full_body = Full::new(range_bytes).map_err(|never| -> BodyError { match never {} });
                return (BoxBody::new(full_body), format!("RANGE:{}", content_range));
            }
        }

        // For uncached files, seek and read only the needed bytes
        let range_length = end - start + 1;
        match File::open(&self.meta.file_path).await {
            Ok(mut file) => {
                // Seek to start position
                if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                    trace!("Failed to seek file {} for range: {}", self.meta.file_path, e);
                    let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                    return (BoxBody::new(empty), String::new());
                }

                // Read only the range
                let mut buffer = vec![0u8; range_length as usize];
                match file.read_exact(&mut buffer).await {
                    Ok(_) => {
                        let range_bytes = Bytes::from(buffer);
                        let full_body = Full::new(range_bytes).map_err(|never| -> BodyError { match never {} });
                        (BoxBody::new(full_body), format!("RANGE:{}", content_range))
                    }
                    Err(e) => {
                        trace!("Failed to read range from file {}: {}", self.meta.file_path, e);
                        let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                        (BoxBody::new(empty), String::new())
                    }
                }
            }
            Err(e) => {
                trace!("Failed to open file {} for range: {}", self.meta.file_path, e);
                let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                (BoxBody::new(empty), String::new())
            }
        }
    }

    /// Get content for multiple range requests - builds multipart response
    async fn get_multipart_range_content(&self, resolved_ranges: &[(u64, u64)]) -> (BoxBody<Bytes, BodyError>, String) {
        let content_length = self.meta.length;

        // If content is cached, use zero-copy slicing for multipart
        if let Some(raw_content) = &self.content.raw {
            let (body_bytes, content_type) = build_multipart_body(resolved_ranges, raw_content.as_ref(), &self.meta.mime_type, content_length);
            trace!("Serving {} ranges as multipart from cache", resolved_ranges.len());
            let full_body = Full::new(body_bytes).map_err(|never| -> BodyError { match never {} });
            return (BoxBody::new(full_body), format!("MULTIPART:{}", content_type));
        }

        // For uncached files, read each range separately and build multipart
        let mut range_contents: Vec<Vec<u8>> = Vec::with_capacity(resolved_ranges.len());

        match File::open(&self.meta.file_path).await {
            Ok(mut file) => {
                for &(start, end) in resolved_ranges {
                    let range_length = (end - start + 1) as usize;

                    if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                        trace!("Failed to seek file {} for multipart range: {}", self.meta.file_path, e);
                        let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                        return (BoxBody::new(empty), String::new());
                    }

                    let mut buffer = vec![0u8; range_length];
                    if let Err(e) = file.read_exact(&mut buffer).await {
                        trace!("Failed to read multipart range from file {}: {}", self.meta.file_path, e);
                        let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                        return (BoxBody::new(empty), String::new());
                    }
                    range_contents.push(buffer);
                }
            }
            Err(e) => {
                trace!("Failed to open file {} for multipart ranges: {}", self.meta.file_path, e);
                let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                return (BoxBody::new(empty), String::new());
            }
        }

        // Build multipart body from the collected ranges
        let (body_bytes, content_type) = build_multipart_body_from_parts(resolved_ranges, &range_contents, &self.meta.mime_type, content_length);

        trace!("Serving {} ranges as multipart from disk", resolved_ranges.len());
        let full_body = Full::new(body_bytes).map_err(|never| -> BodyError { match never {} });
        (BoxBody::new(full_body), format!("MULTIPART:{}", content_type))
    }

    /// Get the full content stream
    async fn get_full_content_stream(&self, gruxi_request: &mut GruxiRequest) -> (BoxBody<Bytes, BodyError>, String) {

        if self.content.raw.is_none() && self.content.gzip.is_none() {
            trace!("No cached file data content is present, so we return from the filesystem instead (full if small and stream if big)");

            // For smaller files (<= 64 KB), return full content, otherwise stream
            if self.meta.length <= 64 * 1024 {
                // Small file, return full
                let file_bytes = match tokio::fs::read(&self.meta.file_path).await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        trace!("Failed to read file {} for full content: {}", self.meta.file_path, e);
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
                    trace!("Failed to open file {} for streaming: {}", self.meta.file_path, e);
                    let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
                    return (BoxBody::new(empty), String::new());
                }
            };

            let stream = ReaderStream::new(file).map_ok(Frame::data);
            let streambody = http_body_util::BodyExt::map_err(StreamBody::new(stream), box_err);
            return (BoxBody::new(streambody), String::new());
        }

        // We prefer gzip if the client accepts it
        if gruxi_request.check_accepted_encoding("gzip")
            && let Some(gzip_content) = &self.content.gzip {
                trace!("Serving gzipped content from cache");
                let gzipped_bytes = gzip_content.as_ref().clone();
                let boxbody = BoxBody::new(Full::new(gzipped_bytes).map_err(|never| -> BodyError { match never {} }));
                return (boxbody, "gzip".to_string());
            }

        // Otherwise serve raw content
        if let Some(raw_content) = &self.content.raw {
            trace!("Serving raw content from cache");
            let raw_bytes = raw_content.as_ref().clone();
            let boxbody = BoxBody::new(Full::new(raw_bytes).map_err(|never| -> BodyError { match never {} }));
            return (boxbody, "".to_string());
        }

        // If nothing falls to taste, return empty
        let empty = Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} });
        (BoxBody::new(empty), String::new())
    }
}
