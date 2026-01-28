use crate::file::normalized_path::NormalizedPath;
use crate::logging::syslog::{debug, error, trace};
use std::collections::HashMap;
use std::time::Instant;
use tokio::select;

use crate::core::running_state_manager::get_running_state_manager;
use crate::logging::buffered_log::BufferedLog;

// Key is site ID, value is buffered log entries
pub struct AccessLogBuffer {
    pub buffered_logs: HashMap<String, BufferedLog>,
}

impl AccessLogBuffer {
    pub async fn new() -> Self {
        let mut access_log_buffer = AccessLogBuffer { buffered_logs: HashMap::new() };

        // Have a fallback log path in case it could not be resolved
        let default_log_path_result = NormalizedPath::new("./logs", "");
        let mut default_log_available = true;
        let default_log_path = match default_log_path_result {
            Ok(norm) => norm.get_full_path(),
            Err(_) => {
                default_log_available = false;
                "".to_string()
            }
        };

        // We get the config and add the logs we need
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        for site in &config.sites {
            if !site.access_log_enabled {
                continue;
            }

            let site_id = site.id.clone().to_string();
            let log_file_path_result = NormalizedPath::new(&site.access_log_file, "");

            let log_file_path = match log_file_path_result {
                Ok(path) => path.get_full_path(),
                Err(_) => {
                    error(format!("Invalid access log path for site {}: {}. Using default {}.", site_id, site.access_log_file, default_log_path));
                    // We check if the default log path is available
                    if !default_log_available {
                        panic!("Default log path './logs' and the specified access log path '{}' are both not available.", site.access_log_file);
                    }

                    let default_log_path_plus_site = format!("{}/{}.log", default_log_path, site_id);
                    default_log_path_plus_site
                }
            };
            trace(format!("Initialized access log buffer for site {} at path {}", &site.id, &log_file_path));
            access_log_buffer.buffered_logs.insert(site_id.clone(), BufferedLog::new(site_id.clone(), log_file_path));
        }

        access_log_buffer
    }

    pub fn start_flushing_task(&self) {
        tokio::spawn(Self::start_flushing_thread());
    }

    pub fn add_log(&self, site_id: String, log: String) {
        let log_buffer = self.buffered_logs.get(&site_id);
        if let Some(buffer) = log_buffer {
            let buffered_log_result = buffer.buffered_log.lock();
            match buffered_log_result {
                Ok(mut guard) => guard.push(log),
                Err(e) => debug(format!("Failed to acquire lock to add access log entry for site {}: {}", site_id, e)),
            }
        }
        // We currently just fail silently if no log buffer is found for the site_id
    }

    pub fn get_log_buffer(&self, site_id: &str) -> Option<&BufferedLog> {
        self.buffered_logs.get(site_id)
    }

    pub async fn start_flushing_thread() {
        trace("Starting access log write thread".to_string());

        let triggers = crate::core::triggers::get_trigger_handler();

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                error("Failed to get shutdown token - Could not start flushing thread for access logging. Please report a bug".to_string());
                return;
            }
        };

        let stop_services_token_option = triggers.get_token("stop_services").await;
        let stop_services_token = match stop_services_token_option {
            Some(token) => token,
            None => {
                error("Failed to get stop_services token - Could not start flushing thread for access logging. Please report a bug".to_string());
                return;
            }
        };

        let running_state = get_running_state_manager().await.get_running_state_unlocked().await;

        loop {
            select! {
                // Ideally, this would be adjustable according to the work load (such as elapsed time to do a flush in average)
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                        let start_time = Instant::now();
                        let access_log_buffer_rwlock = running_state.get_access_log_buffer();
                        let access_log_buffer = access_log_buffer_rwlock.read().await;

                        for (_site_id, log) in access_log_buffer.buffered_logs.iter() {
                            log.consider_flush(false);
                        }
                        let elapsed = start_time.elapsed().as_millis();
                        if elapsed > 0 {
                            debug(format!("Access log flush cycle completed in {} ms", elapsed));
                        }
                },
                _ = shutdown_token.cancelled() => {
                    trace("Access log write thread received shutdown signal, so flushing remaining logs and exiting".to_string());
                    let access_log_buffer_rwlock = running_state.get_access_log_buffer();
                    let access_log_buffer = access_log_buffer_rwlock.read().await;

                    for (_site_id, log) in access_log_buffer.buffered_logs.iter() {
                        log.consider_flush(true);
                    }
                    break;
                },
                _ = stop_services_token.cancelled() => {
                    trace("Access log write thread received stop services signal, so flushing remaining logs and exiting".to_string());
                    let access_log_buffer_rwlock = running_state.get_access_log_buffer();
                    let access_log_buffer = access_log_buffer_rwlock.read().await;

                    for (_site_id, log) in access_log_buffer.buffered_logs.iter() {
                        log.consider_flush(true);
                    }
                    break;
                }
            }
        }
    }
}
