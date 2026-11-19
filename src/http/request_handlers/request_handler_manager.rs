use std::{collections::HashMap, sync::Arc};

use crate::{
    config::{request_handler::RequestHandler, site::Site}, error::gruxi_error::GruxiError, http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse}, runtime::binding_manager::BindingRuntime, trace,
};
use crate::http::request_handlers::processors::processor_manager::ProcessorManager;

pub struct RequestHandlerManager {
    pub request_handlers: Arc<HashMap<String, RequestHandler>>,
}

impl RequestHandlerManager {
    pub async fn new() -> Self {
        // Get the config, to determine what we need
        let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration();

        Self::from_handlers(config.sites.iter().flat_map(|site| site.request_handlers.iter()))
    }

    pub fn from_handlers<'a>(handlers: impl Iterator<Item = &'a RequestHandler>) -> Self {
        Self { request_handlers: Arc::new(handlers.map(|handler| (handler.id.clone(), handler.clone())).collect()) }
    }

    pub async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site, binding_runtime: Arc<BindingRuntime>, processors: &ProcessorManager) -> Result<GruxiResponse, GruxiError> {
        let lowercased_path = gruxi_request.get_path().to_lowercase();
        let mut found = false;
        for handler in &site.request_handlers {
                found = true;
                    // Check if enabled
                    if !handler.is_enabled {
                        continue;
                    }

                    // Check that it matches
                    if handler.matches_url(&lowercased_path) {
                        // We call the handle request. If we get an error, we continue to the next one
                        let response_result = handler.handle_request(gruxi_request, site, &binding_runtime, processors).await;
                        if response_result.is_err() {
                            // Some of the errors are not critical, so we just log and continue
                            continue;
                        }
                        return response_result;
            }
        }
        if !found {
            trace!("No request handler found for request path '{}'", lowercased_path);
        }
        Ok(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_FOUND.as_u16()))
    }
}
