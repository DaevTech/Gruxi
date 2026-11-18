use crate::api::common::json_error;
use crate::config::site::Site;
use crate::error;
use crate::error::{gruxi_error::GruxiError, gruxi_error_enums::{GruxiErrorKind, SiteError}};
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use hyper::StatusCode;

pub(super) fn change_error(error: GruxiError) -> GruxiResponse {
    match error.kind {
        GruxiErrorKind::Site(SiteError::NotFound) => json_error(StatusCode::NOT_FOUND, "Site not found"),
        GruxiErrorKind::Site(SiteError::BindingNotFound) => json_error(StatusCode::NOT_FOUND, "Binding not found"),
        GruxiErrorKind::Site(SiteError::Conflict) => json_error(StatusCode::CONFLICT, "Site with same id already exists"),
        GruxiErrorKind::Site(SiteError::Referenced) => json_error(StatusCode::CONFLICT, "Site is referenced by a binding"),
        _ => {
            error!("Site API failed: {:?}", error);
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "Site operation failed")
        }
    }
}

pub(super) async fn read_site(request: &mut GruxiRequest) -> Result<Site, GruxiResponse> {
    if request.get_path() != "/api/sites" {
        return Err(json_error(StatusCode::NOT_FOUND, "Site not found"));
    }
    let body = request.get_body_bytes().await;
    let mut site: Site = serde_json::from_slice(&body).map_err(|_| json_error(StatusCode::BAD_REQUEST, "Invalid JSON"))?;
    site.sanitize();
    site.validate().map_err(|errors| json_error(StatusCode::BAD_REQUEST, &errors.join(", ")))?;
    Ok(site)
}
