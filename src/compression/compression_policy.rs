pub struct CompressionPolicy {
    compressible_content_types: Vec<String>,
    compressible_content_types_cache: DashMap<String, bool>,
}

use dashmap::DashMap;

use crate::trace;

impl CompressionPolicy {
    pub fn new(compressible_content_types: Vec<String>) -> Self {
        CompressionPolicy {
            compressible_content_types,
            compressible_content_types_cache: DashMap::new(),
        }
    }

    pub fn clear_cache(&self) {
        self.compressible_content_types_cache.clear();
    }

    pub fn should_compress(&self, mime_type: &str, content_length: u64) -> bool {
        self.should_compress_mime_type(mime_type) && self.should_compress_content_length(content_length)
    }

    fn should_compress_mime_type(&self, mime_type: &str) -> bool {
        // First, check cache if we should even consider this mime type
        if let Some(cached_result) = self.compressible_content_types_cache.get(mime_type) {
            trace!("[Compression]: Mime type '{}' is compression friendly (cached): {:?}", mime_type, cached_result);
            return *cached_result;
        }

        let is_compressible = self.compressible_content_types.iter().any(|ct| mime_type.starts_with(ct));
        self.compressible_content_types_cache.insert(mime_type.to_string(), is_compressible);
        trace!("[Compression]: Mime type '{}' is compression friendly: {}", mime_type, is_compressible);
        is_compressible
    }

    fn should_compress_content_length(&self, content_length: u64) -> bool {
        if (content_length == 0 || content_length > 1024) && content_length < 10 * 1024 * 1024 {
            trace!("[Compression]: Content length '{}' is compression friendly", content_length);
            true
        } else {
            trace!("[Compression]: Content length '{}' is not compression friendly", content_length);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_should_compress() {
        let compressible_content_types = vec!["text/".to_string(), "application/json".to_string()];
        let compression = CompressionPolicy::new(compressible_content_types);

        assert!(compression.should_compress("text/html", 2048));
        assert!(compression.should_compress("application/json", 2048));
        assert!(!compression.should_compress("image/png", 2048));
        assert!(!compression.should_compress("text/html", 512)); // Too small
        assert!(!compression.should_compress("text/html", 11 * 1024 * 1024)); // Too large
    }
}
