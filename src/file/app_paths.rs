use std::{path::PathBuf, sync::OnceLock};

use serde::Serialize;

use crate::core::command_line_args::get_command_line_args;

#[derive(Debug, Serialize)]
pub struct AppPaths {
    pub is_service: bool,
    pub binary_path: PathBuf,
    pub working_dir: PathBuf,
    pub certificates_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub db_dir: PathBuf,
    pub default_www_dir: PathBuf,
    pub default_admin_portal_dir: PathBuf,
}

impl Default for AppPaths {
    fn default() -> Self {
        Self::new()
    }
}

impl AppPaths {
    pub fn new() -> Self {
        // Determine if we are running as a service by checking the command line arguments for the "service" flag
        let cli = get_command_line_args();
        let run_as_service = cli.get_flag("service");

        // Based on Linux/Windows, we try to determine the correct paths
        #[cfg(target_os = "windows")]
        {
            Self::get_app_paths_win(run_as_service)
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Self::get_app_paths_linux(run_as_service);
        }
    }

    fn get_app_paths_win(run_as_service: bool) -> Self {
        let binary_path = Self::get_executable_path();

        // If we are running as service on Windows, working dir will be system32 which is not a cool place.
        // We set working dir to current exe parent in that case. And if not running as service, just get working dir
        let current_working_dir = if run_as_service {
            let parent_path_option = binary_path.parent();
            match parent_path_option {
                Some(parent_working_dir) => parent_working_dir.to_path_buf(),
                None => Self::get_current_working_dir(),
            }
        } else {
            Self::get_current_working_dir()
        };

        // Certificates
        let certificates_dir = current_working_dir.join("certs");
        let logs_dir = current_working_dir.join("logs");
        let db_dir = current_working_dir.join("db");
        let default_www_dir = current_working_dir.join("www-default");
        let default_admin_portal_dir = current_working_dir.join("www-admin");

        AppPaths {
            is_service: run_as_service,
            binary_path,
            working_dir: current_working_dir.to_path_buf(),
            certificates_dir,
            logs_dir,
            db_dir,
            default_www_dir,
            default_admin_portal_dir,
        }
    }

    // On Linux, if we are running as a service, we detect the paths based on existence
    #[cfg(not(target_os = "windows"))]
    fn get_app_paths_linux(run_as_service: bool) -> Self {
        let current_working_dir = Self::get_current_working_dir();
        let binary_path = Self::get_executable_path();

        if run_as_service {
            // If we run as a service, we check that binary is running from /usr/bin/gruxi and then adhere to FHS (Filesystem Hierarchy Standard)
            if binary_path != PathBuf::from("/usr/bin/gruxi") {
                eprintln!(
                    "When running as a service on Linux, the binary must be located at /usr/bin/gruxi. Current binary path: {}",
                    binary_path.display()
                );
                std::process::exit(1);
            }

            let certificates_dir = PathBuf::from("/var/lib/gruxi/certs");
            let logs_dir = PathBuf::from("/var/log/gruxi");
            let db_dir = PathBuf::from("/var/lib/gruxi/db");
            let default_www_dir = PathBuf::from("/usr/share/gruxi/www-default");
            let default_admin_portal_dir = PathBuf::from("/usr/share/gruxi/www-admin");

            AppPaths {
                is_service: run_as_service,
                binary_path,
                working_dir: current_working_dir,
                certificates_dir,
                logs_dir,
                db_dir,
                default_www_dir,
                default_admin_portal_dir,
            }
        } else {
            // If we are not running as a service, we use the same logic as on Windows
            Self::get_app_paths_win(false)
        }
    }

    fn get_current_working_dir() -> PathBuf {
        let current_working_dir_result = std::env::current_dir();
        match current_working_dir_result {
            Ok(path) => path,
            Err(e) => {
                eprintln!("Failed to determine current working directory: {}", e);
                std::process::exit(1);
            }
        }
    }

    fn get_executable_path() -> PathBuf {
        let binary_path_result = std::env::current_exe();
        match binary_path_result {
            Ok(path) => path,
            Err(e) => {
                eprintln!("Failed to determine current executable path: {}", e);
                std::process::exit(1);
            }
        }
    }
}

static APP_PATH_SINGLETON: OnceLock<AppPaths> = OnceLock::new();

pub fn get_app_paths() -> &'static AppPaths {
    APP_PATH_SINGLETON.get_or_init(AppPaths::new)
}
