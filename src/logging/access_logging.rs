use crate::logging::syslog::{debug, error, trace};
use std::collections::HashMap;
use std::time::Instant;
use tokio::select;

use crate::core::running_state_manager::get_running_state_manager;
use crate::file::file_util::get_full_file_path;
use crate::logging::buffered_log::BufferedLog;

// Key is site ID, value is buffered log entries
pub struct AccessLogBuffer {
    pub buffered_logs: HashMap<String, BufferedLog>,
}

impl AccessLogBuffer {
    pub async fn new() -> Self {
        let mut access_log_buffer = AccessLogBuffer { buffered_logs: HashMap::new() };

        // Have a fallback log path in case it could not be resolved
        let default_log_path = get_full_file_path(&"./logs".to_string()).unwrap();

        // We get the config and add the logs we need
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        for site in &config.sites {
            if !site.access_log_enabled {
                continue;
            }

            let site_id = site.id.clone().to_string();
            let log_file_path_result = get_full_file_path(&site.access_log_file);

            let log_file_path = match log_file_path_result {
                Ok(path) => path,
                Err(_) => {
                    error(format!("Invalid access log path for site {}: {}. Using default {}.", site_id, site.access_log_file, default_log_path));
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
            buffer.buffered_log.lock().unwrap().push(log);
        }
        // We currently just fail silently if no log buffer is found for the site_id
    }

    pub fn get_log_buffer(&self, site_id: &str) -> Option<&BufferedLog> {
        self.buffered_logs.get(site_id)
    }

    pub async fn start_flushing_thread() {
        trace("Starting access log write thread".to_string());

        let triggers = crate::core::triggers::get_trigger_handler();
        let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
        let service_stop_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

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
                _ = service_stop_token.cancelled() => {
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
