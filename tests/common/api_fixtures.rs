#![allow(dead_code)]

use super::api_client::{ApiClient, ApiResponse};
use serde_json::Value;

/// Find a free local TCP port (high range, so it will not collide with Gruxi's own bindings).
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

/// A resource created through the API that is deleted again on drop, even if the test fails.
///
/// Reuse for new endpoints by constructing it with the resource's collection path
/// (e.g. `/api/sites`) and the id of the created item.
pub struct CreatedResource {
    client: ApiClient,
    path: String,
    pub id: String,
    pub payload: Value,
}

impl CreatedResource {
    /// POST the payload to `collection_path` and return a guard. The payload must contain an `id`.
    pub async fn create(client: &ApiClient, collection_path: &str, payload: Value) -> (Self, ApiResponse) {
        let response = client.post(collection_path, Some(&payload)).await;
        let id = payload["id"].as_str().expect("payload needs an id").to_string();
        let guard = Self {
            client: client.clone(),
            path: format!("{}/{}", collection_path.trim_end_matches('/'), id),
            id,
            payload,
        };
        (guard, response)
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Drop for CreatedResource {
    fn drop(&mut self) {
        let client = self.client.clone();
        let path = self.path.clone();
        // Drop is sync and may run inside a runtime, so use a separate thread. A 404 is fine.
        let _ = std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread().enable_all().build() {
                rt.block_on(async move {
                    client.delete(&path).await;
                });
            }
        })
        .join();
    }
}

/// A valid, unique binding payload on a free local port.
pub fn binding_payload() -> Value {
    serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "ip": "127.0.0.1",
        "port": free_port(),
        "is_admin": false,
        "is_telemetry": false,
        "is_tls": false,
        "protocol": "Tcp",
    })
}
