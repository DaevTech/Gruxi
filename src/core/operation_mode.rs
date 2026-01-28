use crate::core::{command_line_args::cmd_get_operation_mode, database_connection::get_database_connection};
use crate::logging::syslog::error;
use sqlite::State;
use std::sync::{
    RwLock,
    atomic::{AtomicBool, Ordering},
};

// Operation mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OperationMode {
    DEV,
    DEBUG,
    PRODUCTION,
    ULTIMATE,
}

pub fn load_operation_mode() -> OperationMode {
    // Parse command line args
    let mut opmode = cmd_get_operation_mode();

    // If operation is not set in command line, load only this field from db
    if opmode.is_empty() {
        let connection_result = get_database_connection();
        let connection = match connection_result {
            Ok(conn) => conn,
            Err(e) => {
                error(format!("Failed to get database connection: {}", e));
                return OperationMode::PRODUCTION;
            }
        };

        let stmt_result = connection.prepare("SELECT gruxi_value FROM gruxi WHERE gruxi_key = 'operation_mode'");
        let mut stmt = match stmt_result {
            Ok(s) => s,
            Err(e) => {
                error(format!("Failed to prepare operation_mode query: {}", e));
                return OperationMode::PRODUCTION;
            }
        };

        let mode_str_option = stmt.next();
        let mode_str = match mode_str_option {
            Ok(opt) => opt,
            Err(e) => {
                error(format!("Failed to execute operation_mode query: {}", e));
                return OperationMode::PRODUCTION;
            }
        };

        let mode_str: Option<String> = match mode_str {
            State::Row => stmt.read::<String, _>(0).ok(),
            State::Done => None,
        };

        if let Some(mode) = mode_str {
            opmode = mode;
        } else {
            opmode = "PRODUCTION".to_string();
        }
    }

    match_string_to_operation_mode(&opmode).unwrap_or(OperationMode::PRODUCTION)
}

pub fn match_string_to_operation_mode(mode_str: &str) -> Option<OperationMode> {
    match mode_str {
        "DEV" => Some(OperationMode::DEV),
        "DEBUG" => Some(OperationMode::DEBUG),
        "PRODUCTION" => Some(OperationMode::PRODUCTION),
        "ULTIMATE" => Some(OperationMode::ULTIMATE),
        _ => None,
    }
}

static IS_OPERATION_MODE_LOADED: AtomicBool = AtomicBool::new(false);
static OPERATION_MODE_SINGLETON: RwLock<OperationMode> = RwLock::new(OperationMode::PRODUCTION);

pub fn get_operation_mode() -> OperationMode {
    if IS_OPERATION_MODE_LOADED.load(Ordering::SeqCst) == false {
        let loaded_mode = load_operation_mode();
        let mode_write_result = OPERATION_MODE_SINGLETON.write();
        let mut mode_write = match mode_write_result {
            Ok(mw) => mw,
            Err(e) => {
                error(format!("Failed to acquire write lock for operation mode: {} - Returning default", e));
                return OperationMode::PRODUCTION;
            }
        };

        *mode_write = loaded_mode;
        IS_OPERATION_MODE_LOADED.store(true, Ordering::SeqCst);
    }

    match OPERATION_MODE_SINGLETON.read() {
        Ok(mode_read) => *mode_read,
        Err(e) => {
            error(format!("Failed to acquire read lock for operation mode: {} - Returning default", e));
            return OperationMode::PRODUCTION;
        }
    }
}

pub fn get_operation_mode_as_string() -> String {
    match get_operation_mode() {
        OperationMode::DEV => "DEV".to_string(),
        OperationMode::DEBUG => "DEBUG".to_string(),
        OperationMode::PRODUCTION => "PRODUCTION".to_string(),
        OperationMode::ULTIMATE => "ULTIMATE".to_string(),
    }
}

pub fn is_valid_operation_mode(mode_str: &str) -> bool {
    match_string_to_operation_mode(mode_str).is_some()
}

pub fn set_new_operation_mode(new_mode: String) -> bool {
    match match_string_to_operation_mode(&new_mode) {
        Some(mode) => {
            let mode_write_result = OPERATION_MODE_SINGLETON.write();
            let mut mode_write = match mode_write_result {
                Ok(mw) => mw,
                Err(e) => {
                    error(format!("Failed to acquire write lock for operation mode: {} - Returning false", e));
                    return false;
                }
            };
            *mode_write = mode;
            drop(mode_write);

            // Update db, if exist, update it else insert new
            let connection_result = get_database_connection();
            let connection = match connection_result {
                Ok(conn) => conn,
                Err(e) => {
                    error(format!("Failed to get database connection: {} - Returning false", e));
                    return false;
                }
            };

            // Check if operation_mode exists
            let stmt_result = connection.prepare("SELECT gruxi_value FROM gruxi WHERE gruxi_key = 'operation_mode'");
            let mut stmt = match stmt_result {
                Ok(s) => s,
                Err(e) => {
                    error(format!("Failed to prepare select query: {} - Returning false", e));
                    return false;
                }
            };

            let existing_id: Option<i64> = match stmt.next() {
                Ok(state) => match state {
                    State::Row => stmt.read::<i64, _>(0).ok(),
                    State::Done => None,
                },
                Err(e) => {
                    error(format!("Failed to execute select query: {} - Returning false", e));
                    return false;
                }
            };

            drop(stmt);

            if let Some(_) = existing_id {
                // Update existing record
                let update_stmt_result = connection.prepare("UPDATE gruxi SET gruxi_value = ? WHERE gruxi_key = ?");
                let mut update_stmt = match update_stmt_result {
                    Ok(s) => s,
                    Err(e) => {
                        error(format!("Failed to prepare update query: {} - Returning false", e));
                        return false;
                    }
                };

                match update_stmt.bind((1, new_mode.as_str())) {
                    Ok(_) => (),
                    Err(e) => {
                        error(format!("Failed to bind new_mode: {} - Returning false", e));
                        return false;
                    }
                };
                match update_stmt.bind((2, "operation_mode")) {
                    Ok(_) => (),
                    Err(e) => {
                        error(format!("Failed to bind operation_mode key: {} - Returning false", e));
                        return false;
                    }
                };
                match update_stmt.next() {
                    Ok(_) => (),
                    Err(e) => {
                        error(format!("Failed to execute update query: {} - Returning false", e));
                        return false;
                    }
                };
            } else {
                // Insert new record
                let insert_stmt_result = connection.prepare("INSERT INTO gruxi (gruxi_key, gruxi_value) VALUES ('operation_mode', ?)");
                let mut insert_stmt = match insert_stmt_result {
                    Ok(s) => s,
                    Err(e) => {
                        error(format!("Failed to prepare insert query: {} - Returning false", e));
                        return false;
                    }
                };
                match insert_stmt.bind((1, new_mode.as_str())) {
                    Ok(_) => (),
                    Err(e) => {
                        error(format!("Failed to bind new_mode: {} - Returning false", e));
                        return false;
                    }
                };
                match insert_stmt.next() {
                    Ok(_) => (),
                    Err(e) => {
                        error(format!("Failed to execute insert query: {} - Returning false", e));
                        return false;
                    }
                };
            }

            // Trigger operation_mode_changed event
            let triggers = crate::core::triggers::get_trigger_handler();
            tokio::spawn(async move {
                triggers.run_trigger("operation_mode_changed").await;
            });

            true
        }
        None => {
            error(format!("Attempted to set invalid operation mode: {}", new_mode));
            false
        }
    }
}
