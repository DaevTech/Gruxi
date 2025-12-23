use hyper::{Response, body::Bytes};
use uuid::Uuid;
use crate::{configuration::site::Site, http::{http_util::empty_response_with_status, request_handlers::processor_trait::ProcessorTrait, requests::grux_request::GruxRequest}};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyProcessor {
    pub id: String, // Unique identifier for the processor
}

impl ProxyProcessor {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
        }
    }
}

impl ProcessorTrait for ProxyProcessor {
    fn sanitize(&mut self) {}

    fn validate(&self) -> Result<(), Vec<String>> {
        let errors = Vec::new();

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn handle_request(&self, _grux_request: &mut GruxRequest, _site: &Site) -> Result<Response<http_body_util::combinators::BoxBody<Bytes, hyper::Error>>, ()> {
        // Implementation for handling proxy requests
        return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
    }

    fn get_type(&self) -> String {
        "proxy".to_string()
    }

    fn get_default_pretty_name(&self) -> String {
        "Proxy Processor".to_string()
    }
}
