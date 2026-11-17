use crate::api::common::json_response;
use crate::api::sites::common::{change_error, read_site};
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use crate::runtime::site_manager::get_site_manager;
use hyper::StatusCode;

pub async fn handle(request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let site = match read_site(request).await {
        Ok(site) => site,
        Err(response) => return Ok(response),
    };
    Ok(match get_site_manager().update_site(site.clone()) {
        Ok(()) => json_response(StatusCode::OK, &site),
        Err(error) => change_error(error),
    })
}
