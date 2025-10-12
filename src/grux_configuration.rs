use crate::{grux_configuration_struct::*, grux_core::database_connection::get_database_connection};
use log::trace;
use log::{info, warn};
use serde_json;
use sqlite::Connection;
use sqlite::State;
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
            let operation_mode = crate::grux_core::operation_mode::get_operation_mode();
            info!("No configuration found, creating default configuration");

            // Load default configuration based on operation mode
            let mut default_configuration = {
                if matches!(operation_mode, crate::grux_core::operation_mode::OperationMode::DEV) {
                    info!("Loading dev configuration");
                    Configuration::get_testing()
                } else {
                    Configuration::get_default()
                }
            };

            save_configuration(&mut default_configuration)?;

            // Update schema version to 1
            connection
                .execute(format!("UPDATE schema_version SET version = 1",))
                .map_err(|e| format!("Failed to update schema version: {}", e))?;

            default_configuration
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

/// Save a new configuration to the database
/// Returns Ok(true) if changes were saved, Ok(false) if no changes were needed
pub fn save_configuration(config: &mut Configuration) -> Result<bool, String> {
    // First validate the configuration
    config.validate().map_err(|errors| format!("Configuration validation failed: {}", errors.join("; ")))?;

    // Check if the configuration is different from what's currently in the database
    let current_config = load_configuration()?;

    // Serialize both configurations to JSON for comparison
    let new_config_json = serde_json::to_string(config).map_err(|e| format!("Failed to serialize new configuration: {}", e))?;
    let current_config_json = serde_json::to_string(&current_config).map_err(|e| format!("Failed to serialize current configuration: {}", e))?;

    // If configurations are identical, no need to save
    if new_config_json == current_config_json {
        return Ok(false); // No changes were made
    }

    // Do the actual saving
    let connection = get_database_connection()?;

    // Begin transaction for atomicity
    connection.execute("BEGIN TRANSACTION").map_err(|e| format!("Failed to begin transaction: {}", e))?;

    // Save core configuration (file cache, gzip, server settings)
    save_core_config(&connection, &config.core)?;

    // Save servers
    for server in &mut config.servers {
        for binding in &mut server.bindings {
            save_binding(&connection, binding)?;
        }
    }

    // Save request handlers
    for handler in &config.request_handlers {
        save_request_handler(&connection, handler)?;
    }

    // Commit transaction
    connection.execute("COMMIT").map_err(|e| format!("Failed to commit transaction: {}", e))?;

    info!("Configuration saved successfully");

    // Note: The configuration will only take effect after a server restart
    // In a production system, you might want to add hot-reloading functionality

    Ok(true) // Changes were saved
}

/// Load the configuration from the normalized database tables
pub fn load_configuration() -> Result<Configuration, String> {
    let connection = sqlite::open("./grux.db").map_err(|e| format!("Failed to open database connection: {}", e))?;

    // Load all configuration components
    let core = load_core_config(&connection)?;
    let servers = load_servers(&connection)?;
    let request_handlers = load_request_handlers(&connection)?;

    Ok(Configuration { servers, core, request_handlers })
}

fn save_core_config(connection: &Connection, core: &Core) -> Result<(), String> {
    // Save file cache (single record)
    connection
        .execute(format!(
            "INSERT OR REPLACE INTO file_cache (is_enabled, cache_item_size, cache_max_size_per_file, cache_item_time_between_checks, cleanup_thread_interval, max_item_lifetime, forced_eviction_threshold) VALUES ({}, {}, {}, {}, {}, {}, {})",
            if core.file_cache.is_enabled { 1 } else { 0 },
            core.file_cache.cache_item_size,
            core.file_cache.cache_max_size_per_file,
            core.file_cache.cache_item_time_between_checks,
            core.file_cache.cleanup_thread_interval,
            core.file_cache.max_item_lifetime,
            core.file_cache.forced_eviction_threshold
        ))
        .map_err(|e| format!("Failed to insert file cache config: {}", e))?;

    // Save gzip config with comma-separated content types
    let content_types = core.gzip.compressible_content_types.join(",");
    connection
        .execute(format!(
            "INSERT OR REPLACE INTO gzip (is_enabled, compressible_content_types) VALUES ({}, '{}')",
            if core.gzip.is_enabled { 1 } else { 0 },
            content_types.replace("'", "''") // Escape single quotes
        ))
        .map_err(|e| format!("Failed to insert gzip config: {}", e))?;

    // Save server settings
    save_server_settings(connection, "max_body_size", &core.server_settings.max_body_size.to_string())?;

    Ok(())
}

fn save_server_settings(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    connection
        .execute(format!(
            "INSERT OR REPLACE INTO server_settings (setting_key, setting_value) VALUES ('{}', '{}')",
            key.replace("'", "''"),
            value.replace("'", "''")
        ))
        .map_err(|e| format!("Failed to insert/update server setting {}: {}", key, e))?;
    Ok(())
}

fn save_binding(connection: &Connection, binding: &mut Binding) -> Result<(), String> {
    // Insert binding with site data
    if binding.id == 0 {
        // New binding, insert it
        connection
            .execute(format!(
                "INSERT INTO bindings (ip, port, is_admin, is_tls) VALUES ('{}', {}, {}, {})",
                binding.ip.replace("'", "''"),
                binding.port,
                if binding.is_admin { 1 } else { 0 },
                if binding.is_tls { 1 } else { 0 }
            ))
            .map_err(|e| format!("Failed to insert binding: {}", e))?;
        trace!("Inserted new binding: {:?}", binding);
        let mut last_inserted_id_statement = connection
            .prepare("SELECT last_insert_rowid()")
            .map_err(|e| format!("Failed to prepare last_insert_rowid query: {}", e))?;

        match last_inserted_id_statement.next().map_err(|e| format!("Failed to execute last_insert_rowid query: {}", e))? {
            State::Row => binding.id = last_inserted_id_statement.read::<i64, _>(0).map_err(|e| format!("Failed to read last inserted id: {}", e))? as usize,
            State::Done => binding.id = 0, // No version found, assume 0
        }
        trace!("Inserted new binding with id: {:?}", binding.id)
    } else {
        // Existing binding, update it
        connection
            .execute(format!(
                "UPDATE bindings SET ip = '{}', port = {}, is_admin = {}, is_tls = {} WHERE id = {}",
                binding.ip.replace("'", "''"),
                binding.port,
                if binding.is_admin { 1 } else { 0 },
                if binding.is_tls { 1 } else { 0 },
                binding.id
            ))
            .map_err(|e| format!("Failed to update binding: {}", e))?;
    }

    // After saving the bindings, we save the sites
    for site in &mut binding.sites {
        if site.id == 0 {
            // New site, insert it
            connection
                .execute(format!(
                    "INSERT INTO sites (binding_id, is_default, is_enabled, hostnames, web_root, web_root_index_file_list, enabled_handlers, tls_cert_path, tls_cert_content, tls_key_path, tls_key_content, rewrite_functions) VALUES ({}, {}, {}, '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}')",
                    binding.id,
                    if site.is_default { 1 } else { 0 },
                    if site.is_enabled { 1 } else { 0 },
                    site.hostnames.join(",").replace("'", "''"),
                    site.web_root.replace("'", "''"),
                    site.web_root_index_file_list.join(",").replace("'", "''"),
                    site.enabled_handlers.join(",").replace("'", "''"),
                    site.tls_cert_path.replace("'", "''"),
                    site.tls_cert_content.replace("'", "''"),
                    site.tls_key_path.replace("'", "''"),
                    site.tls_key_content.replace("'", "''"),
                    site.rewrite_functions.join(",").replace("'", "''")
                ))
                .map_err(|e| format!("Failed to insert site: {}", e))?;
            trace!("Inserted new site: {:?}", site);
            let mut last_inserted_id_statement = connection
                .prepare("SELECT last_insert_rowid()")
                .map_err(|e| format!("Failed to prepare last_insert_rowid query: {}", e))?;

            match last_inserted_id_statement.next().map_err(|e| format!("Failed to execute last_insert_rowid query: {}", e))? {
                State::Row => site.id = last_inserted_id_statement.read::<i64, _>(0).map_err(|e| format!("Failed to read last inserted id: {}", e))? as usize,
                State::Done => site.id = 0, // No version found, assume 0
            }
            trace!("Inserted new site with id: {:?}", site.id);
        } else {
            // Existing site, update it
            connection
                .execute(format!(
                    "UPDATE sites SET is_default = {}, is_enabled = {}, hostnames = '{}', web_root = '{}', web_root_index_file_list = '{}', enabled_handlers = '{}', tls_cert_path = '{}', tls_cert_content = '{}', tls_key_path = '{}', tls_key_content = '{}', rewrite_functions = '{}' WHERE id = {}",
                    if site.is_default { 1 } else { 0 },
                    if site.is_enabled { 1 } else { 0 },
                    site.hostnames.join(",").replace("'", "''"),
                    site.web_root.replace("'", "''"),
                    site.web_root_index_file_list.join(",").replace("'", "''"),
                    site.enabled_handlers.join(",").replace("'", "''"),
                    site.tls_cert_path.replace("'", "''"),
                    site.tls_cert_content.replace("'", "''"),
                    site.tls_key_path.replace("'", "''"),
                    site.tls_key_content.replace("'", "''"),
                    site.rewrite_functions.join(",").replace("'", "''"),
                    site.id
                ))
                .map_err(|e| format!("Failed to update site: {}", e))?;
        }
    }

    Ok(())
}

fn save_request_handler(connection: &Connection, handler: &RequestHandler) -> Result<(), String> {
    // Prepare comma-separated strings
    let file_match_str = handler.file_match.join(",");
    let extra_config_str = handler.extra_handler_config.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<String>>().join(",");
    let extra_env_str = handler.extra_environment.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<String>>().join(",");

    let ip_and_port = if handler.ip_and_port.is_empty() {
        "NULL".to_string()
    } else {
        format!("'{}'", handler.ip_and_port.replace("'", "''"))
    };
    let other_webroot = if handler.other_webroot.is_empty() {
        "NULL".to_string()
    } else {
        format!("'{}'", handler.other_webroot.replace("'", "''"))
    };

    // Insert request handler with comma-separated fields
    connection
        .execute(format!(
            "INSERT OR REPLACE INTO request_handlers (id, is_enabled, name, handler_type, request_timeout, concurrent_threads, file_match, executable, ip_and_port, other_webroot, extra_handler_config, extra_environment) VALUES ({}, {}, '{}', '{}', {}, {}, '{}', '{}', {}, {}, '{}', '{}')",
            handler.id,
            if handler.is_enabled { 1 } else { 0 },
            handler.name.replace("'", "''"),
            handler.handler_type.replace("'", "''"),
            handler.request_timeout,
            handler.concurrent_threads,
            file_match_str.replace("'", "''"),
            handler.executable.replace("'", "''"),
            ip_and_port,
            other_webroot,
            extra_config_str.replace("'", "''"),
            extra_env_str.replace("'", "''")
        ))
        .map_err(|e| format!("Failed to insert request handler: {}", e))?;

    Ok(())
}

fn load_core_config(connection: &Connection) -> Result<Core, String> {
    // Load file cache (single record with id=1)
    let mut statement = connection.prepare("SELECT * FROM file_cache").map_err(|e| format!("Failed to prepare file cache query: {}", e))?;

    let file_cache = match statement.next().map_err(|e| format!("Failed to execute file cache query: {}", e))? {
        sqlite::State::Row => FileCache {
            is_enabled: statement.read::<i64, _>(0).map_err(|e| format!("Failed to read file cache enabled: {}", e))? != 0,
            cache_item_size: statement.read::<i64, _>(1).map_err(|e| format!("Failed to read cache item size: {}", e))? as usize,
            cache_max_size_per_file: statement.read::<i64, _>(2).map_err(|e| format!("Failed to read max size per file type: {}", e))? as usize,
            cache_item_time_between_checks: statement.read::<i64, _>(3).map_err(|e| format!("Failed to read time between checks: {}", e))? as usize,
            cleanup_thread_interval: statement.read::<i64, _>(4).map_err(|e| format!("Failed to read cleanup interval: {}", e))? as usize,
            max_item_lifetime: statement.read::<i64, _>(5).map_err(|e| format!("Failed to read max item lifetime: {}", e))? as usize,
            forced_eviction_threshold: statement.read::<i64, _>(6).map_err(|e| format!("Failed to read eviction threshold: {}", e))? as usize,
        },
        sqlite::State::Done => {
            warn!("No file cache configuration found, using default");
            FileCache {
                is_enabled: false,
                cache_item_size: 1000,
                cache_max_size_per_file: 1024 * 1024, // 1 MB default
                cache_item_time_between_checks: 20,
                cleanup_thread_interval: 10,
                max_item_lifetime: 60,
                forced_eviction_threshold: 70,
            }
        }
    };

    drop(statement);

    // Load gzip configuration (single record with comma-separated content types)
    let mut statement = connection.prepare("SELECT * FROM gzip").map_err(|e| format!("Failed to prepare gzip query: {}", e))?;

    let (gzip_enabled, compressible_content_types) = match statement.next().map_err(|e| format!("Failed to execute gzip query: {}", e))? {
        sqlite::State::Row => {
            let enabled: i64 = statement.read(0).map_err(|e| format!("Failed to read gzip enabled: {}", e))?;
            let content_types_str: String = statement.read(1).map_err(|e| format!("Failed to read content types: {}", e))?;
            let content_types = content_types_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            (enabled != 0, content_types)
        }
        sqlite::State::Done => {
            warn!("No gzip configuration found, using default");
            (true, vec!["text/".to_string(), "application/json".to_string(), "application/javascript".to_string()])
        }
    };

    drop(statement);

    // Load server settings, with key/value pairs
    let mut statement = connection
        .prepare("SELECT * FROM server_settings")
        .map_err(|e| format!("Failed to prepare server settings query: {}", e))?;

    // Each row is a key/value pair, where key should be checked against known settings in the server settings struct
    let mut server_settings = ServerSettings { max_body_size: 0 };

    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute server settings query: {}", e))? {
        let key: String = statement.read(0).map_err(|e| format!("Failed to read key: {}", e))?;
        let value: i64 = statement.read(1).map_err(|e| format!("Failed to read value: {}", e))?;

        match key.as_str() {
            "max_body_size" => server_settings.max_body_size = value as usize,
            _ => continue,
        }
    }

    drop(statement);

    Ok(Core {
        file_cache,
        gzip: Gzip {
            is_enabled: gzip_enabled,
            compressible_content_types,
        },
        server_settings,
    })
}

fn load_servers(connection: &Connection) -> Result<Vec<Server>, String> {
    // Load all bindings and group them into servers
    // For simplicity, we'll create one server with all bindings, but this could be enhanced
    // to group admin vs non-admin bindings into separate servers
    let bindings = load_bindings(connection)?;

    if bindings.is_empty() {
        warn!("No bindings found, returning empty server list");
        return Ok(vec![]);
    }

    // Group bindings - for now, put all in one server
    // In the future, you might want to separate admin and non-admin bindings
    let servers = vec![Server { bindings }];

    Ok(servers)
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

        // Parse comma-separated site data
        let sites = load_sites(connection, binding_id)?;

        bindings.push(Binding {
            id: binding_id as usize,
            ip,
            port: port as u16,
            is_admin: is_admin != 0,
            is_tls: is_tls != 0,
            sites,
        });
    }

    Ok(bindings)
}

fn load_sites(connection: &Connection, binding_id: i64) -> Result<Vec<Site>, String> {
    let mut statement = connection
        .prepare("SELECT * FROM sites WHERE binding_id = ?")
        .map_err(|e| format!("Failed to prepare sites query: {}", e))?;

    statement.bind((1, binding_id)).map_err(|e| format!("Failed to bind binding_id: {}", e))?;

    let mut sites = Vec::new();
    while let sqlite::State::Row = statement.next().map_err(|e| format!("Failed to execute sites query: {}", e))? {
        let site_id: i64 = statement.read(0).map_err(|e| format!("Failed to read site id: {}", e))?;
        let is_default: i64 = statement.read(2).map_err(|e| format!("Failed to read is_default: {}", e))?;
        let is_enabled: i64 = statement.read(3).map_err(|e| format!("Failed to read is_enabled: {}", e))?;

        // Hostnames is comma separated
        let hostnames_str: String = statement.read(4).map_err(|e| format!("Failed to read hostnames: {}", e))?;
        let hostnames = parse_comma_separated_list(&hostnames_str);

        let web_root: String = statement.read(5).map_err(|e| format!("Failed to read web_root: {}", e))?;

        // Index files is comma separated
        let web_root_index_file_list_str: String = statement.read(6).map_err(|e| format!("Failed to read web_root_index_file_list: {}", e))?;
        let web_root_index_file_list = parse_comma_separated_list(&web_root_index_file_list_str);

        let enabled_handlers_str: String = statement.read(7).map_err(|e| format!("Failed to read enabled_handlers: {}", e))?;
        let enabled_handlers = parse_comma_separated_list(&enabled_handlers_str);

        let tls_cert_path: String = statement.read(8).ok().unwrap_or_default();
        let tls_cert_content: String = statement.read(9).ok().unwrap_or_default();
        let tls_key_path: String = statement.read(10).ok().unwrap_or_default();
        let tls_key_content: String = statement.read(11).ok().unwrap_or_default();

        // Rewrite functions is comma separated
        let rewrite_functions_str: String = statement.read(12).map_err(|e| format!("Failed to read rewrite_functions: {}", e))?;
        let rewrite_functions: Vec<String> = parse_comma_separated_list(&rewrite_functions_str);

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
        });
    }

    Ok(sites)
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
