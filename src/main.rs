use grux::admin_portal::http_admin_api::initialize_admin_site;
use grux::core::async_runtime_handlers;
use grux::core::async_runtime_handlers::AsyncRuntimeHandlers;
use grux::core::background_tasks::start_background_tasks;
use grux::core::database_schema;
use grux::core::operation_mode::get_operation_mode;
use grux::external_request_handlers::external_request_handlers;
use grux::grux_log;
use grux::http::http_server;
use grux::{configuration::load_configuration::check_configuration, core::shutdown_manager::get_shutdown_manager};
use log::{error, info};

fn main() {
    let logo = r#"
  ________
 /  _____/______ __ _____  ___
/   \  __\_  __ \  |  \  \/  /
\    \_\  \  | \/  |  />    <
 \______  /__|  |____//__/\_ \
        \/   WEBSERVER      \/
"#;
    println!("{}", logo);

    // Create the runtimes
    let http_server_runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    let grux_background_tasks_runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
    // Store the runtimes in a global singleton for access later
    async_runtime_handlers::set_async_runtime_handlers(AsyncRuntimeHandlers::new(http_server_runtime.handle().clone(), grux_background_tasks_runtime.handle().clone()));

    // Start the basics, logging etc.
    start_grux_basics();

    // Start the background tasks
    grux_background_tasks_runtime.block_on(async {
        start_background_tasks_thread().await;
    });

    // Start the actual http server listening thread
    http_server_runtime.block_on(async {
        start_main_server_thread().await; // This never exits
    });
}

fn start_grux_basics() {
    // Load operation mode
    let operation_mode = get_operation_mode();

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
    if let Err(e) = database_schema::initialize_database() {
        error!("Failed to initialize database: {}", e);
        std::process::exit(1);
    }
    info!("Database initialized");

    // Load configuration and check for errors
    let configuration_check_result = check_configuration();
    if let Err(e) = configuration_check_result {
        error!("Failed to load configuration: {}", e);
        std::process::exit(1);
    }
    info!("Configuration loaded");
}

async fn start_main_server_thread() {
    // Initialize admin site
    initialize_admin_site();

    // Initialize any external handlers, such as PHP, if needed
    external_request_handlers::get_request_handlers();
    info!("External request handlers initialized");

    // Init server bindings and start serving those bits
    http_server::initialize_server();

    // Wait for shutdown signal
    let shutdown_manager = get_shutdown_manager();
    let cancellation_token = shutdown_manager.get_cancellation_token();
    cancellation_token.cancelled().await;

    // Waiting a few seconds to allow graceful shutdown
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    std::process::exit(0);
}

async fn start_background_tasks_thread() {
    // Load background tasks
    start_background_tasks();
}
