use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Telemetry {
    pub bearer_token: Option<String>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Telemetry {
            bearer_token: None,
        }
    }

    pub fn sanitize(&mut self) {
        if let Some(token) = &mut self.bearer_token {
            *token = token.trim().to_string();
            if token.is_empty() {
                self.bearer_token = None;
            }
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        Ok(())
    }
}
