use crate::api::common::{json_error, json_response};
use crate::api::sites::common::change_error;
use crate::error::gruxi_error::GruxiError;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::runtime::site_manager::get_site_manager;
use hyper::StatusCode;

pub async fn handle(segments: &[&str], method: &str) -> Result<GruxiResponse, GruxiError> {
    if segments[3].is_empty() {
        return Ok(json_error(StatusCode::NOT_FOUND, "Site not found"));
    }
    let manager = get_site_manager();
    match (method, segments) {
        ("GET", [_, _, _, site_id, "bindings"]) => Ok(match manager.get_binding_ids_for_site(site_id) {
            Ok(ids) => json_response(StatusCode::OK, &ids),
            Err(error) => change_error(error),
        }),
        ("PUT" | "DELETE", [_, _, _, site_id, "bindings", binding_id]) if !binding_id.is_empty() => Ok(match manager.change_relationship(site_id, binding_id, method == "PUT") {
            Ok(()) => json_response(StatusCode::OK, &serde_json::json!({"site_id": site_id, "binding_id": binding_id})),
            Err(error) => change_error(error),
        }),
        _ => Ok(json_error(StatusCode::NOT_FOUND, "Relationship not found")),
    }
}
