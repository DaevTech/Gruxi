use dashmap::DashMap;

use crate::util::sliding_time_window::SlidingTimeWindow;

pub struct ResourceAccessCounter {
    access_counters: DashMap<String, SlidingTimeWindow>,
    window_duration: std::time::Duration,
    window_size: u32,
}

impl ResourceAccessCounter {
    pub fn new(window_duration: std::time::Duration, window_size: u32) -> Self {
        Self {
            access_counters: DashMap::with_shard_amount(64),
            window_duration,
            window_size,
        }
    }

    pub fn record_access(&self, resource_id: &str) -> bool {
        let mut counter = self
            .access_counters
            .entry(resource_id.to_string())
            .or_insert_with(|| SlidingTimeWindow::new(self.window_duration, self.window_size));

        counter.hit()
    }
}
