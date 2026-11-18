use crate::api::common::json_response;
use crate::api::managed_systems::common::{change_error, read_managed_system};
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use crate::runtime::managed_systems_manager::get_managed_systems_manager;
use hyper::StatusCode;

pub async fn handle(request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let system = match read_managed_system(request).await {
        Ok(system) => system,
        Err(response) => return Ok(response),
    };
    let result = get_managed_systems_manager().await.lock().await.update_managed_system(&system).await;
    if let Err(error) = result {
        return Ok(change_error(error));
    }
    Ok(json_response(StatusCode::OK, &system))
}
