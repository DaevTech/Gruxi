use crate::core::{running_state_manager::get_running_state_manager, triggers::get_trigger_handler};
use crate::logging::syslog::{debug, trace};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::{select, sync::OnceCell};

pub struct MonitoringState {
    requests_served: AtomicUsize,
    requests_served_last: AtomicUsize,
    requests_served_per_sec: AtomicUsize,
    requests_in_progress: AtomicUsize,
    server_start_time: std::time::Instant,
    file_cache_enabled: AtomicBool,
    file_cache_current_items: AtomicUsize,
    file_cache_max_items: AtomicUsize,

}

impl MonitoringState {
    pub async fn new() -> Self {
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let configuration = cached_configuration.get_configuration().await;

        MonitoringState {
            requests_served: AtomicUsize::new(0),      // Updated from http server
            requests_served_last: AtomicUsize::new(0), // Updated from monitoring thread
            requests_served_per_sec: AtomicUsize::new(0),
            requests_in_progress: AtomicUsize::new(0), // Updated from http server
            server_start_time: std::time::Instant::now(),
            file_cache_enabled: AtomicBool::new(configuration.core.file_cache.is_enabled),
            file_cache_current_items: AtomicUsize::new(0), // Updated from monitoring thread
            file_cache_max_items: AtomicUsize::new(configuration.core.file_cache.cache_item_size),
        }
    }

    // Background monitoring task.
    pub fn initialize_monitoring(&self) {
        debug("Monitoring initialized");
        tokio::spawn(Self::monitoring_task());
    }

    async fn monitoring_task() {
        let update_interval_seconds: usize = 10;
        let update_interval = tokio::time::Duration::from_secs(update_interval_seconds as u64);

        let triggers = get_trigger_handler();
        let configuration_trigger = triggers.get_trigger("reload_configuration").expect("Failed to get reload_configuration trigger");
        let mut configuration_token = configuration_trigger.read().await.clone();

        loop {
            let monitoring_state = get_monitoring_state().await;

            // Calculate requests per second
            let current_requests = monitoring_state.get_requests_served();
            let last_requests = monitoring_state.requests_served_last.load(Ordering::SeqCst);
            let requests_diff = current_requests.saturating_sub(last_requests);
            let requests_per_sec: f64 = requests_diff as f64 / update_interval_seconds as f64;
            monitoring_state.requests_served_per_sec.store(requests_per_sec.to_bits() as usize, Ordering::SeqCst);
            monitoring_state.requests_served_last.store(current_requests, Ordering::SeqCst);

            // Fetch some data from file cache
            {
                let running_state_manager = get_running_state_manager().await;
                let running_state = running_state_manager.get_running_state();
                let unlocked_running_state = running_state.read().await;
                let file_reader_cache = unlocked_running_state.get_file_reader_cache();

                monitoring_state.file_cache_current_items.store(file_reader_cache.get_current_item_count() as usize, Ordering::SeqCst);

                // Clone the configuration values we need, then drop the guard
                let (file_cache_enabled, file_cache_max_items) = {
                    let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
                    let configuration = cached_configuration.get_configuration().await;
                    (configuration.core.file_cache.is_enabled, configuration.core.file_cache.cache_item_size)
                };
                monitoring_state.file_cache_enabled.store(file_cache_enabled, Ordering::SeqCst);
                monitoring_state.file_cache_max_items.store(file_cache_max_items, Ordering::SeqCst);
            }

            trace("Monitoring data updated");

            select! {
                _ = configuration_token.cancelled() => {
                    // Get a new token
                    let configuration_trigger = triggers.get_trigger("reload_configuration").expect("Failed to get reload_configuration trigger");
                    configuration_token = configuration_trigger.read().await.clone();
                },
                _ = tokio::time::sleep(update_interval) => {}
            }
        }
    }

    pub fn increment_requests_served(&self) {
        self.requests_served.fetch_add(1, Ordering::SeqCst);
        self.requests_in_progress.fetch_add(1, Ordering::SeqCst);
    }

    pub fn decrement_requests_in_progress(&self) {
        self.requests_in_progress.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn get_requests_served(&self) -> usize {
        self.requests_served.load(Ordering::SeqCst)
    }

    pub async fn get_json(&self) -> serde_json::Value {
        let monitoring_state = get_monitoring_state().await;

        // Get the requests in progress minus one to account for the current monitoring request
        let requests_in_progress = monitoring_state.requests_in_progress.load(Ordering::SeqCst) - 1;

        serde_json::json!({
            "requests_served": monitoring_state.get_requests_served(),
            "requests_per_sec": f64::from_bits(monitoring_state.requests_served_per_sec.load(Ordering::Relaxed) as u64),
            "requests_in_progress": requests_in_progress,
            "uptime_seconds": monitoring_state.server_start_time.elapsed().as_secs(),
            "file_cache": {
                "enabled": monitoring_state.file_cache_enabled.load(Ordering::SeqCst),
                "current_items": monitoring_state.file_cache_current_items.load(Ordering::SeqCst),
                "max_items": monitoring_state.file_cache_max_items.load(Ordering::SeqCst),
            }
        })
    }
}

static CURRENT_STATE_SINGLETON: OnceCell<MonitoringState> = OnceCell::const_new();

pub async fn get_monitoring_state() -> &'static MonitoringState {
    CURRENT_STATE_SINGLETON.get_or_init(|| async { MonitoringState::new().await }).await
}
