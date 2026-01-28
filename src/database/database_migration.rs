use sqlite::Connection;

use crate::{
    core::database_connection::get_database_connection,
    database::database_schema::{get_schema_version},
};

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
