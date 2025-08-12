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
        .prepare("SELECT configuration FROM grux_config WHERE type = 'base' ORDER BY id DESC LIMIT 1")
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

    // Explicitly drop the statement before dropping the connection, all to prevent dangling connection to db
    drop(statement);
    drop(connection);

    let config = Config::builder()
        .add_source(config::File::from_str(&configuration_json, config::FileFormat::Json))
        .build()
        .map_err(|e| format!("Failed to build configuration: {}", e))?;

    // Validate the configuration, to check if it matches with the configuration struct
    config.clone().try_deserialize::<Configuration>().map_err(|e| format!("Failed to deserialize configuration: {}", e))?;

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

/// Get the current configuration from the database as a Configuration struct
pub fn get_current_configuration_from_db() -> Result<Configuration, String> {
    let connection = sqlite::open("./grux.db")
        .map_err(|e| format!("Failed to open database connection: {}", e))?;

    let mut statement = connection
        .prepare("SELECT configuration FROM grux_config WHERE type = 'base' ORDER BY id DESC LIMIT 1")
        .map_err(|e| format!("Failed to prepare statement: {}", e))?;

    let row_state = statement.next()
        .map_err(|e| format!("Failed to execute statement: {}", e))?;

    if row_state == State::Row {
        let configuration_json: String = statement.read(0)
            .map_err(|e| format!("Failed to read row: {}", e))?;

        let config: Configuration = serde_json::from_str(&configuration_json)
            .map_err(|e| format!("Failed to deserialize configuration: {}", e))?;

        drop(statement);
        drop(connection);

        Ok(config)
    } else {
        drop(statement);
        drop(connection);

        // Return default configuration if none exists
        Ok(Configuration::new())
    }
}

/// Save a new configuration to the database
/// Returns Ok(true) if changes were saved, Ok(false) if no changes were needed
pub fn save_configuration(config: &Configuration) -> Result<bool, String> {
    // First validate the configuration
    config.validate().map_err(|errors| {
        format!("Configuration validation failed: {}", errors.join("; "))
    })?;

    // Check if the configuration is different from what's currently in the database
    let current_config = get_current_configuration_from_db()?;

    // Serialize both configurations to JSON for comparison
    let new_config_json = serde_json::to_string(config)
        .map_err(|e| format!("Failed to serialize new configuration: {}", e))?;
    let current_config_json = serde_json::to_string(&current_config)
        .map_err(|e| format!("Failed to serialize current configuration: {}", e))?;

    // If configurations are identical, no need to save
    if new_config_json == current_config_json {
        return Ok(false); // No changes were made
    }

    // Save to database
    let connection = sqlite::open("./grux.db")
        .map_err(|e| format!("Failed to open database connection: {}", e))?;

    // Update or insert the configuration
    let mut statement = connection
        .prepare("INSERT OR REPLACE INTO grux_config (type, configuration) VALUES ('base', ?)")
        .map_err(|e| format!("Failed to prepare update statement: {}", e))?;

    statement
        .bind((1, new_config_json.as_str()))
        .map_err(|e| format!("Failed to bind parameter: {}", e))?;

    statement
        .next()
        .map_err(|e| format!("Failed to execute update statement: {}", e))?;

    drop(statement);
    drop(connection);

    // Note: The configuration will only take effect after a server restart
    // In a production system, you might want to add hot-reloading functionality

    Ok(true) // Changes were saved
}
