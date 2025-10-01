use clap::Parser;
use grux::grux_configuration;
use grux::grux_core::grux_command_line_args::GruxCommandLineArgs;
use grux::grux_core::grux_operation_mode::load_operation_mode;
use grux::grux_database;
use grux::grux_external_request_handlers;
use grux::grux_http_server;
use grux::grux_log;
use log::{error, info};

fn main() {
    // Parse command line args
    let cli = GruxCommandLineArgs::parse();

    // Load operation mode
    let operation_mode = load_operation_mode(cli.opmode);

    // Initialize logging
    match grux_log::init_logging(operation_mode) {
        Ok(_) => {}
        Err(e) => {
            error!("Failed to initialize logging: {}", e);
            std::process::exit(1);
        }
    }

    // Starting grux
    let version = env!("CARGO_PKG_VERSION", "unknown");
    info!("Starting grux {}...", version);
    info!("Operation mode: {:?}", operation_mode);

    // Load configuration and check for errors
    let configuration_check_result = grux_configuration::check_configuration();
    if let Err(e) = configuration_check_result {
        error!("Failed to load configuration: {}", e);
        std::process::exit(1);
    }
    info!("Configuration loaded");

    // Initialize database tables and migrations
    if let Err(e) = grux_database::initialize_database() {
        error!("Failed to initialize database: {}", e);
        std::process::exit(1);
    }
    info!("Database initialized");

    // Initialize any external handlers, such as PHP, if needed
    grux_external_request_handlers::get_request_handlers();
    info!("External request handlers initialized");

    // Init server bindings and start serving those bits
    if let Err(e) = grux_http_server::initialize_server() {
        error!("Error initializing server: {}", e);
        std::process::exit(1);
    }
}
