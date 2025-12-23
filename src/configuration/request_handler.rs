use http_body_util::combinators::BoxBody;
use hyper::Response;
use hyper::body::Bytes;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    configuration::site::Site,
    core::running_state_manager::get_running_state_manager,
    http::{request_handlers::processor_trait::ProcessorTrait, requests::grux_request::GruxRequest},
    logging::syslog::{debug, error, trace},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestHandler {
    pub id: String,             // Generated uuid, unique, so it can be referenced from sites as a handler
    pub is_enabled: bool,       // Whether it is enabled or not
    pub name: String,           // A name to identify the handler for the user, self chosen
    pub priority: u8,           // Lower values indicate higher priority, equal values are not guaranteed order and therefore should be avoided
    pub processor_type: String, // Reference to the type of processor, e.g., "static_file", "proxy", "php" etc.
    // Reference to specific processor that will handle the request
    pub processor_id: String, // The processor ID
    // Match patterns
    pub url_match: Vec<String>, // /api, /admin/1*, *.php etc (use * to match all URLs)
}

impl RequestHandler {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "New Request Handler".to_string(),
            priority: 1,
            processor_type: "".to_string(),
            processor_id: String::new(),
            url_match: vec!["*".to_string()],
        }
    }

    // Check URL match, can be * or /path or /path* or .html or .php*
    pub fn matches_url(&self, url_path: &str) -> bool {
        for pattern in &self.url_match {
            if pattern == "*" {
                return true;
            } else if pattern.starts_with('*') {
                let suffix = &pattern[1..]; // Remove the '*' character
                if url_path.ends_with(suffix) {
                    return true;
                }
            } else if pattern.starts_with('/') {
                if url_path.starts_with(pattern) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1]; // Remove the '*' character
                if url_path.starts_with(prefix) {
                    return true;
                }
            } else {
                if url_path == pattern {
                    return true;
                }
            }
        }
        false
    }

    pub fn sanitize(&mut self) {
        // Trim and clean ID
        self.id = self.id.trim().to_string();

        // Trim and clean name
        self.name = self.name.trim().to_string();

        // Clean url match patterns: trim, remove empty, ensure proper prefix
        self.url_match = self.url_match.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate ID
        if self.id.trim().is_empty() {
            errors.push("ID cannot be empty".to_string());
        }

        // Validate name
        if self.name.trim().is_empty() {
            errors.push("Name cannot be empty".to_string());
        }

        // Validate URL match patterns
        if self.url_match.is_empty() {
            errors.push("URL match patterns cannot be empty, use * to match all URLs".to_string());
        } else {
            for (pattern_idx, pattern) in self.url_match.iter().enumerate() {
                if pattern.trim().is_empty() {
                    errors.push(format!("URL match pattern {} cannot be empty", pattern_idx + 1));
                } else if !(pattern.starts_with('/') || pattern.starts_with('*') || pattern.ends_with('*')) {
                    errors.push(format!("URL match pattern '{}' should start with '/' or '*' or end with '*'", pattern));
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub async fn handle_request(&self, grux_request: &mut GruxRequest, site: &Site) -> Result<Response<BoxBody<Bytes, hyper::Error>>, ()> {
        let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
        let processor_manager = running_state.get_processor_manager();

        // Depending on request handler type, we get the appropriate processor
        let response_result = match self.processor_type.as_str() {
            "static" => {
                trace(format!("Handling request with static file processor id '{}'", &self.processor_id));
                let pm_option = processor_manager.get_static_file_processor_by_id(&self.processor_id);
                match pm_option {
                    Some(p) => p.handle_request(grux_request, &site).await,
                    None => {
                        debug(format!("Static files processor with id '{}' not found for request handler '{}'", &self.processor_id, &self.name));
                        return Err(());
                    }
                }
            }
            "php" => {
                trace(format!("Handling request with PHP processor id '{}'", &self.processor_id));
                let pm_option = processor_manager.get_php_processor_by_id(&self.processor_id);
                match pm_option {
                    Some(p) => p.handle_request(grux_request, &site).await,
                    None => {
                        debug(format!("PHP processor with id '{}' not found for request handler '{}'", &self.processor_id, &self.name));
                        return Err(());
                    }
                }
            }
            "proxy" => {
                trace(format!("Handling request with proxy processor id '{}'", &self.processor_id));
                let pm_option = processor_manager.get_proxy_processor_by_id(&self.processor_id);
                match pm_option {
                    Some(p) => p.handle_request(grux_request, &site).await,
                    None => {
                        debug(format!("Proxy processor with id '{}' not found for request handler '{}'", &self.processor_id, &self.name));
                        return Err(());
                    }
                }
            }
            _ => {
                error(format!(
                    "Request handler with unknown type '{}' not found for request handler with id '{}'",
                    &self.processor_type, &self.id
                ));
                return Err(());
            }
        };

        if response_result.is_err() {
            debug(format!("Processor with id '{}' for request handler id '{}' failed to handle request", self.processor_id, &self.id));
            return Err(());
        }

        response_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_handler_validation_valid() {
        let handler = create_valid_handler();
        let result = handler.validate();
        assert!(result.is_ok(), "Valid handler should pass validation but got errors: {:?}", result.err());
    }

    #[test]
    fn test_request_handler_validation_empty_id() {
        let mut handler = create_valid_handler();
        handler.id = "".to_string();

        let result = handler.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("ID cannot be empty")));
    }

    #[test]
    fn test_request_handler_validation_empty_name() {
        let mut handler = create_valid_handler();
        handler.name = "".to_string();

        let result = handler.validate();
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.contains("Name cannot be empty")));
    }

    fn create_valid_handler() -> RequestHandler {
        RequestHandler::new()
    }
}
