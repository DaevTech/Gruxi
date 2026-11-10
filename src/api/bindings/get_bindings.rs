use crate::api::common::json_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use hyper::StatusCode;

pub async fn handle(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    if let Some(id) = gruxi_request.get_path().strip_prefix("/api/bindings/") {
        if id.is_empty() || id.contains('/') {
            return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
        }
        get(id).await
    } else {
        get_all().await
    }
}

async fn get(_id: &str) -> Result<GruxiResponse, GruxiError> {
    Ok(json_error(StatusCode::NOT_IMPLEMENTED, "Not implemented yet"))
}

async fn get_all() -> Result<GruxiResponse, GruxiError> {
    Ok(json_error(StatusCode::NOT_IMPLEMENTED, "Not implemented yet"))
}
