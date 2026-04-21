use crate::file::app_paths::get_app_paths;
use crate::file::normalized_path::NormalizedPath;
use crate::{error, trace};
use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;
use tokio::select;

use crate::core::running_state_manager::get_running_state_manager;
use crate::logging::access_log_entry::AccessLogEntry;
use crate::logging::buffered_log::BufferedLog;

use crossbeam_channel::{Receiver, Sender, TrySendError, bounded};

// Key is site ID, value is buffered log entries
pub struct AccessLogBuffer {
    buffered_logs: HashMap<String, BufferedLog>,
    // We store the raw log entries in the channel, as we want to keep the formatting and file writing separate
    // so that the formatting does not block the request handling, and the file writing does not block the formatting
    sender: Sender<AccessLogEntry>,
    receiver: Receiver<AccessLogEntry>,
}

impl AccessLogBuffer {
    pub async fn new() -> Self {
        let (sender, receiver) = bounded(10000);
        let mut access_log_buffer = AccessLogBuffer {
            buffered_logs: HashMap::new(),
            sender,
            receiver,
        };

        // Have a fallback log path in case it could not be resolved
        let app_paths = get_app_paths();
        let default_log_path_result = NormalizedPath::new(&app_paths.logs_dir.display().to_string(), "", true);
        let mut default_log_available = true;
        let default_log_path = match default_log_path_result {
            Ok(norm) => norm.get_full_path().to_string(),
            Err(_) => {
                default_log_available = false;
                "".to_string()
            }
        };

        // We get the config and add the logs we need
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();

        for site in &config.sites {
            if !site.access_log_enabled {
                continue;
            }

            let site_id = site.id.clone().to_string();

            // We try to figure out if the access file from configuration is relative or absolute path
            let test_path = Path::new(&site.access_log_file);
            let log_file_path_result = if test_path.is_absolute() {
                // Absolute path
                NormalizedPath::new(&site.access_log_file, "", true)
            } else {
                // Relative path, so we add the logs directory in front of it
                NormalizedPath::new(&app_paths.logs_dir.display().to_string(), &site.access_log_file, true)
            };

            let log_file_path = match log_file_path_result {
                Ok(path) => path.get_full_path().to_string(),
                Err(_) => {
                    error!("Invalid access log path for site '{}': {}. Using default '{}'.", site_id, site.access_log_file, default_log_path);
                    // We check if the default log path is available
                    if !default_log_available {
                        panic!("Default log path '{}' and the specified access log path '{}' are both not available.", default_log_path, site.access_log_file);
                    }

                    format!("{}/{}.log", default_log_path, site_id)
                }
            };
            trace!("Initialized access log buffer for site '{}' at path '{}'", &site.id, &log_file_path);
            access_log_buffer.buffered_logs.insert(site_id.clone(), BufferedLog::new(log_file_path.to_string(), 100000));
        }

        access_log_buffer
    }

    pub fn start_flushing_task(&self) {
        tokio::spawn(Self::start_flushing_thread());
        tokio::spawn(Self::start_formatting_thread());
    }

    pub fn add_log(&self, access_log_entry: AccessLogEntry) {
        if let Err(TrySendError::Disconnected(_)) = self.sender.try_send(access_log_entry) {
            // Channel disconnected - this shouldn't happen in normal operation
            error!("Access log channel disconnected - This shouldn't happen under normal operation - Report a bug if you see this");
        }
    }

    pub fn get_log_buffer(&self, site_id: &str) -> Option<&BufferedLog> {
        self.buffered_logs.get(site_id)
    }

    fn add_logs_to_buffer(&self) {
        let mut logs_received = 0;
        while let Ok(log_entry) = self.receiver.try_recv() {
            if let Some(buffered_log) = self.buffered_logs.get(&log_entry.site_id) {
                buffered_log.add_log(log_entry.format_for_log());
                logs_received += 1;
            } else {
                error!("Received log entry for unknown site ID '{}': {}", log_entry.site_id, log_entry.format_for_log());
            }
        }
        if logs_received > 0 {
            trace!("Received {} access log entries in this cycle", logs_received);
        }
    }

    pub async fn start_formatting_thread() {
        trace!("Starting access log formatting thread");

        let triggers = crate::core::triggers::get_trigger_handler();

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get shutdown token - Could not start formatting thread for access logging. Please report a bug");
                return;
            }
        };

        let stop_services_token_option = triggers.get_token("stop_services").await;
        let stop_services_token = match stop_services_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get stop_services token - Could not start formatting thread for access logging. Please report a bug");
                return;
            }
        };

        let running_state = get_running_state_manager().await.get_running_state();
        let access_log_buffer = running_state.get_access_log_buffer();

        loop {
            select! {
                _ = tokio::time::sleep(std::time::Duration::from_millis(250)) => {
                    // We drain the channel of all pending log entries and add them to the respective buffered logs
                    access_log_buffer.add_logs_to_buffer();
                },
                _ = shutdown_token.cancelled() => {
                    // we could have some logs still in the channel, but we just leave them, as the write thread will get them
                    break;
                },
                _ = stop_services_token.cancelled() => {
                    // we could have some logs still in the channel, but we just leave them, as the write thread will get them
                    break;
                }
            }
        }
    }

    pub async fn start_flushing_thread() {
        trace!("Starting access log write thread");

        let triggers = crate::core::triggers::get_trigger_handler();

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get shutdown token - Could not start flushing thread for access logging. Please report a bug");
                return;
            }
        };

        let stop_services_token_option = triggers.get_token("stop_services").await;
        let stop_services_token = match stop_services_token_option {
            Some(token) => token,
            None => {
                error!("Failed to get stop_services token - Could not start flushing thread for access logging. Please report a bug");
                return;
            }
        };

        let running_state = get_running_state_manager().await.get_running_state();
        let access_log_buffer = running_state.get_access_log_buffer();

        loop {
            select! {
                // Ideally, this would be adjustable according to the work load (such as elapsed time to do a flush in average)
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                        let start_time = Instant::now();

                        for (_site_id, log) in access_log_buffer.buffered_logs.iter() {
                            log.flush(false).await;
                        }
                        let elapsed = start_time.elapsed().as_millis();
                        if elapsed > 0 {
                            trace!("Access log flush cycle completed in {} ms", elapsed);
                        }
                },
                _ = shutdown_token.cancelled() => {
                    trace!("Access log write thread received shutdown signal, so flushing remaining logs and exiting");
                    access_log_buffer.add_logs_to_buffer();
                    for (_site_id, log) in access_log_buffer.buffered_logs.iter() {
                        log.flush(true).await;
                    }
                    break;
                },
                _ = stop_services_token.cancelled() => {
                    trace!("Access log write thread received stop services signal, so flushing remaining logs and exiting");
                    access_log_buffer.add_logs_to_buffer();
                    for (_site_id, log) in access_log_buffer.buffered_logs.iter() {
                        log.flush(true).await;
                    }
                    break;
                }
            }
        }
    }
}
