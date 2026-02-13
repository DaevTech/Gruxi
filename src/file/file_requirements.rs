use std::path::Path;

pub fn check_file_requirements() -> Result<(), String> {
    check_dir_path_exist_readable_writable(Path::new("./db"))?;
    check_dir_path_exist_readable_writable(Path::new("./logs"))?;
    check_dir_path_exist_readable_writable(Path::new("./certs"))?;

    // Additionaliy check that if the ./db/gruxi.db file exist, it is readable and writable
    let db_file_path = Path::new("./db/gruxi.db");
    if db_file_path.exists() {
        if !db_file_path.is_file() {
            return Err(format!("./db/gruxi.db exists but is not a file"));
        }

        // Check that we can read the file
        match std::fs::File::open(&db_file_path) {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to read ./db/gruxi.db file: {}", e));
            }
        }

        // Check that we can write to the file (by opening it in append mode)
        match std::fs::OpenOptions::new().append(true).open(&db_file_path) {
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to write to ./db/gruxi.db file: {}", e));
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
