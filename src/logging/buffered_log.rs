use log::trace;
use std::sync::{Mutex};
use std::fs::write;

pub struct BufferedLog {
    pub log_id: String,
    pub log_file_path: String,
    pub buffered_log: Mutex<Vec<String>>,
    pub log_count_flush: usize,
}

impl BufferedLog {
    pub fn new(id: String, full_file_path: String) -> Self {
        let buffered_log = BufferedLog {
            log_id: id,
            log_file_path: full_file_path,
            buffered_log: Mutex::new(Vec::new()),
            log_count_flush: 50,
        };

        // Create the log file and path if it does not exist
        if let Some(parent) = std::path::Path::new(&buffered_log.log_file_path).parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        write(&buffered_log.log_file_path, "").unwrap();

        buffered_log
    }

    pub fn add_log(&mut self, log: String) {
        let mut log_buffer = self.buffered_log.lock().unwrap();
        log_buffer.push(log);
    }

    pub fn consider_flush(&self) {
        // Get lock
        let mut log_buffer = self.buffered_log.lock().unwrap();
        if log_buffer.len() < self.log_count_flush {
            return;
        }

        trace!("Writing {} access log entries for log id {}", log_buffer.len(), &self.log_id);

        // Append the log to the file path
        let log_data = log_buffer.join("\n") + "\n";
        if let Err(e) = std::fs::OpenOptions::new().append(true).open(&self.log_file_path).and_then(|mut file| {
            use std::io::Write;
            file.write_all(log_data.as_bytes())
        }) {
            trace!("Failed to write access log for log id {}: {}", &self.log_id, e);
        }

        // Clear data and releases the lock
        log_buffer.clear();
    }
}
