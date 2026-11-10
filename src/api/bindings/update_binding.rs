use crate::api::common::*;
use crate::config::binding::Binding;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use hyper::StatusCode;

pub async fn patch(_gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    /*
    let mut configuration = match load_configuration().await {
        Ok(configuration) => configuration,
        Err(response) => return Ok(response),
    };
    let Some(id) = gruxi_request.get_path().strip_prefix("/api/bindings/") else {
        return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
    };
    if id.is_empty() || id.contains('/') {
        return Ok(json_error(StatusCode::NOT_FOUND, "Binding not found"));
    }
     */
    Ok(json_error(StatusCode::NOT_IMPLEMENTED, "Not implemented yet"))
}

pub async fn replace(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let body = gruxi_request.get_body_bytes().await;

    // Parse body into Binding directly
    let binding: Binding = match parse_body(&body) {
        Ok(binding) => binding,
        Err(response) => return Ok(response),
    };

    // Validate the binding before proceeding
    if binding.validate().is_err() {
        return Ok(json_error(StatusCode::BAD_REQUEST, "Invalid binding"));
    }

    Ok(json_error(StatusCode::NOT_IMPLEMENTED, "Not implemented yet"))
}
