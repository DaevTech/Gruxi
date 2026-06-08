use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Caching {
    pub is_short_lived_caches_allowed: bool, // Whether we allow short lived caches
}

impl Caching {
    pub fn new() -> Self {
        Caching { is_short_lived_caches_allowed: true }
    }

    pub fn sanitize(&mut self) {}

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let errors = Vec::new();

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

impl Default for Caching {
    fn default() -> Self {
        Self::new()
    }
}
