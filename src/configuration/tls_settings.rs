use email_address::{EmailAddress, Options};
use serde::{Deserialize, Serialize};

use crate::file::normalized_path::NormalizedPath;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TlsSettings {
    pub account_email: String,
    pub use_staging_server: bool,
    pub certificate_cache_path: String,
}

impl TlsSettings {
    pub fn new() -> Self {
        Self {
            account_email: String::new(),
            use_staging_server: false,
            certificate_cache_path: String::new(),
        }
    }

    pub fn sanitize(&mut self) {
        self.account_email = self.account_email.trim().to_string();
        self.certificate_cache_path = self.certificate_cache_path.trim().to_string();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate account_email
        if !self.account_email.is_empty() {
            let email_valid_result = EmailAddress::parse_with_options(&self.account_email, Options::default().with_required_tld().without_display_text());

            if email_valid_result.is_err() {
                errors.push(format!("Invalid email address for LetEncrypt account: {}", &self.account_email));
            }
        }

        // Validate certificate_cache_path by normalizing it and seeing if that gives off any errors
        if !self.certificate_cache_path.is_empty() {
            let normalized_path = NormalizedPath::new(&self.certificate_cache_path, "");
            if normalized_path.is_err() {
                errors.push(format!("Invalid certificate cache path: {}", &self.certificate_cache_path));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
