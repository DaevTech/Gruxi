use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    configuration::site::Site,
    core::running_state_manager::get_running_state_manager,
    error::{grux_error::GruxError, grux_error_enums::*},
    http::{
        request_handlers::processor_trait::ProcessorTrait,
        request_response::{grux_request::GruxRequest, grux_response::GruxResponse},
    },
    logging::syslog::trace,
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
    // Input can be path only or path+query, we only care about path here, but if there is query, it will still work
    pub fn matches_url(&self, url_path: &str) -> bool {
        // If the url_path contains '?', we only care about the part before it
        let url_path = match url_path.find('?') {
            Some(pos) => &url_path[..pos],
            None => url_path,
        };

        // We always compare on lowercase
        let url_path = url_path.to_lowercase();

        for pattern in &self.url_match {
            let pattern = pattern.to_lowercase();

            if pattern == "*" {
                return true;
            } else if pattern.starts_with('*') {
                let suffix = &pattern[1..]; // Remove the '*' character
                if url_path.ends_with(suffix) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1]; // Remove the '*' character

                if url_path.starts_with(prefix) {
                    return true;
                }
            } else if pattern.starts_with('/') {
                if url_path == pattern {
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

    pub async fn handle_request(&self, grux_request: &mut GruxRequest, site: &Site) -> Result<GruxResponse, GruxError> {
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
                        return Err(GruxError::new(
                            GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::Internal),
                            format!("Static files processor with id '{}' not found for request handler '{}'", &self.processor_id, &self.name),
                        ));
                    }
                }
            }
            "php" => {
                trace(format!("Handling request with PHP processor id '{}'", &self.processor_id));
                let pm_option = processor_manager.get_php_processor_by_id(&self.processor_id);
                match pm_option {
                    Some(p) => p.handle_request(grux_request, &site).await,
                    None => {
                        return Err(GruxError::new(
                            GruxErrorKind::PHPProcessor(PHPProcessorError::Internal),
                            format!("PHP processor with id '{}' not found for request handler '{}'", &self.processor_id, &self.name),
                        ));
                    }
                }
            }
            "proxy" => {
                trace(format!("Handling request with proxy processor id '{}'", &self.processor_id));
                let pm_option = processor_manager.get_proxy_processor_by_id(&self.processor_id);
                match pm_option {
                    Some(p) => p.handle_request(grux_request, &site).await,
                    None => {
                        return Err(GruxError::new(
                            GruxErrorKind::ProxyProcessor(ProxyProcessorError::Internal),
                            format!("Proxy processor with id '{}' not found for request handler '{}'", &self.processor_id, &self.name),
                        ));
                    }
                }
            }
            _ => {
                return Err(GruxError::new(
                    GruxErrorKind::Internal("Unknown processor type"),
                    format!("Request handler with unknown type '{}' not found for request handler with id '{}'", &self.processor_type, &self.id),
                ));
            }
        };

        if response_result.is_err() {
            // Some of the errors are not critical, so we just log and continue
            // But some we want to convey back to the user directly
            let err = response_result.as_ref().err().unwrap();
            match err.kind {
                // Static file errors that we want to convey directly
                GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(_)) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16()));
                }
                GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
                }
                GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::FileBlockedDueToSecurity(_)) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16())); // We dont want to expose that it was blocked due to security
                }

                // Proxy errors that we want to convey directly
                GruxErrorKind::ProxyProcessor(ProxyProcessorError::UpstreamUnavailable) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::BAD_GATEWAY.as_u16()));
                }
                GruxErrorKind::ProxyProcessor(ProxyProcessorError::UpstreamTimeout) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::GATEWAY_TIMEOUT.as_u16()));
                }
                GruxErrorKind::ProxyProcessor(ProxyProcessorError::ConnectionFailed) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::BAD_GATEWAY.as_u16()));
                }

                // PHP errors that we want to convey directly
                GruxErrorKind::PHPProcessor(PHPProcessorError::PathError(_)) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16()));
                }
                GruxErrorKind::PHPProcessor(PHPProcessorError::FileNotFound) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()));
                }
                GruxErrorKind::PHPProcessor(PHPProcessorError::Timeout) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::GATEWAY_TIMEOUT.as_u16()));
                }
                GruxErrorKind::PHPProcessor(PHPProcessorError::Connection) => {
                    return Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::BAD_GATEWAY.as_u16()));
                }

                // Other errors we have logged, but will continue to the next handler
                _ => {}
            }
        }

        response_result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_handler_matches_url_all() {
        let mut handler = create_valid_handler();
        handler.url_match = vec!["*".to_string()];

        assert!(handler.matches_url("/any/path"));
        assert!(handler.matches_url("/api"));
        assert!(handler.matches_url("/admin/dashboard"));
        assert!(handler.matches_url("/index.php"));
        assert!(handler.matches_url("/whatever/my.php"));
        assert!(handler.matches_url("/static/image.png"));
    }

    #[test]
    fn test_request_handler_matches_url_specific_subpath() {
        let mut handler = create_valid_handler();
        handler.url_match = vec!["/api".to_string()];

        assert!(!handler.matches_url("/any/path"));
        assert!(handler.matches_url("/api"));
        assert!(!handler.matches_url("/api/myendpoint"));
        assert!(!handler.matches_url("/admin/dashboard"));
        assert!(!handler.matches_url("/index.php"));
        assert!(!handler.matches_url("/whatever/my.php"));
        assert!(!handler.matches_url("/static/image.png"));
    }

    #[test]
    fn test_request_handler_matches_url_wildcard_subpath_trailing_slash() {
        let mut handler = create_valid_handler();
        handler.url_match = vec!["/admin/*".to_string()];

        assert!(!handler.matches_url("/any/path"));
        assert!(!handler.matches_url("/api"));
        assert!(handler.matches_url("/admin/dashboard"));
        assert!(handler.matches_url("/admin/dashboard/mysettings"));
        assert!(handler.matches_url("/admin/dashboard/mysettings?query=1"));
        assert!(!handler.matches_url("/admin?query=1"));
        assert!(handler.matches_url("/admin/?query=1"));
        assert!(!handler.matches_url("/index.php"));
        assert!(!handler.matches_url("/whatever/my.php"));
        assert!(!handler.matches_url("/static/image.png"));
    }

    #[test]
    fn test_request_handler_matches_url_wildcard_subpath_without_trailing_slash() {
        let mut handler = create_valid_handler();
        handler.url_match = vec!["/admin*".to_string()];

        assert!(!handler.matches_url("/any/path"));
        assert!(!handler.matches_url("/api"));
        assert!(handler.matches_url("/admin/dashboard"));
        assert!(handler.matches_url("/admin/dashboard/mysettings"));
        assert!(handler.matches_url("/admin/dashboard/mysettings?query=1"));
        assert!(handler.matches_url("/admin?query=1"));
        assert!(handler.matches_url("/admin/?query=1"));
        assert!(!handler.matches_url("/index.php"));
        assert!(!handler.matches_url("/whatever/my.php"));
        assert!(!handler.matches_url("/static/image.png"));
    }

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
