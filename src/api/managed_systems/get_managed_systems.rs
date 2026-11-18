use crate::api::common::json_response;
use crate::api::managed_systems::common::change_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use crate::runtime::managed_systems_manager::get_managed_systems_manager;
use hyper::StatusCode;

pub async fn handle(request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let systems = match get_managed_systems_manager().await.lock().await.get_configuration_managed_systems() {
        Ok(systems) => systems,
        Err(error) => return Ok(change_error(error)),
    };
    if let Some(id) = request.get_path().strip_prefix("/api/managed-systems/") {
        if id.is_empty() || id.contains('/') {
            return Ok(crate::api::common::json_error(StatusCode::NOT_FOUND, "Managed system not found"));
        }
        return match systems.iter().find(|system| system.id == id) {
            Some(system) => Ok(json_response(StatusCode::OK, system)),
            None => Ok(crate::api::common::json_error(StatusCode::NOT_FOUND, "Managed system not found")),
        };
    }
    Ok(json_response(StatusCode::OK, &systems))
}
