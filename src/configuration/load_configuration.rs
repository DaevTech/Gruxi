use crate::configuration::binding_site_relation::BindingSiteRelationship;
use crate::grux_core::operation_mode::{OperationMode, get_operation_mode};
use crate::{
    configuration::{binding::Binding, configuration::Configuration, core::Core, request_handler::RequestHandler, save_configuration::save_configuration, site::Site},
    grux_core::database_connection::get_database_connection,
};
use log::info;
use sqlite::Connection;
use sqlite::State;
use std::collections::HashMap;
use std::sync::OnceLock;

// Load the configuration from the database or create a default one if it doesn't exist
fn init() -> Result<Configuration, String> {
    let connection = get_database_connection()?;

    // Check if we need to load the default configuration
    let schema_version = {
        let mut statement = connection
            .prepare("SELECT version FROM schema_version ORDER BY version DESC LIMIT 1")
            .map_err(|e| format!("Failed to prepare schema version query: {}", e))?;

        match statement.next().map_err(|e| format!("Failed to execute schema version query: {}", e))? {
            State::Row => statement.read::<i64, _>(0).map_err(|e| format!("Failed to read schema version: {}", e))?,
            State::Done => 0, // No version found, assume 0
        }
    };

    let configuration = {
        if schema_version == 0 {
            // No schema version found, likely first run - create default configuration

            info!("No configuration found, creating default configuration");

            let mut configuration = Configuration::get_default();

            // Load default configuration based on operation mode
            let operation_mode = get_operation_mode();
            if operation_mode == OperationMode::DEV {
                info!("Loading dev configuration");
                Configuration::add_testing_to_configuration(&mut configuration);
            }

            // Process the binding-site relationships
            handle_relationship_binding_sites(&configuration.binding_sites, &mut configuration.bindings, &mut configuration.sites);

            // For bindings and sites, we need to set all the id's to 0, so they will be saved correctly as new records
            for binding in configuration.bindings.iter_mut() {
                binding.id = 0;
            }
            for site in configuration.sites.iter_mut() {
                site.id = 0;
            }

            save_configuration(&mut configuration)?;

            // Update schema version to 1
            connection
                .execute(format!("UPDATE schema_version SET version = 1",))
                .map_err(|e| format!("Failed to update schema version: {}", e))?;

            configuration
        } else {
            // Load existing configuration
            load_configuration()?
        }
    };

    Ok(configuration)
}

// Get the configuration
pub fn get_configuration() -> &'static Configuration {
    static CONFIG: OnceLock<Configuration> = OnceLock::new();
    CONFIG.get_or_init(|| init().unwrap_or_else(|e| panic!("Failed to initialize configuration: {}", e)))
}

// Load the configuration and return any errors
// Should be used in the main function to check configuration
pub fn check_configuration() -> Result<Configuration, String> {
    init()
}

/// Load the configuration from the normalized database tables
/// Returns the data from db as fresh, use the singleton get_configuration() for cached access
pub fn load_configuration() -> Result<Configuration, String> {
    let connection = get_database_connection()?;

    // Load all configuration components
    let mut bindings = load_bindings(&connection)?;
    let mut sites = load_sites(&connection)?;
    let binding_sites = load_binding_sites_relationships(&connection)?;
    let core = load_core_config(&connection)?;
    let request_handlers = load_request_handlers(&connection)?;

    // Process the binding-site relationships
    handle_relationship_binding_sites(&binding_sites, &mut bindings, &mut sites);

    Ok(Configuration {
        bindings,
        sites,
        binding_sites,
        core,
        request_handlers,
    })
}

pub fn handle_relationship_binding_sites(relationships: &Vec<BindingSiteRelationship>, bindings: &mut Vec<Binding>, sites: &mut Vec<Site>) {
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

        let web_root: String = statement.read(4).map_err(|e| format!("Failed to read web_root: {}", e))?;

        // Index files is comma separated
        let web_root_index_file_list_str: String = statement.read(5).map_err(|e| format!("Failed to read web_root_index_file_list: {}", e))?;
        let web_root_index_file_list = parse_comma_separated_list(&web_root_index_file_list_str);

        let enabled_handlers_str: String = statement.read(6).map_err(|e| format!("Failed to read enabled_handlers: {}", e))?;
        let enabled_handlers = parse_comma_separated_list(&enabled_handlers_str);

        let tls_cert_path: String = statement.read(7).ok().unwrap_or_default();
        let tls_cert_content: String = statement.read(8).ok().unwrap_or_default();
        let tls_key_path: String = statement.read(9).ok().unwrap_or_default();
        let tls_key_content: String = statement.read(10).ok().unwrap_or_default();

        // Rewrite functions is comma separated
        let rewrite_functions_str: String = statement.read(11).map_err(|e| format!("Failed to read rewrite_functions: {}", e))?;
        let rewrite_functions: Vec<String> = parse_comma_separated_list(&rewrite_functions_str);

        // Access log
        let access_log_enabled: i64 = statement.read(12).map_err(|e| format!("Failed to read access_log_enabled: {}", e))?;
        let access_log_path: String = statement.read(13).map_err(|e| format!("Failed to read access_log_path: {}", e))?;

        sites.push(Site {
            id: site_id as usize,
            hostnames,
            is_default: is_default != 0,
            is_enabled: is_enabled != 0,
            web_root,
            web_root_index_file_list,
            enabled_handlers,
            tls_cert_path,
            tls_cert_content,
            tls_key_path,
            tls_key_content,
            rewrite_functions,
            access_log_enabled: access_log_enabled != 0,
            access_log_path,
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
        .prepare("SELECT * FROM request_handlers")
        .map_err(|e| format!("Failed to prepare request handlers query: {}", e))?;

    let mut request_handlers = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute request handlers query: {}", e))? {
        let handler_id: String = statement.read(0).map_err(|e| format!("Failed to read handler id: {}", e))?;
        let is_enabled: i64 = statement.read(1).map_err(|e| format!("Failed to read is_enabled: {}", e))?;
        let name: String = statement.read(2).map_err(|e| format!("Failed to read name: {}", e))?;
        let handler_type: String = statement.read(3).map_err(|e| format!("Failed to read handler type: {}", e))?;
        let request_timeout: i64 = statement.read(4).map_err(|e| format!("Failed to read request timeout: {}", e))?;
        let concurrent_threads: i64 = statement.read(5).map_err(|e| format!("Failed to read concurrent threads: {}", e))?;
        let file_match_str: Option<String> = statement.read(6).ok();
        let executable: String = statement.read(7).map_err(|e| format!("Failed to read executable: {}", e))?;
        let ip_and_port: Option<String> = statement.read(8).ok();
        let other_webroot: Option<String> = statement.read(9).ok();
        let extra_handler_config_str: Option<String> = statement.read(10).ok();
        let extra_environment_str: Option<String> = statement.read(11).ok();

        // Parse comma-separated strings
        let file_match = parse_comma_separated_list(&file_match_str.unwrap_or_default());
        let extra_handler_config = parse_key_value_pairs(&extra_handler_config_str.unwrap_or_default());
        let extra_environment = parse_key_value_pairs(&extra_environment_str.unwrap_or_default());

        request_handlers.push(RequestHandler {
            id: handler_id,
            is_enabled: is_enabled != 0,
            name,
            handler_type,
            request_timeout: request_timeout as usize,
            concurrent_threads: concurrent_threads as usize,
            file_match,
            executable,
            ip_and_port: ip_and_port.unwrap_or_default(),
            other_webroot: other_webroot.unwrap_or_default(),
            extra_handler_config,
            extra_environment,
        });
    }

    Ok(request_handlers)
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
