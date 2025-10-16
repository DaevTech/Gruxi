use crate::grux_core::monitoring::get_monitoring_state;

pub fn start_background_tasks() {

    // Init monitoring and start background task
    get_monitoring_state().initialize_monitoring();
}
