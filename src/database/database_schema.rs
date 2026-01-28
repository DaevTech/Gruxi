use sqlite::State;

use crate::core::database_connection::get_database_connection;

pub const CURRENT_DB_SCHEMA_VERSION: i32 = 4;

pub struct DatabaseSchema {
    pub version: i32,
    pub init_sql: Vec<String>,
}

impl DatabaseSchema {
    pub fn new() -> Self {
        let init_sql = get_init_sql();

        Self {
            version: CURRENT_DB_SCHEMA_VERSION,
            init_sql,
        }
    }
}

pub fn initialize_database() -> Result<(), String> {
    let connection = get_database_connection()?;

    // Get database schema and apply it
    let database_schema = DatabaseSchema::new();
    for sql in database_schema.init_sql {
        connection.execute(&sql).map_err(|e| format!("Failed to execute init SQL: {}. Error: {}", sql, e))?;
    }

    Ok(())
}

pub fn get_schema_version() -> i32 {
    let connection_result = get_database_connection();
    let connection = match connection_result {
        Ok(conn) => conn,
        Err(_) => {
            return 0;
        }
    };

    let statement_result = connection.prepare("SELECT gruxi_value FROM gruxi WHERE gruxi_key = 'schema_version' LIMIT 1");
    let mut statement = match statement_result {
        Ok(s) => s,
        Err(_) => {
            return 0;
        }
    };

    let state_result = statement.next();
    let state = match state_result {
        Ok(s) => s,
        Err(_) => {
            return 0;
        }
    };

    match state {
        State::Row => {
            let version: i64 = statement.read(0).unwrap_or(0);
            version as i32
        }
        State::Done => 0, // No version found, assume 0
    }
}

pub fn set_schema_version(version: i32) -> Result<(), String> {
    let connection = get_database_connection()?;
    connection
        .execute(format!("UPDATE gruxi SET gruxi_value = '{}' WHERE gruxi_key = 'schema_version';", version))
        .map_err(|e| format!("Failed to set schema version: {}", e))?;
    Ok(())
}

//
//  SQL Statements for initializing the database schema
//
fn get_init_sql() -> Vec<String> {
    vec![
        // Gruxi key/value table, to store global gruxi settings that is not normally changed and managed outside of the normal configuration
        "CREATE TABLE IF NOT EXISTS gruxi (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        gruxi_key TEXT NOT NULL,
        gruxi_value TEXT NOT NULL
    );"
        .to_string(),
        // Insert the 0 schema version if not present, so that we will load defaults, which is typically at first load
        "INSERT INTO gruxi (gruxi_key, gruxi_value) SELECT 'schema_version', '0' WHERE NOT EXISTS (SELECT 1 FROM gruxi WHERE gruxi_key = 'schema_version');".to_string(),
        // Server settings configuration
        "CREATE TABLE IF NOT EXISTS server_settings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        setting_key TEXT NOT NULL,
        setting_value TEXT NOT NULL
    );"
        .to_string(),
        // Bindings table
        "CREATE TABLE IF NOT EXISTS bindings (
        id TEXT NOT NULL PRIMARY KEY,
        ip TEXT NOT NULL,
        port INTEGER NOT NULL,
        is_admin BOOLEAN NOT NULL DEFAULT 0,
        is_tls BOOLEAN NOT NULL DEFAULT 0
    );"
        .to_string(),
        // Sites table
        "CREATE TABLE IF NOT EXISTS sites (
        id TEXT NOT NULL PRIMARY KEY,
        is_default BOOLEAN NOT NULL DEFAULT 0,
        is_enabled BOOLEAN NOT NULL DEFAULT 1,
        hostnames TEXT NOT NULL DEFAULT '',
        tls_cert_path TEXT NOT NULL DEFAULT '',
        tls_cert_content TEXT NOT NULL DEFAULT '',
        tls_key_path TEXT NOT NULL DEFAULT '',
        tls_key_content TEXT NOT NULL DEFAULT '',
        request_handlers TEXT NOT NULL DEFAULT '',
        rewrite_functions TEXT NOT NULL DEFAULT '',
        access_log_enabled BOOLEAN NOT NULL DEFAULT 0,
        access_log_file TEXT NOT NULL DEFAULT '',
        extra_headers TEXT NOT NULL DEFAULT '',
        tls_automatic_enabled BOOLEAN NOT NULL DEFAULT 0
    );"
        .to_string(),
        // Junction table for many-to-many relationship between bindings and sites
        "CREATE TABLE IF NOT EXISTS binding_sites (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        binding_id INTEGER NOT NULL,
        site_id INTEGER NOT NULL,
        FOREIGN KEY (binding_id) REFERENCES bindings (id) ON DELETE CASCADE,
        FOREIGN KEY (site_id) REFERENCES sites (id) ON DELETE CASCADE,
        UNIQUE(binding_id, site_id)
    );"
        .to_string(),
        // Request handlers
        "CREATE TABLE IF NOT EXISTS request_handler (
        id TEXT PRIMARY KEY,
        is_enabled BOOLEAN NOT NULL DEFAULT 1,
        name TEXT NOT NULL DEFAULT '',
        processor_type TEXT NOT NULL DEFAULT '',
        processor_id TEXT NOT NULL DEFAULT '',
        url_match TEXT NOT NULL DEFAULT ''
    );"
        .to_string(),
        // Processor table
        "CREATE TABLE IF NOT EXISTS static_file_processors (
        id TEXT PRIMARY KEY,
        web_root TEXT NOT NULL DEFAULT '',
        web_root_index_file_list TEXT NOT NULL DEFAULT ''
    );"
        .to_string(),
        // PHP processors table
        "CREATE TABLE IF NOT EXISTS php_processors (
        id TEXT PRIMARY KEY,
        served_by_type TEXT NOT NULL DEFAULT '',
        php_cgi_handler_id TEXT NOT NULL DEFAULT '',
        fastcgi_ip_and_port TEXT NOT NULL DEFAULT '',
        request_timeout INTEGER NOT NULL DEFAULT 30,
        local_web_root TEXT NOT NULL DEFAULT '',
        fastcgi_web_root TEXT NOT NULL DEFAULT '',
        server_software_spoof TEXT NOT NULL DEFAULT ''
    );"
        .to_string(),
        // Proxy processors table
        "CREATE TABLE IF NOT EXISTS proxy_processors (
        id TEXT PRIMARY KEY,
        proxy_type TEXT NOT NULL DEFAULT '',
        upstream_servers TEXT NOT NULL DEFAULT '',
        load_balancing_strategy TEXT NOT NULL DEFAULT '',
        timeout_seconds INTEGER NOT NULL DEFAULT 30,
        health_check_path TEXT NOT NULL DEFAULT '',
        health_check_interval_seconds INTEGER NOT NULL DEFAULT 60,
        health_check_timeout_seconds INTEGER NOT NULL DEFAULT 5,
        url_rewrites TEXT NOT NULL DEFAULT '',
        preserve_host_header BOOLEAN NOT NULL DEFAULT 0,
        forced_host_header TEXT NOT NULL DEFAULT '',
        verify_tls_certificates BOOLEAN NOT NULL DEFAULT 1
    );"
        .to_string(),
        // PHP-CGI handlers table
        "CREATE TABLE IF NOT EXISTS php_cgi_handlers (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL DEFAULT '',
        request_timeout INTEGER NOT NULL DEFAULT 30,
        concurrent_threads INTEGER NOT NULL DEFAULT 0,
        executable TEXT NOT NULL DEFAULT ''
    );"
        .to_string(),
        // Users table for admin portal
        "CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                username TEXT NOT NULL UNIQUE,
                password_hash TEXT NOT NULL,
                created_at TEXT NOT NULL,
                last_login TEXT,
                is_active BOOLEAN NOT NULL DEFAULT 1
            )"
        .to_string(),
        // User session table
        "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                user_id INTEGER NOT NULL,
                username TEXT NOT NULL,
                token TEXT NOT NULL UNIQUE,
                expires_at TEXT NOT NULL,
                created_at TEXT NOT NULL,
                FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
            )"
        .to_string(),
    ]
}
