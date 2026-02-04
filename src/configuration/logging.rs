use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Logging {
    pub log_rotation_enabled: bool,

    // Rotate by size and/or time
    pub rotate_by_size: bool,
    pub max_log_file_size_mb: u64,
    pub rotate_by_time: bool,
    pub log_time_rotation_type: String, // "daily", "weekly", "monthly"

    // Delete old logs
    pub delete_old_logs: bool,
    pub max_log_age_days: u32,
}

impl Logging {
    pub fn new() -> Self {
        Self {
            // Apply some sane defaults
            log_rotation_enabled: true, // We enable log rotation by default, as i like that
            rotate_by_size: true,
            max_log_file_size_mb: 100, // 100 MB default
            rotate_by_time: true,
            log_time_rotation_type: "daily".to_string(),
            delete_old_logs: true,
            max_log_age_days: 30,
        }
    }

    pub fn sanitize(&mut self) {
        self.log_time_rotation_type = self.log_time_rotation_type.trim().to_lowercase();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // If rotate_by_size is enabled, max_log_file_size_mb must be > 0
        if self.rotate_by_size && self.max_log_file_size_mb == 0 {
            errors.push("Max log file size must be greater than 0 MB if size-based rotation is enabled.".to_string());
        }

        // If rotate_by_time is enabled, log_time_rotation_type must be valid
        if self.rotate_by_time {
            let valid_types = vec!["daily", "weekly", "monthly"];
            if !valid_types.contains(&self.log_time_rotation_type.as_str()) {
                errors.push("Log time rotation type must be one of: daily, weekly, monthly.".to_string());
            }
        }

        // If delete_old_logs is enabled, max_log_age_days must be > 0
        if self.delete_old_logs && self.max_log_age_days == 0 {
            errors.push("Max log age must be greater than 0 days if deleting old logs is enabled.".to_string());
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
