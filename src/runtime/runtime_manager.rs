use crate::{
    config::{cached_configuration::get_cached_configuration, configuration::Configuration},
    runtime::binding_manager::{BindingRuntimeConfiguration, get_binding_manager},
    runtime::site_manager::init_site_manager,
};
use std::{sync::OnceLock, time::Duration};

pub struct RuntimeManager;

impl RuntimeManager {
    pub fn new() -> Self {
        Self
    }

    pub fn init(&mut self) {
        // Fetch our current configuration for init
        let configuration = get_cached_configuration().get_configuration();

        // Start sites
        self.init_sites(&configuration);

        // Start bindings
        self.init_bindings(&configuration);
    }

    fn init_bindings(&mut self, configuration: &Configuration) {
        let runtime_binding_configuration = BindingRuntimeConfiguration {
            hard_connection_timeout: Duration::from_secs(configuration.core.server_settings.max_connection_duration_seconds),
        };

        let mut binding_manager = get_binding_manager().lock().expect("binding manager lock poisoned");
        for binding in &configuration.bindings {
            let binding_result = binding_manager.add_binding(binding.clone(), runtime_binding_configuration.clone());
            // We panic on binding problems during initialization
            if let Err(e) = binding_result {
                panic!("Failed to add binding: {:?}", e);
            }
        }
    }

    fn init_sites(&mut self, configuration: &Configuration) {
        init_site_manager(configuration).expect("Failed to initialize site manager");
    }
}

static RUNTIME_MANAGER_SINGLETON: OnceLock<RuntimeManager> = OnceLock::new();

pub fn get_runtime_manager() -> &'static RuntimeManager {
    RUNTIME_MANAGER_SINGLETON.get_or_init(|| {
        let mut manager = RuntimeManager::new();
        manager.init();
        manager
    })
}
