use serde::{Deserialize, Serialize};
use sqlite::Connection;
use uuid::Uuid;

use crate::{
    core::database_connection::get_database_connection,
    error::{
        gruxi_error::GruxiError,
        gruxi_error_enums::{DatabaseError, GruxiErrorKind},
    },
    trace,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Binding {
    pub id: String,
    pub ip: String,
    pub port: u16,
    pub is_admin: bool,
    pub is_telemetry: bool,
    pub is_tls: bool,
    pub protocol: Protocol,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl Default for Binding {
    fn default() -> Self {
        Self::new()
    }
}

impl Binding {
    pub fn new() -> Self {
        Binding {
            id: Uuid::new_v4().to_string(),
            ip: "0.0.0.0".to_string(),
            port: 80,
            is_admin: false,
            is_telemetry: false,
            is_tls: false,
            protocol: Protocol::Tcp,
        }
    }

    pub fn sanitize(&mut self) {
        self.ip = self.ip.trim().to_string();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate IP address
        if self.ip.is_empty() {
            errors.push("IP address cannot be empty".to_string());
        } else if self.ip.parse::<std::net::IpAddr>().is_err() {
            errors.push(format!("Invalid IP address: {}", self.ip));
        }

        // Validate port
        if self.port == 0 {
            errors.push("Port cannot be 0".to_string());
        }

        // Validate common TLS port usage
        if self.is_tls && self.port == 80 {
            errors.push("Port 80 is typically used for HTTP, not HTTPS. Consider using port 443 for TLS".to_string());
        }
        if !self.is_tls && self.port == 443 {
            errors.push("Port 443 is typically used for HTTPS, not HTTP. Consider using port 80 for non-TLS or enable TLS".to_string());
        }

        // Admin binding specific validations
        if self.is_admin {
            // Admin bindings should typically use TLS for security
            if !self.is_tls {
                errors.push("Admin binding should use TLS for security".to_string());
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn save(&self) -> Result<(), GruxiError> {
        let connection = match get_database_connection() {
            Ok(conn) => conn,
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::ConnectionFailed),
                    format!("Failed to get database connection: {}", e),
                ));
            }
        };

        match self.save_to_db(&connection) {
            Ok(_) => (),
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::QueryFailed),
                    format!("Failed to save binding to DB: {}", e),
                ));
            }
        }

        trace!("Inserted binding with content: {:?}", self);

        Ok(())
    }

    fn save_to_db(&self, connection: &Connection) -> Result<(), sqlite::Error> {
        let mut insert_statement = connection.prepare(
            "INSERT INTO bindings (id, ip, port, is_tls, protocol) VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET ip=excluded.ip, port=excluded.port, is_tls=excluded.is_tls, protocol=excluded.protocol",
        )?;
        insert_statement.bind((1, self.id.as_str()))?;
        insert_statement.bind((2, self.ip.as_str()))?;
        insert_statement.bind((3, self.port as i64))?;
        insert_statement.bind((4, if self.is_tls { 1 } else { 0 }))?;
        insert_statement.bind((5, if self.protocol == Protocol::Tcp { "tcp" } else { "udp" }))?;
        insert_statement.next()?;
        Ok(())
    }

    pub fn delete(&self) -> Result<(), GruxiError> {
        let connection = match get_database_connection() {
            Ok(conn) => conn,
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::ConnectionFailed),
                    format!("Failed to get database connection: {}", e),
                ));
            }
        };

        match self.delete_from_db(&connection) {
            Ok(_) => (),
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::QueryFailed),
                    format!("Failed to delete binding from DB: {}", e),
                ));
            }
        }
        trace!("Deleted binding with id: {}", self.id);

        Ok(())
    }

    fn delete_from_db(&self, connection: &Connection) -> Result<(), sqlite::Error> {
        let mut statement = connection.prepare("DELETE FROM bindings WHERE id = ?")?;
        statement.bind((1, self.id.as_str()))?;
        statement.next()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::database_schema::DatabaseSchema;

    #[test]
    fn updating_binding_preserves_site_relationships() {
        let connection = sqlite::open(":memory:").unwrap();
        connection.execute("PRAGMA foreign_keys=ON").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        let mut binding = Binding::new();
        binding.save_to_db(&connection).unwrap();
        connection.execute("INSERT INTO sites (id) VALUES ('site-1')").unwrap();
        let mut statement = connection.prepare("INSERT INTO binding_sites (binding_id, site_id) VALUES (?, 'site-1')").unwrap();
        statement.bind((1, binding.id.as_str())).unwrap();
        statement.next().unwrap();
        drop(statement);
        binding.port = 8080;
        binding.save_to_db(&connection).unwrap();
        let mut statement = connection.prepare("SELECT b.port FROM bindings b JOIN binding_sites bs ON b.id = bs.binding_id WHERE bs.site_id = 'site-1'").unwrap();
        assert_eq!(statement.next().unwrap(), sqlite::State::Row);
        assert_eq!(statement.read::<i64, _>(0).unwrap(), 8080);
    }
}
