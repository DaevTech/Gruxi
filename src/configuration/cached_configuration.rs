use crate::{
    configuration::{configuration::Configuration, load_configuration::init},
    core::triggers::get_trigger_handler,
};
use crate::logging::syslog::trace;
use tokio::sync::RwLock;
use std::sync::{Arc, OnceLock};

pub struct CachedConfiguration {
    pub configuration: Arc<RwLock<Configuration>>,
}

impl CachedConfiguration {
    pub fn new() -> Self {
        let configuration = init().expect("Failed to load configuration");
        CachedConfiguration {
            configuration: Arc::new(RwLock::new(configuration)),
        }
    }

    pub async fn get_configuration(&self) -> tokio::sync::RwLockReadGuard<'_, Configuration> {
        self.configuration.read().await
    }

    pub async fn check_if_cached_configuration_should_be_refreshed() {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        trace("Starting thread to monitor for configuration refresh signal");

        let triggers = get_trigger_handler();
        let refresh_trigger = triggers.get_trigger("refresh_cached_configuration").expect("Failed to get refresh_cached_configuration trigger");
        let mut refresh_trigger_token = refresh_trigger.read().await.clone();

        loop {
            refresh_trigger_token.cancelled().await;
            trace("Refresh cached configuration trigger received, reloading configuration");

            {
                let new_configuration = init().expect("Failed to reload configuration");
                let cached_configuration = get_cached_configuration();
                let mut config_write_guard = cached_configuration.configuration.write().await;
                *config_write_guard = new_configuration;
            }

            // Get new token for next time
            let refresh_trigger = triggers.get_trigger("refresh_cached_configuration").expect("Failed to get refresh_cached_configuration trigger");
            refresh_trigger_token = refresh_trigger.read().await.clone();

            trace("Cached configuration successfully refreshed");
        }
    }
}

static CACHED_CONFIGURATION_SINGLETON: OnceLock<CachedConfiguration> = OnceLock::new();

pub fn get_cached_configuration() -> &'static CachedConfiguration {
    CACHED_CONFIGURATION_SINGLETON.get_or_init(|| {
        let cached_config = CachedConfiguration::new();
        tokio::spawn(CachedConfiguration::check_if_cached_configuration_should_be_refreshed());
        cached_config
    })
}
