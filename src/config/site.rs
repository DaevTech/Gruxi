use serde::{Deserialize, Serialize};
use sqlite::Connection;
use uuid::Uuid;

use crate::error::{
    gruxi_error::GruxiError,
    gruxi_error_enums::{DatabaseError, GruxiErrorKind, SiteError},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderKV {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Site {
    pub id: String,
    pub hostnames: Vec<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    // Automatic TLS
    pub tls_automatic_enabled: bool,
    // TLS certificate path or actual content
    pub tls_cert_path: String,
    pub tls_cert_content: String,
    // TLS private key path or actual content
    pub tls_key_path: String,
    pub tls_key_content: String,
    // Rewrite functions to apply
    pub rewrite_functions: Vec<String>, // List of rewrite functions to apply
    // List of request handelers (and therefore processors) to use, in order of priority
    pub request_handlers: Vec<String>, // List of request handler IDs for this site
    #[serde(default)]
    pub extra_headers: Vec<HeaderKV>,
    // Logs
    pub access_log_enabled: bool,
    pub access_log_file: String,
    // Enforce TLS
    pub force_tls: bool,
    pub force_tls_port: u16,
    // Canonical host (for redirecting non-canonical hosts to the canonical one - Needs to be one of the hostnames in the hostnames list)
    pub canonical_host: String,
}

// Supported rewrite functions
pub static REWRITE_FUNCTIONS: &[&str] = &["OnlyWebRootIndexForSubdirs"];

impl Default for Site {
    fn default() -> Self {
        Self::new()
    }
}

impl Site {
    pub fn new() -> Self {
        Site {
            id: Uuid::new_v4().to_string(),
            hostnames: vec!["*".to_string()],
            is_default: false,
            is_enabled: true,
            tls_automatic_enabled: false,
            tls_cert_path: String::new(),
            tls_cert_content: String::new(),
            tls_key_path: String::new(),
            tls_key_content: String::new(),
            request_handlers: Vec::new(),
            rewrite_functions: Vec::new(),
            extra_headers: Vec::new(),
            access_log_enabled: false,
            access_log_file: String::new(),
            force_tls: false,
            force_tls_port: 443,
            canonical_host: String::new(),
        }
    }

    pub fn sanitize(&mut self) {
        // Trim whitespace from hostnames
        for hostname in &mut self.hostnames {
            *hostname = hostname.trim().to_string();
        }
        // Lowercase hostnames for consistent matching (hostnames are case-insensitive)
        for hostname in &mut self.hostnames {
            *hostname = hostname.to_lowercase();
        }

        // Trim whitespace from rewrite functions
        for func in &mut self.rewrite_functions {
            *func = func.trim().to_string();
        }

        for handler in &mut self.request_handlers {
            *handler = handler.trim().to_string();
        }

        // Trim whitespace from access log file
        self.access_log_file = self.access_log_file.trim().to_string();

        // Trim whitespace from extra headers
        for kv in &mut self.extra_headers {
            kv.key = kv.key.trim().to_string();
            kv.value = kv.value.trim().to_string();
        }

        // Trim whitespace from canonical host and make it lowercase for consistent matching
        self.canonical_host = self.canonical_host.trim().to_string();
        self.canonical_host = self.canonical_host.to_lowercase();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        if self.id.is_empty() || self.id == "." || self.id == ".." || !self.id.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"-._~".contains(&byte)) {
            errors.push("Invalid site id".to_string());
        }

        if self.hostnames.iter().chain(&self.request_handlers).chain(&self.rewrite_functions).any(|value| value.trim().is_empty() || value.contains(',')) {
            errors.push("Site lists cannot contain empty values or commas".to_string());
        }

        if self.extra_headers.iter().any(|header| header.key.contains([',', '=']) || header.value.contains(',')) {
            errors.push("Extra header keys cannot contain commas or equals signs, and values cannot contain commas".to_string());
        }

        // Validate hostnames
        if self.hostnames.is_empty() {
            errors.push("Site must have at least one hostname".to_string());
        }

        for (hostname_idx, hostname) in self.hostnames.iter().enumerate() {
            if hostname.trim().is_empty() {
                errors.push(format!("Hostname {} cannot be empty", hostname_idx + 1));
            } else if hostname.trim() != "*" && hostname.trim().len() < 3 {
                errors.push(format!("Hostname '{}' is too short (minimum 3 characters unless wildcard '*')", hostname.trim()));
            }
        }

        // Validate the rewrite functions
        for (func_idx, func) in self.rewrite_functions.iter().enumerate() {
            if func.trim().is_empty() {
                errors.push(format!("Rewrite function {} cannot be empty", func_idx + 1));
            }
        }

        // Validate the rewrite functions are unique
        let mut unique_funcs = std::collections::HashSet::new();
        for func in &self.rewrite_functions {
            if !unique_funcs.insert(func) {
                errors.push(format!("Duplicate rewrite function found: '{}'", func));
            }
        }

        // Rewrite functions values must be within the known list
        let known_rewrite_functions: Vec<&str> = REWRITE_FUNCTIONS.to_vec();
        for func in &self.rewrite_functions {
            if !known_rewrite_functions.contains(&func.as_str()) {
                errors.push(format!("Unknown rewrite function: '{}'", func));
            }
        }

        // Validate access log configuration
        if self.access_log_enabled {
            if self.access_log_file.trim().is_empty() {
                errors.push("Access log file cannot be empty when access logging is enabled".to_string());
            } else {
                // Check that the access_log_file points to a file, not a directory
                let access_log_file = std::path::Path::new(&self.access_log_file);

                // If the path exists, check if it's a directory
                if access_log_file.exists() && access_log_file.is_dir() {
                    errors.push(format!("Access log file '{}' points to a directory, not a file", self.access_log_file));
                }

                // Check if the path looks like a directory (ends with / or \)
                let trimmed_path = self.access_log_file.trim();
                if trimmed_path.ends_with('/') || trimmed_path.ends_with('\\') {
                    errors.push(format!("Access log file '{}' appears to be a directory path. It needs to point to a file.", self.access_log_file));
                }

                // Check if parent directory is valid (if the file doesn't exist yet)
                if let Some(parent) = access_log_file.parent()
                    && !parent.as_os_str().is_empty() && parent.exists() && !parent.is_dir() {
                        errors.push(format!("Access log file parent path '{}' exists but is not a directory", parent.display()));
                    }
            }
        }

        // If automatic TLS is enabled, each hostname must a valid domain and public facing
        if self.tls_automatic_enabled {
            for hostname in self.hostnames.iter() {
                let hostname_trimmed = hostname.trim();
                if let Err(err_msg) = Site::verify_hostname(hostname_trimmed) {
                    errors.push(err_msg);
                }
            }
        }

        // Validate extra headers (optional but keys/values must be non-empty when present)
        for (idx, kv) in self.extra_headers.iter().enumerate() {
            if kv.key.trim().is_empty() {
                errors.push(format!("Extra header {} key cannot be empty", idx + 1));
            }
            if kv.value.trim().is_empty() {
                errors.push(format!("Extra header {} value cannot be empty", idx + 1));
            }
        }

        // If force TLS is enabled, validate the force_tls_port
        if self.force_tls {
            // TLS port needs to be a valid port number (1-65535)
            if self.force_tls_port == 0 {
                errors.push(format!("Force TLS port '{}' is not a valid port number (1-65535)", self.force_tls_port));
            }
            // Check that it's not the same as the default HTTP port
            if self.force_tls_port == 80 {
                errors.push("Force TLS port cannot be 80, as it is the default HTTP port. Please choose a different port that the site is listening on.".to_string());
            }
        }

        // If canonical host is set, it must be one of the hostnames in the hostnames list
        if !self.canonical_host.is_empty() && !self.hostnames.contains(&self.canonical_host) {
            errors.push(format!("Canonical host '{}' must be one of the hostnames in the hostnames list", self.canonical_host));
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn has_rewrite_function(&self, func_name: &str) -> bool {
        self.rewrite_functions.iter().any(|f| f == func_name)
    }

    pub fn exists(connection: &Connection, id: &str) -> Result<bool, GruxiError> {
        let query = || -> Result<bool, sqlite::Error> {
            let mut statement = connection.prepare("SELECT EXISTS(SELECT 1 FROM sites WHERE id = ?)")?;
            statement.bind((1, id))?;
            statement.next()?;
            Ok(statement.read::<i64, _>(0)? != 0)
        };
        query().map_err(|e| database_error("Failed to check whether site exists", e))
    }

    /// Check existence and write on the same connection to keep the operation atomic.
    pub fn save(&self, connection: &Connection, create: bool) -> Result<(), GruxiError> {
        site_transaction(connection, || {
            let exists = Site::exists(connection, &self.id)?;
            if create && exists {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::Conflict)));
            }
            if !create && !exists {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::NotFound)));
            }
            self.save_to_db(connection).map_err(|e| database_error("Failed to save site", e))
        })
    }

    fn save_to_db(&self, connection: &Connection) -> Result<(), sqlite::Error> {
        let headers = self.extra_headers.iter().map(|header| format!("{}={}", header.key, header.value)).collect::<Vec<_>>().join(",");
        let hostnames = self.hostnames.join(",");
        let handlers = self.request_handlers.join(",");
        let rewrites = self.rewrite_functions.join(",");

        // An update must not delete the row, since that would cascade to binding_sites.
        let mut statement = connection.prepare(
            "INSERT INTO sites (id, is_default, is_enabled, hostnames, tls_cert_path, tls_cert_content, \
             tls_key_path, tls_key_content, request_handlers, rewrite_functions, access_log_enabled, \
             access_log_file, extra_headers, tls_automatic_enabled, force_tls, force_tls_port, canonical_host) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(id) DO UPDATE SET \
             is_default=excluded.is_default, is_enabled=excluded.is_enabled, hostnames=excluded.hostnames, \
             tls_cert_path=excluded.tls_cert_path, tls_cert_content=excluded.tls_cert_content, \
             tls_key_path=excluded.tls_key_path, tls_key_content=excluded.tls_key_content, \
             request_handlers=excluded.request_handlers, rewrite_functions=excluded.rewrite_functions, \
             access_log_enabled=excluded.access_log_enabled, access_log_file=excluded.access_log_file, \
             extra_headers=excluded.extra_headers, tls_automatic_enabled=excluded.tls_automatic_enabled, \
             force_tls=excluded.force_tls, force_tls_port=excluded.force_tls_port, canonical_host=excluded.canonical_host",
        )?;
        statement.bind((1, self.id.as_str()))?;
        statement.bind((2, self.is_default as i64))?;
        statement.bind((3, self.is_enabled as i64))?;
        statement.bind((4, hostnames.as_str()))?;
        statement.bind((5, self.tls_cert_path.as_str()))?;
        statement.bind((6, self.tls_cert_content.as_str()))?;
        statement.bind((7, self.tls_key_path.as_str()))?;
        statement.bind((8, self.tls_key_content.as_str()))?;
        statement.bind((9, handlers.as_str()))?;
        statement.bind((10, rewrites.as_str()))?;
        statement.bind((11, self.access_log_enabled as i64))?;
        statement.bind((12, self.access_log_file.as_str()))?;
        statement.bind((13, headers.as_str()))?;
        statement.bind((14, self.tls_automatic_enabled as i64))?;
        statement.bind((15, self.force_tls as i64))?;
        statement.bind((16, self.force_tls_port as i64))?;
        statement.bind((17, self.canonical_host.as_str()))?;
        statement.next()?;
        Ok(())
    }

    pub fn delete(connection: &Connection, id: &str) -> Result<(), GruxiError> {
        site_transaction(connection, || {
            if !Site::exists(connection, id)? {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::NotFound)));
            }
            let referenced = || -> Result<bool, sqlite::Error> {
                let mut statement = connection.prepare("SELECT EXISTS(SELECT 1 FROM binding_sites WHERE site_id = ?)")?;
                statement.bind((1, id))?;
                statement.next()?;
                Ok(statement.read::<i64, _>(0)? != 0)
            };
            if referenced().map_err(|e| database_error("Failed to check site relationships", e))? {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::Referenced)));
            }
            let delete = || -> Result<(), sqlite::Error> {
                let mut statement = connection.prepare("DELETE FROM sites WHERE id = ?")?;
                statement.bind((1, id))?;
                statement.next()?;
                Ok(())
            };
            delete().map_err(|e| database_error("Failed to delete site", e))
        })
    }

    pub fn verify_hostname(hostname: &str) -> Result<(), String> {
        let hostname_trimmed = hostname.trim();

        // Check the basics
        if hostname_trimmed == "*" {
            return Err(format!(
                "Hostname '{}' cannot be wildcard '*' when automatic TLS is enabled - It needs to be public domains, such as example.com",
                hostname_trimmed
            ));
        } else if hostname_trimmed.parse::<std::net::IpAddr>().is_ok() {
            return Err(format!(
                "Hostname '{}' cannot be an IP address when automatic TLS is enabled - It needs to be public domains, such as example.com",
                hostname_trimmed
            ));
        }

        // Validate domain with PSL
        let domain_parsed_option = psl::domain(hostname_trimmed.as_bytes());
        match domain_parsed_option {
            None => {
                return Err(format!(
                    "Hostname '{}' is not a valid domain name for automatic TLS - It needs to be public domains, such as example.com",
                    hostname_trimmed
                ));
            }
            Some(domain_parsed) => {
                let domain_suffix = domain_parsed.suffix();
                if !domain_suffix.is_known() {
                    return Err(format!(
                        "Hostname '{}' has an unknown top level domain - It needs to be public domains, such as example.com",
                        hostname_trimmed
                    ));
                }
            }
        }

        Ok(())
    }
}

fn database_error(context: &str, error: sqlite::Error) -> GruxiError {
    GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), format!("{}: {}", context, error))
}

fn site_transaction(connection: &Connection, operation: impl FnOnce() -> Result<(), GruxiError>) -> Result<(), GruxiError> {
    connection.execute("BEGIN IMMEDIATE").map_err(|e| database_error("Failed to begin site transaction", e))?;
    let result = operation().and_then(|()| connection.execute("COMMIT").map_err(|e| database_error("Failed to commit site transaction", e)));
    if let Err(error) = result {
        if let Err(rollback_error) = connection.execute("ROLLBACK") {
            return Err(database_error(&format!("Failed to roll back site transaction after {:?}: {}", error.kind, error.message), rollback_error));
        }
        return Err(error);
    }
    Ok(())
}

#[test]
fn test_site_persistence_existence_and_conflicts() {
    let connection = sqlite::open(":memory:").unwrap();
    for sql in crate::database::database_schema::DatabaseSchema::new().init_sql {
        connection.execute(sql).unwrap();
    }

    let mut site = Site::new();
    site.id = "quoted'id".to_string();
    assert!(!Site::exists(&connection, &site.id).unwrap());
    assert!(matches!(site.save(&connection, false), Err(GruxiError { kind: GruxiErrorKind::Site(SiteError::NotFound), .. })));
    site.save(&connection, true).unwrap();
    assert!(Site::exists(&connection, &site.id).unwrap());
    assert!(matches!(site.save(&connection, true), Err(GruxiError { kind: GruxiErrorKind::Site(SiteError::Conflict), .. })));

    site.hostnames = vec!["changed.example".to_string()];
    site.save(&connection, false).unwrap();
    assert_eq!(crate::config::load_configuration::load_sites(&connection).unwrap()[0].hostnames, site.hostnames);
    Site::delete(&connection, &site.id).unwrap();
    assert!(!Site::exists(&connection, &site.id).unwrap());
    assert!(matches!(Site::delete(&connection, &site.id), Err(GruxiError { kind: GruxiErrorKind::Site(SiteError::NotFound), .. })));
}

#[test]
fn test_site_validation_storage_constraints() {
    let mut site = Site::new();
    for id in ["", ".", "..", "bad/id", "bad id"] {
        site.id = id.to_string();
        assert!(site.validate().unwrap_err().iter().any(|error| error == "Invalid site id"));
    }

    site.id = "valid-id".to_string();
    for (hostnames, handlers, rewrites) in [
        (vec!["one.example,two.example"], vec![], vec![]),
        (vec!["example.com"], vec!["  "], vec![]),
        (vec!["example.com"], vec!["one,two"], vec![]),
        (vec!["example.com"], vec![], vec!["one,two"]),
    ] {
        site.hostnames = hostnames.into_iter().map(str::to_string).collect();
        site.request_handlers = handlers.into_iter().map(str::to_string).collect();
        site.rewrite_functions = rewrites.into_iter().map(str::to_string).collect();
        assert!(site.validate().unwrap_err().iter().any(|error| error.contains("Site lists")));
    }

    site = Site::new();
    for (key, value) in [("Bad=Key", "value"), ("Bad,Key", "value"), ("Key", "one,two")] {
        site.extra_headers = vec![HeaderKV { key: key.to_string(), value: value.to_string() }];
        assert!(site.validate().unwrap_err().iter().any(|error| error.contains("Extra header keys cannot")));
    }
}

#[test]
fn test_site_sanitize_request_handlers() {
    let mut site = Site::new();
    site.request_handlers = vec![" handler-id ".to_string()];
    site.sanitize();
    assert_eq!(site.request_handlers, vec!["handler-id"]);
    assert!(site.validate().is_ok());
}

#[test]
fn test_configuration_applies_site_storage_validation() {
    let mut configuration = crate::config::configuration::Configuration::new();
    let mut site = Site::new();
    site.id = "invalid/id".to_string();
    configuration.sites.push(site);
    assert!(configuration.validate().unwrap_err().iter().any(|error| error == "Site 1: Invalid site id"));
}

#[test]
fn test_site_validation_access_log_enabled_empty_file() {
    let mut site = Site::new();
    site.access_log_enabled = true;
    site.access_log_file = "".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Access log file cannot be empty when access logging is enabled")));
}

#[test]
fn test_site_validation_access_log_enabled_directory_path() {
    let mut site = Site::new();
    site.access_log_enabled = true;
    site.access_log_file = "/logs/".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("appears to be a directory path")));
}

#[test]
fn test_site_validation_access_log_enabled_windows_directory_path() {
    let mut site = Site::new();
    site.access_log_enabled = true;
    site.access_log_file = "C:\\logs\\".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("appears to be a directory path")));
}

#[test]
fn test_site_validation_access_log_disabled_empty_file() {
    let mut site = Site::new();
    site.access_log_enabled = false;
    site.access_log_file = "".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Access log validation should be skipped when disabled");
}

#[test]
fn test_site_validation_access_log_enabled_valid_file() {
    let mut site = Site::new();
    site.access_log_enabled = true;
    site.access_log_file = "/logs/access.log".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Valid access log file should pass validation");
}

#[test]
fn test_site_validation_access_log_enabled_windows_valid_file() {
    let mut site = Site::new();
    site.access_log_enabled = true;
    site.access_log_file = "C:\\logs\\access.log".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Valid Windows access log file should pass validation");
}

#[test]
fn test_site_validation_rewrite_functions_single_valid() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["OnlyWebRootIndexForSubdirs".to_string()];

    let result = site.validate();
    assert!(result.is_ok(), "Single valid rewrite function should pass validation");
}

#[test]
fn test_site_validation_rewrite_functions_empty_value() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(errors.iter().any(|e| e.contains("Rewrite function 1 cannot be empty")), "Expected error for empty rewrite function");
}

#[test]
fn test_site_validation_rewrite_functions_multiple_with_empty() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["OnlyWebRootIndexForSubdirs".to_string(), "".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Rewrite function 2 cannot be empty")),
        "Expected error for second (empty) rewrite function"
    );
}

#[test]
fn test_site_validation_rewrite_functions_unknown_value() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["UnknownRewriteFunction".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Unknown rewrite function: 'UnknownRewriteFunction'")),
        "Expected error for unknown rewrite function"
    );
}

#[test]
fn test_site_validation_rewrite_functions_mixed_known_and_unknown() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["OnlyWebRootIndexForSubdirs".to_string(), "UnknownRewriteFunction".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Unknown rewrite function: 'UnknownRewriteFunction'")),
        "Expected error for the unknown rewrite function"
    );
    assert!(
        !errors.iter().any(|e| e.contains("Unknown rewrite function: 'OnlyWebRootIndexForSubdirs'")),
        "Known rewrite function should not produce an error"
    );
}

#[test]
fn test_site_validation_rewrite_functions_duplicate_values() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["OnlyWebRootIndexForSubdirs".to_string(), "OnlyWebRootIndexForSubdirs".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Duplicate rewrite function found: 'OnlyWebRootIndexForSubdirs'")),
        "Expected error for duplicate rewrite function"
    );
}

#[test]
fn test_site_validation_rewrite_functions_duplicate_and_unknown() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["OnlyWebRootIndexForSubdirs".to_string(), "OnlyWebRootIndexForSubdirs".to_string(), "AnotherUnknown".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Duplicate rewrite function found: 'OnlyWebRootIndexForSubdirs'")),
        "Expected duplicate rewrite function error"
    );
    assert!(
        errors.iter().any(|e| e.contains("Unknown rewrite function: 'AnotherUnknown'")),
        "Expected unknown rewrite function error"
    );
}

#[test]
fn test_site_validation_rewrite_functions_whitespace_only() {
    let mut site = Site::new();
    site.rewrite_functions = vec!["   ".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Rewrite function 1 cannot be empty")),
        "Whitespace-only rewrite function should be treated as empty"
    );
}
