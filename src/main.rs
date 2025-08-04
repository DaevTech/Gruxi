use grux::grux_http_server;
use grux::grux_configuration;
use grux::grux_log;
use log::{error, info};

fn main() {
    // Initialize logging
    let _log_handle = grux_log::init_logging().unwrap();

    // Starting grux
    let version = env!("CARGO_PKG_VERSION", "unknown");
    info!("Starting grux {}...", version);

    // Load configuration and check for errors
    let configuration_check_result = grux_configuration::check_configuration();
    if let Err(e) = configuration_check_result {
        error!("Failed to load configuration: {}", e);
        std::process::exit(1);
    }
    info!("Configuration loaded successfully.");

    // Load the admin services endpoints


    // Init server bindings and start serving those bits
    if let Err(e) = crate::grux_http_server::initialize_server() {
        error!("Error initializing bindings: {}", e);
        error!("Make sure the port(s) is not already in use and that you have the necessary permissions to bind to it.");
        std::process::exit(1);
    }
}
