use crate::api::common::json_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use hyper::StatusCode;

pub async fn handle(_gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    Ok(json_error(StatusCode::NOT_IMPLEMENTED, "Not implemented yet"))
}
