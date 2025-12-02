use crate::core::monitoring::get_monitoring_state;
use crate::core::os_signal::start_os_signal_handling;

pub async fn start_background_tasks() {
    // Start the OS signal handling
    start_os_signal_handling();

    // Init monitoring and start background task
    get_monitoring_state().await.initialize_monitoring();
}
