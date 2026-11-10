use crate::api::bindings::*;
use crate::api::common::no_route_matched;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::admin_portal::http_admin_api::{require_authentication, require_authenticated_action};
use crate::api::common::json_error;

pub async fn handle_routes(gruxi_request: &mut GruxiRequest, path: &str) -> Result<GruxiResponse, GruxiError> {
    let method = gruxi_request.get_http_method();

    let auth_result = if method == "GET" {
        require_authentication(gruxi_request).await
    } else {
        require_authenticated_action(gruxi_request).await
    };
    match auth_result {
        Ok(Some(_)) => {}
        Ok(None) => return Ok(json_error(hyper::StatusCode::UNAUTHORIZED, "Authentication required")),
        Err(response) => return Ok(response),
    }

    if path.starts_with("/api/bindings") {
        return match method {
            "GET" => get_bindings::handle(gruxi_request).await,
            "POST" => create_binding::handle(gruxi_request).await,
            "PATCH" => update_binding::patch(gruxi_request).await,
            "PUT" => update_binding::replace(gruxi_request).await,
            "DELETE" => delete_binding::handle(gruxi_request).await,
            _ => Err(no_route_matched()),
        };
    }

    // Default, unknown route
    Err(no_route_matched())
}
