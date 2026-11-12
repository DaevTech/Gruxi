use crate::{config::{cached_configuration::get_cached_configuration, configuration::Configuration}, runtime::binding_manager::{BindingRuntimeConfiguration, get_binding_manager}};
use std::{sync::OnceLock, time::Duration};

pub struct RuntimeManager;

impl RuntimeManager {
    pub fn new() -> Self {
        Self
    }

    pub fn init(&mut self) {
        // Fetch our current configuration for init
        let configuration = get_cached_configuration().get_configuration();

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
}

static RUNTIME_MANAGER_SINGLETON: OnceLock<RuntimeManager> = OnceLock::new();

pub fn get_runtime_manager() -> &'static RuntimeManager {
    RUNTIME_MANAGER_SINGLETON.get_or_init(|| {
        let mut manager = RuntimeManager::new();
        manager.init();
        manager
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::binding::Binding;

    #[tokio::test]
    async fn startup_and_later_additions_share_the_binding_manager() {
        let mut configuration = Configuration::new();
        let mut startup_binding = Binding::new();
        startup_binding.ip = "127.0.0.1".to_string();
        startup_binding.port = 0;
        configuration.bindings.push(startup_binding.clone());

        RuntimeManager::new().init_bindings(&configuration);

        let mut manager = get_binding_manager().lock().expect("binding manager lock poisoned");
        assert_eq!(manager.get_binding_runtimes().len(), 1);
        assert_eq!(manager.get_binding_runtimes()[0].binding.id, startup_binding.id);

        let runtime_config = BindingRuntimeConfiguration {
            hard_connection_timeout: Duration::from_secs(30),
        };
        assert!(manager.add_binding(startup_binding, runtime_config.clone()).is_err());
        assert_eq!(manager.get_binding_runtimes().len(), 1);

        let mut added_binding = Binding::new();
        added_binding.ip = "0.0.0.0".to_string();
        added_binding.port = 0;
        manager.add_binding(added_binding.clone(), runtime_config.clone()).expect("binding should start");
        assert_eq!(manager.get_binding_runtimes().len(), 2);
        assert_eq!(manager.get_binding_runtimes()[1].binding.id, added_binding.id);

        let mut invalid_binding = Binding::new();
        invalid_binding.ip = "invalid".to_string();
        assert!(manager.add_binding(invalid_binding, runtime_config).is_err());
        assert_eq!(manager.get_binding_runtimes().len(), 2);

        for binding in manager.get_binding_runtimes() {
            binding.stop_binding_token.cancel();
        }
    }
}
