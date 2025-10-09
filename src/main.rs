use clap::Parser;
use grux::grux_configuration;
use grux::grux_core::command_line_args::CommandLineArgs;
use grux::grux_core::operation_mode::load_operation_mode;
use grux::grux_database;
use grux::grux_external_request_handlers;
use grux::grux_http_server;
use grux::grux_log;
use log::{error, info};

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let logo = r#"
  ________
 /  _____/______ __ _____  ___
/   \  __\_  __ \  |  \  \/  /
\    \_\  \  | \/  |  />    <
 \______  /__|  |____//__/\_ \
        \/   WEBSERVER      \/
"#;

    println!("{}", logo);

    // Parse command line args
    let cli = CommandLineArgs::parse();

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

    // Starting Grux
    let version = env!("CARGO_PKG_VERSION");
    info!("Starting Grux {}...", version);
    info!("Operation mode: {:?}", operation_mode);

    // Initialize database tables and migrations
    if let Err(e) = grux_database::initialize_database() {
        error!("Failed to initialize database: {}", e);
        std::process::exit(1);
    }
    info!("Database initialized");

    // Load configuration and check for errors
    let configuration_check_result = grux_configuration::check_configuration();
    if let Err(e) = configuration_check_result {
        error!("Failed to load configuration: {}", e);
        std::process::exit(1);
    }
    info!("Configuration loaded");


    // Initialize any external handlers, such as PHP, if needed
    grux_external_request_handlers::get_request_handlers();
    info!("External request handlers initialized");

    // Init server bindings and start serving those bits
    if let Err(e) = grux_http_server::initialize_server().await {
        error!("Error initializing server: {}", e);
        std::process::exit(1);
    }
}
