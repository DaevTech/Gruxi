use serde::{Deserialize, Serialize};

use crate::configuration::site::Site;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminPortal {
    pub is_enabled: bool,
    pub domain_name: String,
    pub tls_automatic_enabled: bool,
    pub tls_certificate_path: Option<String>,
    pub tls_key_path: Option<String>,
}

impl AdminPortal {
    pub fn new() -> Self {
        let is_enabled = !crate::core::command_line_args::cmd_disable_admin_portal();

        AdminPortal {
            is_enabled,
            domain_name: "".to_string(),
            tls_automatic_enabled: false,
            tls_certificate_path: None,
            tls_key_path: None,
        }
    }

    pub fn sanitize(&mut self) {
        // Trim the strings if they exist
        self.domain_name = self.domain_name.trim().to_lowercase();
        if self.domain_name.is_empty() {
            self.domain_name = "".to_string();
        }

        if let Some(cert_path) = &mut self.tls_certificate_path {
            *cert_path = cert_path.trim().to_string();
        }
        if let Some(key_path) = &mut self.tls_key_path {
            *key_path = key_path.trim().to_string();
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate domain_name if tls_automatic_enabled
        if self.tls_automatic_enabled {
            if !self.domain_name.is_empty() {
                // Verify that domain is valid and public
                if let Err(_) = Site::verify_hostname(&self.domain_name) {
                    errors.push("Admin portal automatic TLS requires a valid public domain name to be configured".to_string());
                }
            } else {
                errors.push("Admin portal automatic TLS requires a domain name to be configured".to_string());
            }
        }

        // Check TLS paths actually exist if provided (only relevant when not using automatic TLS)
        if !self.tls_automatic_enabled {
            if let Some(cert_path) = &self.tls_certificate_path {
                if !cert_path.is_empty() && !std::path::Path::new(cert_path).exists() {
                    errors.push(format!("TLS certificate path does not exist: {}", cert_path));
                }
            }
            if let Some(key_path) = &self.tls_key_path {
                if !key_path.is_empty() && !std::path::Path::new(key_path).exists() {
                    errors.push(format!("TLS key path does not exist: {}", key_path));
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn get_domain_name(&self) -> String {
        self.domain_name.clone()
    }

    pub fn get_tls_certificate_path(&self) -> String {
        self.tls_certificate_path.clone().unwrap_or_default()
    }

    pub fn get_tls_key_path(&self) -> String {
        self.tls_key_path.clone().unwrap_or_default()
    }
}
