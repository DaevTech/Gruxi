use crate::{
    configuration::{request_handler::RequestHandler, site::Site},
    core::running_state_manager::get_running_state_manager,
    external_request_handlers::{php_handler::PHPHandler},
    http::http_util::empty_response_with_status,
};
use http_body_util::combinators::BoxBody;
use hyper::Response;
use hyper::body::Bytes;
use log::{debug, error};
use std::collections::HashMap;

pub struct ExternalRequestHandlers {
    pub id_to_type: HashMap<String, String>,
    pub php: HashMap<String, PHPHandler>,
}

// Supported rewrite functions
pub static REWRITE_FUNCTIONS: &[&str] = &["OnlyWebRootIndexForSubdirs"];

// A trait for external request handlers
#[allow(async_fn_in_trait)]
pub trait ExternalRequestHandler {
    fn start(&self);
    fn stop(&self);
    fn get_file_matches(&self) -> Vec<String>;
    async fn handle_request(
        &self,
        method: &hyper::Method,
        uri: &hyper::Uri,
        headers: &hyper::HeaderMap,
        body: &Vec<u8>,
        site: &Site,
        full_file_path: &String,
        remote_ip: &str,
        http_version: &String,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error>;
    fn get_handler_type(&self) -> String;
}

impl ExternalRequestHandlers {
    pub async fn new() -> Self {
        // Get the config, to determine what we need
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        // Run through all the configured sites in configuration and determine which is actually referenced
        let mut handler_ids_used = HashMap::new();

        for site in &config.sites {
            for handler in &site.enabled_handlers {
                if !handler_ids_used.contains_key(handler) {
                    handler_ids_used.insert(handler.clone(), true);
                }
            }
        }

        debug!("Enabled external request handlers found in configuration: {:?}", handler_ids_used);

        // Go through our configured handlers and check they are enabled
        let mut handler_type_to_load: HashMap<String, RequestHandler> = HashMap::new();

        let external_handlers: &Vec<RequestHandler> = &config.request_handlers;
        for handler in external_handlers {
            if handler.is_enabled {
                // Check if the handler is in our enabled list
                if handler_ids_used.contains_key(&handler.id) {
                    if !handler_type_to_load.contains_key(&handler.id) {
                        handler_type_to_load.insert(handler.id.clone(), handler.clone());
                    }
                }
            }
        }

        debug!("Enabled external request handler types found in configuration: {:?}", handler_type_to_load);

        // Start the handlers with the type we want
        let mut php = HashMap::new();
        let mut id_to_type = HashMap::new();

        for (handler_id, handler) in handler_type_to_load {
            // Determine the concurrent threads. Can be set in config or we determine it based on CPU cores
            // 0 = automatically based on CPU cores
            let concurrent_threads = if handler.concurrent_threads == 0 {
                let cpus = num_cpus::get_physical();
                cpus
            } else if handler.concurrent_threads < 1 {
                1
            } else {
                handler.concurrent_threads
            };

            match handler.handler_type.as_str() {
                "php" => {
                    let php_handler = PHPHandler::new(
                        handler.executable.clone(),
                        handler.ip_and_port.clone(),
                        handler.request_timeout,
                        concurrent_threads,
                        handler.other_webroot.clone(),
                        handler.extra_handler_config,
                        handler.extra_environment,
                    );
                    php_handler.start();
                    debug!("PHP handler with id {} started and added to external request handlers.", &handler_id);
                    id_to_type.insert(handler_id.clone(), "php".to_string());
                    php.insert(handler_id.clone(), php_handler);
                }
                _ => {
                    error!("Unknown handler type in config: {}", handler.handler_type);
                }
            }
        }
        ExternalRequestHandlers { php, id_to_type }
    }

    // Check if handler is relevant for a request
    pub async fn is_handler_relevant(&self, handler_id: &str, full_file_path: &String) -> bool {
        let running_state_manager = get_running_state_manager().await;
        let running_state = running_state_manager.get_running_state();
        let unlocked_running_state = running_state.read().await;
        let external_request_handlers = unlocked_running_state.get_external_request_handlers();
        let handlers = external_request_handlers.read().await;

        // Get the handler type of the id, then call the appropriate handler
        let handler_type = match handlers.id_to_type.get(handler_id) {
            Some(handler_type) => handler_type,
            None => return false,
        };

        // For each type, we fetch the handler and call its handle_request method
        match handler_type.as_str() {
            "php" => {
                if let Some(php_handler) = self.php.get(handler_id) {
                    for file_match in php_handler.get_file_matches() {
                        if full_file_path.ends_with(&file_match) {
                            return true;
                        }
                    }
                }
            }
            _ => {
                error!("Unknown handler type: {}", handler_type);
            }
        }

        false
    }

    // Handle an external request
    pub async fn handle_external_request(
        &self,
        handler_id: &str,
        method: &hyper::Method,
        uri: &hyper::Uri,
        headers: &hyper::HeaderMap,
        body: &Vec<u8>,
        site: &Site,
        full_file_path: &String,
        remote_ip: &str,
        http_version: &String,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
        let running_state_manager = get_running_state_manager().await;
        let running_state = running_state_manager.get_running_state();
        let unlocked_running_state = running_state.read().await;
        let external_request_handlers = unlocked_running_state.get_external_request_handlers();
        let handlers = external_request_handlers.read().await;

        // Get the handler type of the id, then call the appropriate handler
        let handler_type = match handlers.id_to_type.get(handler_id) {
            Some(handler_type) => handler_type,
            None => return Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR)),
        };

        // For each type, we fetch the handler and call its handle_request method
        match handler_type.as_str() {
            "php" => {
                if let Some(php_handler) = self.php.get(handler_id) {
                    php_handler.handle_request(method, uri, headers, body, site, full_file_path, remote_ip, http_version).await
                } else {
                    error!("PHP handler with id {} not found.", handler_id);
                    Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR))
                }
            }
            _ => {
                error!("Unknown handler type: {}", handler_type);
                Ok(empty_response_with_status(hyper::StatusCode::INTERNAL_SERVER_ERROR))
            }
        }
    }
}
