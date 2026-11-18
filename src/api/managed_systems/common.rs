use crate::api::common::json_error;
use crate::error;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{GruxiErrorKind, ManagedSystemErrorKind};
use crate::external_connections::managed_system::php_cgi::PhpCgi;
use crate::http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse};
use hyper::StatusCode;

pub(super) async fn read_managed_system(request: &mut GruxiRequest) -> Result<PhpCgi, GruxiResponse> {
    if request.get_path() != "/api/managed-systems" {
        return Err(json_error(StatusCode::NOT_FOUND, "Managed system not found"));
    }
    let body = request.get_body_bytes().await;
    let mut system: PhpCgi = serde_json::from_slice(&body).map_err(|_| json_error(StatusCode::BAD_REQUEST, "Invalid JSON"))?;
    system.sanitize();
    system.validate().map_err(|errors| json_error(StatusCode::BAD_REQUEST, &errors.join(", ")))?;
    Ok(system)
}

pub(super) fn change_error(error: GruxiError) -> GruxiResponse {
    match &error.kind {
        GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::NotFound) => json_error(StatusCode::NOT_FOUND, "Managed system not found"),
        GruxiErrorKind::ManagedSystem(ManagedSystemErrorKind::Conflict) => json_error(StatusCode::CONFLICT, "Managed system with same id already exists"),
        _ => {
            error!("Managed systems API failed: {:?}", error);
            json_error(StatusCode::INTERNAL_SERVER_ERROR, "Managed system operation failed")
        }
    }
}
