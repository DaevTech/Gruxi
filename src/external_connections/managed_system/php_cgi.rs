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
    pub name: String,
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
    pub fn new(id: String, name: String, request_timeout: u32, concurrent_threads: u32, executable: String) -> Self {
        // Get the singleton port manager instance
        let port_manager = get_port_manager().clone();

        Self {
            id,
            name,
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

    pub fn sanitize(&mut self) {
        // Clean up executable path
        self.executable = self.executable.trim().to_string();

        // Clean up name
        self.name = self.name.trim().to_string();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate that ID is not empty
        if self.id.is_empty() {
            errors.push("PHP-CGI ID cannot be empty.".to_string());
        }

        // Should have non-empty name
        if self.name.is_empty() {
            errors.push("PHP-CGI name cannot be empty.".to_string());
        }

        // Validate that request is larger than zero
        if self.request_timeout < 1 {
            errors.push("PHP-CGI request timeout must be at least 1 second.".to_string());
        }

        // Validate executable path
        if self.executable.is_empty() {
            errors.push("PHP-CGI executable path cannot be empty.".to_string());
        }

        // Validate that executable exists
        if !self.executable.is_empty() && !std::path::Path::new(&self.executable).exists() {
            errors.push(format!("PHP-CGI executable not found at path: {}", self.executable));
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
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
        let port = match self.assigned_port {
            Some(p) => p,
            None => {
                return Err("Assigned port is missing after allocation".to_string());
            }
        };

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

        let shutdown_token_option = triggers.get_token("shutdown").await;
        let shutdown_token = match shutdown_token_option {
            Some(token) => token,
            None => {
                error("Failed to get shutdown token - PHP-CGI monitoring thread exiting - Please report a bug".to_string());
                return;
            }
        };

        let stop_services_token_option = triggers.get_token("stop_services").await;
        let stop_services_token = match stop_services_token_option {
            Some(token) => token,
            None => {
                error("Failed to get stop_services token - PHP-CGI monitoring thread exiting - Please report a bug".to_string());
                return;
            }
        };

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
