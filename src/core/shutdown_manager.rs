use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

pub struct ShutdownManager {
    pub should_terminate: AtomicBool,
    pub cancellation_token: CancellationToken,
}

impl ShutdownManager {
    pub fn new() -> Self {
        ShutdownManager {
            should_terminate: AtomicBool::new(false),
            cancellation_token: CancellationToken::new(),
        }
    }

    pub fn should_terminate(&self) -> bool {
        self.should_terminate.load(Ordering::SeqCst)
    }

    pub fn initiate_shutdown(&self) {
        if self.should_terminate() {
            // We are already shutting down
            return;
        }
        self.should_terminate.store(true, Ordering::SeqCst);
        self.cancellation_token.cancel();
    }

    pub fn get_cancellation_token(&self) -> CancellationToken {
        self.cancellation_token.clone()
    }
}

static SHUTDOWN_MANAGER_SINGLETON: OnceLock<ShutdownManager> = OnceLock::new();

pub fn get_shutdown_manager() -> &'static ShutdownManager {
    SHUTDOWN_MANAGER_SINGLETON.get_or_init(|| ShutdownManager::new())
}
