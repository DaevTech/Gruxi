use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::{
    configuration::{request_handler::RequestHandler, site::Site},
    error::grux_error::GruxError,
    http::request_response::{grux_request::GruxRequest, grux_response::GruxResponse},
    logging::syslog::trace,
};

pub struct RequestHandlerManager {
    pub request_handlers: Arc<RwLock<HashMap<String, RequestHandler>>>,
}

impl RequestHandlerManager {
    pub async fn new() -> Self {
        let initial_request_handlers = Self::get_request_handlers_from_configuration().await;

        RequestHandlerManager {
            request_handlers: Arc::new(RwLock::new(initial_request_handlers)),
        }
    }

    async fn get_request_handlers_from_configuration() -> HashMap<String, RequestHandler> {
        // Get the config, to determine what we need
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;

        let mut new_request_handlers = HashMap::new();

        for handler in &config.request_handlers {
            new_request_handlers.insert(handler.id.clone(), handler.clone());
        }
        new_request_handlers
    }

    pub async fn handle_request(&self, grux_request: &mut GruxRequest, site: &Site) -> Result<GruxResponse, GruxError> {
        let request_handler_read_lock = self.request_handlers.read().await;

        for request_handler_id in site.request_handlers.iter() {
            if let Some(handler) = request_handler_read_lock.get(request_handler_id) {
                // Check if enabled
                if !handler.is_enabled {
                    continue;
                }

                // Check that it matches
                if handler.matches_url(&grux_request.get_path_and_query()) {
                    // We call the handle request. If we get an error, we continue to the next one
                    let response_result = handler.handle_request(grux_request, site).await;
                    if response_result.is_err() {
                        // Some of the errors are not critical, so we just log and continue
                        continue;
                    }
                    return response_result;
                }
            }
        }

        trace(format!("No request handler found for request path '{}'", &grux_request.get_path_and_query()));
        Ok(GruxResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()))
    }
}
