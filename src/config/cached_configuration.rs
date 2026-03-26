use crate::{
    config::{configuration::Configuration},
    core::triggers::get_trigger_handler,
};
use crate::trace;
use arc_swap::ArcSwap;
use std::sync::{Arc, OnceLock};

pub struct CachedConfiguration {
    configuration: ArcSwap<Configuration>,
}

impl Default for CachedConfiguration {
    fn default() -> Self {
        Self::new()
    }
}

impl CachedConfiguration {
    pub fn new() -> Self {
        let configuration = super::load_configuration::init();
        CachedConfiguration {
            configuration: ArcSwap::from_pointee(configuration),
        }
    }

    pub fn get_configuration(&self) -> Arc<Configuration> {
        self.configuration.load_full()
    }

    pub async fn check_if_cached_configuration_should_be_refreshed() {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        trace!("Starting thread to monitor for configuration refresh signal for the cached configuration");

        let triggers = get_trigger_handler();
        let refresh_trigger_result = triggers.get_token("refresh_cached_configuration").await;
        let mut refresh_trigger_token = match refresh_trigger_result {
            Some(trigger) => trigger,
            None => {
                panic!("Failed to get refresh_cached_configuration trigger - Configuration reload task aborted - Please report a bug");
            }
        };

        loop {
            refresh_trigger_token.cancelled().await;
            trace!("Refresh cached configuration trigger received, reloading configuration");

            {
                let new_configuration = super::load_configuration::init();
                let cached_configuration = get_cached_configuration();
                cached_configuration.configuration.store(Arc::new(new_configuration));
            }

            // Get new token for next time
            let refresh_trigger_result = triggers.get_token("refresh_cached_configuration").await;
            refresh_trigger_token = match refresh_trigger_result {
                Some(trigger) => trigger,
                None => {
                    panic!("Failed to get refresh_cached_configuration trigger - Configuration reload task aborted - Please report a bug");
                }
            };

            trace!("Cached configuration successfully refreshed");
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
