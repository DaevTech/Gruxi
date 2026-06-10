use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use crate::{
    compression::response_compression::compress_content,
    config::configuration::Configuration,
    file::file_entry::{ContentCache, FileEntry, FileMeta},
    http::caching::etag::etag_strong_from_metadata,
    trace,
    util::access_counters::ACCESS_COUNTERS,
    warn,
};

use hyper::body::Bytes;
use moka::future::Cache;

pub const CACHE_404_MAX_SIZE: u64 = 10000; // Max size of cache is currently hardcoded, but we may need to let it scale with X sites in some clever way, instead of global max

pub struct FileReaderCache {
    // Normal content cache
    cache: Cache<String, Arc<FileEntry>>,

    // 404 cache
    cache_404: Cache<String, ()>,

    // Short lived cache when primary cache is disabled
    cache_short_lived: Option<Cache<String, Arc<FileEntry>>>,

    // General cache settings
    is_caching_enabled: bool,
    max_file_size: u64,

    // Compression related
    gzip_enabled: bool,
    compressible_content_types: Vec<String>,

    // Caching related headers
    etag_enabled: bool,
    last_modified_header_enabled: bool,
    expires_header_enabled: bool,
    cache_control_header_enabled: bool,
}

impl FileReaderCache {
    pub fn new(config: &Configuration) -> Self {
        // Get some data from file cache config
        let file_cache_config = &config.core.file_cache;
        let is_caching_enabled = file_cache_config.is_enabled;
        let max_file_size = file_cache_config.cache_max_size_per_file;
        let capacity = file_cache_config.cache_item_size;
        let max_item_lifetime = file_cache_config.max_item_lifetime;

        // Gzip enabled
        let gzip_config = &config.core.gzip;
        let compressible_content_types = &gzip_config.compressible_content_types;
        let gzip_enabled = gzip_config.is_enabled;

        // Caching short lived cache
        let is_short_lived_caches_allowed = config.core.caching.is_short_lived_caches_allowed;

        // HTTP caching related settings
        let http_caching_config = &config.core.http_caching;
        // Etag enabled
        let etag_enabled = http_caching_config.enable_header_etag;
        // Last modified header enabled
        let last_modified_header_enabled = http_caching_config.enable_header_last_modified;
        // Expires header enabled
        let expires_header_enabled = http_caching_config.enable_header_expires;
        // Cache-control header enabled
        let cache_control_header_enabled = http_caching_config.enable_header_cache_control;

        // Create the actual caches
        let cache = Cache::builder().max_capacity(capacity).time_to_live(Duration::from_secs(max_item_lifetime)).build();

        // 404 cache
        let cache_404 = Cache::builder().max_capacity(CACHE_404_MAX_SIZE).time_to_live(Duration::from_secs(max_item_lifetime)).build();

        // Short lived cache when primary cache is disabled
        let cache_short_lived = if !is_caching_enabled && is_short_lived_caches_allowed {
            let cache = Cache::builder().max_capacity(capacity).time_to_live(Duration::from_secs(10)).build();
            Some(cache) // Initialize short-lived cache
        } else {
            None
        };

        FileReaderCache {
            cache,
            cache_404,
            is_caching_enabled,
            max_file_size,
            gzip_enabled,
            compressible_content_types: compressible_content_types.clone(),
            etag_enabled,
            last_modified_header_enabled,
            expires_header_enabled,
            cache_control_header_enabled,
            cache_short_lived,
        }
    }

    pub fn get_current_item_count(&self) -> u64 {
        self.cache.entry_count()
    }

    pub fn get_404_cache_item_count(&self) -> u64 {
        self.cache_404.entry_count()
    }

    pub fn clear_cache(&self) {
        self.cache.invalidate_all();
        self.cache_404.invalidate_all();
        if let Some(cache) = &self.cache_short_lived {
            cache.invalidate_all();
        }

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
            && let Some(cached_entry) = self.cache.get(file_path).await
        {
            trace!("File found in cache: {}", file_path);
            return Ok(cached_entry);
        }

        // Also check the 404 cache to short circuit if we know the file doesn't exist, to avoid unnecessary disk reads
        if self.is_caching_enabled && self.cache_404.get(file_path).await.is_some() {
            trace!("File found in 404 cache, so we short circuit and return empty result for file: '{}'", file_path);
            return Ok(self.get_empty_file_with_path(file_path));
        }

        // If caching is disabled, we check our short lived cache for the file
        if !self.is_caching_enabled
            && let Some(cache) = &self.cache_short_lived
            && let Some(cached_entry) = cache.get(file_path).await
        {
            trace!("File found in short-lived cache: {}", file_path);
            return Ok(cached_entry);
        }

        // Not found in caches, so we populate it, maybe saving it to cache if enabled
        trace!("File/dir not found in cache, reading from disk: {}", file_path);
        let metadata_result = tokio::fs::metadata(file_path).await;
        let (length, is_directory, last_modified) = match metadata_result {
            Ok(metadata) => (metadata.len(), metadata.is_dir(), metadata.modified().unwrap_or(SystemTime::now())),
            Err(_) => {
                // If file doesn't exist, we add to 404 cache and return an empty result, to avoid unnecessary disk reads for non-existent files in the future
                if self.is_caching_enabled {
                    self.cache_404.insert(file_path.to_string(), ()).await;
                }
                return Ok(self.get_empty_file_with_path(file_path));
            }
        };

        // Determine MIME type, if we have a file
        let mut mime_type = String::new();
        if !is_directory {
            mime_type = mime_guess::from_path(file_path).first_or_octet_stream().to_string();
            trace!("Guessed MIME type for {}: {}", file_path, mime_type);
        }

        // Calculate ETag if enabled
        let etag_header = if self.etag_enabled && !is_directory {
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
                exists: true,
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
        if self.is_caching_enabled {
            self.populate_file_entry_with_content(&mut file_entry).await;
        } else if self.cache_short_lived.is_some() {
            // If main cache is disabled, we trigger access counter for this file to determine if it should be added to short lived cache
            let hits_ceiling = ACCESS_COUNTERS.file_access_counter.record_access(&file_path_string);
            if hits_ceiling {
                trace!("File access counter hit ceiling for file {}, so we will add it to short-lived cache", file_path_string);
                self.populate_file_entry_with_content(&mut file_entry).await;

                if let Some(cache) = &self.cache_short_lived {
                    let file_entry_arc = Arc::new(file_entry);
                    cache.insert(file_path_string.clone(), file_entry_arc.clone()).await;
                    return Ok(file_entry_arc);
                }
            }
        }

        // Create Arc to return
        let file_entry_arc = Arc::new(file_entry);

        // Add to cache if enabled
        if self.is_caching_enabled {
            trace!("Adding file to cache: {:?}", &file_entry_arc.meta);
            self.cache.insert(file_path_string.clone(), file_entry_arc.clone()).await;
        }

        Ok(file_entry_arc)
    }

    async fn populate_file_entry_with_content(&self, file_entry: &mut FileEntry) {
        if !file_entry.meta.is_directory && file_entry.meta.length <= self.max_file_size {
            match tokio::fs::read(&file_entry.meta.file_path).await {
                Ok(file_bytes) => {
                    file_entry.content.raw = Some(Bytes::from(file_bytes));

                    if self.should_compress(&file_entry.meta.mime_type, file_entry.meta.length) {
                        let raw_content = file_entry.content.raw.as_ref().unwrap();
                        match compress_content(raw_content) {
                            Ok(compressed_bytes) => {
                                file_entry.content.gzip = Some(Bytes::from(compressed_bytes));
                            }
                            Err(e) => {
                                warn!("Failed to compress file {}: {}", file_entry.meta.file_path, e);
                            }
                        }
                    }
                    trace!("File content cached for file: {}", file_entry.meta.file_path);
                }
                Err(e) => {
                    trace!("Failed to read file {}: {}", file_entry.meta.file_path, e);
                }
            }
        }
    }

    // Check if a MIME type should be compressed
    pub fn should_compress(&self, mime_type: &str, content_length: u64) -> bool {
        if self.gzip_enabled {
            let check_should_compress =
                (content_length == 0 || content_length > 1024) && content_length < 10 * 1024 * 1024 && self.compressible_content_types.iter().any(|ct| mime_type.starts_with(ct));
            trace!(
                "Should compress check for MIME type {} and content_length: {} - Result: {}",
                mime_type, content_length, check_should_compress
            );
            return check_should_compress;
        }
        false
    }
}
