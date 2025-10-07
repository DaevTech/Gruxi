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
    pub sites: Vec<Site>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Site {
    pub hostnames: Vec<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    pub web_root: String,
    pub web_root_index_file_list: Vec<String>,
    pub enabled_handlers: Vec<String>, // List of enabled handler IDs for this site
    // Optional PEM file paths for this specific site; if not provided and served over TLS, a self-signed cert may be generated
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    #[serde(default)]
    pub tls_key_path: Option<String>,
    pub rewrite_functions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Configuration {
    pub servers: Vec<Server>,
    pub admin_site: AdminSite,
    pub core: Core,
    pub request_handlers: Vec<RequestHandler>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestHandler {
    pub id: String,                                  // Generated id, unique, so it can be referenced from sites as a handler
    pub is_enabled: bool,                            // Whether it is enabled or not
    pub name: String,                                // A name to identify the handler, self chosen
    pub handler_type: String,                        // e.g., "php", "python", etc. Used by the handlers to identify if they should handle requests
    pub request_timeout: usize,                      // Seconds
    pub max_concurrent_threads: usize,              // 0 = automatically based on CPU cores
    pub file_match: Vec<String>,                     // .php, .html, etc
    pub executable: String,                          // Path to the executable or script that handles the request, like php-cgi.exe location for PHP on windows
    pub ip_and_port: String,                         // IP and port to connect to the handler, e.g. 127.0.0.1:9000 for FastCGI passthrough
    pub other_webroot: String,                       // Optional webroot to use when passing to the handler, if different from the site's webroot
    pub extra_handler_config: Vec<(String, String)>, // Key/value pairs for extra handler configuration
    pub extra_environment: Vec<(String, String)>,    // Key/value pairs to add to environment, passed on to the handler
}

impl Configuration {
    pub fn new() -> Self {
        let default_site = Site {
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-default".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            enabled_handlers: vec!["php_handler".to_string()], // For testing
            //enabled_handlers: vec![], // No specific handlers enabled by default
            tls_cert_path: None,
            tls_key_path: None,
            rewrite_functions: vec![],
        };

        let admin_site = Site {
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-admin".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            enabled_handlers: vec![], // No specific handlers enabled by default
            tls_cert_path: None,
            tls_key_path: None,
            rewrite_functions: vec![],
        };

        let test_wp_site = Site {
            hostnames: vec!["gruxsite".to_string()],
            is_default: false,
            is_enabled: true,
            web_root: "D:/dev/test-sites/grux-wp-site1".to_string(),
            web_root_index_file_list: vec!["index.php".to_string()],
            enabled_handlers: vec!["php_handler".to_string()], // For testing
            tls_cert_path: None,
            tls_key_path: None,
            rewrite_functions: vec!["OnlyWebRootIndexForSubdirs".to_string()],
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
            sites: vec![default_site.clone(), test_wp_site],
        };

        let default_binding_tls = Binding {
            ip: "0.0.0.0".to_string(),
            port: 443,
            is_admin: false,
            is_tls: true,
            sites: vec![default_site.clone()],
        };

        let default_server = Server {
            bindings: vec![default_binding, default_binding_tls],
        };
        let admin_server = Server { bindings: vec![admin_binding] };

        let admin_site = AdminSite { is_admin_portal_enabled: true };

        let file_cache = FileCache {
            is_enabled: false,
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

        let request_handlers = vec![RequestHandler {
            id: "php_handler".to_string(),
            is_enabled: true,
            name: "PHP Handler".to_string(),
            handler_type: "php".to_string(),
            request_timeout: 30,        // seconds
            max_concurrent_threads: 0, // 0 = automatically based on CPU cores
            file_match: vec![".php".to_string()],
            executable: "D:/dev/php/8.2.9/php-cgi.exe".to_string(), // Path to the PHP CGI executable (windows only)
            //ip_and_port: "127.0.0.1:9000".to_string(), // IP and port to connect to the handler (only for FastCGI, like PHP-FPM - primarily Linux, but also Windows with something like php-cgi.exe running in fastcgi mode or php-fpm in Docker/WSL)
            ip_and_port: "".to_string(), // IP and port to connect to the handler (only for FastCGI, like PHP-FPM - primarily Linux, but also Windows with something like php-cgi.exe running in fastcgi mode or php-fpm in Docker/WSL)
            //other_webroot: "/var/www/html".to_string(),
            other_webroot: "".to_string(),
            extra_handler_config: vec![],
            extra_environment: vec![],
        }];

        Configuration {
            servers: vec![default_server, admin_server],
            admin_site,
            core,
            request_handlers,
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
        let total_admin_bindings = self.servers.iter().flat_map(|server| &server.bindings).filter(|binding| binding.is_admin).count();

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

        // Validate request handlers
        for (handler_idx, handler) in self.request_handlers.iter().enumerate() {
            if let Err(handler_errors) = handler.validate() {
                for error in handler_errors {
                    errors.push(format!("Request Handler {}: {}", handler_idx + 1, error));
                }
            }
        }

        // Check for duplicate request handler IDs
        let mut handler_ids = std::collections::HashSet::new();
        for handler in &self.request_handlers {
            if !handler_ids.insert(&handler.id) {
                errors.push(format!("Duplicate request handler ID: '{}'", handler.id));
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

impl Site {
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

impl RequestHandler {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate ID
        if self.id.trim().is_empty() {
            errors.push("Request handler ID cannot be empty".to_string());
        } else if self.id.trim().len() < 3 {
            errors.push("Request handler ID must be at least 3 characters long".to_string());
        } else if !self.id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            errors.push("Request handler ID can only contain alphanumeric characters, underscores, and hyphens".to_string());
        }

        // Validate name
        if self.name.trim().is_empty() {
            errors.push("Request handler name cannot be empty".to_string());
        }

        // Validate handler type
        if self.handler_type.trim().is_empty() {
            errors.push("Handler type cannot be empty".to_string());
        } else {
            // Validate known handler types
            let valid_types = ["php", "python", "node", "static", "proxy"];
            if !valid_types.contains(&self.handler_type.trim()) {
                errors.push(format!("Unknown handler type '{}'. Valid types are: {}", self.handler_type, valid_types.join(", ")));
            }
        }

        // Validate request timeout
        if self.request_timeout == 0 {
            errors.push("Request timeout cannot be 0 seconds".to_string());
        } else if self.request_timeout > 3600 {
            errors.push("Request timeout cannot exceed 3600 seconds (1 hour)".to_string());
        }

        // Validate max concurrent threads
        if self.max_concurrent_threads > 1000 {
            errors.push("Max concurrent threads cannot exceed 1000".to_string());
        }

        // Validate file match patterns
        if self.file_match.is_empty() {
            errors.push("File match patterns cannot be empty".to_string());
        } else {
            for (pattern_idx, pattern) in self.file_match.iter().enumerate() {
                if pattern.trim().is_empty() {
                    errors.push(format!("File match pattern {} cannot be empty", pattern_idx + 1));
                } else if !pattern.starts_with('.') && !pattern.starts_with('*') {
                    errors.push(format!("File match pattern '{}' should start with '.' or '*'", pattern));
                }
            }
        }

        // Validate executable path
        if self.executable.trim().is_empty() {
            errors.push("Executable path cannot be empty".to_string());
        }

        // Validate IP and port
        if !self.ip_and_port.trim().is_empty() {
            // Basic format validation for IP:port
            if !self.ip_and_port.contains(':') {
                errors.push("IP and port must be in format 'IP:PORT'".to_string());
            } else {
                let parts: Vec<&str> = self.ip_and_port.split(':').collect();
                if parts.len() != 2 {
                    errors.push("IP and port must be in format 'IP:PORT'".to_string());
                } else {
                    // Validate IP part
                    if parts[0].parse::<std::net::IpAddr>().is_err() {
                        errors.push(format!("Invalid IP address in '{}': {}", self.ip_and_port, parts[0]));
                    }

                    // Validate port part
                    if let Ok(port) = parts[1].parse::<u16>() {
                        if port == 0 {
                            errors.push("Port cannot be 0".to_string());
                        }
                    } else {
                        errors.push(format!("Invalid port in '{}': {}", self.ip_and_port, parts[1]));
                    }
                }
            }
        }

        // Validate extra handler config
        for (config_idx, (key, value)) in self.extra_handler_config.iter().enumerate() {
            if key.trim().is_empty() {
                errors.push(format!("Extra handler config key {} cannot be empty", config_idx + 1));
            }
            if value.trim().is_empty() {
                errors.push(format!("Extra handler config value {} cannot be empty", config_idx + 1));
            }
        }

        // Validate extra environment variables
        for (env_idx, (key, value)) in self.extra_environment.iter().enumerate() {
            if key.trim().is_empty() {
                errors.push(format!("Environment variable key {} cannot be empty", env_idx + 1));
            }
            if value.trim().is_empty() {
                errors.push(format!("Environment variable value {} cannot be empty", env_idx + 1));
            }

            // Check for valid environment variable name format
            if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
                errors.push(format!("Environment variable key '{}' can only contain alphanumeric characters and underscores", key));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
