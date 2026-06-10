use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileCache {
    pub is_enabled: bool,                   // Whether main file cache is enabled
    pub cache_item_size: u64,               // in number of items, to aim for, not a hard limit
    pub cache_max_size_per_file: u64,       // Max size per file in bytes
    pub max_item_lifetime: u64,             // in seconds
}

impl Default for FileCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FileCache {
    pub fn new() -> Self {
        FileCache {
            is_enabled: true,
            cache_item_size: 1000,
            cache_max_size_per_file: 1024 * 1024, // bytes
            max_item_lifetime: 60,                // seconds
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

        // Validate max_item_lifetime
        if self.max_item_lifetime == 0 {
            errors.push("Max item lifetime cannot be 0".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
