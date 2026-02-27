use std::path::Path;

use crate::file::app_paths::get_app_paths;

pub fn check_file_requirements() -> Result<(), String> {
    let app_paths = get_app_paths();

    check_dir_path_exist_readable_writable(&app_paths.db_dir)?;
    check_dir_path_exist_readable_writable(&app_paths.logs_dir)?;
    check_dir_path_exist_readable_writable(&app_paths.certificates_dir)?;

    // Additionally check that if the gruxi.db file exist, it is readable and writable
    let db_file_path = app_paths.db_dir.join("gruxi.db");
    if db_file_path.exists() {
        if !db_file_path.is_file() {
            return Err(format!("{} exists but is not a file", db_file_path.display()));
        }

        // Check that we can read the file
        match std::fs::File::open(&db_file_path) {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to read {} file: {}", db_file_path.display(), e));
            }
        }

        // Check that we can write to the file (by opening it in append mode)
        match std::fs::OpenOptions::new().append(true).open(&db_file_path) {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to write to {} file: {}", db_file_path.display(), e));
            }
        }
    }

    Ok(())
}

fn check_dir_path_exist_readable_writable(path: &Path) -> Result<(), String> {
    // Check that the directory exists (or create it if it doesn't exist)
    if !path.exists() {
        std::fs::create_dir_all(path).map_err(|e| format!("Failed to create '{}' directory: {}", path.display(), e))?;
    }

    // Check that the directory is writable
    if !path.is_dir() {
        return Err(format!("{} exists but is not a directory", path.display()));
    }
    let test_file_path = path.join("test_write_permissions.tmp");
    match std::fs::File::create(&test_file_path) {
        Ok(_) => {
            // Clean up the test file after checking
            let _ = std::fs::remove_file(&test_file_path);
        }
        Err(e) => {
            return Err(format!("Failed to write to '{}' directory: {}", path.display(), e));
        }
    }

    // Check if we can read content from the directory (by trying to read the directory entries)
    match std::fs::read_dir(path) {
        Ok(_) => {}
        Err(e) => {
            return Err(format!("Failed to read from '{}' directory: {}", path.display(), e));
        }
    }

    Ok(())
}
