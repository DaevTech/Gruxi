use log::{debug, error, trace};
use std::time::Instant;
use std::{collections::HashMap, sync::OnceLock};
use tokio::select;

use crate::configuration::load_configuration::get_configuration;
use crate::core::shutdown_manager::get_shutdown_manager;
use crate::grux_file_util::get_full_file_path;
use crate::logging::buffered_log::BufferedLog;

// Key is site ID, value is buffered log entries
pub struct AccessLogBuffer {
    pub buffered_logs: HashMap<String, BufferedLog>,
}

impl AccessLogBuffer {
    pub fn new() -> Self {
        let mut access_log_buffer = AccessLogBuffer { buffered_logs: HashMap::new() };

        // Have a fallback log path in case it could not be resolved
        let default_log_path = get_full_file_path(&"./logs".to_string()).unwrap();

        // We get the config and add the logs we need
        let config = get_configuration();

        for site in &config.sites {
            if !site.access_log_enabled {
                continue;
            }

            let site_id = site.id.clone().to_string();
            let log_file_path_result = get_full_file_path(&site.access_log_file);

            let log_file_path = match log_file_path_result {
                Ok(path) => path,
                Err(_) => {
                    error!("Invalid access log path for site {}: {}. Using default {}.", site_id, site.access_log_file, default_log_path);
                    let default_log_path_plus_site = format!("{}/{}.log", default_log_path, site_id);
                    default_log_path_plus_site
                }
            };
            trace!("Initialized access log buffer for site {} at path {}", &site.id, &log_file_path);
            access_log_buffer.buffered_logs.insert(site_id.clone(), BufferedLog::new(site_id.clone(), log_file_path));
        }

        access_log_buffer
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

    pub fn start_flushing_thread(&self) {
        tokio::spawn(Self::write_loops());
    }

    async fn write_loops() {
        let buffered_logs = get_access_log_buffer();
        trace!("Starting access log write thread");

        let shutdown_manager = get_shutdown_manager();
        let cancellation_token = shutdown_manager.get_cancellation_token();

        loop {
            select! {
                // Ideally, this would be adjustable according to the work load (such as elapsed time to do a flush in average)
                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                        let start_time = Instant::now();
                        for (_site_id, log) in buffered_logs.buffered_logs.iter() {
                            log.consider_flush(false);
                        }
                        let elapsed = start_time.elapsed().as_millis();
                        if elapsed > 0 {
                            debug!("Access log flush cycle completed in {} ms", elapsed);
                        }

                },
                _ = cancellation_token.cancelled() => {
                    trace!("Access log write thread received shutdown signal, so flushing remaining logs and exiting");
                    for (_site_id, log) in buffered_logs.buffered_logs.iter() {
                        log.consider_flush(true);
                    }
                    break;
                }
            }
        }
    }
}

// Get the configuration
pub fn get_access_log_buffer() -> &'static AccessLogBuffer {
    static CONFIG: OnceLock<AccessLogBuffer> = OnceLock::new();
    CONFIG.get_or_init(|| AccessLogBuffer::new())
}
