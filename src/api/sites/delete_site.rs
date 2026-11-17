use crate::api::common::json_error;
use crate::api::sites::common::change_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use crate::runtime::site_manager::get_site_manager;
use hyper::StatusCode;

pub async fn handle(request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let Some(id) = request.get_path().strip_prefix("/api/sites/").filter(|id| !id.is_empty() && !id.contains('/')) else {
        return Ok(json_error(StatusCode::NOT_FOUND, "Site not found"));
    };
    Ok(match get_site_manager().delete_site(id) {
        Ok(()) => json_error(StatusCode::OK, "Site deleted successfully"),
        Err(error) => change_error(error),
    })
}
