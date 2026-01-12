use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HeaderKV {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Site {
    pub id: usize,
    pub hostnames: Vec<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    // TLS certificate path or actual content
    pub tls_cert_path: String,
    pub tls_cert_content: String,
    // TLS private key path or actual content
    pub tls_key_path: String,
    pub tls_key_content: String,
    pub rewrite_functions: Vec<String>, // List of rewrite functions to apply
    pub request_handlers: Vec<String>,  // List of request handler IDs for this site
    #[serde(default)]
    pub extra_headers: Vec<HeaderKV>,
    // Logs
    pub access_log_enabled: bool,
    pub access_log_file: String,
}

// Supported rewrite functions
pub static REWRITE_FUNCTIONS: &[&str] = &["OnlyWebRootIndexForSubdirs"];

impl Site {
    pub fn new() -> Self {
        Site {
            id: 0,
            hostnames: vec!["*".to_string()],
            is_default: false,
            is_enabled: true,
            tls_cert_path: String::new(),
            tls_cert_content: String::new(),
            tls_key_path: String::new(),
            tls_key_content: String::new(),
            request_handlers: Vec::new(),
            rewrite_functions: Vec::new(),
            extra_headers: Vec::new(),
            access_log_enabled: false,
            access_log_file: String::new(),
        }
    }

    pub fn sanitize(&mut self) {
        // Trim whitespace from hostnames
        for hostname in &mut self.hostnames {
            *hostname = hostname.trim().to_string();
        }

        // Trim whitespace from rewrite functions
        for func in &mut self.rewrite_functions {
            *func = func.trim().to_string();
        }

        // Trim whitespace from access log file
        self.access_log_file = self.access_log_file.trim().to_string();

        // Trim whitespace from extra headers
        for kv in &mut self.extra_headers {
            kv.key = kv.key.trim().to_string();
            kv.value = kv.value.trim().to_string();
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

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
                if let Some(parent) = access_log_file.parent() {
                    if !parent.as_os_str().is_empty() && parent.exists() && !parent.is_dir() {
                        errors.push(format!("Access log file parent path '{}' exists but is not a directory", parent.display()));
                    }
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

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub fn get_rewrite_functions_hashmap(&self) -> std::collections::HashMap<String, ()> {
        let mut hashmap = std::collections::HashMap::new();
        for func in &self.rewrite_functions {
            hashmap.insert(func.clone(), ());
        }
        hashmap
    }
}

#[test]
fn test_site_validation_access_log_enabled_empty_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Access log file cannot be empty when access logging is enabled")));
}

#[test]
fn test_site_validation_access_log_enabled_directory_path() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "/logs/".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("appears to be a directory path")));
}

#[test]
fn test_site_validation_access_log_enabled_windows_directory_path() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "C:\\logs\\".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("appears to be a directory path")));
}

#[test]
fn test_site_validation_access_log_disabled_empty_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = false;
    site.access_log_file = "".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Access log validation should be skipped when disabled");
}

#[test]
fn test_site_validation_access_log_enabled_valid_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "/logs/access.log".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Valid access log file should pass validation");
}

#[test]
fn test_site_validation_access_log_enabled_windows_valid_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "C:\\logs\\access.log".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Valid Windows access log file should pass validation");
}

#[test]
fn test_site_validation_rewrite_functions_single_valid() {
    let mut site = create_valid_site();
    site.rewrite_functions = vec!["OnlyWebRootIndexForSubdirs".to_string()];

    let result = site.validate();
    assert!(result.is_ok(), "Single valid rewrite function should pass validation");
}

#[test]
fn test_site_validation_rewrite_functions_empty_value() {
    let mut site = create_valid_site();
    site.rewrite_functions = vec!["".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(errors.iter().any(|e| e.contains("Rewrite function 1 cannot be empty")), "Expected error for empty rewrite function");
}

#[test]
fn test_site_validation_rewrite_functions_multiple_with_empty() {
    let mut site = create_valid_site();
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
    let mut site = create_valid_site();
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
    let mut site = create_valid_site();
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
    let mut site = create_valid_site();
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
    let mut site = create_valid_site();
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
    let mut site = create_valid_site();
    site.rewrite_functions = vec!["   ".to_string()];

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();

    assert!(
        errors.iter().any(|e| e.contains("Rewrite function 1 cannot be empty")),
        "Whitespace-only rewrite function should be treated as empty"
    );
}

#[cfg(test)]
fn create_valid_site() -> Site {
    Site {
        id: 1,
        hostnames: vec!["example.com".to_string()],
        is_default: false,
        is_enabled: true,
        tls_cert_path: "".to_string(),
        tls_cert_content: "".to_string(),
        tls_key_path: "".to_string(),
        tls_key_content: "".to_string(),
        request_handlers: vec![],
        rewrite_functions: vec![],
        extra_headers: vec![],
        access_log_enabled: false,
        access_log_file: "".to_string(),
    }
}
