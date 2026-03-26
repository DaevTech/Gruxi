use crate::config::site::Site;
use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{GruxiErrorKind, TelemetryApiError};
use crate::file::normalized_path::NormalizedPath;
use crate::http::http_server::ConnectionContext;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::telemetry::metrics_exporter;
use http::HeaderValue;
use std::sync::Arc;
use tokio_util::bytes;

const TEXT_PLAIN_PROM_HEADER_VALUE: HeaderValue =
    HeaderValue::from_static("text/plain; version=0.0.4; charset=utf-8");
const TEXT_PLAIN_HEADER_VALUE: HeaderValue = HeaderValue::from_static("text/plain");
const JSON_HEADER_VALUE: HeaderValue = HeaderValue::from_static("application/json");

pub async fn handle_telemetry_routes(
    gruxi_request: &mut GruxiRequest,
    _site: &Site,
    _connection_context: &Arc<ConnectionContext>,
) -> Result<GruxiResponse, GruxiError> {
    let path = gruxi_request.get_path();
    let method = gruxi_request.get_http_method();

    let normalized_path_result = NormalizedPath::new("", path);
    let normalized_path = match normalized_path_result {
        Ok(np) => np,
        Err(_) => {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::TelemetryApi(
                TelemetryApiError::NoRouteMatched,
            )));
        }
    };
    let path_cleaned = normalized_path.get_path();

    if path_cleaned == "/metrics" && method == "GET" {
        handle_metrics_endpoint(gruxi_request).await
    } else if path_cleaned == "/healthcheck" && method == "GET" {
        handle_healthcheck_endpoint().await
    } else {
        Err(GruxiError::new_with_kind_only(GruxiErrorKind::TelemetryApi(
            TelemetryApiError::NoRouteMatched,
        )))
    }
}

async fn handle_metrics_endpoint(
    gruxi_request: &GruxiRequest,
) -> Result<GruxiResponse, GruxiError> {
    // Get the configured bearer token
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let configuration = cached_configuration.get_configuration();
    let configured_token = &configuration.core.telemetry.bearer_token;

    let configured_token = match configured_token {
        Some(token) if !token.is_empty() => token,
        _ => {
            // No token configured — metrics endpoint not available
            let mut response = GruxiResponse::new_with_bytes(
                hyper::StatusCode::SERVICE_UNAVAILABLE.as_u16(),
                bytes::Bytes::from(r#"{"error": "Metrics endpoint not configured"}"#),
            );
            response
                .headers_mut()
                .insert("Content-Type", JSON_HEADER_VALUE);
            return Ok(response);
        }
    };

    // Extract bearer token from request
    let request_token = extract_bearer_token(gruxi_request);
    match request_token {
        Some(token) if constant_time_eq(token.as_bytes(), configured_token.as_bytes()) => {
            // Token valid — render metrics
        }
        _ => {
            let mut response = GruxiResponse::new_with_bytes(
                hyper::StatusCode::UNAUTHORIZED.as_u16(),
                bytes::Bytes::from(r#"{"error": "Unauthorized"}"#),
            );
            response
                .headers_mut()
                .insert("Content-Type", JSON_HEADER_VALUE);
            return Ok(response);
        }
    }

    let metrics_text = metrics_exporter::render_metrics().await;

    let mut response = GruxiResponse::new_with_bytes(
        hyper::StatusCode::OK.as_u16(),
        bytes::Bytes::from(metrics_text),
    );
    response
        .headers_mut()
        .insert("Content-Type", TEXT_PLAIN_PROM_HEADER_VALUE);
    Ok(response)
}

async fn handle_healthcheck_endpoint() -> Result<GruxiResponse, GruxiError> {
    let mut response = GruxiResponse::new_with_bytes(
        hyper::StatusCode::OK.as_u16(),
        bytes::Bytes::from("The server is healthy"),
    );
    response
        .headers_mut()
        .insert("Content-Type", TEXT_PLAIN_HEADER_VALUE);
    Ok(response)
}

fn extract_bearer_token(gruxi_request: &GruxiRequest) -> Option<String> {
    if let Some(auth_header) = gruxi_request.get_headers().get("Authorization")
        && let Ok(auth_str) = auth_header.to_str()
        && auth_str.starts_with("Bearer ")
    {
        return Some(auth_str[7..].to_string());
    }
    None
}

/// Constant-time byte comparison to prevent timing attacks on token validation.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}
