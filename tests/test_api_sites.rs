mod common;

use common::api_client::ApiClient;
use common::api_fixtures::{CreatedResource, binding_payload, site_payload};
use serde_json::json;

const SITES: &str = "/api/sites";

async fn create_site(client: &ApiClient) -> CreatedResource {
    let (created, response) = CreatedResource::create(client, SITES, site_payload()).await;
    assert_eq!(response.status, 201, "{}", response.body);
    assert_eq!(response.json, created.payload);
    created
}

#[tokio::test]
async fn site_crud_and_round_trip() {
    let client = ApiClient::login().await;
    let created = create_site(&client).await;
    assert_eq!(client.get(created.path()).await.json, created.payload);
    let list = client.get(SITES).await;
    assert_eq!(list.status, 200);
    assert!(list.json.as_array().unwrap().iter().any(|site| site["id"] == created.payload["id"]));
    assert_eq!(client.post(SITES, Some(&created.payload)).await.status, 409);

    let mut updated = created.payload.clone();
    updated["hostnames"] = json!([" EXAMPLE.ORG "]);
    updated["canonical_host"] = json!("EXAMPLE.ORG");
    updated["extra_headers"] = json!([{"key":"X-Example", "value":"O'Reilly=ok"}]);
    updated["tls_cert_content"] = json!("certificate's content");
    let response = client.put(SITES, Some(&updated)).await;
    assert_eq!(response.status, 200, "{}", response.body);
    updated["hostnames"] = json!(["example.org"]);
    updated["canonical_host"] = json!("example.org");
    assert_eq!(response.json, updated);
    assert_eq!(client.get(created.path()).await.json, updated);

    assert_eq!(client.delete(created.path()).await.status, 200);
    assert_eq!(client.get(created.path()).await.status, 404);
}

#[tokio::test]
async fn deleting_referenced_site_preserves_relationship() {
    struct RelationshipGuard {
        connection: sqlite::Connection,
        site_id: String,
    }
    impl Drop for RelationshipGuard {
        fn drop(&mut self) {
            let result = (|| {
                let mut statement = self.connection.prepare("DELETE FROM binding_sites WHERE site_id = ?")?;
                statement.bind((1, self.site_id.as_str()))?;
                statement.next()?;
                Ok::<_, sqlite::Error>(())
            })();
            if let Err(error) = result {
                eprintln!("Failed to clean up test relationship: {}", error);
            }
        }
    }

    let client = ApiClient::login().await;
    let site = create_site(&client).await;
    let (binding, response) = CreatedResource::create(&client, "/api/bindings", binding_payload()).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let connection = gruxi::core::database_connection::get_database_connection().unwrap();
    let relationship = RelationshipGuard { connection, site_id: site.id.clone() };
    let mut statement = relationship.connection.prepare("INSERT INTO binding_sites (binding_id, site_id) VALUES (?, ?)").unwrap();
    statement.bind((1, binding.id.as_str())).unwrap();
    statement.bind((2, site.id.as_str())).unwrap();
    statement.next().unwrap();
    drop(statement);

    assert_eq!(client.delete(site.path()).await.status, 409);
    assert_eq!(client.get(site.path()).await.json, site.payload);
    let mut statement = relationship.connection.prepare("SELECT COUNT(*) FROM binding_sites WHERE site_id = ?").unwrap();
    statement.bind((1, site.id.as_str())).unwrap();
    statement.next().unwrap();
    assert_eq!(statement.read::<i64, _>(0).unwrap(), 1);
}

#[tokio::test]
async fn sites_reject_invalid_requests_and_unknown_ids() {
    let client = ApiClient::login().await;
    let created = create_site(&client).await;
    for method in ["POST", "PUT"] {
        assert_eq!(client.request_raw(method, SITES, "{ invalid").await.status, 400);
        let mut invalid = created.payload.clone();
        invalid["hostnames"] = json!([]);
        assert_eq!(client.request(method, SITES, Some(&invalid)).await.status, 400);
        invalid["hostnames"] = json!(["one.example,two.example"]);
        assert_eq!(client.request(method, SITES, Some(&invalid)).await.status, 400);
        invalid = created.payload.clone();
        invalid["extra_headers"] = json!([{"key":"X-Test", "value":"one,two"}]);
        assert_eq!(client.request(method, SITES, Some(&invalid)).await.status, 400);
        invalid = created.payload.clone();
        invalid["request_handlers"] = json!([" "]);
        assert_eq!(client.request(method, SITES, Some(&invalid)).await.status, 400);
        invalid = created.payload.clone();
        invalid["extra_headers"] = json!([{"key":"Bad=Key", "value":"value"}]);
        assert_eq!(client.request(method, SITES, Some(&invalid)).await.status, 400);
        invalid = created.payload.clone();
        invalid["id"] = json!("bad/id");
        assert_eq!(client.request(method, SITES, Some(&invalid)).await.status, 400);
    }
    assert_eq!(client.put(SITES, Some(&site_payload())).await.status, 404);
    for path in ["/api/sites/unknown", "/api/sites/", "/api/sites/a/b"] {
        assert_eq!(client.get(path).await.status, 404);
        assert_eq!(client.delete(path).await.status, 404);
    }
    assert_eq!(client.post(created.path(), Some(&created.payload)).await.status, 404);
    assert_eq!(client.put(created.path(), Some(&created.payload)).await.status, 404);
}

#[tokio::test]
async fn sites_require_authentication_and_csrf() {
    let anonymous = ApiClient::unauthenticated();
    let payload = site_payload();
    assert_eq!(anonymous.get(SITES).await.status, 401);
    assert_eq!(anonymous.get("/api/sites/some-id").await.status, 401);
    assert_eq!(anonymous.post(SITES, Some(&payload)).await.status, 401);
    assert_eq!(anonymous.put(SITES, Some(&payload)).await.status, 401);
    assert_eq!(anonymous.delete("/api/sites/some-id").await.status, 401);

    let no_csrf = ApiClient::login().await.without_csrf();
    assert_eq!(no_csrf.post(SITES, Some(&payload)).await.status, 403);
    assert_eq!(no_csrf.put(SITES, Some(&payload)).await.status, 403);
    assert_eq!(no_csrf.delete("/api/sites/some-id").await.status, 403);
}
