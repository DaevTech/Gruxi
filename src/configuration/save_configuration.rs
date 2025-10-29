use crate::configuration::binding::Binding;
use crate::configuration::configuration::Configuration;
use crate::configuration::core::Core;
use crate::configuration::load_configuration::load_configuration;
use crate::configuration::request_handler::RequestHandler;
use crate::configuration::site::Site;
use crate::grux_core::database_connection::get_database_connection;
use log::info;
use log::trace;
use serde_json;
use sqlite::Connection;
use sqlite::State;

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

    // Save bindings
    for binding in &mut config.bindings {
        save_binding(&connection, binding)?;
    }
    // Check if any of the bindings need to be deleted, if no longer present in config
    let current_binding_ids: Vec<usize> = current_config.bindings.iter().map(|b| b.id).collect();
    let new_binding_ids: Vec<usize> = config.bindings.iter().map(|b| b.id).collect();
    for binding_id in current_binding_ids {
        if !new_binding_ids.contains(&binding_id) && binding_id != 0 {
            // Delete binding
            connection
                .execute(format!("DELETE FROM bindings WHERE id = {}", binding_id))
                .map_err(|e| format!("Failed to delete binding with id {}: {}", binding_id, e))?;
        }
    }

    // Save sites
    for site in &mut config.sites {
        save_site(&connection, site)?;
    }
    // Check if any of the sites need to be deleted, if no longer present in config
    let current_site_ids: Vec<usize> = current_config.sites.iter().map(|s| s.id).collect();
    let new_site_ids: Vec<usize> = config.sites.iter().map(|s| s.id).collect();
    for site_id in current_site_ids {
        if !new_site_ids.contains(&site_id) && site_id != 0 {
            // Delete site
            connection
                .execute(format!("DELETE FROM sites WHERE id = {}", site_id))
                .map_err(|e| format!("Failed to delete site with id {}: {}", site_id, e))?;
        }
    }

    // Save the binding-site relationships
    // First, clear existing relationships
    connection
        .execute("DELETE FROM binding_sites")
        .map_err(|e| format!("Failed to clear existing binding-site relationships: {}", e))?;

    for relationship in &config.binding_sites {
        connection
            .execute(format!(
                "INSERT OR REPLACE INTO binding_sites (binding_id, site_id) VALUES ({}, {})",
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

    info!("Configuration saved successfully");

    // Note: The configuration will only take effect after a server restart
    // In a production system, you might want to add hot-reloading functionality

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

fn save_binding(connection: &Connection, binding: &mut Binding) -> Result<(), String> {
    // Handle new bindings (id == 0) by setting id to NULL for auto-increment
    let id_value = if binding.id == 0 { "NULL".to_string() } else { binding.id.to_string() };

    // Use INSERT OR REPLACE to handle both new and existing bindings
    connection
        .execute(format!(
            "INSERT OR REPLACE INTO bindings (id, ip, port, is_admin, is_tls) VALUES ({}, '{}', {}, {}, {})",
            id_value,
            binding.ip.replace("'", "''"),
            binding.port,
            if binding.is_admin { 1 } else { 0 },
            if binding.is_tls { 1 } else { 0 }
        ))
        .map_err(|e| format!("Failed to insert/replace binding: {}", e))?;

    // If this was a new binding (id == 0), get the auto-generated ID
    if binding.id == 0 {
        let mut last_inserted_id_statement = connection
            .prepare("SELECT last_insert_rowid()")
            .map_err(|e| format!("Failed to prepare last_insert_rowid query: {}", e))?;

        match last_inserted_id_statement.next().map_err(|e| format!("Failed to execute last_insert_rowid query: {}", e))? {
            State::Row => binding.id = last_inserted_id_statement.read::<i64, _>(0).map_err(|e| format!("Failed to read last inserted id: {}", e))? as usize,
            State::Done => binding.id = 0, // No ID found, assume 0
        }
        trace!("Inserted new binding with id: {:?}", binding.id);
    } else {
        trace!("Updated existing binding with id: {:?}", binding.id);
    }

    Ok(())
}

pub fn save_site(connection: &Connection, site: &mut Site) -> Result<(), String> {
    // Insert or update site
    if site.id == 0 {
        // New site, insert it
        connection
                .execute(format!(
                    "INSERT INTO sites (is_default, is_enabled, hostnames, web_root, web_root_index_file_list, enabled_handlers, tls_cert_path, tls_cert_content, tls_key_path, tls_key_content, rewrite_functions, access_log_enabled, access_log_path) VALUES ({}, {}, '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', {}, '{}')",
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
                    site.access_log_path.replace("'", "''")
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
                    "UPDATE sites SET is_default = {}, is_enabled = {}, hostnames = '{}', web_root = '{}', web_root_index_file_list = '{}', enabled_handlers = '{}', tls_cert_path = '{}', tls_cert_content = '{}', tls_key_path = '{}', tls_key_content = '{}', rewrite_functions = '{}', access_log_enabled = {}, access_log_path = '{}' WHERE id = {}",
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
                    site.access_log_path.replace("'", "''"),
                    site.id
                ))
                .map_err(|e| format!("Failed to update site: {}", e))?;
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
