use crate::configuration::core::Core;
use crate::configuration::file_cache::FileCache;
use crate::configuration::gzip::Gzip;
use crate::configuration::request_handler::RequestHandler;
use crate::configuration::server_settings::ServerSettings;
use crate::configuration::site::Site;
use crate::configuration::{binding::Binding, binding_site_relation::BindingSiteRelationship};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Configuration {
    pub version: String,
    pub bindings: Vec<Binding>,
    pub sites: Vec<Site>,
    pub binding_sites: Vec<BindingSiteRelationship>,
    pub core: Core,
    pub request_handlers: Vec<RequestHandler>,
}

pub static CURRENT_CONFIGURATION_VERSION: i32 = 1;

impl Configuration {
    pub fn new() -> Self {
        Configuration {
            version: CURRENT_CONFIGURATION_VERSION.to_string(),
            bindings: vec![],
            sites: vec![],
            binding_sites: vec![],
            core: Core {
                file_cache: FileCache {
                    is_enabled: false,
                    cache_item_size: 1000,
                    cache_max_size_per_file: 1024 * 1024 * 1,
                    cache_item_time_between_checks: 20, // seconds
                    cleanup_thread_interval: 10,        // seconds
                    max_item_lifetime: 60,              // seconds
                    forced_eviction_threshold: 70,      // 1-99 %
                },
                gzip: Gzip {
                    is_enabled: false,
                    compressible_content_types: vec![
                        "text/".to_string(),
                        "application/javascript".to_string(),
                        "application/json".to_string(),
                        "application/xml".to_string(),
                        "application/xhtml+xml".to_string(),
                        "application/x-javascript".to_string(),
                        "text/css".to_string(),
                        "text/html".to_string(),
                        "text/javascript".to_string(),
                        "application/x-yaml".to_string(),
                        "image/svg+xml".to_string(),
                        "application/font-woff".to_string(),
                        "application/font-woff2".to_string(),
                    ],
                },
                server_settings: ServerSettings {
                    max_body_size: 10 * 1024 * 1024, // 10 MB
                    blocked_file_patterns: vec![
                        "*.tmp".to_string(),
                        "*.log".to_string(),
                        "*.bak".to_string(),
                        "*.config".to_string(),
                        ".*".to_string(),
                        "*.php".to_string(),
                    ],
                    whitelisted_file_patterns: vec!["*/.well-known/*".to_string()],
                    operation_mode: "PRODUCTION".to_string(),
                },
            },
            request_handlers: vec![],
        }
    }

    // Sanitize the configuration before use
    pub fn sanitize(&mut self) {
        // Sanitize sites
        for site in &mut self.sites {
            site.sanitize();
        }
        /*
        // Sanitize bindings
        for binding in &mut self.bindings {
            binding.sanitize();
        }

        // Sanitize core settings
        self.core.sanitize();

        // Sanitize request handlers
        for handler in &mut self.request_handlers {
            handler.sanitize();
        }
        */
    }

    // Validates the entire configuration
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate sites
        for (site_idx, site) in self.sites.iter().enumerate() {
            if let Err(site_errors) = site.validate() {
                for error in site_errors {
                    errors.push(format!("Site {}: {}", site_idx + 1, error));
                }
            }
        }

        // Validate bindings

        // First check that none of the bindings have duplicate IP/port combinations
        let mut binding_combinations = std::collections::HashSet::new();
        for binding in &self.bindings {
            let combo = format!("{}:{}", binding.ip, binding.port);
            if !binding_combinations.insert(combo) {
                errors.push(format!("Duplicate binding for IP/Port combination: {}:{}", binding.ip, binding.port));
            }
        }
        // Check the individual bindings
        for (binding_idx, binding) in self.bindings.iter().enumerate() {
            if let Err(binding_errors) = binding.validate() {
                for error in binding_errors {
                    errors.push(format!("Binding {}: {}", binding_idx + 1, error));
                }
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

    pub fn add_testing_to_configuration(configuration: &mut Configuration) {
        let test_wp_site = Site {
            id: 3,
            hostnames: vec!["gruxsite".to_string()],
            is_default: false,
            is_enabled: true,
            web_root: "D:/dev/grux-website".to_string(),
            web_root_index_file_list: vec!["index.php".to_string()],
            enabled_handlers: vec!["1".to_string()], // For testing
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            rewrite_functions: vec!["OnlyWebRootIndexForSubdirs".to_string()],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "./logs/gruxsite-access-log.log".to_string(),
        };
        configuration.sites.push(test_wp_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 2, site_id: 3 });

        let testing_site = Site {
            id: 4,
            hostnames: vec!["gruxtest".to_string()],
            is_default: false,
            is_enabled: true,
            web_root: "./www-testing".to_string(),
            web_root_index_file_list: vec!["index.php".to_string()],
            enabled_handlers: vec!["1".to_string()], // For testing
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            rewrite_functions: vec![],
            extra_headers: vec![],
            access_log_enabled: true,
            access_log_file: "./logs/gruxtest-access-log.log".to_string(),
        };
        configuration.sites.push(testing_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 2, site_id: 4 });

        // Enable file cache
        configuration.core.file_cache.is_enabled = true;

        // Enable PHP, using Windows
        configuration.request_handlers[0].executable = "D:/dev/php/8.4.13nts/php-cgi.exe".to_string();
        configuration.request_handlers[0].ip_and_port = "".to_string();
        configuration.request_handlers[0].other_webroot = "".to_string();
    }

    pub fn get_default() -> Self {
        let mut configuration = Self::new();

        // Bindings
        let admin_binding = Binding {
            id: 1,
            ip: "0.0.0.0".to_string(),
            port: 8000,
            is_admin: true,
            is_tls: true,
            sites: Vec::new(),
        };
        configuration.bindings.push(admin_binding);

        let default_binding = Binding {
            id: 2,
            ip: "0.0.0.0".to_string(),
            port: 80,
            is_admin: false,
            is_tls: false,
            sites: Vec::new(),
        };
        configuration.bindings.push(default_binding);

        let default_binding_tls = Binding {
            id: 3,
            ip: "0.0.0.0".to_string(),
            port: 443,
            is_admin: false,
            is_tls: true,
            sites: Vec::new(),
        };
        configuration.bindings.push(default_binding_tls);

        // Sites
        let default_site = Site {
            id: 1,
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-default".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            enabled_handlers: vec![], // No specific handlers enabled by default
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            rewrite_functions: vec![],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };
        configuration.sites.push(default_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 2, site_id: 1 });
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 3, site_id: 1 });

        let admin_site = Site {
            id: 2,
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            web_root: "./www-admin".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
            enabled_handlers: vec![], // No specific handlers enabled by default
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            rewrite_functions: vec![],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };
        configuration.sites.push(admin_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 1, site_id: 2 });

        // Add request handlers
        let php_request_handler = RequestHandler {
            id: "1".to_string(),
            is_enabled: true,
            name: "PHP Handler".to_string(),
            handler_type: "php".to_string(),
            request_timeout: 30,   // seconds
            concurrent_threads: 0, // 0 = automatically based on CPU cores on this machine - If PHP
            file_match: vec![".php".to_string()],
            executable: "".to_string(),              // Path to the PHP CGI executable (windows only)
            ip_and_port: "php-fpm:9000".to_string(), // IP and port to connect to the handler (only for FastCGI, like PHP-FPM - primarily Linux, but also Windows with something like php-cgi.exe running in fastcgi mode or php-fpm in Docker/WSL)
            other_webroot: "/var/www/html".to_string(),
            extra_handler_config: vec![],
            extra_environment: vec![],
        };

        configuration.request_handlers.push(php_request_handler);

        configuration
    }
}
