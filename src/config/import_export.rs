use crate::config::load_configuration::fetch_configuration_in_db;
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

    // Load JSON into a value to validate its version
    let loose_typed: serde_json::Value = serde_json::from_str(&file_contents).map_err(|e| format!("Failed to parse configuration file {}: {}", path.display(), e))?;

    // Check that versions match
    if loose_typed["version"] != crate::config::configuration::CURRENT_CONFIGURATION_VERSION {
        return Err(format!(
            "Configuration version mismatch: expected {}, found {}",
            crate::config::configuration::CURRENT_CONFIGURATION_VERSION,
            loose_typed["version"].as_i64().unwrap_or(-1)
        ));
    }

    // Deserialize JSON to Configuration struct
    let mut configuration: crate::config::configuration::Configuration =
        serde_json::from_value(loose_typed).map_err(|e| format!("Failed to deserialize configuration from file {}: {}", path.display(), e))?;

    // Save configuration to database
    crate::config::save_configuration::save_configuration(&mut configuration, false).map_err(|e| format!("Failed to save imported configuration to database: {:?}", e))?;

    println!("Configuration successfully imported from {}", path.display());

    Ok(())
}

pub fn validate_configuration_file(path: &PathBuf) -> Result<(), String> {
    // Read file contents
    let file_contents = std::fs::read_to_string(path).map_err(|e| format!("Failed to read configuration file {}: {}", path.display(), e))?;

    // Load json into loose typed to validate version
    let loose_typed: serde_json::Value = serde_json::from_str(&file_contents).map_err(|e| format!("Failed to parse configuration file {}: {}", path.display(), e))?;

    // Check that versions match
    if loose_typed["version"] != crate::config::configuration::CURRENT_CONFIGURATION_VERSION {
        return Err(format!(
            "Configuration version mismatch: expected {}, found {}",
            crate::config::configuration::CURRENT_CONFIGURATION_VERSION,
            loose_typed["version"].as_i64().unwrap_or(-1)
        ));
    }

    // Deserialize JSON to Configuration struct to ensure it's valid
    let configuration: crate::config::configuration::Configuration =
        serde_json::from_value(loose_typed).map_err(|e| format!("Failed to deserialize configuration from file {}: {}", path.display(), e))?;

    configuration.validate().map_err(|e| format!("Configuration validation failed: {:?}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::configuration::Configuration;

    #[test]
    fn validates_current_format_and_rejects_flat_handlers() {
        struct TestFile(PathBuf);
        impl Drop for TestFile {
            fn drop(&mut self) {
                if let Err(error) = std::fs::remove_file(&self.0) {
                    eprintln!("Failed to remove test configuration file: {}", error);
                }
            }
        }

        let file = TestFile(std::env::temp_dir().join(format!("gruxi-config-{}.json", uuid::Uuid::new_v4())));
        let config = Configuration::get_default();
        let mut value = serde_json::to_value(&config).unwrap();
        std::fs::write(&file.0, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(validate_configuration_file(&file.0).is_ok());

        let handler = value["sites"][0]["request_handlers"][0].clone();
        let processor = handler["processor"]["config"].clone();
        value["sites"][0]["request_handlers"] = serde_json::json!([handler["id"]]);
        value["request_handlers"] = serde_json::json!([{
            "id": handler["id"], "name": handler["name"], "is_enabled": handler["is_enabled"],
            "url_match": handler["url_match"], "processor_type": "static", "processor_id": processor["id"]
        }]);
        value["static_file_processors"] = serde_json::json!([processor]);
        value["php_processors"] = serde_json::json!([]);
        value["proxy_processors"] = serde_json::json!([]);
        std::fs::write(&file.0, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(validate_configuration_file(&file.0).unwrap_err().contains("Failed to deserialize configuration"));
    }
}
