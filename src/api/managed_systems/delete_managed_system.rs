use crate::api::common::json_error;
use crate::api::managed_systems::common::change_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use crate::runtime::managed_systems_manager::get_managed_systems_manager;
use hyper::StatusCode;

pub async fn handle(request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let Some(id) = request.get_path().strip_prefix("/api/managed-systems/").filter(|id| !id.is_empty() && !id.contains('/')) else {
        return Ok(json_error(StatusCode::NOT_FOUND, "Managed system not found"));
    };
    let result = get_managed_systems_manager().await.lock().await.delete_managed_system(id).await;
    if let Err(error) = result {
        return Ok(change_error(error));
    }
    Ok(json_error(StatusCode::OK, "Managed system deleted successfully"))
}
