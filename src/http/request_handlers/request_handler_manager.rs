use std::{collections::HashMap, sync::Arc};

use crate::{
    config::{request_handler::RequestHandler, site::Site},
    error::gruxi_error::GruxiError,
    http::{
        http_server::ConnectionContext,
        request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
    },
    trace,
};

pub struct RequestHandlerManager {
    pub request_handlers: Arc<HashMap<String, Vec<RequestHandler>>>,
}

impl RequestHandlerManager {
    pub async fn new() -> Self {
        // Get the config, to determine what we need
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();

        // Make a hashmap of the request handlers, so we can easily access them by id, before matching with site
        let mut request_handlers = HashMap::new();
        for handler in &config.request_handlers {
            request_handlers.insert(handler.id.clone(), handler.clone());
        }

        let mut new_request_handlers = HashMap::new();
        for site in &config.sites {
            for request_handler_id in &site.request_handlers {
                if let Some(handler) = request_handlers.get(request_handler_id) {
                    new_request_handlers.entry(site.id.clone()).or_insert_with(Vec::new).push(handler.clone());
                }
            }
        }

        RequestHandlerManager {
            request_handlers: Arc::new(new_request_handlers),
        }
    }

    pub async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site, connection_context: &ConnectionContext) -> Result<GruxiResponse, GruxiError> {
        let rh = self.request_handlers.get(&site.id);
        let lowercased_path = gruxi_request.get_path().to_lowercase();
        match rh {
            Some(handlers) => {
                for handler in handlers.iter() {
                    // Check if enabled
                    if !handler.is_enabled {
                        continue;
                    }

                    // Check that it matches
                    if handler.matches_url(&lowercased_path) {
                        // We call the handle request. If we get an error, we continue to the next one
                        let response_result = handler.handle_request(gruxi_request, site, connection_context).await;
                        if response_result.is_err() {
                            // Some of the errors are not critical, so we just log and continue
                            continue;
                        }
                        return response_result;
                    }
                }
                Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()))
            }
            None => {
                trace!("No request handler found for request path '{}'", lowercased_path);
                Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()))
            }
        }
    }
}
