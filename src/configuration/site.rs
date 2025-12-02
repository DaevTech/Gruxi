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
    pub web_root: String,
    pub web_root_index_file_list: Vec<String>,
    pub enabled_handlers: Vec<String>, // List of enabled handler IDs for this site
    // TLS certificate path or actual content
    pub tls_cert_path: String,
    pub tls_cert_content: String,
    // TLS private key path or actual content
    pub tls_key_path: String,
    pub tls_key_content: String,
    pub rewrite_functions: Vec<String>,
    #[serde(default)]
    pub extra_headers: Vec<HeaderKV>,
    // Logs
    pub access_log_enabled: bool,
    pub access_log_file: String,
}

impl Site {
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

        // Validate web root
        if self.web_root.trim().is_empty() {
            errors.push("Web root cannot be empty".to_string());
        }

        // Validate index files
        if self.web_root_index_file_list.is_empty() {
            errors.push("Site must have at least one index file".to_string());
        }

        for (file_idx, file) in self.web_root_index_file_list.iter().enumerate() {
            if file.trim().is_empty() {
                errors.push(format!("Index file {} cannot be empty", file_idx + 1));
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

#[cfg(test)]
fn create_valid_site() -> Site {
    Site {
        id: 1,
        hostnames: vec!["example.com".to_string()],
        is_default: false,
        is_enabled: true,
        web_root: "./www-default".to_string(),
        web_root_index_file_list: vec!["index.html".to_string()],
        enabled_handlers: vec![],
        tls_cert_path: "".to_string(),
        tls_cert_content: "".to_string(),
        tls_key_path: "".to_string(),
        tls_key_content: "".to_string(),
        rewrite_functions: vec![],
        extra_headers: vec![],
        access_log_enabled: false,
        access_log_file: "".to_string(),
    }
}
