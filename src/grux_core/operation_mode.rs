use std::sync::OnceLock;

// Operation mode
#[derive(Debug, Clone, Copy)]
pub enum OperationMode {
    DEV,
    DEBUG,
    PRODUCTION,
    SPEEDTEST,
}
// Set the operation mode here
// Change this to OperationMode::Production when deploying to production
// Or set via an environment variable or config file as needed
pub static GRUX_OPERATION_MODE: OperationMode = OperationMode::PRODUCTION;

pub fn load_operation_mode(from_cli: Option<String>) -> OperationMode {
    // Here you can implement logic to set the operation mode based on environment variables or config files
    // For example, read an environment variable and set GRUX_OPERATION_MODE accordingly
    from_cli.map(|s| match s.as_str() {
        "DEV" => OperationMode::DEV,
        "DEBUG" => OperationMode::DEBUG,
        "PRODUCTION" => OperationMode::PRODUCTION,
        "SPEEDTEST" => OperationMode::SPEEDTEST,
        _ => OperationMode::DEV,
    }).unwrap_or(OperationMode::PRODUCTION)
}

static OPERATION_MODE_SINGLETON: OnceLock<OperationMode> = OnceLock::new();

pub fn get_operation_mode() -> OperationMode {
    *OPERATION_MODE_SINGLETON.get_or_init(|| load_operation_mode(None))
}