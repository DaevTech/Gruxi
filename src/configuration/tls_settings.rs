use email_address::{EmailAddress, Options};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TlsSettings {
    pub account_email: String,
    pub use_staging_server: bool,
}

impl TlsSettings {
    pub fn new() -> Self {
        Self {
            account_email: String::new(),
            use_staging_server: false
        }
    }

    pub fn sanitize(&mut self) {
        self.account_email = self.account_email.trim().to_string();
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

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}
