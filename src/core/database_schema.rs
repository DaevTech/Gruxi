use crate::core::database_connection::get_database_connection;

pub const CURRENT_DB_SCHEMA_VERSION: i32 = 1;

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

//
//  SQL Statements for initializing the database schema
//
fn get_init_sql() -> Vec<String> {
    vec![
        // Grux key/value table, to store global grux settings that is not normally changed and managed outside of the normal configuration
        "CREATE TABLE IF NOT EXISTS grux (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        grux_key TEXT NOT NULL,
        grux_value TEXT NOT NULL
    );"
        .to_string(),
        // Insert the 0 schema version if not present, so that we will load defaults, which is typically at first load
        "INSERT INTO grux (grux_key, grux_value) SELECT 'schema_version', '0' WHERE NOT EXISTS (SELECT 1 FROM grux WHERE grux_key = 'schema_version');".to_string(),
        // Server settings configuration
        "CREATE TABLE IF NOT EXISTS server_settings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        setting_key TEXT NOT NULL,
        setting_value TEXT NOT NULL
    );"
        .to_string(),
        // Bindings table
        "CREATE TABLE IF NOT EXISTS bindings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ip TEXT NOT NULL,
        port INTEGER NOT NULL,
        is_admin BOOLEAN NOT NULL DEFAULT 0,
        is_tls BOOLEAN NOT NULL DEFAULT 0
    );"
        .to_string(),
        // Sites table
        "CREATE TABLE IF NOT EXISTS sites (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        is_default BOOLEAN NOT NULL DEFAULT 0,
        is_enabled BOOLEAN NOT NULL DEFAULT 1,
        hostnames TEXT NOT NULL DEFAULT '',
        web_root TEXT NOT NULL,
        web_root_index_file_list TEXT NOT NULL DEFAULT '',
        enabled_handlers TEXT NOT NULL DEFAULT '',
        tls_cert_path TEXT NOT NULL DEFAULT '',
        tls_cert_content TEXT NOT NULL DEFAULT '',
        tls_key_path TEXT NOT NULL DEFAULT '',
        tls_key_content TEXT NOT NULL DEFAULT '',
        rewrite_functions TEXT NOT NULL DEFAULT '',
        access_log_enabled BOOLEAN NOT NULL DEFAULT 0,
        access_log_file TEXT NOT NULL DEFAULT '',
        extra_headers TEXT NOT NULL DEFAULT ''
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
        "CREATE TABLE IF NOT EXISTS request_handlers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        is_enabled BOOLEAN NOT NULL DEFAULT 1,
        name TEXT NOT NULL DEFAULT '',
        handler_type TEXT NOT NULL DEFAULT '',
        request_timeout INTEGER NOT NULL DEFAULT 30,
        concurrent_threads INTEGER NOT NULL DEFAULT 0,
        file_match TEXT NOT NULL DEFAULT '',
        executable TEXT NOT NULL DEFAULT '',
        ip_and_port TEXT NOT NULL DEFAULT '',
        other_webroot TEXT NOT NULL DEFAULT '',
        extra_handler_config TEXT NOT NULL DEFAULT '',
        extra_environment TEXT NOT NULL DEFAULT ''
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
