use crate::api::common::*;
use crate::config::binding::Binding;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::runtime::binding_manager::{BindingRuntimeConfiguration, get_binding_manager};
use hyper::StatusCode;

pub async fn handle(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let body = gruxi_request.get_body_bytes().await;

    // Map from json to Binding
    let binding: Result<Binding, _> = serde_json::from_slice(&body);
    if let Err(_) = binding {
        return Ok(json_error(StatusCode::BAD_REQUEST, "Invalid JSON"));
    }
    let binding = binding.unwrap();

    // Validate the content
    if let Err(errors) = binding.validate() {
        return Ok(json_error(StatusCode::BAD_REQUEST, &errors.join(", ")));
    }

    let mut binding_manager = match get_binding_manager().lock() {
        Ok(m) => m,
        Err(_) => {
            return Ok(json_error(StatusCode::INTERNAL_SERVER_ERROR, "Binding manager lock poisoned"));
        }
    };

    let current_bindings = match binding_manager.get_configuration_bindings() {
        Ok(b) => b,
        Err(_) => {
            return Ok(json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to get bindings from database"));
        }
    };

    // Make sure it exist
    if !current_bindings.iter().any(|b| b.id == binding.id) {
        return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
    }

    // Save the updated binding to the configuration
    match binding.save() {
        Ok(_) => {}
        Err(e) => {
            return Ok(json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to save binding: {:?}", e)));
        }
    }

    // Restart the binding through the shared binding manager.
    let runtime_config = BindingRuntimeConfiguration::default();

    // We try to add the new binding to the manager and start it, but accept failure
    match binding_manager.add_binding(binding.clone(), runtime_config) {
        Ok(_) => {}
        Err(_) => {
            // Accept if it fails for whatever reason
        }
    }

    Ok(json_response(hyper::StatusCode::OK, &binding))
}
