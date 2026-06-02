use std::time::Duration;

use crate::util::resource_access_counter::ResourceAccessCounter;

pub struct AccessCounters {
    pub compression_access_counter: ResourceAccessCounter,
}

impl AccessCounters {
    pub fn new() -> Self {
        Self {
            // Compression access counter with a 5-second window and if a resource is accessed more than 5 times within the window, it will be considered hot and cached
            compression_access_counter: ResourceAccessCounter::new(Duration::from_secs(5), 5),
        }
    }
}
