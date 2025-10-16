use grux::external_request_handlers::external_request_handlers;
use grux::grux_admin::http_admin_api::initialize_admin_site;
use grux::grux_configuration;
use grux::grux_core::async_runtime_handlers;
use grux::grux_core::async_runtime_handlers::AsyncRuntimeHandlers;
use grux::grux_core::background_tasks::start_background_tasks;
use grux::grux_core::database_schema;
use grux::grux_core::operation_mode::get_operation_mode;
use grux::grux_http_server;
use grux::grux_log;
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
    let configuration_check_result = grux_configuration::check_configuration();
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
    if let Err(e) = grux_http_server::initialize_server().await {
        error!("Error initializing server: {}", e);
        std::process::exit(1);
    }
}

async fn start_background_tasks_thread() {
    // Load background tasks
    start_background_tasks();
}
