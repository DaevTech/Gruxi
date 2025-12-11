use crate::configuration::binding::Binding;
use crate::configuration::configuration::Configuration;
use crate::configuration::core::Core;
use crate::configuration::load_configuration::fetch_configuration_in_db;
use crate::configuration::request_handler::RequestHandler;
use crate::configuration::site::HeaderKV;
use crate::configuration::site::Site;
use crate::core::database_connection::get_database_connection;
use crate::logging::syslog::{info, trace};
use serde_json;
use sqlite::Connection;
use sqlite::State;

/// Save a new configuration to the database
/// Returns Ok(true) if changes were saved, Ok(false) if no changes were needed
pub fn save_configuration(config: &mut Configuration) -> Result<bool, String> {
    // First, we sanitize the configuration
    config.sanitize();

    // Then we validate the configuration
    config.validate().map_err(|errors| format!("Configuration validation failed: {}", errors.join("; ")))?;

    // Check if the configuration is different from what's currently in the database
    let current_config = fetch_configuration_in_db()?;

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

    // Clear and re-insert all bindings (simpler than update/delete logic)
    connection.execute("DELETE FROM bindings").map_err(|e| format!("Failed to clear existing bindings: {}", e))?;

    for binding in &config.bindings {
        save_binding(&connection, binding)?;
    }

    // Clear and re-insert all sites (simpler than update/delete logic)
    connection.execute("DELETE FROM sites").map_err(|e| format!("Failed to clear existing sites: {}", e))?;

    for site in &config.sites {
        save_site(&connection, site)?;
    }

    // Save the binding-site relationships
    // First, clear existing relationships
    connection
        .execute("DELETE FROM binding_sites")
        .map_err(|e| format!("Failed to clear existing binding-site relationships: {}", e))?;

    for relationship in &config.binding_sites {
        connection
            .execute(format!(
                "INSERT INTO binding_sites (binding_id, site_id) VALUES ({}, {})",
                relationship.binding_id, relationship.site_id
            ))
            .map_err(|e| format!("Failed to insert binding-site relationship: {}", e))?;
    }

    // Save request handlers
    for handler in &config.request_handlers {
        save_request_handler(&connection, handler)?;
    }

    // Commit transaction
    connection.execute("COMMIT").map_err(|e| format!("Failed to commit transaction: {}", e))?;

    info("Configuration saved successfully");

    Ok(true) // Changes were saved
}

fn save_core_config(connection: &Connection, core: &Core) -> Result<(), String> {
    // Save file cache settings
    save_server_settings(connection, "file_cache_is_enabled", &core.file_cache.is_enabled.to_string())?;
    save_server_settings(connection, "file_cache_cache_item_size", &core.file_cache.cache_item_size.to_string())?;
    save_server_settings(connection, "file_cache_cache_max_size_per_file", &core.file_cache.cache_max_size_per_file.to_string())?;
    save_server_settings(connection, "file_cache_cache_item_time_between_checks", &core.file_cache.cache_item_time_between_checks.to_string())?;
    save_server_settings(connection, "file_cache_cleanup_thread_interval", &core.file_cache.cleanup_thread_interval.to_string())?;
    save_server_settings(connection, "file_cache_max_item_lifetime", &core.file_cache.max_item_lifetime.to_string())?;
    save_server_settings(connection, "file_cache_forced_eviction_threshold", &core.file_cache.forced_eviction_threshold.to_string())?;

    // Save gzip settings
    save_server_settings(connection, "gzip_is_enabled", &core.gzip.is_enabled.to_string())?;
    save_server_settings(connection, "gzip_compressible_content_types", &core.gzip.compressible_content_types.join(","))?;

    // Save server settings
    save_server_settings(connection, "max_body_size", &core.server_settings.max_body_size.to_string())?;
    save_server_settings(connection, "blocked_file_patterns", &core.server_settings.blocked_file_patterns.join(","))?;
    save_server_settings(connection, "operation_mode", &core.server_settings.operation_mode)?;

    Ok(())
}

fn save_server_settings(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    // check if it is insert or update
    let mut statement = connection
        .prepare(format!("SELECT COUNT(*) FROM server_settings WHERE setting_key = '{}'", key.replace("'", "''")))
        .map_err(|e| format!("Failed to prepare server settings query: {}", e))?;
    let exists = match statement.next().map_err(|e| format!("Failed to execute server settings query: {}", e))? {
        State::Row => {
            let count: i64 = statement.read(0).map_err(|e| format!("Failed to read count: {}", e))?;
            count > 0
        }
        State::Done => false,
    };
    drop(statement);

    if exists {
        connection
            .execute(format!(
                "UPDATE server_settings SET setting_value = '{}' WHERE setting_key = '{}'",
                value.replace("'", "''"),
                key.replace("'", "''")
            ))
            .map_err(|e| format!("Failed to update server setting {}: {}", key, e))?;
    } else {
        connection
            .execute(format!(
                "INSERT INTO server_settings (setting_key, setting_value) VALUES ('{}', '{}')",
                key.replace("'", "''"),
                value.replace("'", "''")
            ))
            .map_err(|e| format!("Failed to insert/update server setting {}: {}", key, e))?;
    }

    Ok(())
}

fn save_binding(connection: &Connection, binding: &Binding) -> Result<(), String> {
    // Insert binding with explicit ID (all bindings are re-inserted after DELETE FROM bindings)
    connection
        .execute(format!(
            "INSERT INTO bindings (id, ip, port, is_admin, is_tls) VALUES ({}, '{}', {}, {}, {})",
            binding.id,
            binding.ip.replace("'", "''"),
            binding.port,
            if binding.is_admin { 1 } else { 0 },
            if binding.is_tls { 1 } else { 0 }
        ))
        .map_err(|e| format!("Failed to insert binding: {}", e))?;

    trace(format!("Inserted binding with id: {}", binding.id));

    Ok(())
}

pub fn save_site(connection: &Connection, site: &Site) -> Result<(), String> {
    // Remove any site with the same ID first (to avoid conflicts)
    connection
        .execute(format!("DELETE FROM sites WHERE id = {}", site.id))
        .map_err(|e| format!("Failed to delete existing site with id {}: {}", site.id, e))?;

    let extra_headers_str = if site.extra_headers.is_empty() {
        "".to_string()
    } else {
        site.extra_headers
            .iter()
            .map(|HeaderKV { key, value }| format!("{}={}", key.replace("'", "''"), value.replace("'", "''")))
            .collect::<Vec<String>>()
            .join(",")
    };

    connection
        .execute(format!(
            "INSERT INTO sites (id, is_default, is_enabled, hostnames, web_root, web_root_index_file_list, enabled_handlers, tls_cert_path, tls_cert_content, tls_key_path, tls_key_content, rewrite_functions, access_log_enabled, access_log_file, extra_headers) VALUES ({}, {}, {}, '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', {}, '{}', '{}')",
            site.id,
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
            if site.access_log_enabled { 1 } else { 0 },
            site.access_log_file.replace("'", "''"),
            extra_headers_str
        ))
        .map_err(|e| format!("Failed to insert site: {}", e))?;

    trace(format!("Inserted site with id: {}", site.id));

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
