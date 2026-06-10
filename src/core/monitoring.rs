use crate::core::running_state_manager::get_running_state_manager;
use crate::file::file_reader_cache::CACHE_404_MAX_SIZE;
use crate::{debug, trace};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::time::Instant;

#[derive(Debug)]
pub struct MonitoringState {
    requests_served: AtomicUsize,
    requests_served_last: AtomicUsize,
    requests_served_per_sec: AtomicUsize,
    active_connections: AtomicUsize,
    server_start_time: Instant,
    file_cache: FileCacheStats,
}

#[derive(Debug)]
pub struct FileCacheStats {
    enabled: AtomicBool,
    current_items: AtomicUsize,
    max_items: AtomicUsize,
    not_found_cache_items: AtomicUsize,
    not_found_cache_max_items: AtomicUsize,
}

impl MonitoringState {
    pub fn new() -> Self {
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let configuration = cached_configuration.get_configuration();

        MonitoringState {
            requests_served: AtomicUsize::new(0),         // Updated from http server
            requests_served_last: AtomicUsize::new(0),    // Updated from monitoring thread
            requests_served_per_sec: AtomicUsize::new(0), // Calculated in monitoring thread
            active_connections: AtomicUsize::new(0),      // Updated from http server
            server_start_time: Instant::now(), // Server start time is set when the monitoring state is initialized
            file_cache: FileCacheStats {
                enabled: AtomicBool::new(configuration.core.file_cache.is_enabled),
                current_items: AtomicUsize::new(0),
                max_items: AtomicUsize::new(configuration.core.file_cache.cache_item_size as usize),
                not_found_cache_items: AtomicUsize::new(0),
                not_found_cache_max_items: AtomicUsize::new(CACHE_404_MAX_SIZE as usize),
            },
        }
    }

    // Background monitoring task.
    pub fn initialize_monitoring(&self) {
        debug!("Monitoring initialized");
        tokio::spawn(Self::monitoring_task());
    }

    async fn monitoring_task() {
        // Initial wait a bit before starting to gather metrics, to allow the server to start up and serve some requests
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

        // Setup
        let update_interval_seconds = 1;
        let update_interval = tokio::time::Duration::from_secs(update_interval_seconds as u64);

        let mut last_update_instant = Instant::now();

        loop {
            let monitoring_state = get_monitoring_state();
            let elapsed_secs = last_update_instant.elapsed().as_secs_f64();
            last_update_instant = Instant::now();

            // Calculate requests per second
            let current_requests = monitoring_state.get_requests_served();
            let last_requests = monitoring_state.requests_served_last.load(Ordering::Relaxed);
            let requests_diff = current_requests.saturating_sub(last_requests);

            let requests_per_sec: f64 = requests_diff as f64 / elapsed_secs.max(0.001);
            let requests_per_sec = requests_per_sec.round().clamp(0.0, f64::MAX);
            monitoring_state.requests_served_per_sec.store(requests_per_sec.to_bits() as usize, Ordering::Relaxed);
            monitoring_state.requests_served_last.store(current_requests, Ordering::Relaxed);

            // Fetch some data from file cache
            {
                let running_state_manager = get_running_state_manager().await;
                let running_state = running_state_manager.get_running_state();
                let file_reader_cache = running_state.get_file_reader_cache();

                monitoring_state.file_cache.current_items.store(file_reader_cache.get_current_item_count() as usize, Ordering::Relaxed);
                monitoring_state
                    .file_cache
                    .not_found_cache_items
                    .store(file_reader_cache.get_404_cache_item_count() as usize, Ordering::Relaxed);

                // Clone the configuration values we need, then drop the guard
                let (file_cache_enabled, file_cache_max_items) = {
                    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
                    let configuration = cached_configuration.get_configuration();
                    (configuration.core.file_cache.is_enabled, configuration.core.file_cache.cache_item_size as usize)
                };

                monitoring_state.file_cache.enabled.store(file_cache_enabled, Ordering::Relaxed);
                monitoring_state.file_cache.max_items.store(file_cache_max_items, Ordering::Relaxed);
            }

            trace!("Monitoring data updated with data: {:?}", monitoring_state);

            tokio::time::sleep(update_interval).await;
        }
    }

    pub fn increment_requests_served(&self) {
        self.requests_served.fetch_add(1, Ordering::Relaxed);
    }

    pub fn get_requests_served(&self) -> usize {
        self.requests_served.load(Ordering::Relaxed)
    }

    pub fn increment_connections_in_queue(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connections_in_queue(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get_active_connections(&self) -> usize {
        self.active_connections.load(Ordering::Relaxed)
    }

    pub fn get_requests_per_sec(&self) -> f64 {
        f64::from_bits(self.requests_served_per_sec.load(Ordering::Relaxed) as u64)
    }

    pub fn get_uptime_seconds(&self) -> u64 {
        self.server_start_time.elapsed().as_secs()
    }

    pub fn get_file_cache_enabled(&self) -> bool {
        self.file_cache.enabled.load(Ordering::Relaxed)
    }

    pub fn get_file_cache_current_items(&self) -> usize {
        self.file_cache.current_items.load(Ordering::Relaxed)
    }

    pub fn get_file_cache_max_items(&self) -> usize {
        self.file_cache.max_items.load(Ordering::Relaxed)
    }

    pub fn get_file_not_found_cache_current_items(&self) -> usize {
        self.file_cache.not_found_cache_items.load(Ordering::Relaxed)
    }

    pub fn get_file_not_found_cache_max_items(&self) -> usize {
        self.file_cache.not_found_cache_max_items.load(Ordering::Relaxed)
    }

    pub async fn get_json(&self) -> serde_json::Value {
        let monitoring_state = get_monitoring_state();

        // Get the active connections minus one to account for the current monitoring request
        let active_connections = monitoring_state.active_connections.load(Ordering::Relaxed);

        serde_json::json!({
            "requests_served": monitoring_state.get_requests_served(),
            "requests_per_sec": f64::from_bits(monitoring_state.requests_served_per_sec.load(Ordering::Relaxed) as u64),
            "active_connections": active_connections,
            "uptime_seconds": monitoring_state.server_start_time.elapsed().as_secs(),
            "file_cache": {
                "enabled": monitoring_state.file_cache.enabled.load(Ordering::Relaxed),
                "current_items": monitoring_state.file_cache.current_items.load(Ordering::Relaxed),
                "max_items": monitoring_state.file_cache.max_items.load(Ordering::Relaxed),
                "not_found_current_items": monitoring_state.file_cache.not_found_cache_items.load(Ordering::Relaxed),
                "not_found_max_items": monitoring_state.file_cache.not_found_cache_max_items.load(Ordering::Relaxed),
            }
        })
    }
}

impl Default for MonitoringState {
    fn default() -> Self {
        Self::new()
    }
}

static CURRENT_STATE_SINGLETON: OnceLock<MonitoringState> = OnceLock::new();

pub fn get_monitoring_state() -> &'static MonitoringState {
    CURRENT_STATE_SINGLETON.get_or_init(MonitoringState::new)
}
