use chrono::{DateTime, Duration, Utc};
use crate::logging::syslog::info;
use random_password_generator::generate_password;
use serde::{Deserialize, Serialize};
use sqlite::Connection;
use uuid::Uuid;

use crate::core::database_connection::get_database_connection;

#[derive(Debug, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
    pub is_active: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user_id: i64,
    pub username: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub fn create_default_admin_user(connection: &Connection) -> Result<(), String> {
    // Check if admin user already exists
    let mut statement = connection
        .prepare("SELECT COUNT(*) FROM users WHERE username = 'admin'")
        .map_err(|e| format!("Failed to prepare admin check statement: {}", e))?;

    let admin_exists = match statement.next() {
        Ok(sqlite::State::Row) => {
            let count: i64 = statement.read(0).unwrap_or(0);
            count > 0
        }
        _ => false,
    };

    let mut need_to_clear_sessions = false;

    if !admin_exists {
        let (random_password, password_hash) = get_random_hashed_password();

        let created_at = Utc::now().to_rfc3339();

        connection
            .execute(format!(
                "INSERT INTO users (username, password_hash, created_at, is_active) VALUES ('{}', '{}', '{}', 1)",
                "admin", password_hash, created_at
            ))
            .map_err(|e| format!("Failed to create default admin user: {}", e))?;

        info(format!("Default admin user created with username 'admin' and password '{}'", random_password));
        need_to_clear_sessions = true;
    }

    if need_to_clear_sessions {
        // Invalidate all existing sessions for admin user
        invalidate_sessions_for_user(connection, "admin")?;
    }

    Ok(())
}

fn invalidate_sessions_for_user(connection: &Connection, username: &str) -> Result<(), String> {
    connection
        .execute(format!("DELETE FROM sessions WHERE username = '{}'", username))
        .map_err(|e| format!("Failed to invalidate sessions for user {}: {}", username, e))?;
    Ok(())
}

pub fn reset_admin_password() -> Result<String, String> {
    let connection = get_database_connection()?;

    let (random_password, password_hash) = get_random_hashed_password();
    connection
        .execute(format!("UPDATE users SET password_hash = '{}' WHERE username = 'admin'", password_hash))
        .map_err(|e| format!("Failed to reset admin password: {}", e))?;
    info(format!("Password changed for user 'admin' to: {}", random_password));

    // Invalidate all existing sessions for admin user
    invalidate_sessions_for_user(&connection, "admin")?;

    Ok(random_password)
}

fn get_random_hashed_password() -> (String, String) {
    let random_password = generate_password(true, true, false, 20);
    let password_hash = bcrypt::hash(&random_password, bcrypt::DEFAULT_COST).expect("Failed to hash password");
    (random_password, password_hash)
}

pub fn authenticate_user(username: &str, password: &str) -> Result<Option<User>, String> {
    let connection = get_database_connection()?;

    let mut statement = connection
        .prepare("SELECT id, username, password_hash, created_at, last_login, is_active FROM users WHERE username = ? AND is_active = 1")
        .map_err(|e| format!("Failed to prepare authentication statement: {}", e))?;

    statement.bind((1, username)).map_err(|e| format!("Failed to bind username: {}", e))?;

    match statement.next().map_err(|e| format!("Failed to execute authentication query: {}", e))? {
        sqlite::State::Row => {
            let id: i64 = statement.read(0).map_err(|e| format!("Failed to read user id: {}", e))?;
            let db_username: String = statement.read(1).map_err(|e| format!("Failed to read username: {}", e))?;
            let password_hash: String = statement.read(2).map_err(|e| format!("Failed to read password hash: {}", e))?;
            let created_at_str: String = statement.read(3).map_err(|e| format!("Failed to read created_at: {}", e))?;
            let last_login_str: Option<String> = statement.read(4).map_err(|e| format!("Failed to read last_login: {}", e))?;
            let is_active: i64 = statement.read(5).map_err(|e| format!("Failed to read is_active: {}", e))?;
            let is_active = is_active != 0;

            // Verify password
            let password_valid = bcrypt::verify(password, &password_hash).map_err(|e| format!("Failed to verify password: {}", e))?;

            if password_valid {
                let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                    .map_err(|e| format!("Failed to parse created_at: {}", e))?
                    .with_timezone(&Utc);

                let last_login = match last_login_str {
                    Some(login_str) => Some(DateTime::parse_from_rfc3339(&login_str).map_err(|e| format!("Failed to parse last_login: {}", e))?.with_timezone(&Utc)),
                    None => None,
                };

                // Update last login time
                let now = Utc::now().to_rfc3339();
                connection
                    .execute(format!("UPDATE users SET last_login = '{}' WHERE id = {}", now, id))
                    .map_err(|e| format!("Failed to update last login: {}", e))?;

                Ok(Some(User {
                    id,
                    username: db_username,
                    password_hash,
                    created_at,
                    last_login,
                    is_active,
                }))
            } else {
                Ok(None) // Invalid password
            }
        }
        sqlite::State::Done => Ok(None), // User not found
    }
}

pub fn create_session(user: &User) -> Result<Session, String> {
    let connection = get_database_connection()?;

    let session_id = Uuid::new_v4().to_string();
    let token = Uuid::new_v4().to_string();
    let created_at = Utc::now();
    let expires_at = created_at + Duration::hours(24); // Session expires in 24 hours

    let session = Session {
        id: session_id.clone(),
        user_id: user.id,
        username: user.username.clone(),
        token: token.clone(),
        expires_at,
        created_at,
    };

    connection
        .execute(format!(
            "INSERT INTO sessions (id, user_id, username, token, expires_at, created_at) VALUES ('{}', {}, '{}', '{}', '{}', '{}')",
            session.id,
            session.user_id,
            session.username,
            session.token,
            session.expires_at.to_rfc3339(),
            session.created_at.to_rfc3339()
        ))
        .map_err(|e| format!("Failed to create session: {}", e))?;

    info(format!("Created session for user: {}", user.username));
    Ok(session)
}

pub fn verify_session_token(token: &str) -> Result<Option<Session>, String> {
    let connection = get_database_connection()?;

    // Clean up expired sessions first
    cleanup_expired_sessions(&connection)?;

    let mut statement = connection
        .prepare("SELECT id, user_id, username, token, expires_at, created_at FROM sessions WHERE token = ?")
        .map_err(|e| format!("Failed to prepare session verification statement: {}", e))?;

    statement.bind((1, token)).map_err(|e| format!("Failed to bind session token: {}", e))?;

    match statement.next().map_err(|e| format!("Failed to execute session verification query: {}", e))? {
        sqlite::State::Row => {
            let id: String = statement.read(0).map_err(|e| format!("Failed to read session id: {}", e))?;
            let user_id: i64 = statement.read(1).map_err(|e| format!("Failed to read user_id: {}", e))?;
            let username: String = statement.read(2).map_err(|e| format!("Failed to read username: {}", e))?;
            let session_token: String = statement.read(3).map_err(|e| format!("Failed to read token: {}", e))?;
            let expires_at_str: String = statement.read(4).map_err(|e| format!("Failed to read expires_at: {}", e))?;
            let created_at_str: String = statement.read(5).map_err(|e| format!("Failed to read created_at: {}", e))?;

            let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
                .map_err(|e| format!("Failed to parse expires_at: {}", e))?
                .with_timezone(&Utc);

            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map_err(|e| format!("Failed to parse created_at: {}", e))?
                .with_timezone(&Utc);

            // Check if session is still valid (not expired)
            if expires_at > Utc::now() {
                Ok(Some(Session {
                    id,
                    user_id,
                    username,
                    token: session_token,
                    expires_at,
                    created_at,
                }))
            } else {
                Ok(None) // Session expired
            }
        }
        sqlite::State::Done => Ok(None), // Session not found
    }
}

pub fn invalidate_session(token: &str) -> Result<bool, String> {
    let connection = get_database_connection()?;

    // First check if session exists
    let mut statement = connection
        .prepare("SELECT COUNT(*) FROM sessions WHERE token = ?")
        .map_err(|e| format!("Failed to prepare session check statement: {}", e))?;

    statement.bind((1, token)).map_err(|e| format!("Failed to bind session token: {}", e))?;

    let session_exists = match statement.next().map_err(|e| format!("Failed to execute session check query: {}", e))? {
        sqlite::State::Row => {
            let count: i64 = statement.read(0).map_err(|e| format!("Failed to read session count: {}", e))?;
            count > 0
        }
        sqlite::State::Done => false,
    };

    if session_exists {
        connection
            .execute(format!("DELETE FROM sessions WHERE token = '{}'", token))
            .map_err(|e| format!("Failed to delete session: {}", e))?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn cleanup_expired_sessions(connection: &Connection) -> Result<(), String> {
    let now = Utc::now().to_rfc3339();
    connection
        .execute(format!("DELETE FROM sessions WHERE expires_at < '{}'", now))
        .map_err(|e| format!("Failed to cleanup expired sessions: {}", e))?;

    Ok(())
}

pub fn cleanup_all_expired_sessions() -> Result<u64, String> {
    let connection = get_database_connection()?;

    // First count the expired sessions
    let now = Utc::now().to_rfc3339();
    let mut statement = connection
        .prepare("SELECT COUNT(*) FROM sessions WHERE expires_at < ?")
        .map_err(|e| format!("Failed to prepare expired sessions count statement: {}", e))?;

    statement.bind((1, now.as_str())).map_err(|e| format!("Failed to bind expiration time: {}", e))?;

    let expired_count = match statement.next().map_err(|e| format!("Failed to execute expired sessions count query: {}", e))? {
        sqlite::State::Row => {
            let count: i64 = statement.read(0).map_err(|e| format!("Failed to read expired sessions count: {}", e))?;
            count as u64
        }
        sqlite::State::Done => 0,
    };

    // Drop statement before using connection again
    drop(statement);

    // Delete expired sessions
    connection
        .execute(format!("DELETE FROM sessions WHERE expires_at < '{}'", now))
        .map_err(|e| format!("Failed to cleanup expired sessions: {}", e))?;

    if expired_count > 0 {
        info(format!("Cleaned up {} expired sessions", expired_count));
    }

    Ok(expired_count)
}
