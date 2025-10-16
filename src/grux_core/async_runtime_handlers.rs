use std::sync::{OnceLock};

use tokio::runtime::Handle;

pub struct AsyncRuntimeHandlers {
    pub background_tasks_handle: Handle,
    pub http_server_handle: Handle,
}

impl AsyncRuntimeHandlers {
    pub fn new(http_server_handle: Handle, background_tasks_handle: Handle) -> Self {
        AsyncRuntimeHandlers {
            background_tasks_handle,
            http_server_handle,
        }
    }
}

static CURRENT_STATE_SINGLETON: OnceLock<AsyncRuntimeHandlers> = OnceLock::new();

pub fn set_async_runtime_handlers(handlers: AsyncRuntimeHandlers) -> &'static AsyncRuntimeHandlers {
    CURRENT_STATE_SINGLETON.get_or_init(|| handlers)
}

pub fn get_async_runtime_handlers() -> &'static AsyncRuntimeHandlers {
    CURRENT_STATE_SINGLETON.get().expect("AsyncRuntimeHandlers not initialized")
}