use crate::api::common::*;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use hyper::StatusCode;


pub async fn handle(gruxi_request: &mut GruxiRequest) -> Result<GruxiResponse, GruxiError> {
    let _body = gruxi_request.get_body_bytes().await;

    Ok(json_error(StatusCode::NOT_IMPLEMENTED, "Not implemented yet"))
}
