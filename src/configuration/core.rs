use crate::configuration::tls_settings::TlsSettings;
use crate::configuration::{admin_portal::AdminPortal, file_cache::FileCache};
use crate::configuration::gzip::Gzip;
use crate::configuration::server_settings::ServerSettings;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Core {
    pub file_cache: FileCache,
    pub gzip: Gzip,
    pub server_settings: ServerSettings,
    pub admin_portal: AdminPortal,
    pub tls_settings: TlsSettings,
}

impl Core {
    pub fn sanitize(&mut self) {
        self.file_cache.sanitize();
        self.gzip.sanitize();
        self.server_settings.sanitize();
        self.admin_portal.sanitize();
        self.tls_settings.sanitize();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate file cache settings
        if let Err(file_cache_errors) = self.file_cache.validate() {
            for error in file_cache_errors {
                errors.push(format!("File Cache: {}", error));
            }
        }

        // Validate gzip settings
        if let Err(gzip_errors) = self.gzip.validate() {
            for error in gzip_errors {
                errors.push(format!("Gzip: {}", error));
            }
        }

        // Validate server settings
        if let Err(server_settings_errors) = self.server_settings.validate() {
            for error in server_settings_errors {
                errors.push(format!("Server Settings: {}", error));
            }
        }

        // Validate admin portal settings
        if let Err(admin_portal_errors) = self.admin_portal.validate() {
            for error in admin_portal_errors {
                errors.push(format!("Admin Portal: {}", error));
            }
        }

        // Validate TLS settings
        if let Err(tls_errors) = self.tls_settings.validate() {
            for error in tls_errors {
                errors.push(format!("TLS Settings: {}", error));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
