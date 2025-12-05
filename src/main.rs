use grux::configuration::cached_configuration::get_cached_configuration;
use grux::core::command_line_args::{check_for_command_line_actions, get_command_line_args};
use grux::core::database_schema;
use grux::core::operation_mode::get_operation_mode;
use grux::core::running_state_manager::get_running_state_manager;
use grux::core::triggers::get_trigger_handler;
use grux::logging::syslog::{error, info};
use grux::{admin_portal::init::initialize_admin_site, core::background_tasks::start_background_tasks};
use tokio::select;

#[tokio::main]
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

    // Start the basics, logging etc.
    start_grux_basics();

    // Start the running state manager thread, which also listens for configuration changes
    tokio::spawn(async {
        // Start tasks that run in the background
        start_background_tasks().await;

        // Start the running state, which are all the configuration dependent parts
        let running_state_manager = get_running_state_manager().await;

        // Start the main http server
        grux::http::http_server::initialize_server().await;

        let triggers = get_trigger_handler();

        let shutdown_token_trigger = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger");
        let shutdown_token = shutdown_token_trigger.read().await.clone();

        loop {
            let configuration_trigger = triggers.get_trigger("reload_configuration").expect("Failed to get reload_configuration trigger");
            let configuration_token = configuration_trigger.read().await.clone();

            select! {
                _ = configuration_token.cancelled() => {
                    info("Reloading running state due to configuration change");
                    running_state_manager.set_new_running_state().await;
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    grux::http::http_server::initialize_server().await;
                }
                _ = shutdown_token.cancelled() => {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();

    // Waiting a little while to allow graceful shutdown
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    std::process::exit(0);
}

fn start_grux_basics() {
    // Load commandline args
    get_command_line_args();
    check_for_command_line_actions();

    // Initialize database tables and migrations
    if let Err(e) = database_schema::initialize_database() {
        error(format!("Failed to initialize database: {}", e));
        std::process::exit(1);
    }

    // Load operation mode
    let operation_mode = get_operation_mode();

    let version = env!("CARGO_PKG_VERSION");
    info(format!("Starting Grux {}", version));
    info(format!("Operation mode: {:?}", operation_mode));

    //start_sys_log();

    // Load the configuration early to catch any errors
    match grux::configuration::load_configuration::init() {
        Ok(_) => {
            // Load the cached configuration, so it is ready to go
            get_cached_configuration();
        }
        Err(e) => {
            error(format!("Failed to load configuration: {}", e));
            std::process::exit(1);
        }
    }

    // Initialize the admin site
    initialize_admin_site();
}
