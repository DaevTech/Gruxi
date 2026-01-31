use std::path::PathBuf;

use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
use tokio::io::AsyncWriteExt;

/// Channel capacity for log entries
const CHANNEL_CAPACITY: usize = 1000;

pub struct BufferedLog {
    log_file_path: PathBuf,
    sender: Sender<String>,
    receiver: Receiver<String>,
}

impl BufferedLog {
    pub fn new(full_file_path: String) -> Self {
        let (sender, receiver) = bounded(CHANNEL_CAPACITY);
        let mut buffered_log = BufferedLog {
            log_file_path: PathBuf::from(&full_file_path),
            sender,
            receiver,
        };

        // Create the log file and path if it does not exist
        let log_path = &buffered_log.log_file_path;
        if let Some(parent) = log_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                panic!("Failed to create log directory {}: {}", parent.display(), e);
            }
        }

        // Check if log file is indeed a file or a directory, if directory, add a default filename
        if log_path.exists() && log_path.is_dir() {
            // If it's a directory, append a default log filename
            buffered_log.log_file_path.push("logfile.log");
        }

        // Create the log file if it does not exist
        if !buffered_log.log_file_path.exists() {
            if let Err(e) = std::fs::File::create(&buffered_log.log_file_path) {
                panic!("Failed to create log file {}: {}", buffered_log.log_file_path.display(), e);
            }
        }

        buffered_log
    }

    /// Add a log entry to the buffer (lock-free)
    pub fn add_log(&self, log: String) {
        // try_send is non-blocking; if channel is full, we drop the log
        // This prevents backpressure from slowing down request handling
        if let Err(TrySendError::Disconnected(_)) = self.sender.try_send(log) {
            // Channel disconnected - this shouldn't happen in normal operation
            eprintln!("Log channel disconnected");
        }
        // If Full, we silently drop - acceptable for logging under extreme load
    }

    /// Flush all pending log entries to disk
    /// If `force_flush` is true, also sync data to disk (for shutdown)
    pub async fn flush(&self, force_flush: bool) {
        // Drain all available logs from the channel (non-blocking)
        let mut logs_to_write = Vec::with_capacity(self.receiver.len().min(CHANNEL_CAPACITY));
        while let Ok(log) = self.receiver.try_recv() {
            logs_to_write.push(log);
        }

        // If nothing to write, we're done
        if logs_to_write.is_empty() {
            return;
        }

        // Build log data efficiently with pre-calculated capacity
        let total_len: usize = logs_to_write.iter().map(|s| s.len() + 1).sum();
        let mut log_data = String::with_capacity(total_len);
        for entry in &logs_to_write {
            log_data.push_str(entry);
            log_data.push('\n');
        }

        // Append the log to the file path using async I/O
        let write_result = async {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.log_file_path)
                .await?;
            file.write_all(log_data.as_bytes()).await?;
            // Only sync to disk on force flush (shutdown), otherwise let OS buffer
            if force_flush {
                file.sync_data().await?;
            }
            Ok::<(), std::io::Error>(())
        }
        .await;

        if let Err(e) = write_result {
            eprintln!("Failed to write buffered log to file {}: {}", self.log_file_path.display(), e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffered_log_new_path_is_directory() {
        let log = BufferedLog::new("./temp_test_data/".to_string());
        assert!(log.log_file_path.ends_with("logfile.log"));
    }

    #[test]
    fn test_buffered_log_check_log_created() {
        let log = BufferedLog::new("./temp_test_data/test_access.log".to_string());
        assert!(log.log_file_path.exists());
        assert!(log.log_file_path.is_file());
        let log_str = std::fs::read_to_string(&log.log_file_path);
        match log_str {
            Ok(s) => assert!(s.is_empty()),
            Err(e) => panic!("Failed to read created log file: {}", e),
        }
    }
}
