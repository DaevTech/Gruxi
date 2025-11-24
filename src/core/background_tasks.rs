use crate::core::monitoring::get_monitoring_state;
use crate::core::os_signal::start_os_signal_handling;
use crate::logging::access_logging::get_access_log_buffer;

pub fn start_background_tasks() {
    // Start the OS signal handling
    start_os_signal_handling();

    // Init monitoring and start background task
    get_monitoring_state().initialize_monitoring();

    // Start the access log log buffering thread
    get_access_log_buffer().start_flushing_thread();
}
