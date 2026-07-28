use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gzip {
    pub is_enabled: bool,
    pub compressible_content_types: Vec<String>,
}

impl Default for Gzip {
    fn default() -> Self {
        Self::new()
    }
}

impl Gzip {
    pub fn new() -> Self {
        Gzip {
            is_enabled: true,
            compressible_content_types: vec![
                "text/".to_string(),
                "application/javascript".to_string(),
                "application/json".to_string(),
                "application/xml".to_string(),
                "application/xhtml+xml".to_string(),
                "application/x-javascript".to_string(),
                "application/x-yaml".to_string(),
                "image/svg+xml".to_string(),
                "application/font-woff".to_string(),
                "application/font-woff2".to_string(),
            ],
        }
    }

    pub fn sanitize(&mut self) {
        // Clean compressible_content_types: trim, remove empty
        self.compressible_content_types = self.compressible_content_types.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate compressible content types
        if self.is_enabled && self.compressible_content_types.is_empty() {
            errors.push("At least one compressible content type must be specified when gzip is enabled".to_string());
        }

        for (content_type_idx, content_type) in self.compressible_content_types.iter().enumerate() {
            if content_type.trim().is_empty() {
                errors.push(format!("Content type {} cannot be empty", content_type_idx + 1));
            }

            // Basic validation for content type format
            if !content_type.contains('/') && !content_type.ends_with('/') {
                errors.push(format!("Content type '{}' appears to be invalid format (should contain '/' or end with '/')", content_type));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
