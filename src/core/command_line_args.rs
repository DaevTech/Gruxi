use std::{path::PathBuf, sync::OnceLock};

use clap::{Arg, ArgMatches, Command};

pub fn load_command_line_args() -> ArgMatches {
    // Parse command line args
    Command::new("Grux")
        .version(env!("CARGO_PKG_VERSION"))
        .arg(
            Arg::new("opmode")
                .short('o')
                .long("opmode")
                .help("Mode of operation")
                .value_parser(["DEV", "DEBUG", "PRODUCTION", "SPEEDTEST"]),
        )
        .arg(
            Arg::new("reset-admin-password")
                .long("reset-admin-password")
                .help("Reset the admin password and exit")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("export-configuration")
                .short('e')
                .long("export-conf")
                .help("Export the configuration to a file")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("export-configuration-and-exit")
                .long("export-conf-exit")
                .help("Export the configuration to a file and exit")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("import-configuration")
                .short('i')
                .long("import-conf")
                .help("Import the configuration from a file")
                .value_parser(clap::value_parser!(PathBuf))
                .value_parser(validate_existing_file),
        )
        .arg(
            Arg::new("import-configuration-and-exit")
                .long("import-conf-exit")
                .help("Import the configuration from a file and exit")
                .value_parser(clap::value_parser!(PathBuf))
                .value_parser(validate_existing_file),
        )
        .get_matches()
}

fn validate_existing_file(s: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(s);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", s));
    }
    if !path.is_file() {
        return Err(format!("Path is not a file: {}", s));
    }
    Ok(path)
}

pub fn cmd_get_operation_mode() -> String {
    let cli = get_command_line_args();
    cli.get_one::<String>("opmode").map(|s| s.to_string()).unwrap_or("".to_string())
}

pub fn cmd_should_reset_admin_password() -> bool {
    let cli = get_command_line_args();
    cli.get_flag("reset-admin-password")
}

pub fn check_for_command_line_actions() {
    let cli = get_command_line_args();

    if cmd_should_reset_admin_password() {
        crate::core::admin_user::reset_admin_password().expect("Failed to reset admin password");
        std::process::exit(0);
    }

    // Check for export configuration
    if let Some(path) = cli.get_one::<PathBuf>("export-configuration") {
        crate::configuration::import_export::export_configuration_to_file(path, false).expect("Failed to export configuration");
    }

    if let Some(path) = cli.get_one::<PathBuf>("export-configuration-and-exit") {
        crate::configuration::import_export::export_configuration_to_file(path, true).expect("Failed to export configuration");
    }

    // Check for import configuration
    if let Some(path) = cli.get_one::<PathBuf>("import-configuration") {
        crate::configuration::import_export::import_configuration_from_file(path, false).expect("Failed to import configuration");
    }

    if let Some(path) = cli.get_one::<PathBuf>("import-configuration-and-exit") {
        crate::configuration::import_export::import_configuration_from_file(path, true).expect("Failed to import configuration");
    }
}

static COMMAND_LINE_ARGS_SINGLETON: OnceLock<ArgMatches> = OnceLock::new();

pub fn get_command_line_args() -> &'static ArgMatches {
    COMMAND_LINE_ARGS_SINGLETON.get_or_init(|| load_command_line_args())
}
