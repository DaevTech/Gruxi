/*
// Move to specific processor instead
pub request_timeout: usize,                      // Seconds
pub concurrent_threads: usize,                   // 0 = automatically based on CPU cores on this machine - If PHP-FPM or similar is used, this should match the max children configured there
pub executable: String,                          // Path to the executable or script that handles the request, like php-cgi.exe location for PHP on windows
pub ip_and_port: String,                         // IP and port to connect to the handler, e.g. 127.0.0.1:9000 for FastCGI passthrough
pub other_webroot: String,                       // Optional webroot to use when passing to the handler, if different from the site's webroot
pub extra_handler_config: Vec<(String, String)>, // Key/value pairs for extra handler configuration
pub extra_environment: Vec<(String, String)>,    // Key/value pairs to add to environment, passed on to the handler
*/

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::{
    process::{Child, Command},
    select,
};

use crate::{
    core::triggers::get_trigger_handler,
    external_connections::fastcgi::FastCgi,
    logging::syslog::{error, trace, warn},
    network::port_manager::{PortManager, get_port_manager},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PhpCgi {
    // Unique identifier for the external system
    pub id: String,
    // Inputs from configuration
    pub request_timeout: u32,
    pub concurrent_threads: u32,
    pub executable: String,

    // Internal state
    #[serde(skip)]
    process: Option<Child>,
    #[serde(skip)]
    restart_count: u32,
    #[serde(skip)]
    assigned_port: Option<u16>,
    #[serde(skip)]
    port_manager: PortManager,
    #[serde(skip, default = "Instant::now")]
    last_activity: Instant,
}

impl PhpCgi {
    pub fn new(id: String, request_timeout: u32, concurrent_threads: u32, executable: String) -> Self {
        // Get the singleton port manager instance
        let port_manager = get_port_manager().clone();

        Self {
            id,
            request_timeout,
            concurrent_threads,
            executable,
            process: None,
            restart_count: 0,
            assigned_port: None,
            port_manager,
            last_activity: Instant::now(),
        }
    }

    pub fn get_max_children_processes(&self) -> u32 {
        // Determine the concurrent threads. Can be set in config or we determine it based on CPU cores
        // 0 = automatically based on CPU cores
        if self.concurrent_threads == 0 {
            let cpus = num_cpus::get_physical();
            cpus as u32
        } else if self.concurrent_threads < 1 {
            1
        } else {
            self.concurrent_threads as u32
        }
    }

    // Start the PHP-CGI process and returns the assigned port
    pub async fn start(&mut self) -> Result<u16, String> {
        if cfg!(target_os = "linux") {
            return Err("PHP-CGI external system should only be used on Windows - On Linux, use PHP-FPM or similar.".to_string());
        }

        // Allocate a port if we don't have one
        if self.assigned_port.is_none() {
            self.assigned_port = self.port_manager.allocate_port("php-main-process".to_string()).await;
            if self.assigned_port.is_none() {
                return Err("Failed to allocate port for PHP-CGI process".to_string());
            }
        }

        let port = self.assigned_port.unwrap();
        let mut cmd = Command::new(&self.executable);
        cmd.kill_on_drop(true);

        // Setup command line arguments for PHP-CGI
        cmd.arg("-b").arg(format!("127.0.0.1:{}", port));

        // Set environment variable for FastCGI children
        cmd.env("PHP_FCGI_CHILDREN", self.get_max_children_processes().to_string());
        cmd.env("PHP_FCGI_MAX_REQUESTS", "10000"); // Request limit before restart the child process

        match cmd.spawn() {
            Ok(child) => {
                self.process = Some(child);
                self.restart_count += 1;
                self.last_activity = Instant::now();
                trace(format!("PHP-CGI process started successfully on port {} (restart count: {})", port, self.restart_count));
            }
            Err(e) => {
                error(format!("Failed to start PHP-CGI process: {}", e));
                // Release the port if process failed to start
                if let Some(port) = self.assigned_port {
                    self.port_manager.release_port(port).await;
                    self.assigned_port = None;
                }
                return Err(format!("Failed to start PHP-CGI: {}", e));
            }
        }

        Ok(port)
    }

    pub async fn start_monitoring_thread(mut instance: PhpCgi) {
        let triggers = get_trigger_handler();
        let shutdown_token = triggers.get_trigger("shutdown").expect("Failed to get shutdown trigger").read().await.clone();
        let stop_services_token = triggers.get_trigger("stop_services").expect("Failed to get stop_services trigger").read().await.clone();

        loop {
            select! {
                _ = shutdown_token.cancelled() => {
                    trace("Shutdown signal received, stopping PHP processes if running".to_string());
                    instance.stop().await;
                    break;
                },
                _ = stop_services_token.cancelled() => {
                    trace("Stop services signal received, stopping PHP processes if running".to_string());
                    instance.stop().await;
                    break;
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {
                    if let Err(e) = instance.ensure_running().await {
                        error(format!("Failed to ensure PHP-CGI process is running: {}", e));
                    }
                }
            }
        }
    }

    async fn is_alive(&mut self) -> bool {
        if let Some(ref mut process) = self.process.as_mut() {
            match process.try_wait() {
                Ok(Some(_)) => {
                    warn("PHP-CGI process has exited".to_string());
                    self.process = None;
                    false
                }
                Ok(None) => true, // Process is still running
                Err(e) => {
                    error(format!("Error checking PHP-CGI process status: {}", e));
                    self.process = None;
                    false
                }
            }
        } else {
            false
        }
    }

    async fn send_keep_alive(&mut self) -> bool {
        if let Some(port) = self.assigned_port {
            let ip_and_port = format!("127.0.0.1:{}", port);
            match FastCgi::send_fastcgi_keep_alive(&ip_and_port).await {
                Ok(_) => {
                    self.last_activity = Instant::now();
                    true
                }
                Err(e) => {
                    error(format!("Keep-alive FastCGI request failed: {}", e));
                    false
                }
            }
        } else {
            false
        }
    }

    async fn ensure_running(&mut self) -> Result<(), String> {
        if !self.is_alive().await {
            warn("PHP-CGI process is not running, restarting...".to_string());
            // Wait a bit before restarting to avoid rapid restart loops
            tokio::time::sleep(Duration::from_millis(1000)).await;
            self.start().await?;
        } else {
            // Check if we need to send a keep-alive
            let time_since_activity = self.last_activity.elapsed();
            if time_since_activity >= Duration::from_secs(10) {
                if !self.send_keep_alive().await {
                    warn("Keep-alive failed, restarting PHP-CGI process".to_string());
                    self.stop().await;
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    self.start().await?;
                }
            }
        }
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            trace("Stopping PHP-CGI process".to_string());
            if let Err(e) = process.kill().await {
                error(format!("Failed to kill PHP-CGI process: {}", e));
            }
        }

        // Release the assigned port
        if let Some(port) = self.assigned_port.take() {
            self.port_manager.release_port(port).await;
        }
    }
}
