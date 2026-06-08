use std::{sync::LazyLock, time::Duration};

use crate::util::resource_access_counter::ResourceAccessCounter;

pub struct AccessCounters {
    pub compression_access_counter: ResourceAccessCounter,
    pub file_access_counter: ResourceAccessCounter,
}

impl AccessCounters {
    pub fn new() -> Self {
        Self {
            compression_access_counter: ResourceAccessCounter::new(Duration::from_secs(5), 5),
            file_access_counter: ResourceAccessCounter::new(Duration::from_secs(5), 5),
        }
    }
}

pub static ACCESS_COUNTERS: LazyLock<AccessCounters> = LazyLock::new(AccessCounters::new);

impl Default for AccessCounters {
    fn default() -> Self {
        Self::new()
    }
}
