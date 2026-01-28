use gruxi::core::command_line_args::{check_for_command_line_actions, get_command_line_args};
use gruxi::core::operation_mode::get_operation_mode;
use gruxi::core::running_state_manager::get_running_state_manager;
use gruxi::core::triggers::get_trigger_handler;
use gruxi::database::database_schema::initialize_database;
use gruxi::logging::syslog::{error, info};
use gruxi::{admin_portal::init::initialize_admin_site, core::background_tasks::start_background_tasks};
use tokio::select;

#[tokio::main]
async fn main() {
    let logo = r#"
  ________                   .__
 /  _____/______ __ _____  __|__|
/   \  __\_  __ \  |  \  \/  /  |
\    \_\  \  | \/  |  />    <|  |
 \______  /__|  |____//__/\_ \__|
        \/     WEBSERVER    \/
"#;
    println!("{}", logo);

    // Start the basics, logging etc.
    start_gruxi_basics();

    // Start the running state manager thread, which also listens for configuration changes
    let join_handle = tokio::spawn(async {
        // Start tasks that run in the background
        start_background_tasks().await;

        // Start the running state, which are all the configuration dependent parts
        let running_state_manager = get_running_state_manager().await;

        // Start the main http server
        gruxi::http::http_server::initialize_server().await;

        let triggers = get_trigger_handler();

        let shutdown_token_trigger_option = triggers.get_trigger("shutdown");
        let shutdown_token_trigger = match shutdown_token_trigger_option {
            Some(trigger) => trigger,
            None => {
                error("Failed to get shutdown trigger - If this happens, please report a bug");
                return;
            }
        };
        let shutdown_token = shutdown_token_trigger.read().await.clone();

        loop {
            let configuration_trigger_option = triggers.get_trigger("reload_configuration");
            let configuration_trigger = match configuration_trigger_option {
                Some(trigger) => trigger,
                None => {
                    error("Failed to get reload_configuration trigger - If this happens, please report a bug");
                    return;
                }
            };
            let configuration_token = configuration_trigger.read().await.clone();

            select! {
                _ = configuration_token.cancelled() => {
                    info("Reloading running state due to configuration change");
                    running_state_manager.set_new_running_state().await;
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    gruxi::http::http_server::initialize_server().await;
                }
                _ = shutdown_token.cancelled() => {
                    break;
                }
            }
        }
    })
    .await;
    if let Err(e) = join_handle {
        error(format!("Main loop task exited with error: {}", e));
    }

    // Waiting a little while to allow graceful shutdown
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    std::process::exit(0);
}

fn start_gruxi_basics() {
    // Load commandline args
    get_command_line_args();
    check_for_command_line_actions();

    // Initialize database tables and migrations
    if let Err(e) = initialize_database() {
        error(format!("Failed to initialize database: {}", e));
        std::process::exit(1);
    }

    // Load operation mode
    let operation_mode = get_operation_mode();

    let version = env!("CARGO_PKG_VERSION");
    info(format!("Starting Gruxi {}", version));
    info(format!("Operation mode: {:?}", operation_mode));

    // Load the configuration early to catch any errors
    gruxi::configuration::load_configuration::init();

    // Initialize the admin site
    match initialize_admin_site() {
        Ok(_) => (),
        Err(_) => {
            error("Failed to initialize admin site");
            std::process::exit(1);
        }
    };
}
