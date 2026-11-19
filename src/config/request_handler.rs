use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    config::site::Site, error::{gruxi_error::GruxiError, gruxi_error_enums::*}, http::{
        request_handlers::{processor_trait::ProcessorTrait, processors::{processor_manager::ProcessorManager, php_processor::PHPProcessor, proxy_processor::ProxyProcessor, static_files_processor::StaticFileProcessor}},
        request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
    }, runtime::binding_manager::BindingRuntime,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestHandler {
    pub id: String,             // Generated uuid, unique identifier for this handler
    pub is_enabled: bool,       // Whether it is enabled or not
    pub name: String,           // A name to identify the handler for the user, self chosen
    pub processor: Processor,
    // Match patterns
    pub url_match: Vec<String>, // /api, /admin/1*, *.php etc (use * to match all URLs)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "config", rename_all = "lowercase")]
pub enum Processor {
    Static(StaticFileProcessor),
    Php(PHPProcessor),
    Proxy(ProxyProcessor),
}

impl Processor {
    pub fn id(&self) -> &str {
        match self {
            Self::Static(processor) => &processor.id,
            Self::Php(processor) => &processor.id,
            Self::Proxy(processor) => &processor.id,
        }
    }

    pub fn sanitize(&mut self) {
        match self {
            Self::Static(processor) => processor.sanitize(),
            Self::Php(processor) => processor.sanitize(),
            Self::Proxy(processor) => processor.sanitize(),
        }
    }

    pub fn validate(&self) -> Result<(), Vec<String>> {
        match self {
            Self::Static(processor) => processor.validate(),
            Self::Php(processor) => processor.validate(),
            Self::Proxy(processor) => processor.validate(),
        }
    }

    pub fn initialize(&mut self) {
        match self {
            Self::Static(processor) => processor.initialize(),
            Self::Php(processor) => processor.initialize(),
            Self::Proxy(processor) => processor.initialize(),
        }
    }
}

impl Default for RequestHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestHandler {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            is_enabled: true,
            name: "New Request Handler".to_string(),
            processor: Processor::Static(StaticFileProcessor::new(String::new(), Vec::new())),
            url_match: vec!["*".to_string()],
        }
    }

    /// Check URL match, can be * or /path or /path* or .html or .php*
    /// Input is expected to be already lowercased, since matching is case insensitive
    pub fn matches_url(&self, url_path: &str) -> bool {
        // Patterns will be lowercase when loaded as configuration
        for pattern in &self.url_match {
            if pattern.eq_ignore_ascii_case("*") {
                return true;
            } else if let Some(suffix) = pattern.strip_prefix('*') {
                // Remove the '*' character
                if url_path.ends_with(suffix) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                let prefix = &pattern[..pattern.len() - 1]; // Remove the '*' character

                if url_path.starts_with(prefix) {
                    return true;
                }
            } else {
                if url_path.eq_ignore_ascii_case(pattern) {
                    return true;
                }
            }
        }
        false
    }

    pub fn sanitize(&mut self) {
        self.processor.sanitize();
        // Trim and clean ID
        self.id = self.id.trim().to_string();

        // Trim and clean name
        self.name = self.name.trim().to_string();

        // Clean url match patterns: trim, remove empty, ensure proper prefix
        self.url_match = self.url_match.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

        // Lowercase url match patterns, since matching should be case insensitive
        self.url_match = self.url_match.iter().map(|s| s.to_lowercase()).collect();
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

            // Check patterns for lowercase, since matching is case insensitive
            for pattern in self.url_match.iter() {
                if *pattern != pattern.to_lowercase() {
                    errors.push(format!("URL match pattern '{}' should be lowercase since matching is case insensitive", pattern));
                }
            }
        }

        if let Err(processor_errors) = self.processor.validate() {
            errors.extend(processor_errors);
        }
        if self.processor.id().trim().is_empty() {
            errors.push("Processor ID cannot be empty".to_string());
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    pub async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site, binding_runtime: &BindingRuntime, processor_manager: &ProcessorManager) -> Result<GruxiResponse, GruxiError> {

        // Depending on request handler type, we get the appropriate processor
        let response_result = match &self.processor {
            Processor::Static(processor) => processor.handle_request(gruxi_request, site, binding_runtime).await,
            Processor::Php(processor) => processor.handle_request(gruxi_request, site, binding_runtime).await,
            Processor::Proxy(processor) => processor.handle_request_with_manager(gruxi_request, processor_manager).await,
        };

        match &response_result {
            Ok(_) => response_result,
            Err(err) => {
                match err.kind {
                    // Static file errors that we want to convey directly
                    GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(_)) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16())),

                    // Proxy errors that we want to convey directly
                    GruxiErrorKind::ProxyProcessor(ProxyProcessorError::UpstreamUnavailable) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::BAD_GATEWAY.as_u16())),
                    GruxiErrorKind::ProxyProcessor(ProxyProcessorError::UpstreamTimeout) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::GATEWAY_TIMEOUT.as_u16())),
                    GruxiErrorKind::ProxyProcessor(ProxyProcessorError::ConnectionFailed) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::BAD_GATEWAY.as_u16())),

                    // PHP errors that we want to convey directly
                    GruxiErrorKind::PHPProcessor(PHPProcessorError::PathError(_)) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16())),
                    GruxiErrorKind::PHPProcessor(PHPProcessorError::Timeout) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::GATEWAY_TIMEOUT.as_u16())),
                    GruxiErrorKind::PHPProcessor(PHPProcessorError::Connection) => Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::BAD_GATEWAY.as_u16())),

                    // Other errors we have logged, but will continue to the next handler
                    _ => response_result,
                }
            }
        }
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
    fn test_request_handler_matches_url_wildcard_files() {
        let mut handler = create_valid_handler();
        handler.url_match = vec!["*.php".to_string()];

        assert!(handler.matches_url("/any/path/index.php"));
        assert!(!handler.matches_url("/php"));
        assert!(handler.matches_url("/index.php"));
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
        let mut handler = RequestHandler::new();
        handler.processor = Processor::Static(StaticFileProcessor::new("./www-default".into(), vec![]));
        handler
    }
}
