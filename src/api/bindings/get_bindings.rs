use crate::api::common::{json_error, json_response};
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::runtime::binding_manager::get_binding_manager;
use hyper::StatusCode;

pub async fn handle(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    if let Some(id) = gruxi_request.get_path().strip_prefix("/api/bindings/") {
        if id.is_empty() || id.contains('/') {
            return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
        }
        get(id).await
    } else {
        get_all().await
    }
}

async fn get(id: &str) -> Result<GruxiResponse, GruxiError> {
    let binding_manager = match get_binding_manager().lock() {
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

    if let Some(binding) = current_bindings.iter().find(|binding| binding.id == id) {
        if binding.is_admin || binding.is_telemetry {
            return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
        }
        Ok(json_response(StatusCode::OK, &binding))
    } else {
        Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"))
    }
}

async fn get_all() -> Result<GruxiResponse, GruxiError> {
    let binding_manager = match get_binding_manager().lock() {
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

    // Return elements, but only if they are not admin or telemetry
    let bindings: Vec<_> = current_bindings.iter().filter(|binding| !binding.is_admin && !binding.is_telemetry).collect();

    Ok(json_response(hyper::StatusCode::OK, &bindings))
}
