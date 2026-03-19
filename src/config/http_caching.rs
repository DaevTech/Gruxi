use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HttpCaching {
    // We only set this, and we internally control the others
    pub enabled_caching: bool,

    // Internally controlled
    pub enable_header_etag: bool,
    pub enable_header_last_modified: bool,
    pub enable_header_expires: bool,
    pub enable_header_cache_control: bool,
}

impl Default for HttpCaching {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpCaching {
    pub fn new() -> Self {
        Self {
            enabled_caching: true,
            enable_header_etag: true,
            enable_header_last_modified: true,
            enable_header_expires: true,
            enable_header_cache_control: true,
        }
    }

    pub fn update_headers_settings(&mut self) {
        if self.enabled_caching {
            self.enable_header_etag = true;
            self.enable_header_last_modified = true;
            self.enable_header_expires = true;
            self.enable_header_cache_control = true;
        } else {
            self.enable_header_etag = false;
            self.enable_header_last_modified = false;
            self.enable_header_expires = false;
            self.enable_header_cache_control = false;
        }
    }

    pub fn sanitize(&mut self) {}

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let errors = Vec::new();

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
