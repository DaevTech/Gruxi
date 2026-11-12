mod common;

use common::api_client::ApiClient;
use common::api_fixtures::{CreatedResource, binding_payload};
use serde_json::json;

const BINDINGS: &str = "/api/bindings";

async fn create_binding(client: &ApiClient) -> CreatedResource {
    let (guard, response) = CreatedResource::create(client, BINDINGS, binding_payload()).await;
    assert_eq!(response.status, 201, "create failed: {}", response.body);
    guard
}

// ---------- create ----------

#[tokio::test]
async fn create_binding_returns_created_binding() {
    let client = ApiClient::login().await;
    let (guard, response) = CreatedResource::create(&client, BINDINGS, binding_payload()).await;

    assert_eq!(response.status, 201, "{}", response.body);
    assert_eq!(response.json, guard.payload);
}

#[tokio::test]
async fn create_binding_rejects_invalid_json() {
    let client = ApiClient::login().await;
    let response = client.request_raw("POST", BINDINGS, "{ not json").await;
    assert_eq!(response.status, 400);
}

#[tokio::test]
async fn create_binding_rejects_invalid_values() {
    let client = ApiClient::login().await;
    let mut payload = binding_payload();
    payload["ip"] = json!("not-an-ip");
    let response = client.post(BINDINGS, Some(&payload)).await;
    assert_eq!(response.status, 400);
    assert!(response.error().unwrap_or("").contains("Invalid IP"));
}

#[tokio::test]
async fn create_binding_rejects_admin_and_telemetry() {
    let client = ApiClient::login().await;
    for field in ["is_admin", "is_telemetry"] {
        let mut payload = binding_payload();
        payload[field] = json!(true);
        let response = client.post(BINDINGS, Some(&payload)).await;
        assert_eq!(response.status, 400, "{} should be rejected", field);
    }
}

#[tokio::test]
async fn create_binding_rejects_duplicate_id_and_settings() {
    let client = ApiClient::login().await;
    let existing = create_binding(&client).await;

    let response = client.post(BINDINGS, Some(&existing.payload)).await;
    assert_eq!(response.status, 409);

    let mut same_settings = existing.payload.clone();
    same_settings["id"] = json!(uuid::Uuid::new_v4().to_string());
    let response = client.post(BINDINGS, Some(&same_settings)).await;
    assert_eq!(response.status, 409);
}

// ---------- get ----------

#[tokio::test]
async fn get_binding_by_id_returns_binding() {
    let client = ApiClient::login().await;
    let created = create_binding(&client).await;

    let response = client.get(created.path()).await;
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(response.json, created.payload);
}

#[tokio::test]
async fn get_binding_unknown_id_returns_not_found() {
    let client = ApiClient::login().await;
    let response = client.get("/api/bindings/does-not-exist").await;
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn get_binding_with_extra_path_segment_returns_not_found() {
    let client = ApiClient::login().await;
    let response = client.get("/api/bindings/a/b").await;
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn get_all_bindings_contains_created_and_hides_admin_and_telemetry() {
    let client = ApiClient::login().await;
    let created = create_binding(&client).await;

    let response = client.get(BINDINGS).await;
    assert_eq!(response.status, 200);
    let list = response.json.as_array().expect("expected a JSON array");
    assert!(list.iter().any(|b| b["id"] == created.payload["id"]));
    assert!(list.iter().all(|b| b["is_admin"] == false && b["is_telemetry"] == false));
}

// ---------- update ----------

#[tokio::test]
async fn update_binding_persists_changes() {
    let client = ApiClient::login().await;
    let created = create_binding(&client).await;

    let mut updated = created.payload.clone();
    updated["port"] = json!(common::api_fixtures::free_port());
    let response = client.put(BINDINGS, Some(&updated)).await;
    assert_eq!(response.status, 200, "{}", response.body);
    assert_eq!(response.json, updated);

    let fetched = client.get(created.path()).await;
    assert_eq!(fetched.json["port"], updated["port"]);
}

#[tokio::test]
async fn update_binding_unknown_id_returns_not_found() {
    let client = ApiClient::login().await;
    let response = client.put(BINDINGS, Some(&binding_payload())).await;
    assert_eq!(response.status, 404);
}

#[tokio::test]
async fn update_binding_rejects_invalid_json_and_values() {
    let client = ApiClient::login().await;
    let created = create_binding(&client).await;

    let response = client.request_raw("PUT", BINDINGS, "{ not json").await;
    assert_eq!(response.status, 400);

    let mut invalid = created.payload.clone();
    invalid["port"] = json!(0);
    let response = client.put(BINDINGS, Some(&invalid)).await;
    assert_eq!(response.status, 400);
}

// ---------- delete ----------

#[tokio::test]
async fn delete_binding_removes_binding() {
    let client = ApiClient::login().await;
    let created = create_binding(&client).await;

    let response = client.delete(created.path()).await;
    assert_eq!(response.status, 200, "{}", response.body);

    assert_eq!(client.get(created.path()).await.status, 404);
}

#[tokio::test]
async fn delete_binding_unknown_id_returns_not_found() {
    let client = ApiClient::login().await;
    assert_eq!(client.delete("/api/bindings/does-not-exist").await.status, 404);
}

#[tokio::test]
async fn delete_binding_without_id_returns_not_found() {
    let client = ApiClient::login().await;
    assert_eq!(client.delete("/api/bindings/").await.status, 404);
}

// ---------- authentication / CSRF ----------

#[tokio::test]
async fn bindings_endpoints_require_authentication() {
    let client = ApiClient::unauthenticated();
    let payload = binding_payload();

    assert_eq!(client.get(BINDINGS).await.status, 401);
    assert_eq!(client.get("/api/bindings/some-id").await.status, 401);
    assert_eq!(client.post(BINDINGS, Some(&payload)).await.status, 401);
    assert_eq!(client.put(BINDINGS, Some(&payload)).await.status, 401);
    assert_eq!(client.delete("/api/bindings/some-id").await.status, 401);
}

#[tokio::test]
async fn bindings_modifying_endpoints_require_csrf_token() {
    let client = ApiClient::login().await.without_csrf();
    let payload = binding_payload();

    assert_eq!(client.post(BINDINGS, Some(&payload)).await.status, 403);
    assert_eq!(client.put(BINDINGS, Some(&payload)).await.status, 403);
    assert_eq!(client.delete("/api/bindings/some-id").await.status, 403);
}
