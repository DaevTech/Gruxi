use crate::configuration::core::Core;
use crate::configuration::file_cache::FileCache;
use crate::configuration::gzip::Gzip;
use crate::configuration::request_handler::RequestHandler;
use crate::configuration::server_settings::ServerSettings;
use crate::configuration::site::Site;
use crate::configuration::{binding::Binding, binding_site_relation::BindingSiteRelationship};
use crate::external_connections::managed_system::php_cgi::PhpCgi;
use crate::http::request_handlers::processor_trait::ProcessorTrait;
use crate::http::request_handlers::processors::php_processor::PHPProcessor;
use crate::http::request_handlers::processors::proxy_processor::{ProxyProcessor, ProxyProcessorRewrite};
use crate::http::request_handlers::processors::static_files_processor::StaticFileProcessor;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct Configuration {
    pub version: i32,
    pub bindings: Vec<Binding>,
    pub sites: Vec<Site>,
    pub binding_sites: Vec<BindingSiteRelationship>,
    pub core: Core,
    // Request handlers and the processors they use
    pub request_handlers: Vec<RequestHandler>,
    pub static_file_processors: Vec<StaticFileProcessor>,
    pub php_processors: Vec<PHPProcessor>,
    pub proxy_processors: Vec<ProxyProcessor>,
    // External systems, such as PHP-CGI instances, FastCGI handlers, etc.
    pub php_cgi_handlers: Vec<PhpCgi>,
}

pub static CURRENT_CONFIGURATION_VERSION: i32 = 2;

impl Configuration {
    pub fn new() -> Self {
        Configuration {
            version: CURRENT_CONFIGURATION_VERSION,
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
                },
            },
            request_handlers: vec![],
            static_file_processors: vec![],
            php_processors: vec![],
            proxy_processors: vec![],
            php_cgi_handlers: vec![],
        }
    }

    // Sanitize the configuration before use
    pub fn sanitize(&mut self) {
        // Sanitize bindings
        for binding in &mut self.bindings {
            binding.sanitize();
        }

        // Sanitize sites
        for site in &mut self.sites {
            site.sanitize();
        }

        // Sanitize core settings
        self.core.sanitize();

        // Sanitize request handlers
        for handler in &mut self.request_handlers {
            handler.sanitize();
        }

        // Sanitize static file processors
        for processor in &mut self.static_file_processors {
            processor.sanitize();
        }

        // Sanitize PHP processors
        for processor in &mut self.php_processors {
            processor.sanitize();
        }

        // Sanitize proxy processors
        for processor in &mut self.proxy_processors {
            processor.sanitize();
        }

        // Sanitize external systems
        for php_cgi in &mut self.php_cgi_handlers {
            php_cgi.sanitize();
        }
    }

    // Validates the entire configuration
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Check that the version is the current version
        if self.version != CURRENT_CONFIGURATION_VERSION {
            errors.push(format!("Configuration version '{}' does not match current version '{}'", self.version, CURRENT_CONFIGURATION_VERSION));
        }

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

        // Valdidate processors
        for processor in &self.static_file_processors {
            if let Err(processor_errors) = processor.validate() {
                for error in processor_errors {
                    errors.push(format!("Static File Processor {}: {}", processor.id, error));
                }
            }
        }
        for processor in &self.php_processors {
            if let Err(processor_errors) = processor.validate() {
                for error in processor_errors {
                    errors.push(format!("PHP Processor {}: {}", processor.id, error));
                }
            }
        }
        for processor in &self.proxy_processors {
            if let Err(processor_errors) = processor.validate() {
                for error in processor_errors {
                    errors.push(format!("Proxy Processor {}: {}", processor.id, error));
                }
            }
        }

        // Validate external systems
        for (_, php_cgi) in self.php_cgi_handlers.iter().enumerate() {
            if let Err(php_cgi_errors) = php_cgi.validate() {
                for error in php_cgi_errors {
                    errors.push(format!("PHP-CGI Handler '{}': {}", php_cgi.id, error));
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
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

        // Static file processor for first site
        let request1_static_processor = StaticFileProcessor::new("./www-default".to_string(), vec!["index.html".to_string()]);

        // Request handler for first site
        let request_handler1 = RequestHandler {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "Static File Handler".to_string(),
            processor_type: "static".to_string(),
            processor_id: request1_static_processor.id.clone(),
            url_match: vec!["*".to_string()],
        };

        // Sites
        let default_site = Site {
            id: 1,
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            request_handlers: vec![request_handler1.id.clone()],
            rewrite_functions: vec![],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };
        configuration.sites.push(default_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 2, site_id: 1 });
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 3, site_id: 1 });
        configuration.request_handlers.push(request_handler1);
        configuration.static_file_processors.push(request1_static_processor);

        // Static file processor for admin site
        let request2_static_processor = StaticFileProcessor::new("./www-admin".to_string(), vec!["index.html".to_string()]);

        // Request handler for admin site
        let request_handler2 = RequestHandler {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "Static File Handler".to_string(),
            processor_type: "static".to_string(),
            processor_id: request2_static_processor.id.clone(),
            url_match: vec!["*".to_string()],
        };

        let admin_site = Site {
            id: 2,
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            request_handlers: vec![request_handler2.id.clone()],
            rewrite_functions: vec![],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };
        configuration.sites.push(admin_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 1, site_id: 2 });
        configuration.request_handlers.push(request_handler2);
        configuration.static_file_processors.push(request2_static_processor);

        // External systems
        let php1_cgi_id = Uuid::new_v4().to_string();
        let php_cgi_handler = PhpCgi::new(php1_cgi_id.clone(), "PHP 8.4 handler".to_string(), 30, 0, "D:/dev/php/8.4.13nts/php-cgi.exe".to_string());
        configuration.php_cgi_handlers.push(php_cgi_handler);

        // Request handler for php
        let mut request2_php_processor = PHPProcessor::new();
        request2_php_processor.served_by_type = "win-php-cgi".to_string();
        request2_php_processor.php_cgi_handler_id = php1_cgi_id.clone();
        request2_php_processor.local_web_root = "D:/dev/gruxi-website".to_string();

        let request_handler2 = RequestHandler {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "PHP processor".to_string(),
            processor_type: "php".to_string(),
            processor_id: request2_php_processor.id.clone(),
            url_match: vec!["*".to_string()],
        };

        // Request handler for the static files
        let request3_static_processor = StaticFileProcessor::new("D:/dev/gruxi-website".to_string(), vec!["".to_string()]);
        let request_handler3 = RequestHandler {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "Static File Handler".to_string(),
            processor_type: "static".to_string(),
            processor_id: request3_static_processor.id.clone(),
            url_match: vec!["*".to_string()],
        };

        let gruxi_site = Site {
            id: 3,
            hostnames: vec!["gruxisite".to_string()],
            is_default: false,
            is_enabled: true,
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            request_handlers: vec![request_handler3.id.clone(), request_handler2.id.clone()],
            rewrite_functions: vec!["OnlyWebRootIndexForSubdirs".to_string()],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };
        configuration.sites.push(gruxi_site);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 2, site_id: 3 });
        configuration.request_handlers.push(request_handler2);
        configuration.request_handlers.push(request_handler3);
        configuration.static_file_processors.push(request3_static_processor);
        configuration.php_processors.push(request2_php_processor);

        // Request handler for the static files
        let mut request4_proxy_processor = ProxyProcessor::new();
        request4_proxy_processor.upstream_servers = vec!["http://192.168.0.186:5000".to_string()];
        request4_proxy_processor.url_rewrites = vec![ProxyProcessorRewrite {
            from: "/test".to_string(),
            to: "/tests1".to_string(),
            is_case_insensitive: true,
        }];
        request4_proxy_processor.verify_tls_certificates = false;

        let request_handler4 = RequestHandler {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "Proxy test".to_string(),
            processor_type: "proxy".to_string(),
            processor_id: request4_proxy_processor.id.clone(),
            url_match: vec!["*".to_string()],
        };

        let gruxi_proxy = Site {
            id: 4,
            hostnames: vec!["gruxiproxy".to_string()],
            is_default: false,
            is_enabled: true,
            tls_cert_path: "".to_string(),
            tls_cert_content: "".to_string(),
            tls_key_path: "".to_string(),
            tls_key_content: "".to_string(),
            request_handlers: vec![request_handler4.id.clone()],
            rewrite_functions: vec!["OnlyWebRootIndexForSubdirs".to_string()],
            extra_headers: vec![],
            access_log_enabled: false,
            access_log_file: "".to_string(),
        };
        configuration.sites.push(gruxi_proxy);
        configuration.binding_sites.push(BindingSiteRelationship { binding_id: 2, site_id: 4 });
        configuration.request_handlers.push(request_handler4);
        configuration.proxy_processors.push(request4_proxy_processor);

        configuration
    }
}
