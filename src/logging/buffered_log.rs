use log::trace;
use std::fs::write;
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
        let buffered_log = BufferedLog {
            log_id: id,
            log_file_path: full_file_path,
            buffered_log: Mutex::new(Vec::new()),
            seconds_before_force_flush: 5,
            log_count_flush: 10,
            last_flush: Mutex::new(Instant::now()),
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

        // If empty, we are done
        if log_buffer.is_empty() {
            return;
        }

        // If not enough time has passed and not enough logs, skip
        let elapsed = self.last_flush.lock().unwrap().elapsed().as_secs() as usize;
        if elapsed < self.seconds_before_force_flush && log_buffer.len() < self.log_count_flush {
            return;
        }

        // If either condition met, flush
        let logs_to_write = log_buffer.len();
        trace!("Writing {} access log entries for log id {}", logs_to_write, &self.log_id);

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
        let mut last_flush = self.last_flush.lock().unwrap();
        *last_flush = Instant::now();
    }
}
