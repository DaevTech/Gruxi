use crate::grux_core::monitoring::get_monitoring_state;
use crate::logging::access_logging::get_access_log_buffer;

pub fn start_background_tasks() {

    // Init monitoring and start background task
    get_monitoring_state().initialize_monitoring();

    // Start the access log log buffering thread
    get_access_log_buffer().start_flushing_thread();
}
