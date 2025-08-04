use crate::grux_configuration_struct::Configuration;
use config::Config;
use log::info;
use serde_json;
use sqlite::State;
use std::sync::OnceLock;

// Load the configuration from the database or create a default one if it doesn't exist
fn init() -> Result<Config, String> {
    let connection = sqlite::open("./grux.db").map_err(|e| format!("Failed to open database connection: {}", e))?;

    connection
        .execute("CREATE TABLE IF NOT EXISTS grux_config (id INTEGER PRIMARY KEY AUTOINCREMENT, type VARCHAR(100), configuration TEXT)")
        .map_err(|e| format!("Failed to create configuration table: {}", e))?;

    let mut statement = connection
        .prepare("SELECT configuration FROM grux_config WHERE type = 'base'")
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let row_state = statement.next().map_err(|e| format!("Failed to execute statement: {}", e))?;

    let configuration_json: String;

    if row_state == State::Row {
        configuration_json = statement.read(0).map_err(|e| format!("Failed to read row: {}", e))?;
    } else {
        info!("No configuration found, using default settings.");

        let default_configuration = Configuration::new();
        configuration_json = serde_json::to_string(&default_configuration).map_err(|e| format!("Failed to serialize default configuration: {}", e))?;

        // Write the default configuration to the database
        connection
            .execute(format!("INSERT INTO grux_config (type, configuration) VALUES ('{}', '{}')", "base", configuration_json))
            .map_err(|e| format!("Failed to insert default configuration into database: {}", e))?;
    }

    // Explicitly drop the statement before dropping the connection
    drop(statement);
    drop(connection);

    let config = Config::builder()
        .add_source(config::File::from_str(&configuration_json, config::FileFormat::Json))
        .build()
        .map_err(|e| format!("Failed to build configuration: {}", e))?;
    // Explicitly drop the statement before dropping the connection

    Ok(config)
}

// Get the configuration
pub fn get_configuration() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| init().unwrap_or_else(|e| panic!("Failed to initialize configuration: {}", e)))
}

// Load the configuration and return any errors
// Should be used in the main function to check configuration
pub fn check_configuration() -> Result<Config, String> {
    init()
}
