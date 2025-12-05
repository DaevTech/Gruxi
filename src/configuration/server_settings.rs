use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerSettings {
    pub max_body_size: usize, // in bytes
    pub blocked_file_patterns: Vec<String>,
    pub whitelisted_file_patterns: Vec<String>,
    pub operation_mode: String
}
