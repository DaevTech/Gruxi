use crate::{config::logging::Logging, core::triggers::get_trigger_handler, error, file::app_paths::get_app_paths, info, trace, warn};
use std::path::PathBuf;
use tokio::select;

#[allow(unused)]
pub struct LogRotation {
    log_thread: tokio::task::JoinHandle<()>,
}

impl LogRotation {
    pub async fn new() -> Self {
        // Get the configuration for log rotation from the global config
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();

        // Create pathbuf to logs dir from environment
        let app_paths = get_app_paths();
        let log_dir = app_paths.logs_dir.clone();

        LogRotation {
            log_thread: tokio::spawn(Self::do_rotation_wait(config.core.logging.clone(), log_dir)),
        }
    }

    async fn do_rotation_wait(mut logging_config: Logging, log_dir: PathBuf) {
        if !logging_config.log_rotation_enabled {
            // Log rotation is disabled; exit the task
            return;
        }

        let interval_for_check = tokio::time::Duration::from_secs(60); // Check every minute

        trace!(
            "Starting log rotation task, with check interval of {} seconds and configuration: {:?}",
            interval_for_check.as_secs(),
            logging_config
        );

        let triggers = get_trigger_handler();
        let configuration_token_option = triggers.get_token("reload_configuration").await;
        let configuration_token = match configuration_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get reload_configuration token - Log rotation thread exiting - Please report a bug");
                return;
            }
        };

        let stop_services_token_option = triggers.get_token("stop_services").await;
        let stop_services_token = match stop_services_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get stop_services token - Log rotation thread exiting - Please report a bug");
                return;
            }
        };

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get shutdown token - Log rotation thread exiting - Please report a bug");
                return;
            }
        };

        loop {
            select! {
                _ = tokio::time::sleep(interval_for_check) => {
                    // So, for every interval, we check the files in the log rotation dir
                    trace!("Checking log files for rotation needs");
                    Self::perform_rotation_as_needed(&mut logging_config, &log_dir).await;
                },
                _ = configuration_token.cancelled() => {
                    break;
                }
                _ = stop_services_token.cancelled() => {
                    break;
                }
                _ = shutdown_token.cancelled() => {
                    break;
                }
            }
        }
    }

    async fn perform_rotation_as_needed(logging_config: &mut Logging, log_dir: &PathBuf) {
        // Read the files in the log directory
        let read_dir_result = tokio::fs::read_dir(log_dir).await;
        let mut read_dir = match read_dir_result {
            Ok(dir) => dir,
            Err(e) => {
                error!("Failed to read log directory for rotation: {}", e);
                return;
            }
        };

        while let Some(entry_result) = read_dir.next_entry().await.transpose() {
            let entry = match entry_result {
                Ok(e) => e,
                Err(e) => {
                    error!("Failed to read log directory entry for rotation: {}", e);
                    continue;
                }
            };

            // Make sure it ends with .log and not .rotated.log
            let file_path = entry.path();
            let file_path_str = match file_path.to_str() {
                Some(s) => s,
                None => {
                    error!("Failed to convert log file path to string for rotation: {:?}", file_path);
                    continue;
                }
            };
            if !file_path_str.ends_with(".log") {
                trace!("Skipping non-log file in log rotation: {:?}", file_path);
                continue;
            }
            if file_path_str.ends_with(".rotated.log") {
                trace!("Skipping already rotated log file in log rotation: {:?}", file_path);
                if logging_config.delete_old_logs {
                    Self::check_if_rotated_log_should_be_deleted(&file_path, logging_config.max_log_age_days).await;
                }
                continue;
            }

            // Get the metadata on the file
            let metadata_result = entry.metadata().await;
            let metadata = match metadata_result {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to get metadata for log file {:?}: {}", file_path, e);
                    continue;
                }
            };

            trace!("Checking log file {:?} for rotation needs", file_path);
            // Check if we need to rotate by size first
            if logging_config.rotate_by_size {
                let file_size = metadata.len();
                let max_file_size_bytes = logging_config.max_log_file_size_mb * 1024 * 1024;
                trace!("Log file {:?} considered for rotation with size: {} bytes and max: {} bytes", file_path, file_size, max_file_size_bytes);
                if file_size >= max_file_size_bytes {
                    // Perform rotation
                    let file_path = file_path.clone();
                    Self::rotate_file(&file_path).await;
                    continue;
                }
            }

            // Second up, we check if we need to rotate by age
            if logging_config.rotate_by_time {
                let created_time = match metadata.created() {
                    Ok(t) => t,
                    Err(_) => {
                        // Created is not support, so we skip time-based rotation globally and let the user know
                        warn!("File system does not support created time for log rotation; disabling time-based log rotation");
                        logging_config.rotate_by_time = false;
                        return;
                    }
                };

                let age_duration = chrono::Duration::from_std(std::time::SystemTime::now().duration_since(created_time).unwrap_or_default()).unwrap_or_default();
                trace!("Log file {:?} age duration: {:?}", entry.path(), age_duration);

                let should_rotate = match logging_config.log_time_rotation_type.as_str() {
                    "daily" => {
                        age_duration.num_days() >= 1
                    }
                    "weekly" => {
                        age_duration.num_days() >= 7
                    }
                    "monthly" => {
                        age_duration.num_days() >= 30
                    }
                    _ => {
                        logging_config.rotate_by_time = false;
                        warn!("Invalid log time rotation type: {} - Skipping time-based rotation", logging_config.log_time_rotation_type);
                        return;
                    }
                };

                if should_rotate {
                    // Perform rotation
                    let file_path = file_path.clone();
                    Self::rotate_file(&file_path).await;
                    continue;
                }
            }
        }
    }

    async fn rotate_file(file_path: &PathBuf) {
        // Perform rotation
        let rotated_file_path = file_path.with_extension(format!("{}.rotated.log", chrono::Local::now().format("%Y%m%d.%H%M%S")));
        let rename_result = tokio::fs::rename(&file_path, &rotated_file_path).await;
        match rename_result {
            Ok(_) => {
                info!("Rotated log file {:?} to {:?}", file_path, rotated_file_path);
            }
            Err(e) => {
                error!("Failed to rotate log file {:?}: {}", file_path, e);
            }
        }
    }

    async fn check_if_rotated_log_should_be_deleted(file_path: &PathBuf, max_age_days: u32) {
        let metadata_result = tokio::fs::metadata(file_path).await;
        let metadata = match metadata_result {
            Ok(m) => m,
            Err(e) => {
                error!("Failed to get metadata for rotated log file {:?}: {}", file_path, e);
                return;
            }
        };

        let modified_time = match metadata.modified() {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to get modified time for rotated log file {:?}: {}", file_path, e);
                return;
            }
        };

        let age_duration = chrono::Duration::from_std(std::time::SystemTime::now().duration_since(modified_time).unwrap_or_default()).unwrap_or_default();
        if age_duration.num_days() >= max_age_days as i64 {
            let delete_result = tokio::fs::remove_file(file_path).await;
            match delete_result {
                Ok(_) => {
                    info!("Deleted old rotated log file {:?} (age: {} days)", file_path, age_duration.num_days());
                }
                Err(e) => {
                    error!("Failed to delete old rotated log file {:?}: {}", file_path, e);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir() -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        let dir = PathBuf::from(format!("./temp_test_data/log_rotation_tests_{}", nanos));
        fs::create_dir_all(&dir).expect("Failed to create temp test directory");
        dir
    }

    fn create_file_with_size(path: &PathBuf, size_bytes: usize) {
        let mut file = fs::File::create(path).expect("Failed to create test log file");
        let data = vec![0u8; size_bytes];
        file.write_all(&data).expect("Failed to write test log file");
    }

    #[tokio::test]
    async fn test_rotate_file_creates_rotated_log() {
        let temp_dir = make_temp_dir();
        let log_file = temp_dir.join("test.log");
        create_file_with_size(&log_file, 10);

        LogRotation::rotate_file(&log_file).await;

        assert!(!log_file.exists());

        let mut rotated_found = false;
        for entry in fs::read_dir(&temp_dir).expect("Failed to read temp dir") {
            let entry = entry.expect("Failed to read temp dir entry");
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with("test.") && file_name.ends_with(".rotated.log") {
                rotated_found = true;
                break;
            }
        }

        assert!(rotated_found, "Rotated log file was not created");

        // Clean up directory
        fs::remove_dir_all(&temp_dir).expect("Failed to clean up temp test directory");
    }

    #[tokio::test]
    async fn test_perform_rotation_by_size_rotates_only_log_files() {
        let temp_dir = make_temp_dir();
        let log_file = temp_dir.join("size.log");
        let rotated_log_file = temp_dir.join("already.rotated.log");
        let non_log_file = temp_dir.join("notes.txt");

        create_file_with_size(&log_file, 2 * 1024 * 1024);
        create_file_with_size(&rotated_log_file, 10);
        create_file_with_size(&non_log_file, 10);

        let mut logging_config = Logging {
            log_rotation_enabled: true,
            rotate_by_size: true,
            max_log_file_size_mb: 1,
            rotate_by_time: false,
            log_time_rotation_type: "daily".to_string(),
            delete_old_logs: false,
            max_log_age_days: 0,
        };

        LogRotation::perform_rotation_as_needed(&mut logging_config, &temp_dir).await;

        assert!(!log_file.exists(), "Log file was not rotated by size");
        assert!(rotated_log_file.exists(), "Already rotated log file should remain");
        assert!(non_log_file.exists(), "Non-log file should remain");

        let mut rotated_found = false;
        for entry in fs::read_dir(&temp_dir).expect("Failed to read temp dir") {
            let entry = entry.expect("Failed to read temp dir entry");
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy();
            if file_name.starts_with("size.") && file_name.ends_with(".rotated.log") {
                rotated_found = true;
                break;
            }
        }
        assert!(rotated_found, "Rotated log file was not created for size.log");

        // Clean up directory
        fs::remove_dir_all(&temp_dir).expect("Failed to clean up temp test directory");
    }

    #[tokio::test]
    async fn test_perform_rotation_invalid_time_type_disables_time_rotation() {
        let temp_dir = make_temp_dir();
        let log_file = temp_dir.join("time.log");
        create_file_with_size(&log_file, 10);

        let mut logging_config = Logging {
            log_rotation_enabled: true,
            rotate_by_size: false,
            max_log_file_size_mb: 1,
            rotate_by_time: true,
            log_time_rotation_type: "invalid".to_string(),
            delete_old_logs: false,
            max_log_age_days: 0,
        };

        LogRotation::perform_rotation_as_needed(&mut logging_config, &temp_dir).await;

        assert!(!logging_config.rotate_by_time, "Time-based rotation should be disabled for invalid type");
        assert!(log_file.exists(), "Log file should remain when time rotation is invalid");

        // Clean up directory
        fs::remove_dir_all(&temp_dir).expect("Failed to clean up temp test directory");
    }
}
