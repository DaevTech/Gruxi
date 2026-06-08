use std::time::Duration;

use hyper::body::Bytes;
use moka::future::{Cache, CacheBuilder};

/// Short lived caching of compressed body data if a resource is accessed frequently
pub struct CompressionCache {
    cache: Cache<String, Bytes>,
}

const COMPRESSION_CACHE_MAX_CAPACITY: u64 = 100;
const COMPRESSION_CACHE_TTL_SECONDS: u64 = 10;
const COMPRESSION_CACHE_TTI_SECONDS: u64 = 5;
const COMPRESSION_CACHE_MAX_SIZE: usize = 1024 * 1024; // 1 MB

impl CompressionCache {
    pub fn new() -> Self {
        let cache = CacheBuilder::new(COMPRESSION_CACHE_MAX_CAPACITY)
            .time_to_live(Duration::from_secs(COMPRESSION_CACHE_TTL_SECONDS))
            .time_to_idle(Duration::from_secs(COMPRESSION_CACHE_TTI_SECONDS))
            .build();
        Self { cache }
    }

    pub async fn get(&self, key: &String) -> Option<Bytes> {
        self.cache.get(key).await
    }

    pub async fn insert(&self, key: String, value: Bytes) {
        if value.len() > COMPRESSION_CACHE_MAX_SIZE {
            // Don't cache if the compressed content is larger than the max size
            return;
        }
        self.cache.insert(key, value).await;
    }

    pub fn clear(&self) {
        self.cache.invalidate_all();
    }
}

impl Default for CompressionCache {
    fn default() -> Self {
        Self::new()
    }
}
