use sqlite::Connection;

use crate::{core::database_connection::get_database_connection, database::database_schema::get_schema_version};

pub fn migrate_database() -> i32 {
    // Get our current schema version from db
    let mut schema_version = get_schema_version();
    if schema_version < 1 {
        return 0;
    }

    let connection_result = get_database_connection();
    let connection = match connection_result {
        Ok(conn) => conn,
        Err(e) => {
            panic!("Failed to get database connection for migration: {}", e);
        }
    };

    // Migration from 2 to 3
    if schema_version == 2 {
        let result = migrate_db_helper(&connection, 2, 3, migrate_db_2_to_3);
        if let Err(e) = result {
            panic!("Database migration from version 2 to 3 failed: {}", e);
        }
        schema_version = 3;
    }
    // Migration from 3 to 4
    if schema_version == 3 {
        let result = migrate_db_helper(&connection, 3, 4, migrate_db_3_to_4);
        if let Err(e) = result {
            panic!("Database migration from version 3 to 4 failed: {}", e);
        }
        schema_version = 4;
    }
    if schema_version == 4 {
        let result = migrate_db_helper(&connection, 4, 5, migrate_db_4_to_5);
        if let Err(e) = result {
            panic!("Database migration from version 4 to 5 failed: {}", e);
        }
        schema_version = 5;
    }
    if schema_version == 5 {
        let result = migrate_db_helper(&connection, 5, 6, migrate_db_5_to_6);
        if let Err(e) = result {
            panic!("Database migration from version 5 to 6 failed: {}", e);
        }
        schema_version = 6;
    }
    if schema_version == 6 {
        let result = migrate_db_helper(&connection, 6, 7, migrate_db_6_to_7);
        if let Err(e) = result {
            panic!("Database migration from version 6 to 7 failed: {}", e);
        }
        schema_version = 7;
    }
    if schema_version == 7 {
        let result = migrate_db_helper(&connection, 7, 8, migrate_db_7_to_8);
        if let Err(e) = result {
            panic!("Database migration from version 7 to 8 failed: {}", e);
        }
        schema_version = 8;
    }
    if schema_version == 8 {
        let result = migrate_db_helper(&connection, 8, 9, migrate_db_8_to_9);
        if let Err(e) = result {
            panic!("Database migration from version 8 to 9 failed: {}", e);
        }
        schema_version = 9;
    }


    schema_version
}

fn migrate_db_helper(connection: &Connection, from_version: i32, to_version: i32, migration_fn: fn(&Connection) -> Result<(), sqlite::Error>) -> Result<(), String> {
    if let Err(e) = connection.execute("BEGIN IMMEDIATE TRANSACTION;") {
        return Err(format!("Failed to begin transaction for database migration from version {} to {}: {}", from_version, to_version, e));
    }

    let migration_result: Result<(), sqlite::Error> = (|| {
        migration_fn(connection)?;

        // Update schema version
        connection.execute(format!("UPDATE gruxi SET gruxi_value = '{}' WHERE gruxi_key = 'schema_version';", to_version))?;

        Ok(())
    })();

    match migration_result {
        Ok(()) => {
            if let Err(e) = connection.execute("COMMIT;") {
                let _ = connection.execute("ROLLBACK;");
                return Err(format!("Failed to commit transaction for database migration from version {} to {}: {}", from_version, to_version, e));
            }
        }
        Err(e) => {
            let _ = connection.execute("ROLLBACK;");
            return Err(format!("Failed to migrate database from version {} to {}: {}", from_version, to_version, e));
        }
    };

    Ok(())
}

fn migrate_db_2_to_3(connection: &Connection) -> Result<(), sqlite::Error> {
    // Add "server_software_spoof" to "php_processors" table
    connection.execute("ALTER TABLE php_processors ADD COLUMN server_software_spoof TEXT NOT NULL DEFAULT '';")?;
    Ok(())
}

fn migrate_db_3_to_4(connection: &Connection) -> Result<(), sqlite::Error> {
    // Add "tls_automatic_enabled" to "sites" table
    connection.execute("ALTER TABLE sites ADD COLUMN tls_automatic_enabled BOOLEAN NOT NULL DEFAULT 0;")?;
    Ok(())
}

fn migrate_db_4_to_5(connection: &Connection) -> Result<(), sqlite::Error> {
    // Remove "tls_certificate_cache_path" from "server_settings" table
    connection.execute("DELETE from server_settings WHERE setting_key = 'tls_certificate_cache_path';")?;
    // Remove "file_cache_cache_item_time_between_checks" from "server_settings" table
    connection.execute("DELETE from server_settings WHERE setting_key = 'file_cache_cache_item_time_between_checks';")?;
    // Update "file_cache_cleanup_thread_interval" to be "file_cache_update_thread_interval" in "server_settings" table
    connection.execute("UPDATE server_settings SET setting_key = 'file_cache_update_thread_interval' WHERE setting_key = 'file_cache_cleanup_thread_interval';")?;

    Ok(())
}

fn migrate_db_5_to_6(connection: &Connection) -> Result<(), sqlite::Error> {
    // On sites, for the fields "tls_cert_path" and "tls_key_path", remove any "./certs" part or "certs/" part from the beginning of the path, since we now consider these paths to be relative to the certificates directory if they are not absolute paths. This is to simplify the paths and avoid confusion.
    connection.execute("UPDATE sites SET tls_cert_path = TRIM(REPLACE(tls_cert_path, './certs', ''), '/') WHERE tls_cert_path LIKE './certs/%';")?;
    connection.execute("UPDATE sites SET tls_cert_path = TRIM(REPLACE(tls_cert_path, 'certs', ''), '/') WHERE tls_cert_path LIKE 'certs/%';")?;
    connection.execute("UPDATE sites SET tls_key_path = TRIM(REPLACE(tls_key_path, './certs', ''), '/') WHERE tls_key_path LIKE './certs/%';")?;
    connection.execute("UPDATE sites SET tls_key_path = TRIM(REPLACE(tls_key_path, 'certs', ''), '/') WHERE tls_key_path LIKE 'certs/%';")?;
    // Do the same for the "admin_portal_tls_certificate_path" and "admin_portal_tls_key_path" settings in the "server_settings" table
    connection.execute("UPDATE server_settings SET setting_value = TRIM(REPLACE(setting_value, './certs', ''), '/') WHERE setting_key IN ('admin_portal_tls_certificate_path', 'admin_portal_tls_key_path') AND setting_value LIKE './certs/%';")?;
    connection.execute("UPDATE server_settings SET setting_value = TRIM(REPLACE(setting_value, 'certs', ''), '/') WHERE setting_key IN ('admin_portal_tls_certificate_path', 'admin_portal_tls_key_path') AND setting_value LIKE 'certs/%';")?;

    Ok(())
}

fn migrate_db_6_to_7(connection: &Connection) -> Result<(), sqlite::Error> {
    // Add "force_tls", "force_tls_port", and "canonical_host" to "sites" table
    connection.execute("ALTER TABLE sites ADD COLUMN force_tls BOOLEAN NOT NULL DEFAULT 0;")?;
    connection.execute("ALTER TABLE sites ADD COLUMN force_tls_port INTEGER NOT NULL DEFAULT 443;")?;
    connection.execute("ALTER TABLE sites ADD COLUMN canonical_host TEXT NOT NULL DEFAULT '';")?;
    Ok(())
}

fn migrate_db_7_to_8(connection: &Connection) -> Result<(), sqlite::Error> {
    // Add "is_telemetry" to "bindings" table
    connection.execute("ALTER TABLE bindings ADD COLUMN is_telemetry BOOLEAN NOT NULL DEFAULT 0;")?;
    Ok(())
}

fn migrate_db_8_to_9(connection: &Connection) -> Result<(), sqlite::Error> {
    // Remove two fields used in file cache config that are no longer used.
    connection.execute("DELETE from server_settings WHERE setting_key = 'file_cache_forced_eviction_threshold';")?;
    connection.execute("DELETE from server_settings WHERE setting_key = 'file_cache_update_thread_interval';")?;

    Ok(())
}
