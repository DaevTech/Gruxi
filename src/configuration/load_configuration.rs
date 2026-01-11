use crate::configuration::binding_site_relation::BindingSiteRelationship;
use crate::core::database_schema::CURRENT_DB_SCHEMA_VERSION;
use crate::external_connections::managed_system::php_cgi;
use crate::http::request_handlers::processors::php_processor;
use crate::http::request_handlers::processors::proxy_processor::{ProxyProcessor, ProxyProcessorRewrite};
use crate::http::request_handlers::processors::static_files_processor::StaticFileProcessor;
use crate::logging::syslog::info;
use crate::{
    configuration::{binding::Binding, configuration::Configuration, core::Core, request_handler::RequestHandler, save_configuration::save_configuration, site::HeaderKV, site::Site},
    core::database_connection::get_database_connection,
};
use sqlite::Connection;
use sqlite::State;
use std::collections::HashMap;

// Load the configuration from the database or create a default one if it doesn't exist
pub fn init() -> Result<Configuration, Vec<String>> {
    let connection = get_database_connection().map_err(|e| vec![format!("Failed to get database connection: {}", e)])?;

    // Check if we need to load the default configuration
    let schema_version = get_schema_version();

    let configuration = {
        if schema_version == 0 {
            // No schema version found, likely first run - create default configuration

            info("No configuration found, creating default configuration");

            let mut configuration = Configuration::get_default();

            // Process the binding-site relationships
            handle_relationship_binding_sites(&configuration.binding_sites, &mut configuration.bindings, &mut configuration.sites);

            save_configuration(&mut configuration, true)?;

            // Update schema version to value of constant CURRENT_CONFIGURATION_VERSION
            let current_version = CURRENT_DB_SCHEMA_VERSION;
            connection
                .execute(&format!("UPDATE gruxi SET gruxi_value = {} WHERE gruxi_key = 'schema_version'", current_version))
                .map_err(|e| vec![format!("Failed to update schema version: {}", e)])?;

            configuration
        } else {
            // Load existing configuration
            fetch_configuration_in_db().map_err(|e| vec![format!("Failed to load configuration from database: {}", e)])?
        }
    };

    Ok(configuration)
}

fn get_schema_version() -> i32 {
    let connection_result = get_database_connection();
    if let Err(_) = connection_result {
        return 0;
    }
    let connection = connection_result.unwrap();

    let statement_result = connection.prepare("SELECT gruxi_value FROM gruxi WHERE gruxi_key = 'schema_version' LIMIT 1");
    if let Err(_) = statement_result {
        return 0;
    }
    let mut statement = statement_result.unwrap();

    match statement.next().unwrap() {
        State::Row => {
            let version: i64 = statement.read(0).unwrap_or(0);
            version as i32
        }
        State::Done => 0, // No version found, assume 0
    }
}

// Load the configuration from the normalized database tables - Returns the data from db as fresh
pub fn fetch_configuration_in_db() -> Result<Configuration, String> {

    let schema_version = get_schema_version();

    let connection = get_database_connection()?;

    // Basic sites and bindings
    let mut bindings = load_bindings(&connection)?;
    let sites = load_sites(&connection)?;
    let binding_sites = load_binding_sites_relationships(&connection)?;

    // Server configuration
    let core = load_core_config(&connection)?;

    // Request handlers and attached processors
    let request_handlers = load_request_handlers(&connection)?;
    let static_file_processors = load_static_file_processors(&connection)?;
    let php_processors = load_php_processors(&connection)?;
    let proxy_processors = load_proxy_processors(&connection)?;

    // External systems
    let php_cgi_handlers = load_php_cgi_handlers(&connection)?;

    // Process the binding-site relationships
    handle_relationship_binding_sites(&binding_sites, &mut bindings, &sites);

    // Do a sanitize, in case there are any invalid entries in the database
    let mut configuration = Configuration {
        version: schema_version,
        bindings,
        sites,
        binding_sites,
        core,
        request_handlers,
        static_file_processors,
        php_processors,
        proxy_processors,
        php_cgi_handlers: php_cgi_handlers,
    };
    configuration.sanitize();

    Ok(configuration)
}

fn load_proxy_processors(connection: &Connection) -> Result<Vec<ProxyProcessor>, String> {
    let mut statement = connection
        .prepare("SELECT * FROM proxy_processors")
        .map_err(|e| format!("Failed to prepare Proxy processors query: {}", e))?;

    let mut processors = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute Proxy processors query: {}", e))? {
        let processor_id: String = statement.read(0).map_err(|e| format!("Failed to read processor id: {}", e))?;
        let proxy_type: String = statement.read(1).map_err(|e| format!("Failed to read proxy_type: {}", e))?;
        let upstream_servers_str: String = statement.read(2).map_err(|e| format!("Failed to read upstream_servers: {}", e))?;
        let load_balancing_strategy: String = statement.read(3).map_err(|e| format!("Failed to read load_balancing_strategy: {}", e))?;
        let timeout_seconds: i64 = statement.read(4).map_err(|e| format!("Failed to read timeout_seconds: {}", e))?;
        let health_check_path: String = statement.read(5).map_err(|e| format!("Failed to read health_check_path: {}", e))?;
        let health_check_interval_seconds: i64 = statement.read(6).map_err(|e| format!("Failed to read health_check_interval_seconds: {}", e))?;
        let health_check_timeout_seconds: i64 = statement.read(7).map_err(|e| format!("Failed to read health_check_timeout_seconds: {}", e))?;
        let url_rewrites_str: String = statement.read(8).map_err(|e| format!("Failed to read url_rewrites: {}", e))?;
        let preserve_host_header_int: i64 = statement.read(9).map_err(|e| format!("Failed to read preserve_host_header: {}", e))?;
        let forced_host_header: String = statement.read(10).map_err(|e| format!("Failed to read forced_host_header: {}", e))?;
        let verify_tls_certificates_int: i64 = statement.read(11).map_err(|e| format!("Failed to read verify_tls_certificates: {}", e))?;

        // Upstream servers is stored as comma separated
        let upstream_servers = parse_comma_separated_list(&upstream_servers_str);

        // Url rewrites is stored as JSON array
        let url_rewrites: Vec<ProxyProcessorRewrite> = serde_json::from_str(&url_rewrites_str).map_err(|e| format!("Failed to parse url_rewrites JSON: {}", e))?;

        processors.push(ProxyProcessor {
            id: processor_id,
            proxy_type,
            upstream_servers,
            load_balancing_strategy,
            timeout_seconds: timeout_seconds as u16,
            health_check_path,
            health_check_interval_seconds: health_check_interval_seconds as u32,
            health_check_timeout_seconds: health_check_timeout_seconds as u32,
            url_rewrites,
            preserve_host_header: preserve_host_header_int != 0,
            forced_host_header,
            verify_tls_certificates: verify_tls_certificates_int != 0,
        });
    }
    Ok(processors)
}

fn load_php_processors(connection: &Connection) -> Result<Vec<php_processor::PHPProcessor>, String> {
    let mut statement = connection
        .prepare("SELECT * FROM php_processors")
        .map_err(|e| format!("Failed to prepare PHP processors query: {}", e))?;

    let mut processors = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute PHP processors query: {}", e))? {
        let processor_id: String = statement.read(0).map_err(|e| format!("Failed to read processor id: {}", e))?;
        let served_by_type: String = statement.read(1).map_err(|e| format!("Failed to read served_by_type: {}", e))?;
        let php_cgi_handler_id: String = statement.read(2).map_err(|e| format!("Failed to read php_cgi_handler_id: {}", e))?;
        let fastcgi_ip_and_port: String = statement.read(3).map_err(|e| format!("Failed to read fastcgi_ip_and_port: {}", e))?;
        let request_timeout: i64 = statement.read(4).map_err(|e| format!("Failed to read request_timeout: {}", e))?;
        let local_web_root: String = statement.read(5).map_err(|e| format!("Failed to read local_web_root: {}", e))?;
        let fastcgi_web_root: String = statement.read(6).map_err(|e| format!("Failed to read fastcgi_web_root: {}", e))?;

        processors.push(php_processor::PHPProcessor {
            id: processor_id,
            served_by_type,
            php_cgi_handler_id,
            fastcgi_ip_and_port,
            request_timeout: request_timeout as u32,
            local_web_root,
            fastcgi_web_root,
        });
    }

    Ok(processors)
}

fn load_php_cgi_handlers(connection: &Connection) -> Result<Vec<php_cgi::PhpCgi>, String> {
    let mut statement = connection
        .prepare("SELECT * FROM php_cgi_handlers")
        .map_err(|e| format!("Failed to prepare PHP-CGI handlers query: {}", e))?;

    let mut handlers = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute PHP-CGI handlers query: {}", e))? {
        let handler_id: String = statement.read(0).map_err(|e| format!("Failed to read handler id: {}", e))?;
        let name: String = statement.read(1).map_err(|e| format!("Failed to read name: {}", e))?;
        let request_timeout: i64 = statement.read(2).map_err(|e| format!("Failed to read request_timeout: {}", e))?;
        let concurrent_threads: i64 = statement.read(3).map_err(|e| format!("Failed to read concurrent_threads: {}", e))?;
        let executable: String = statement.read(4).map_err(|e| format!("Failed to read executable: {}", e))?;

        handlers.push(php_cgi::PhpCgi::new(handler_id, name, request_timeout as u32, concurrent_threads as u32, executable));
    }

    Ok(handlers)
}

pub fn handle_relationship_binding_sites(relationships: &Vec<BindingSiteRelationship>, bindings: &mut Vec<Binding>, sites: &Vec<Site>) {
    // For sites and binding, generate hashmaps for quick lookup
    let mut binding_map = bindings.iter_mut().map(|b| (b.id, b)).collect::<HashMap<_, _>>();
    let site_map = sites.iter().map(|s| (s.id, s)).collect::<HashMap<_, _>>();

    for relationship in relationships {
        if let Some(binding) = binding_map.get_mut(&(relationship.binding_id as usize)) {
            if let Some(site) = site_map.get(&(relationship.site_id as usize)) {
                binding.add_site((*site).clone());
            }
        }
    }
}

fn load_core_config(connection: &Connection) -> Result<Core, String> {
    // Load server settings (single record with id=1)
    let mut statement = connection
        .prepare("SELECT DISTINCT setting_key, setting_value FROM server_settings")
        .map_err(|e| format!("Failed to prepare server settings query: {}", e))?;

    // Get the default configuration for core
    let configuration = Configuration::get_default();
    let mut core = configuration.core;

    // Each row is a key/value pair, where key should be checked against known settings in the server settings struct
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute core settings query: {}", e))? {
        let key: String = statement.read(0).map_err(|e| format!("Failed to read key: {}", e))?;
        let value: String = statement.read(1).map_err(|e| format!("Failed to read value: {}", e))?;

        match key.as_str() {
            "file_cache_is_enabled" => {
                core.file_cache.is_enabled = value.parse::<bool>().map_err(|e| format!("Failed to parse file_cache_is_enabled: {}", e))?;
            }
            "file_cache_cache_item_size" => {
                core.file_cache.cache_item_size = value.parse::<usize>().map_err(|e| format!("Failed to parse file_cache_cache_item_size: {}", e))?;
            }
            "file_cache_cache_max_size_per_file" => {
                core.file_cache.cache_max_size_per_file = value.parse::<usize>().map_err(|e| format!("Failed to parse file_cache_cache_max_size_per_file: {}", e))?;
            }
            "file_cache_cache_item_time_between_checks" => {
                core.file_cache.cache_item_time_between_checks = value.parse::<usize>().map_err(|e| format!("Failed to parse file_cache_cache_item_time_between_checks: {}", e))?;
            }
            "file_cache_cleanup_thread_interval" => {
                core.file_cache.cleanup_thread_interval = value.parse::<usize>().map_err(|e| format!("Failed to parse file_cache_cleanup_thread_interval: {}", e))?;
            }
            "file_cache_max_item_lifetime" => {
                core.file_cache.max_item_lifetime = value.parse::<usize>().map_err(|e| format!("Failed to parse file_cache_max_item_lifetime: {}", e))?;
            }
            "file_cache_forced_eviction_threshold" => {
                core.file_cache.forced_eviction_threshold = value.parse::<usize>().map_err(|e| format!("Failed to parse file_cache_forced_eviction_threshold: {}", e))?;
            }
            "gzip_is_enabled" => {
                core.gzip.is_enabled = value.parse::<bool>().map_err(|e| format!("Failed to parse gzip_is_enabled: {}", e))?;
            }
            "gzip_compressible_content_types" => {
                core.gzip.compressible_content_types = parse_comma_separated_list(&value);
            }
            "max_body_size" => {
                core.server_settings.max_body_size = value.parse::<usize>().map_err(|e| format!("Failed to parse max_body_size: {}", e))?;
            }
            "blocked_file_patterns" => {
                core.server_settings.blocked_file_patterns = parse_comma_separated_list(&value);
            }
            _ => continue,
        }
    }

    Ok(core)
}

fn load_bindings(connection: &Connection) -> Result<Vec<Binding>, String> {
    let mut statement = connection.prepare("SELECT * FROM bindings").map_err(|e| format!("Failed to prepare bindings query: {}", e))?;

    let mut bindings = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute bindings query: {}", e))? {
        let binding_id: i64 = statement.read(0).map_err(|e| format!("Failed to read binding id: {}", e))?;
        let ip: String = statement.read(1).map_err(|e| format!("Failed to read ip: {}", e))?;
        let port: i64 = statement.read(2).map_err(|e| format!("Failed to read port: {}", e))?;
        let is_admin: i64 = statement.read(3).map_err(|e| format!("Failed to read is_admin: {}", e))?;
        let is_tls: i64 = statement.read(4).map_err(|e| format!("Failed to read is_tls: {}", e))?;

        bindings.push(Binding {
            id: binding_id as usize,
            ip,
            port: port as u16,
            is_admin: is_admin != 0,
            is_tls: is_tls != 0,
            sites: Vec::new(),
        });
    }

    Ok(bindings)
}

fn load_sites(connection: &Connection) -> Result<Vec<Site>, String> {
    let mut statement = connection.prepare("SELECT * FROM sites").map_err(|e| format!("Failed to prepare sites query: {}", e))?;

    let mut sites = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute sites query: {}", e))? {
        let site_id: i64 = statement.read(0).map_err(|e| format!("Failed to read site id: {}", e))?;
        let is_default: i64 = statement.read(1).map_err(|e| format!("Failed to read is_default: {}", e))?;
        let is_enabled: i64 = statement.read(2).map_err(|e| format!("Failed to read is_enabled: {}", e))?;

        // Hostnames is comma separated
        let hostnames_str: String = statement.read(3).map_err(|e| format!("Failed to read hostnames: {}", e))?;
        let hostnames = parse_comma_separated_list(&hostnames_str);

        let tls_cert_path: String = statement.read(4).ok().unwrap_or_default();
        let tls_cert_content: String = statement.read(5).ok().unwrap_or_default();
        let tls_key_path: String = statement.read(6).ok().unwrap_or_default();
        let tls_key_content: String = statement.read(7).ok().unwrap_or_default();

        // Request handlers is comma separated
        let request_handlers_str: String = statement.read(8).map_err(|e| format!("Failed to read request_handlers: {}", e))?;
        let request_handlers: Vec<String> = parse_comma_separated_list(&request_handlers_str);

        // Rewrite functions is comma separated
        let rewrite_functions_str: String = statement.read(9).map_err(|e| format!("Failed to read rewrite_functions: {}", e))?;
        let rewrite_functions: Vec<String> = parse_comma_separated_list(&rewrite_functions_str);

        // Access log
        let access_log_enabled: i64 = statement.read(10).map_err(|e| format!("Failed to read access_log_enabled: {}", e))?;
        let access_log_file: String = statement.read(11).map_err(|e| format!("Failed to read access_log_file: {}", e))?;

        // Optional extra_headers column (comma separated key=value)
        let extra_headers_str: String = statement.read(12).ok().unwrap_or_default();
        let extra_headers_pairs = parse_key_value_pairs(&extra_headers_str);
        let extra_headers: Vec<HeaderKV> = extra_headers_pairs.into_iter().map(|(k, v)| HeaderKV { key: k, value: v }).collect();

        sites.push(Site {
            id: site_id as usize,
            hostnames,
            is_default: is_default != 0,
            is_enabled: is_enabled != 0,
            tls_cert_path,
            tls_cert_content,
            tls_key_path,
            tls_key_content,
            request_handlers,
            rewrite_functions,
            access_log_enabled: access_log_enabled != 0,
            access_log_file,
            extra_headers,
        });
    }

    Ok(sites)
}

fn load_binding_sites_relationships(connection: &Connection) -> Result<Vec<BindingSiteRelationship>, String> {
    let mut statement = connection
        .prepare("SELECT DISTINCT binding_id, site_id FROM binding_sites")
        .map_err(|e| format!("Failed to prepare binding_sites query: {}", e))?;

    let mut binding_sites = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute binding_sites query: {}", e))? {
        let binding_id: i64 = statement.read(0).map_err(|e| format!("Failed to read binding_id: {}", e))?;
        let site_id: i64 = statement.read(1).map_err(|e| format!("Failed to read site_id: {}", e))?;

        binding_sites.push(BindingSiteRelationship {
            binding_id: binding_id as usize,
            site_id: site_id as usize,
        });
    }

    Ok(binding_sites)
}

fn load_request_handlers(connection: &Connection) -> Result<Vec<RequestHandler>, String> {
    let mut statement = connection
        // Select explicit columns to remain compatible with older schemas that may still have a legacy 'priority' column.
        .prepare("SELECT id, is_enabled, name, processor_type, processor_id, url_match FROM request_handler")
        .map_err(|e| format!("Failed to prepare request handlers query: {}", e))?;

    let mut request_handlers = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute request handlers query: {}", e))? {
        let handler_id: String = statement.read(0).map_err(|e| format!("Failed to read handler id: {}", e))?;
        let is_enabled: i64 = statement.read(1).map_err(|e| format!("Failed to read is_enabled: {}", e))?;
        let name: String = statement.read(2).map_err(|e| format!("Failed to read name: {}", e))?;
        let processor_type: String = statement.read(3).map_err(|e| format!("Failed to read processor_type: {}", e))?;
        let processor_id: String = statement.read(4).map_err(|e| format!("Failed to read processor_id: {}", e))?;
        let url_match_str: Option<String> = statement.read(5).ok();

        // Parse comma-separated strings
        let url_match = parse_comma_separated_list(&url_match_str.unwrap_or_default());

        request_handlers.push(RequestHandler {
            id: handler_id,
            is_enabled: is_enabled != 0,
            name,
            processor_type,
            processor_id,
            url_match,
        });
    }

    Ok(request_handlers)
}

fn load_static_file_processors(connection: &Connection) -> Result<Vec<StaticFileProcessor>, String> {
    let mut statement = connection
        .prepare("SELECT * FROM static_file_processors")
        .map_err(|e| format!("Failed to prepare static file processors query: {}", e))?;

    let mut processors = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute static file processors query: {}", e))? {
        let processor_id: String = statement.read(0).map_err(|e| format!("Failed to read processor id: {}", e))?;
        let web_root: String = statement.read(1).map_err(|e| format!("Failed to read web_root: {}", e))?;
        let web_root_index_file_list_str: String = statement.read(2).map_err(|e| format!("Failed to read web_root_index_file_list: {}", e))?;

        let web_root_index_file_list = parse_comma_separated_list(&web_root_index_file_list_str);

        processors.push(StaticFileProcessor {
            id: processor_id,
            web_root,
            web_root_index_file_list,
        });
    }

    Ok(processors)
}

fn parse_comma_separated_list(input: &str) -> Vec<String> {
    if input.is_empty() { Vec::new() } else { input.split(',').map(|s| s.trim().to_string()).collect() }
}

fn parse_key_value_pairs(input: &str) -> Vec<(String, String)> {
    if input.is_empty() {
        Vec::new()
    } else {
        input
            .split(',')
            .filter_map(|pair| {
                let parts: Vec<&str> = pair.splitn(2, '=').collect();
                if parts.len() == 2 {
                    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                } else {
                    None
                }
            })
            .collect()
    }
}
