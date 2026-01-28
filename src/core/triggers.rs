use crate::logging::syslog::{trace, warn};
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

pub struct Triggers {
    pub triggers: HashMap<String, Arc<RwLock<CancellationToken>>>,
}

impl Triggers {
    pub fn new() -> Self {
        let mut triggers = HashMap::new();
        let known_triggers = vec!["refresh_cached_configuration", "reload_configuration", "configuration_changed", "stop_services", "shutdown", "operation_mode_changed"];
        for trigger_name in known_triggers {
            triggers.insert(trigger_name.to_string(), Arc::new(RwLock::new(CancellationToken::new())));
        }

        Triggers { triggers }
    }

    pub async fn get_token(&self, name: &str) -> Option<CancellationToken> {
        let trigger_option = self.triggers.get(name);
        match trigger_option {
            Some(token_lock) => {
                let token_clone = token_lock.clone();
                let token = token_clone.read().await;
                Some(token.clone())
            }
            None => None,
        }
    }

    pub fn get_trigger(&self, name: &str) -> Option<Arc<RwLock<CancellationToken>>> {
        self.triggers.get(name).cloned()
    }

    pub async fn run_trigger(&self, name: &str) {
        if let Some(token_lock) = self.triggers.get(name) {
            let token_clone = token_lock.clone();
            let token = token_clone.read().await;
            trace(format!("Running trigger: {}", name));
            token.cancel();
        } else {
            warn(format!("A non-existent trigger was triggered - Please report as a bug. Trigger: {}", name));
        }
        // When token is used, we renew it for next time
        self.renew_trigger(name).await;
    }

    async fn renew_trigger(&self, name: &str) {
        if let Some(token_lock) = self.triggers.get(name) {
            let mut token = token_lock.write().await;
            *token = CancellationToken::new();
        }
    }
}

static TRIGGERS_SINGLETON: OnceLock<Triggers> = OnceLock::new();

pub fn get_trigger_handler() -> &'static Triggers {
    TRIGGERS_SINGLETON.get_or_init(|| Triggers::new())
}
