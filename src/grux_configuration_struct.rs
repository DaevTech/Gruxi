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
    pub sites: Vec<Sites>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Sites {
    pub hostnames: Vec<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    pub is_ssl: bool,
    pub is_ssl_required: bool,
    pub web_root: String,
    pub web_root_index_file_list: Vec<String>,
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
    pub admin_portal_ip: String,
    pub admin_portal_port: u16,
    pub admin_portal_web_root: String,
    pub admin_portal_index_file: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileCache {
    pub is_enabled: bool,
    pub cache_item_size: usize,
    pub cache_max_size_per_file: usize,
    pub cache_item_time_between_checks: usize,
    pub cleanup_thread_interval: usize,
    pub max_item_lifetime: usize, // in seconds
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
            is_ssl: false,
            is_ssl_required: false,
            web_root: "./www-default".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
        };

        let default_binding = Binding {
            ip: "0.0.0.0".to_string(),
            port: 80,
            is_admin: false,
            sites: vec![default_site],
        };

        let default_server = Server { bindings: vec![default_binding] };

        let admin_site = AdminSite {
            is_admin_portal_enabled: true,
            admin_portal_ip: "0.0.0.0".to_string(),
            admin_portal_port: 8000,
            admin_portal_web_root: "./www-admin".to_string(),
            admin_portal_index_file: "index.html".to_string(),
        };

        let file_cache = FileCache {
            is_enabled: true,
            cache_item_size: 1000,
            cache_max_size_per_file: 1024 * 1024 * 1,
            cache_item_time_between_checks: 20,    // seconds
            cleanup_thread_interval: 10,     // seconds
            max_item_lifetime: 60,           // seconds
            forced_eviction_threshold: 70,   // 1-99 %
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

        let core = Core {
            file_cache: file_cache,
            gzip: gzip,
        };

        Configuration {
            servers: vec![default_server],
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

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Server {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.bindings.is_empty() {
            errors.push("Server must have at least one binding".to_string());
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

        // Validate SSL settings
        if self.is_ssl_required && !self.is_ssl {
            errors.push("SSL cannot be required when SSL is not enabled".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl AdminSite {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate IP address
        if self.admin_portal_ip.is_empty() {
            errors.push("Admin portal IP address cannot be empty".to_string());
        } else if self.admin_portal_ip.parse::<std::net::IpAddr>().is_err() {
            errors.push(format!("Invalid admin portal IP address: {}", self.admin_portal_ip));
        }

        // Validate port
        if self.admin_portal_port == 0 {
            errors.push("Admin portal port cannot be 0".to_string());
        }

        // Validate web root
        if self.admin_portal_web_root.trim().is_empty() {
            errors.push("Admin portal web root cannot be empty".to_string());
        }
        if self.admin_portal_web_root.ends_with("/") {
            errors.push("Admin portal web root cannot end with a slash".to_string());
        }

        // Validate index file
        if self.admin_portal_index_file.trim().is_empty() {
            errors.push("Admin portal index file cannot be empty".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
