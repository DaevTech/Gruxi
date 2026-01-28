use std::sync::Mutex;
use std::time::Instant;

pub struct BufferedLog {
    pub log_id: String,
    pub log_file_path: String,
    pub buffered_log: Mutex<Vec<String>>,
    pub seconds_before_force_flush: usize,
    pub log_count_flush: usize,
    pub last_flush: Mutex<Instant>,
}

impl BufferedLog {
    pub fn new(id: String, full_file_path: String) -> Self {
        let mut buffered_log = BufferedLog {
            log_id: id,
            log_file_path: full_file_path,
            buffered_log: Mutex::new(Vec::new()),
            seconds_before_force_flush: 5,
            log_count_flush: 10,
            last_flush: Mutex::new(Instant::now()),
        };

        // Create the log file and path if it does not exist
        if let Some(parent) = std::path::Path::new(&buffered_log.log_file_path).parent() {
            let dirs_created_result = std::fs::create_dir_all(parent);
            if let Err(e) = dirs_created_result {
                panic!("Failed to create log directory {}: {}", parent.to_string_lossy(), e);
            }
        }

        // Check if log file is indeed a file or a directory, if directory, add a default filename
        let log_path = std::path::Path::new(&buffered_log.log_file_path);
        if log_path.exists() && log_path.is_dir() {
            // If it's a directory, append a default log filename
            let mut log_path_buf = log_path.to_path_buf();
            log_path_buf.push("logfile.log");
            buffered_log.log_file_path = log_path_buf.to_string_lossy().to_string();
        }

        // Create the log file if it does not exist
        if !std::path::Path::new(&buffered_log.log_file_path).exists() {
            let file_create_result = std::fs::File::create(&buffered_log.log_file_path);
            if let Err(e) = file_create_result {
                panic!("Failed to create log file {}: {}", &buffered_log.log_file_path, e);
            }
        }

        buffered_log
    }

    pub fn add_log(&mut self, log: String) {
        let buffered_log_lock = self.buffered_log.lock();
        match buffered_log_lock {
            Ok(mut guard) => guard.push(log),
            Err(_) => {}, // We silently fail to add log if we cant get the lock
        }
    }

    pub fn consider_flush(&self, force_flush: bool) {
        // Get lock
        let mut log_buffer_result = self.buffered_log.lock();

        // If empty, we are done
        if let Ok(ref mut log_buffer) = log_buffer_result {
            if log_buffer.is_empty() {
                return;
            }
        }
        let mut log_buffer = match log_buffer_result {
            Ok(guard) => guard,
            Err(_) => return, // If we cant get the lock, we skip flushing
        };

        // If not enough time has passed and not enough logs, skip
        if !force_flush {
            let last_flush_lock = self.last_flush.lock();
            match last_flush_lock {
                Ok(guard) => {
                    let elapsed = guard.elapsed().as_secs() as usize;
                    if elapsed < self.seconds_before_force_flush && log_buffer.len() < self.log_count_flush {
                        return;
                    }
                },
                Err(_) => return, // If we cant get the lock, we skip flushing
            }
        }

        // Append the log to the file path
        let log_data = log_buffer.join("\n") + "\n";
        if let Err(e) = std::fs::OpenOptions::new().create(true).append(true).open(&self.log_file_path).and_then(|mut file| {
            use std::io::Write;
            file.write_all(log_data.as_bytes())
        }) {
            eprintln!("Failed to write buffered log to file {}: {}", &self.log_file_path, e);
        }

        // Clear data and releases the lock
        log_buffer.clear();
        let last_flush_lock = self.last_flush.lock();
        match last_flush_lock {
            Ok(mut guard) => {
                *guard = Instant::now();
            },
            Err(_) => {}, // If we cant get the lock, we skip updating last flush time
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffered_log_new_path_is_directory() {
        let log = BufferedLog::new("test_log".to_string(), "./temp_test_data/".to_string());
        assert!(log.log_file_path.ends_with("logfile.log"));
    }

    #[test]
    fn test_buffered_log_check_log_created() {
        let log = BufferedLog::new("test_log".to_string(), "./temp_test_data/test_access.log".to_string());
        assert!(std::path::Path::new(&log.log_file_path).exists());
        assert!(std::path::Path::new(&log.log_file_path).is_file());
        let log_str = std::fs::read_to_string(&log.log_file_path);
        match log_str {
            Ok(s) => assert!(s.is_empty()),
            Err(e) => panic!("Failed to read created log file: {}", e),
        }
    }
}
