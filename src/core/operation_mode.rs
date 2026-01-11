use crate::{core::{command_line_args::cmd_get_operation_mode, database_connection::get_database_connection}};
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
        let connection = get_database_connection().expect("Failed to get database connection");
        let mut stmt = connection
            .prepare("SELECT gruxi_value FROM gruxi WHERE gruxi_key = 'operation_mode'")
            .expect("Failed to prepare operation_mode query");

        let mode_str: Option<String> = match stmt.next().expect("Failed to execute operation_mode query") {
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
        let mut mode_write = OPERATION_MODE_SINGLETON.write().unwrap();
        *mode_write = loaded_mode;
        IS_OPERATION_MODE_LOADED.store(true, Ordering::SeqCst);
    }
    *OPERATION_MODE_SINGLETON.read().unwrap()
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
            let mut mode_write = OPERATION_MODE_SINGLETON.write().unwrap();
            *mode_write = mode;
            drop(mode_write);

            // Update db, if exist, update it else insert new
            let connection = get_database_connection().expect("Failed to get database connection");

            // Check if operation_mode exists
            let mut stmt = connection
                .prepare("SELECT gruxi_value FROM gruxi WHERE gruxi_key = 'operation_mode'")
                .expect("Failed to prepare select query");

            let existing_id: Option<i64> = match stmt.next().expect("Failed to execute select query") {
                State::Row => stmt.read::<i64, _>(0).ok(),
                State::Done => None,
            };

            drop(stmt);

            if let Some(_) = existing_id {
                // Update existing record
                let mut update_stmt = connection
                    .prepare("UPDATE gruxi SET gruxi_value = ? WHERE gruxi_key = ?")
                    .expect("Failed to prepare update query");
                update_stmt.bind((1, new_mode.as_str())).expect("Failed to bind new_mode");
                update_stmt.bind((2, "operation_mode")).expect("Failed to bind operation_mode key");
                update_stmt.next().expect("Failed to execute update query");
            } else {
                // Insert new record
                let mut insert_stmt = connection
                    .prepare("INSERT INTO gruxi (gruxi_key, gruxi_value) VALUES ('operation_mode', ?)")
                    .expect("Failed to prepare insert query");
                insert_stmt.bind((1, new_mode.as_str())).expect("Failed to bind new_mode");
                insert_stmt.next().expect("Failed to execute insert query");
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
