use crate::core::monitoring::get_monitoring_state;
use crate::core::os_signal::start_os_signal_handling;
use crate::telemetry::metrics_exporter;

pub async fn start_background_tasks() {
    // Start the OS signal handling
    start_os_signal_handling();

    // Init monitoring and start background task
    get_monitoring_state().await.initialize_monitoring();

    // Initialize the OpenTelemetry metrics exporter (caches monitoring state reference)
    metrics_exporter::initialize_metrics_exporter().await;
}
