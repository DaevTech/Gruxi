use crate::core::running_state::RunningState;
use std::sync::Arc;
use tokio::sync::{OnceCell};
use arc_swap::{ArcSwap, Guard};

pub struct RunningStateManager {
    pub current_running_state: ArcSwap<RunningState>,
}

impl RunningStateManager {
    pub async fn new() -> Self {
        let current_running_state = ArcSwap::from_pointee(RunningState::new().await);
        RunningStateManager { current_running_state }
    }

    pub fn get_running_state(&self) -> Guard<Arc<RunningState>> {
        self.current_running_state.load()
    }

    pub async fn set_new_running_state(&self) {
        let triggers = crate::core::triggers::get_trigger_handler();

        // cancel current token to notify any tasks depending on it
        triggers.run_trigger("stop_services").await;

        // Give a small delay to allow tasks to notice cancellationd
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Setup a new running state
        self.current_running_state.swap(Arc::new(RunningState::new().await));
    }
}

static RUNNING_STATE_MANAGER_SINGLETON: OnceCell<RunningStateManager> = OnceCell::const_new();

pub async fn get_running_state_manager() -> &'static RunningStateManager {
    RUNNING_STATE_MANAGER_SINGLETON.get_or_init(|| async { RunningStateManager::new().await }).await
}
