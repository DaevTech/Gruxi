use grux::grux_configuration;
use std::fs;
use std::path::Path;

#[test]
fn test_configuration_same_reference() {
    // Load configuration twice
    let config1 = grux_configuration::get_configuration();
    let config2 = grux_configuration::get_configuration();

    // Check if both references point to the same instance
    assert!(std::ptr::eq(config1, config2), "Configuration instances should be the same");
}

#[test]
fn test_load_configuration_with_existing_config() {
    // Create copy of the database for testing
    let copied_db_path = "./temp_test_data/grux_test_existing.db";
    let original_db_path = "./grux.db";
    if Path::new(copied_db_path).exists() {
        fs::remove_file(copied_db_path).unwrap();
    }
    fs::copy(original_db_path, copied_db_path).unwrap();

    let result = grux_configuration::check_configuration();
    assert!(result.is_ok());

    assert!(fs::metadata(original_db_path).is_ok(), "Configuration database should exist");

    // Copy back the original database
    if Path::new(copied_db_path).exists() {
        fs::remove_file(original_db_path).unwrap();
        fs::rename(copied_db_path, original_db_path).unwrap();
    }
}
