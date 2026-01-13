use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdminPortal {
    pub is_enabled: bool,
    pub tls_certificate_path: Option<String>,
    pub tls_key_path: Option<String>,
}

impl AdminPortal {
    pub fn new() -> Self {
        let is_enabled = !crate::core::command_line_args::cmd_disable_admin_portal();

        AdminPortal {
            is_enabled,
            tls_certificate_path: None,
            tls_key_path: None,
        }
    }

    pub fn sanitize(&mut self) {
        // Trim the strings if they exist
        if let Some(cert_path) = &mut self.tls_certificate_path {
            *cert_path = cert_path.trim().to_string();
        }
        if let Some(key_path) = &mut self.tls_key_path {
            *key_path = key_path.trim().to_string();
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check TLS paths actually exist if provided
        if let Some(cert_path) = &self.tls_certificate_path {
            if !std::path::Path::new(cert_path).exists() {
                errors.push(format!("TLS certificate path does not exist: {}", cert_path));
            }
        }
        if let Some(key_path) = &self.tls_key_path {
            if !std::path::Path::new(key_path).exists() {
                errors.push(format!("TLS key path does not exist: {}", key_path));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn get_tls_certificate_path(&self) -> String {
        self.tls_certificate_path.clone().unwrap_or_default()
    }

    pub fn get_tls_key_path(&self) -> String {
        self.tls_key_path.clone().unwrap_or_default()
    }
}
