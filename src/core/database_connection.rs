
pub fn get_database_connection() -> Result<sqlite::Connection, String> {
    let mut connection = sqlite::open("./db/gruxi.db").map_err(|e| format!("Failed to open database connection: {}", e))?;
    connection.set_busy_timeout(500).map_err(|e| format!("Failed to set busy timeout: {}", e))?;
    connection.execute("PRAGMA journal_mode=WAL;").map_err(|e| format!("Failed to enable WAL journal mode: {}", e))?;
    connection.execute("PRAGMA foreign_keys=ON;").map_err(|e| format!("Failed to enable foreign key support: {}", e))?;
    Ok(connection)
}
