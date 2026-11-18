use crate::api::common::json_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::runtime::binding_manager::get_binding_manager;
use crate::runtime::site_manager::get_site_manager;
use hyper::StatusCode;

pub async fn handle(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    // Get the id from url
    let binding_id = match gruxi_request.get_path().strip_prefix("/api/bindings/") {
        Some(id) if !id.is_empty() && !id.contains('/') => id.to_string(),
        _ => return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found")),
    };

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

    // Make sure it exist and fetch it to be deleted
    if !current_bindings.iter().any(|b| b.id == binding_id) {
        return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
    }
    let binding = current_bindings.iter().find(|b| b.id == binding_id).unwrap();

    match get_site_manager().delete_binding(binding) {
        Ok(_) => {}
        Err(e) => {
            return Ok(json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to delete binding with id {}: {:?}", &binding.id, e)));
        }
    }

    binding_manager.remove_binding(&binding.id);
    Ok(json_error(StatusCode::OK, "Binding deleted successfully"))
}
