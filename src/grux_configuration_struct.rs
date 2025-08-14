use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Server {
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Binding {
    pub ip: String,
    pub port: u16,
    pub is_admin: bool,
    #[serde(default)]
    pub is_tls: bool,
    pub sites: Vec<Sites>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Sites {
    pub hostnames: Vec<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    pub web_root: String,
    pub web_root_index_file_list: Vec<String>,
    // Optional PEM file paths for this specific site; if not provided and served over TLS, a self-signed cert may be generated
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    #[serde(default)]
    pub tls_key_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Configuration {
    pub servers: Vec<Server>,
    pub admin_site: AdminSite,
    pub core: Core,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminSite {
    pub is_admin_portal_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileCache {
    pub is_enabled: bool,
    pub cache_item_size: usize,
    pub cache_max_size_per_file: usize,
    pub cache_item_time_between_checks: usize,
    pub cleanup_thread_interval: usize,
    pub max_item_lifetime: usize,         // in seconds
    pub forced_eviction_threshold: usize, // 1-99 %
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Gzip {
    pub is_enabled: bool,
    pub compressible_content_types: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Core {
    pub file_cache: FileCache,
    pub gzip: Gzip,
}

impl Configuration {
    pub fn new() -> Self {
        let default_site = Sites {
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-default".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            tls_cert_path: None,
            tls_key_path: None,
        };

        let admin_site = Sites {
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-admin".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            tls_cert_path: None,
            tls_key_path: None,
        };

        let admin_binding = Binding {
            ip: "0.0.0.0".to_string(),
            port: 8000,
            is_admin: true,
            is_tls: true,
            sites: vec![admin_site],
        };

        let default_binding = Binding {
            ip: "0.0.0.0".to_string(),
            port: 80,
            is_admin: false,
            is_tls: false,
            sites: vec![default_site],
        };

        let default_server = Server { bindings: vec![default_binding] };
        let admin_server = Server { bindings: vec![admin_binding] };

        let admin_site = AdminSite {
            is_admin_portal_enabled: true,
        };

        let file_cache = FileCache {
            is_enabled: true,
            cache_item_size: 1000,
            cache_max_size_per_file: 1024 * 1024 * 1,
            cache_item_time_between_checks: 20, // seconds
            cleanup_thread_interval: 10,        // seconds
            max_item_lifetime: 60,              // seconds
            forced_eviction_threshold: 70,      // 1-99 %
        };

        let gzip = Gzip {
            is_enabled: true,
            compressible_content_types: vec![
                "text/".to_string(),
                "application/json".to_string(),
                "application/javascript".to_string(),
                "application/xml".to_string(),
                "image/svg+xml".to_string(),
            ],
        };

        let core = Core { file_cache: file_cache, gzip: gzip };

        Configuration {
            servers: vec![default_server, admin_server],
            admin_site,
            core,
        }
    }

    /// Validates the entire configuration
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate servers
        if self.servers.is_empty() {
            errors.push("Configuration must have at least one server".to_string());
        }

        // Check that there's exactly one admin binding across all servers
        let total_admin_bindings = self.servers.iter()
            .flat_map(|server| &server.bindings)
            .filter(|binding| binding.is_admin)
            .count();

        if total_admin_bindings > 1 {
            errors.push("Configuration must have only have one (or none) admin binding".to_string());
        }

        for (server_idx, server) in self.servers.iter().enumerate() {
            if let Err(server_errors) = server.validate() {
                for error in server_errors {
                    errors.push(format!("Server {}: {}", server_idx + 1, error));
                }
            }
        }

        // Validate admin site
        if let Err(admin_errors) = self.admin_site.validate() {
            for error in admin_errors {
                errors.push(format!("Admin site: {}", error));
            }
        }

        // Validate core settings
        if let Err(core_errors) = self.core.validate() {
            for error in core_errors {
                errors.push(format!("Core: {}", error));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Server {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.bindings.is_empty() {
            errors.push("Server must have at least one binding".to_string());
        }

        // Check that only one binding can be admin
        let admin_bindings_count = self.bindings.iter().filter(|binding| binding.is_admin).count();
        if admin_bindings_count > 1 {
            errors.push("Only one binding per server can be marked as admin".to_string());
        }

        for (binding_idx, binding) in self.bindings.iter().enumerate() {
            if let Err(binding_errors) = binding.validate() {
                for error in binding_errors {
                    errors.push(format!("Binding {}: {}", binding_idx + 1, error));
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Binding {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate IP address
        if self.ip.is_empty() {
            errors.push("IP address cannot be empty".to_string());
        } else if self.ip.parse::<std::net::IpAddr>().is_err() {
            errors.push(format!("Invalid IP address: {}", self.ip));
        }

        // Validate port
        if self.port == 0 {
            errors.push("Port cannot be 0".to_string());
        }

        // Validate common TLS port usage
        if self.is_tls && self.port == 80 {
            errors.push("Port 80 is typically used for HTTP, not HTTPS. Consider using port 443 for TLS".to_string());
        }
        if !self.is_tls && self.port == 443 {
            errors.push("Port 443 is typically used for HTTPS, not HTTP. Consider using port 80 for non-TLS or enable TLS".to_string());
        }

        // Validate sites
        if self.sites.is_empty() {
            errors.push("Binding must have at least one site".to_string());
        }

        let mut has_default = false;
        for (site_idx, site) in self.sites.iter().enumerate() {
            if site.is_default {
                if has_default {
                    errors.push("Only one site per binding can be marked as default".to_string());
                } else {
                    has_default = true;
                }
            }

            if let Err(site_errors) = site.validate() {
                for error in site_errors {
                    errors.push(format!("Site {}: {}", site_idx + 1, error));
                }
            }
        }

        if !has_default && self.sites.len() > 1 {
            errors.push("One site must be marked as default when multiple sites exist".to_string());
        }

        // Admin binding specific validations
        if self.is_admin {
            // Admin bindings should typically use TLS for security
            if !self.is_tls {
                errors.push("Admin binding should use TLS for security".to_string());
            }

            // Admin bindings should have at least one site
            if self.sites.is_empty() {
                errors.push("Admin binding must have at least one site configured".to_string());
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Sites {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate hostnames
        if self.hostnames.is_empty() {
            errors.push("Site must have at least one hostname".to_string());
        }

        for (hostname_idx, hostname) in self.hostnames.iter().enumerate() {
            if hostname.trim().is_empty() {
                errors.push(format!("Hostname {} cannot be empty", hostname_idx + 1));
            } else if hostname.trim() != "*" && hostname.trim().len() < 3 {
                errors.push(format!("Hostname '{}' is too short (minimum 3 characters unless wildcard '*')", hostname.trim()));
            }
        }

        // Validate web root
        if self.web_root.trim().is_empty() {
            errors.push("Web root cannot be empty".to_string());
        }

        // Validate index files
        if self.web_root_index_file_list.is_empty() {
            errors.push("Site must have at least one index file".to_string());
        }

        for (file_idx, file) in self.web_root_index_file_list.iter().enumerate() {
            if file.trim().is_empty() {
                errors.push(format!("Index file {} cannot be empty", file_idx + 1));
            }
        }

        // Validate TLS certificate paths if provided
        if let Some(cert_path) = &self.tls_cert_path {
            if cert_path.trim().is_empty() {
                errors.push("TLS certificate path cannot be empty if specified".to_string());
            }
        }

        if let Some(key_path) = &self.tls_key_path {
            if key_path.trim().is_empty() {
                errors.push("TLS key path cannot be empty if specified".to_string());
            }
        }

        // If one TLS path is provided, both should be provided
        if self.tls_cert_path.is_some() && self.tls_key_path.is_none() {
            errors.push("TLS key path must be provided when certificate path is specified".to_string());
        }
        if self.tls_key_path.is_some() && self.tls_cert_path.is_none() {
            errors.push("TLS certificate path must be provided when key path is specified".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl AdminSite {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let errors = Vec::new();

        // Currently only has is_admin_portal_enabled field which is a boolean,
        // so no validation needed beyond the type system

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Core {
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

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl FileCache {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate cache_item_size
        if self.cache_item_size == 0 {
            errors.push("Max cached items count cannot be 0".to_string());
        }

        // Validate cache_max_size_per_file
        if self.cache_max_size_per_file == 0 {
            errors.push("Max size per file cannot be 0 bytes".to_string());
        }

        // Validate cache_item_time_between_checks
        if self.cache_item_time_between_checks == 0 {
            errors.push("Cache item time between checks cannot be 0".to_string());
        }

        // Validate cleanup_thread_interval
        if self.cleanup_thread_interval == 0 {
            errors.push("Cleanup thread interval cannot be 0".to_string());
        }

        // Validate max_item_lifetime
        if self.max_item_lifetime == 0 {
            errors.push("Max item lifetime cannot be 0".to_string());
        }

        // Validate forced_eviction_threshold (should be between 1-99)
        if self.forced_eviction_threshold == 0 || self.forced_eviction_threshold > 99 {
            errors.push("Forced eviction threshold must be between 1-99%".to_string());
        }

        // Note: cache_item_size is a count of items, cache_max_size_per_file is bytes per file
        // These are different units and cannot be compared directly

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Gzip {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate compressible content types
        if self.is_enabled && self.compressible_content_types.is_empty() {
            errors.push("At least one compressible content type must be specified when gzip is enabled".to_string());
        }

        for (content_type_idx, content_type) in self.compressible_content_types.iter().enumerate() {
            if content_type.trim().is_empty() {
                errors.push(format!("Content type {} cannot be empty", content_type_idx + 1));
            }

            // Basic validation for content type format
            if !content_type.contains('/') && !content_type.ends_with('/') {
                errors.push(format!("Content type '{}' appears to be invalid format (should contain '/' or end with '/')", content_type));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
