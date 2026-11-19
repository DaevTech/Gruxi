use crate::config::admin_portal::AdminPortal;
use crate::config::binding::Protocol;
use crate::config::caching::Caching;
use crate::config::core::Core;
use crate::config::file_cache::FileCache;
use crate::config::gzip::Gzip;
use crate::config::http_caching::HttpCaching;
use crate::config::logging::Logging;
use crate::config::request_handler::{Processor, RequestHandler};
use crate::config::server_settings::ServerSettings;
use crate::config::site::Site;
use crate::config::telemetry::Telemetry;
use crate::config::tls_settings::TlsSettings;
use crate::config::{binding::Binding, binding_site_relation::BindingSiteRelationship};
use crate::external_connections::managed_system::php_cgi::PhpCgi;
use crate::file::app_paths::get_app_paths;
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
    // External systems, such as PHP-CGI instances, FastCGI handlers, etc.
    pub php_cgi_handlers: Vec<PhpCgi>,
}

pub const CURRENT_CONFIGURATION_VERSION: i32 = 11;

impl Default for Configuration {
    fn default() -> Self {
        Self::new()
    }
}

impl Configuration {
    pub fn new() -> Self {
        Configuration {
            version: CURRENT_CONFIGURATION_VERSION,
            bindings: vec![],
            sites: vec![],
            binding_sites: vec![],
            core: Core {
                caching: Caching::new(),
                file_cache: FileCache::new(),
                gzip: Gzip::new(),
                server_settings: ServerSettings::new(),
                admin_portal: AdminPortal::new(),
                telemetry: Telemetry::new(),
                tls_settings: TlsSettings::new(),
                http_caching: HttpCaching::new(),
                logging: Logging::new(),
            },
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

        let mut handler_ids = std::collections::HashSet::new();
        let mut processor_ids = std::collections::HashSet::new();
        // Validate sites
        for (site_idx, site) in self.sites.iter().enumerate() {
            if let Err(site_errors) = site.validate() {
                for error in site_errors {
                    errors.push(format!("Site {}: {}", site_idx + 1, error));
                }
                for handler in &site.request_handlers {
                    if !handler_ids.insert(&handler.id) {
                        errors.push(format!("Duplicate request handler ID: '{}'", handler.id));
                    }
                    if !processor_ids.insert((std::mem::discriminant(&handler.processor), handler.processor.id())) {
                        errors.push(format!("Duplicate processor ID: '{}'", handler.processor.id()));
                    }
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

        // Validate external systems
        for php_cgi in self.php_cgi_handlers.iter() {
            if let Err(php_cgi_errors) = php_cgi.validate() {
                for error in php_cgi_errors {
                    errors.push(format!("PHP-CGI Handler '{}': {}", php_cgi.id, error));
                }
            }
        }

        // Validate that account email in TLS settings, if any of the sites have TLS automatic enabled
        let tls_automatic_sites: Vec<&Site> = self.sites.iter().filter(|s| s.tls_automatic_enabled).collect();
        if !tls_automatic_sites.is_empty() && self.core.tls_settings.account_email.is_empty() {
            errors.push("At least one site has TLS automatic enabled, but no account email is set in TLS settings".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn get_default() -> Self {
        let mut configuration = Self::new();

        // Bindings
        let default_binding = Binding {
            id: Uuid::new_v4().to_string(),
            ip: "0.0.0.0".to_string(),
            port: 80,
            is_admin: false,
            is_telemetry: false,
            is_tls: false,
            protocol: Protocol::Tcp,
        };

        let default_binding_tls = Binding {
            id: Uuid::new_v4().to_string(),
            ip: "0.0.0.0".to_string(),
            port: 443,
            is_admin: false,
            is_telemetry: false,
            is_tls: true,
            protocol: Protocol::Tcp,
        };

        // Static file processor for first site
        let app_paths = get_app_paths();
        let request1_static_processor = StaticFileProcessor::new(app_paths.default_www_dir.to_string_lossy().to_string(), vec!["index.html".to_string()]);

        // Request handler for first site
        let request_handler1 = RequestHandler {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "Static File Handler".to_string(),
            processor: Processor::Static(request1_static_processor),
            url_match: vec!["*".to_string()],
        };

        // Sites
        let mut default_site = Site::new();
        default_site.is_default = true;
        default_site.request_handlers = vec![request_handler1];

        // Default site
        configuration.binding_sites.push(BindingSiteRelationship {
            binding_id: default_binding.id.clone(),
            site_id: default_site.id.clone(),
        });
        configuration.binding_sites.push(BindingSiteRelationship {
            binding_id: default_binding_tls.id.clone(),
            site_id: default_site.id.clone(),
        });
        configuration.sites.push(default_site);

        configuration.bindings.push(default_binding);
        configuration.bindings.push(default_binding_tls);

        configuration
    }
}
