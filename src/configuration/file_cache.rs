use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileCache {
    pub is_enabled: bool,
    pub cache_item_size: u64,              // in number of items, to aim for, not a hard limit
    pub cache_max_size_per_file: u64,      // Max size per file in bytes
    pub cache_update_thread_interval: u64, // in seconds
    pub max_item_lifetime: u64,            // in seconds
    pub forced_eviction_threshold: u64,    // 1-99 %
}

impl FileCache {
    pub fn new() -> Self {
        FileCache {
            is_enabled: true,
            cache_item_size: 1000,
            cache_max_size_per_file: 1 * 1024 * 1024, // bytes
            cache_update_thread_interval: 30,         // seconds
            max_item_lifetime: 60,                    // seconds
            forced_eviction_threshold: 80,            // 1-99%
        }
    }

    pub fn sanitize(&mut self) {}

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate cache_item_size
        if self.cache_item_size == 0 {
            errors.push("Max cached items count cannot be 0".to_string());
        }

        // Validate cache_max_size_per_file
        if self.cache_max_size_per_file == 0 {
            errors.push("Max size per file cannot be 0 bytes".to_string());
        }

        // Validate cache_update_thread_interval
        if self.cache_update_thread_interval == 0 {
            errors.push("Cache update thread interval cannot be 0".to_string());
        }

        // Validate max_item_lifetime
        if self.max_item_lifetime == 0 {
            errors.push("Max item lifetime cannot be 0".to_string());
        }

        // Validate forced_eviction_threshold (should be between 1-99)
        if self.forced_eviction_threshold == 0 || self.forced_eviction_threshold > 99 {
            errors.push("Forced eviction threshold must be between 1-99%".to_string());
        }

        // Note: cache_item_size is a count of items, cache_max_size_per_file is bytes per file
        // These are different units and cannot be compared directly

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
