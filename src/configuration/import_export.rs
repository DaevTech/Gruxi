use crate::configuration::load_configuration::fetch_configuration_in_db;
use std::path::PathBuf;

pub fn export_configuration_to_file(path: &PathBuf) -> Result<(), String> {
    let cached_configuration_result = fetch_configuration_in_db();
    let cached_configuration = match cached_configuration_result {
        Ok(cfg) => cfg,
        Err(e) => {
            return Err(format!("Failed to retrieve configuration from database: {}", e));
        }
    };

    // Serialize configuration to JSON
    let serialized = serde_json::to_string_pretty(&cached_configuration).map_err(|e| format!("Failed to serialize configuration: {}", e))?;

    std::fs::write(path, serialized).map_err(|e| format!("Failed to write configuration to file: {}", e))?;
    println!("Configuration successfully exported to {}", path.display());

    Ok(())
}

pub fn import_configuration_from_file(path: &PathBuf) -> Result<(), String> {
    // Read file contents
    let file_contents = std::fs::read_to_string(path).map_err(|e| format!("Failed to read configuration file {}: {}", path.display(), e))?;

    // Load json into loose typed to validate version and possibly do version migrations later
    let loose_typed: serde_json::Value = serde_json::from_str(&file_contents).map_err(|e| format!("Failed to parse configuration file {}: {}", path.display(), e))?;

    // Check that versions match
    if loose_typed["version"] != crate::configuration::configuration::CURRENT_CONFIGURATION_VERSION {
        // Here we could add version migration logic in the future

        // If we reach here, versions do not match
        return Err(format!(
            "Configuration version mismatch: expected {}, found {}",
            crate::configuration::configuration::CURRENT_CONFIGURATION_VERSION,
            loose_typed["version"].as_i64().unwrap_or(-1)
        ));
    }

    // Deserialize JSON to Configuration struct
    let mut configuration: crate::configuration::configuration::Configuration =
        serde_json::from_str(&file_contents).map_err(|e| format!("Failed to deserialize configuration from file {}: {}", path.display(), e))?;

    // Save configuration to database
    crate::configuration::save_configuration::save_configuration(&mut configuration, false).map_err(|e| format!("Failed to save imported configuration to database: {:?}", e))?;

    println!("Configuration successfully imported from {}", path.display());

    Ok(())
}

pub fn validate_configuration_file(path: &PathBuf) -> Result<(), String> {
    // Read file contents
    let file_contents = std::fs::read_to_string(path).map_err(|e| format!("Failed to read configuration file {}: {}", path.display(), e))?;

    // Load json into loose typed to validate version
    let loose_typed: serde_json::Value = serde_json::from_str(&file_contents).map_err(|e| format!("Failed to parse configuration file {}: {}", path.display(), e))?;

    // Check that versions match
    if loose_typed["version"] != crate::configuration::configuration::CURRENT_CONFIGURATION_VERSION {
        return Err(format!(
            "Configuration version mismatch: expected {}, found {}",
            crate::configuration::configuration::CURRENT_CONFIGURATION_VERSION,
            loose_typed["version"].as_i64().unwrap_or(-1)
        ));
    }

    // Deserialize JSON to Configuration struct to ensure it's valid
    let configuration: crate::configuration::configuration::Configuration =
        serde_json::from_str(&file_contents).map_err(|e| format!("Failed to deserialize configuration from file {}: {}", path.display(), e))?;

    configuration.validate().map_err(|e| format!("Configuration validation failed: {:?}", e))?;

    Ok(())
}
