use crate::{error, core::admin_user::create_default_admin_user};

pub fn initialize_admin_site() -> Result<(), ()>{
    // Check if there is at least one admin user
    let connection_result = crate::core::database_connection::get_database_connection();
    let connection = match connection_result {
        Ok(conn) => conn,
        Err(e) => {
            error!("Failed to get database connection: {}", e);
            return Err(());
        }
    };

    let admin_user_result = create_default_admin_user(&connection);
    match admin_user_result {
        Ok(_) => (),
        Err(e) => {
            error!("Failed to create default admin user: {}", e);
            return Err(());
        }
    };

    Ok(())
}