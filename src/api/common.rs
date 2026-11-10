use http::HeaderValue;
use crate::error;
use serde::Deserialize;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{GruxiErrorKind, AdminApiError};
use crate::http::request_response::gruxi_response::GruxiResponse;
use tokio_util::bytes::Bytes;

pub fn no_route_matched() -> GruxiError {
    GruxiError::new_with_kind_only(GruxiErrorKind::AdminApi(AdminApiError::NoRouteMatched))
}

pub fn json_response(status: hyper::StatusCode, value: &impl serde::Serialize) -> GruxiResponse {
    let body = match serde_json::to_vec(value) {
        Ok(body) => body,
        Err(e) => {
            error!("Failed to serialize API response: {}", e);
            let mut response = GruxiResponse::new_with_bytes(
                hyper::StatusCode::INTERNAL_SERVER_ERROR.as_u16(),
                Bytes::from(r#"{"error":"Failed to serialize response"}"#),
            );
            response.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
            return response;
        }
    };
    let mut response = GruxiResponse::new_with_bytes(status.as_u16(), Bytes::from(body));
    response.headers_mut().insert("Content-Type", HeaderValue::from_static("application/json"));
    response
}

pub fn json_error(status: hyper::StatusCode, message: &str) -> GruxiResponse {
    json_response(status, &serde_json::json!({ "error": message }))
}

pub fn parse_body<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, GruxiResponse> {
    serde_json::from_slice(body).map_err(|e| {
        error!("Failed to parse binding request JSON: {}", e);
        json_response(hyper::StatusCode::BAD_REQUEST, &serde_json::json!({ "error": "Invalid JSON format", "details": e.to_string() }))
    })
}
