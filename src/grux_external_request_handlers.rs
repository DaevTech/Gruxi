use crate::{
    grux_configuration::get_configuration,
    grux_configuration_struct::{RequestHandler, Server, Site},
    grux_external_request_handlers::grux_handler_php::PHPHandler,
};
use http_body_util::combinators::BoxBody;
use hyper::Response;
use hyper::body::Bytes;
use log::debug;
use std::{collections::HashMap, sync::OnceLock};
pub mod grux_handler_php;

pub struct ExternalRequestHandlers {
    handlers: HashMap<String, Box<dyn ExternalRequestHandler>>,
}

// Supported rewrite functions
pub static REWRITE_FUNCTIONS: &[&str] = &["OnlyWebRootIndexForSubdirs"];

// A trait for external request handlers
pub trait ExternalRequestHandler: Send + Sync {
    fn start(&self);
    fn stop(&self);
    fn get_file_matches(&self) -> Vec<String>;
    fn handle_request(
        &self,
        method: &hyper::Method,
        uri: &hyper::Uri,
        headers: &hyper::HeaderMap,
        body: Vec<u8>,
        site: &Site,
        full_file_path: &String,
        remote_ip: &String,
        http_version: &String,
    ) -> Response<BoxBody<Bytes, hyper::Error>>;
    fn get_handler_type(&self) -> String;
}

impl ExternalRequestHandlers {
    pub fn new() -> Self {
        let handlers: HashMap<String, Box<dyn ExternalRequestHandler>> = HashMap::new();
        ExternalRequestHandlers { handlers }
    }
}

// Handles external request handlers and their thread pools, such as PHP
fn start_external_request_handlers() -> Result<ExternalRequestHandlers, String> {
    // Get the config, to determine what we need
    let config = get_configuration();

    // Run through all the configured sites in configuration and determine which is actually referenced
    let servers: Vec<Server> = config.get("servers").unwrap();
    let mut handler_ids_used = HashMap::new();

    for server in servers {
        for binding in server.bindings {
            for site in binding.sites {
                for handler in &site.enabled_handlers {
                    if !handler_ids_used.contains_key(handler) {
                        handler_ids_used.insert(handler.clone(), true);
                    }
                }
            }
        }
    }

    debug!("Enabled external request handlers found in configuration: {:?}", handler_ids_used);

    // Load our implemented handlers, so they can be matched with what is configured
    let mut external_request_handlers = ExternalRequestHandlers::new();

    // Go through our configured handlers and load the ones we need
    let mut handler_type_to_load: HashMap<String, RequestHandler> = HashMap::new();

    let external_handlers: Vec<RequestHandler> = config.get("request_handlers").unwrap();
    for handler in external_handlers {
        if handler.is_enabled {
            // Check if the handler is in our enabled list
            if handler_ids_used.contains_key(&handler.id) {
                if !handler_type_to_load.contains_key(&handler.handler_type) {
                    handler_type_to_load.insert(handler.handler_type.clone(), handler);
                }
            }
        }
    }

    debug!("Enabled external request handler types found in configuration: {:?}", handler_type_to_load);

    // Start the handlers with the type we want
    for (handler_type, handler) in handler_type_to_load {
        // Determine the concurrent threads. Can be set in config or we determine it based on CPU cores
        // 0 = automatically based on CPU cores
        let mut concurrent_threads = if handler.concurrent_threads == 0 {
            let cpus = num_cpus::get_physical();
            cpus
        } else if handler.concurrent_threads < 1 {
            1
        } else {
            handler.concurrent_threads
        };
        if concurrent_threads > 3 {
            concurrent_threads -= 1;
        }

        match handler_type.as_str() {
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
                external_request_handlers.handlers.insert(handler.id.clone(), Box::new(php_handler));
                debug!("PHP handler started and added to external request handlers.");
            }
            _ => {
                debug!("Unknown handler type: {}", handler_type);
            }
        }
    }

    Ok(external_request_handlers)
}

// Get the request handlers
pub fn get_request_handlers() -> &'static ExternalRequestHandlers {
    static HANDLERS: OnceLock<ExternalRequestHandlers> = OnceLock::new();
    HANDLERS.get_or_init(|| start_external_request_handlers().unwrap_or_else(|e| panic!("Failed to start request handlers: {}", e)))
}

// Get a request handler with a certain id
pub fn get_request_handler_by_id(id: &str) -> Option<&'static dyn ExternalRequestHandler> {
    let handlers = get_request_handlers();
    handlers.handlers.get(id).map(|h| h.as_ref())
}
