use crate::grux_port_manager::PortManager;
use log::{error, trace, warn};
use tokio::process::{Child, Command};
use std::time::Duration;

/// Structure to manage a persistent PHP-CGI process.
///
/// This handles:
/// - Starting php-cgi.exe with appropriate parameters for Windows
/// - Monitoring process health
/// - Automatic restart when the process dies
/// - Process lifecycle management
/// - Port management through the PortManager
pub struct PhpCgiProcess {
    process: Option<Child>,
    executable_path: String,
    restart_count: u32,
    service_id: String,
    assigned_port: Option<u16>,
    port_manager: PortManager,
}

impl PhpCgiProcess {
    pub fn new(executable_path: String, service_id: String, port_manager: PortManager) -> Self {
        PhpCgiProcess {
            process: None,
            executable_path,
            restart_count: 0,
            service_id,
            assigned_port: None,
            port_manager,
        }
    }

    pub async fn start(&mut self) -> Result<(), String> {
        trace!("Starting PHP-CGI process: {} for service {}", self.executable_path, self.service_id);

        // Allocate a port if we don't have one
        if self.assigned_port.is_none() {
            self.assigned_port = self.port_manager.allocate_port(self.service_id.clone()).await;
            if self.assigned_port.is_none() {
                return Err("Failed to allocate port for PHP-CGI process".to_string());
            }
        }

        let port = self.assigned_port.unwrap();
        let mut cmd = Command::new(&self.executable_path);

        if cfg!(target_os = "windows") {
            // For Windows, use php-cgi.exe in CGI mode with assigned port
            cmd.arg("-b").arg(format!("127.0.0.1:{}", port));
        }

        match cmd.spawn() {
            Ok(child) => {
                self.process = Some(child);
                self.restart_count += 1;
                trace!("PHP-CGI process started successfully on port {} for service {} (restart count: {})",
                      port, self.service_id, self.restart_count);
                Ok(())
            }
            Err(e) => {
                error!("Failed to start PHP-CGI process for service {}: {}", self.service_id, e);
                // Release the port if process failed to start
                if let Some(port) = self.assigned_port {
                    self.port_manager.release_port(port).await;
                    self.assigned_port = None;
                }
                Err(format!("Failed to start PHP-CGI: {}", e))
            }
        }
    }

    pub async fn is_alive(&mut self) -> bool {
        if let Some(ref mut process) = self.process {
            match process.try_wait() {
                Ok(Some(_)) => {
                    warn!("PHP-CGI process for service {} has exited", self.service_id);
                    self.process = None;
                    false
                }
                Ok(None) => true, // Process is still running
                Err(e) => {
                    error!("Error checking PHP-CGI process status for service {}: {}", self.service_id, e);
                    self.process = None;
                    false
                }
            }
        } else {
            false
        }
    }

    pub async fn ensure_running(&mut self) -> Result<(), String> {
        if !self.is_alive().await {
            warn!("PHP-CGI process for service {} is not running, restarting...", self.service_id);
            // Wait a bit before restarting to avoid rapid restart loops
            tokio::time::sleep(Duration::from_millis(1000)).await;
            self.start().await?;
        }
        Ok(())
    }

    pub async fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            trace!("Stopping PHP-CGI process for service {}", self.service_id);
            if let Err(e) = process.kill().await {
                error!("Failed to kill PHP-CGI process for service {}: {}", self.service_id, e);
            }
        }

        // Release the assigned port
        if let Some(port) = self.assigned_port.take() {
            self.port_manager.release_port(port).await;
        }
    }

    pub fn get_port(&self) -> Option<u16> {
        self.assigned_port
    }
}
