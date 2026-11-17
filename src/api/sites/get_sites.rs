use crate::api::common::{json_error, json_response};
use crate::api::sites::common::change_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use crate::runtime::site_manager::get_site_manager;
use hyper::StatusCode;

pub async fn handle(request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let path = request.get_path();
    let id = path.strip_prefix("/api/sites/").filter(|id| !id.is_empty() && !id.contains('/'));
    if path != "/api/sites" && id.is_none() {
        return Ok(json_error(StatusCode::NOT_FOUND, "Site not found"));
    }
    let sites = match get_site_manager().get_configuration_sites() {
        Ok(sites) => sites,
        Err(error) => return Ok(change_error(error)),
    };
    if let Some(id) = id {
        Ok(match sites.iter().find(|site| site.id == id) {
            Some(site) => json_response(StatusCode::OK, site),
            None => json_error(StatusCode::NOT_FOUND, "Site not found"),
        })
    } else {
        Ok(json_response(StatusCode::OK, &sites))
    }
}
