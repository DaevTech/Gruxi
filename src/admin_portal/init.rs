use crate::logging::syslog::error;

pub fn initialize_admin_site() {
    // Check if there is at least one admin user
    let connection_result = crate::core::database_connection::get_database_connection();
    if connection_result.is_err() {
        error(format!("Failed to get database connection: {}", connection_result.err().unwrap()));
        return;
    }
    let connection = connection_result.unwrap();
    crate::core::admin_user::create_default_admin_user(&connection).unwrap_or_else(|e| {
        error(format!("Failed to create default admin user: {}", e));
    });
}