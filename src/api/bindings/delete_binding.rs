use crate::api::common::json_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::runtime::binding_manager::get_binding_manager;
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

    println!("Binding ID to delete: {}", binding_id);
    println!("Loaded bindings from database: {:?}", current_bindings);

    // Make sure it exist and fetch it to be deleted
    if !current_bindings.iter().any(|b| b.id == binding_id) {
        return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
    }
    let binding = current_bindings.iter().find(|b| b.id == binding_id).unwrap();

    // We remove the binding from the manager first to stop it from running
    binding_manager.remove_binding(&binding.id);

    // Now we delete the binding from the configuration/database
    match binding.delete() {
        Ok(_) => {}
        Err(e) => {
            return Ok(json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to delete binding with id {}: {:?}", &binding.id, e)));
        }
    }

    Ok(json_error(StatusCode::OK, "Binding deleted successfully"))
}
