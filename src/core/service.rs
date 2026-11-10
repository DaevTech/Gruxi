use crate::{error, file::app_paths::get_app_paths, runtime::runtime_manager::get_runtime_manager};
use qsu::rt::{InitCtx, RunEnv, SvcEvt, TermCtx, TokioServiceHandler};

use crate::{
    admin_portal::init::initialize_admin_site,
    config::load_configuration,
    core::{background_tasks::start_background_tasks, operation_mode::get_operation_mode, running_state_manager::get_running_state_manager, triggers::get_trigger_handler},
    database::database_schema::initialize_database,
    error::gruxi_error_enums::{GruxiErrorKind, InitAdminPortalError},
    info,
};

pub struct GruxiService;

#[qsu::async_trait]
impl TokioServiceHandler for GruxiService {
    type AppErr = Box<dyn std::error::Error + Send + Sync>;

    async fn init(&mut self, _ictx: &mut InitCtx) -> Result<(), Self::AppErr> {
        start_gruxi_basics();
        Ok(())
    }

    async fn run(&mut self, _re: &RunEnv) -> Result<(), Self::AppErr> {
        // Start tasks that run in the background
        start_background_tasks().await;

        // Start the runtime manager
        let _runtime_manager = get_runtime_manager();

        // Start the running state, which are all the configuration dependent parts
        let _running_state_manager = get_running_state_manager().await;

        let triggers = get_trigger_handler();
        let shutdown_token_trigger_option = triggers.get_trigger("shutdown");
        let shutdown_token_trigger = match shutdown_token_trigger_option {
            Some(trigger) => trigger,
            None => {
                error!("Failed to get shutdown trigger - If this happens, please report a bug");
                return Err("Failed to get shutdown trigger".into());
            }
        };

        // Wait for the shutdown trigger to be triggered
        let shutdown_token = shutdown_token_trigger.read().await.clone();
        shutdown_token.cancelled().await;

        Ok(())
    }

    async fn shutdown(&mut self, _tctx: &mut TermCtx) -> Result<(), Self::AppErr> {
        Ok(())
    }
}

fn start_gruxi_basics() {
    // Initialize database tables and migrations
    if let Err(e) = initialize_database() {
        eprintln!("Failed to initialize database: {}", e);
        std::process::exit(1);
    }

    // Load operation mode
    let operation_mode = get_operation_mode();

    let version = env!("CARGO_PKG_VERSION");
    info!("Starting Gruxi {}", version);
    info!("Operation mode: {:?}", operation_mode);

    // Load the configuration early to catch any errors
    load_configuration::init();

    // Initialize the admin site
    match initialize_admin_site() {
        Ok(_) => (),
        Err(err) => {
            match err.kind {
                GruxiErrorKind::InitAdminPortal(InitAdminPortalError::NoDatabaseConnection(_)) => {
                    error!("Failed to initialize admin site due to database connection error. Please check your database configuration and ensure the database server is running.");
                }
                GruxiErrorKind::InitAdminPortal(InitAdminPortalError::CouldNotCreateAdminUser(_)) => {
                    error!(
                        "Failed to initialize admin site because the default admin user could not be created. This might be due to a database issue or a permissions problem. Please check your database and ensure it is properly configured."
                    );
                }
                _ => {
                    error!("Failed to initialize admin site: unknown error: {}", err.message);
                }
            }
            std::process::exit(1);
        }
    };
}

pub fn svcevt_handler(evt: SvcEvt) {
    if let SvcEvt::Shutdown(_demise) = evt {
        let triggers = get_trigger_handler();
        triggers.run_shutdown_trigger_synchronous();

        // Wait a little while to allow graceful shutdown before exiting process, otherwise the service manager might think the shutdown failed and force kill the process immediately
        std::thread::sleep(std::time::Duration::from_millis(1000));
    }
}

pub fn install_service() {
    println!("Installing Gruxi as a system service...");
    // Get working dir
    let app_paths = get_app_paths();

    let result = qsu::installer::RegSvc::new("gruxi")
        .display_name("Gruxi Web Server")
        .description("High performance web server")
        .netservice()
        .autostart()
        .workdir(app_paths.working_dir.to_string_lossy())
        .arg("--service")
        .register();

    match result {
        Ok(()) => println!("Gruxi service installed successfully."),
        Err(e) => {
            eprintln!("Failed to install Gruxi service: {}", e);
            eprintln!("You may need to run this command with elevated permissions (root/administrator).");
            std::process::exit(1);
        }
    }
}

pub fn remove_service() {
    println!("Removing Gruxi system service...");
    match qsu::installer::uninstall("gruxi") {
        Ok(()) => println!("Gruxi service removed successfully."),
        Err(e) => {
            eprintln!("Failed to remove Gruxi service: {}", e);
            eprintln!("You may need to run this command with elevated permissions (root/administrator).");
            std::process::exit(1);
        }
    }
}
