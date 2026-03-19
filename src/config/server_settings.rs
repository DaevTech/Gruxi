use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerSettings {
    pub max_connection_duration_seconds: u64, // in seconds
    pub max_body_size: u64,                   // in bytes
    pub blocked_file_patterns: Vec<String>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerSettings {
    pub fn new() -> Self {
        ServerSettings {
            max_connection_duration_seconds: 90,
            max_body_size: 10 * 1024 * 1024, // 10 MB
            blocked_file_patterns: vec![
                ".tmp".to_string(),
                ".config".to_string(),
                ".php".to_string(),
                ".sql".to_string(),
                ".bak".to_string(),
                ".old".to_string(),
                ".orig".to_string(),
                ".conf".to_string(),
                ".ini".to_string(),
                ".log".to_string(),
                ".key".to_string(),
                ".pem".to_string(),
            ],
        }
    }

    pub fn sanitize(&mut self) {
        // Ensure blocked file patterns are lowercase for consistent matching and remove any asterisk before extension
        self.blocked_file_patterns = self.blocked_file_patterns.iter().map(|p| p.to_lowercase().replace("*", "")).collect();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate blocked file pattern, ensure they start with a dot
        for pattern in &self.blocked_file_patterns {
            if !pattern.starts_with('.') {
                errors.push(format!("Blocked file pattern must start with a dot: {}", pattern));
            }
        }

        // Validate max_body_size
        if self.max_body_size == 0 {
            errors.push("Max body size cannot be 0".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
