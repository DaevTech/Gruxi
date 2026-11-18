mod common;

use common::api_client::ApiClient;
use common::api_fixtures::{CreatedResource, binding_payload, site_payload};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsConnector;

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

async fn options_status(port: u16) -> u16 {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(b"OPTIONS * HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n").await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8(response).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap()
}

#[tokio::test]
async fn site_binding_relationships_control_routing_and_survive_binding_updates() {
    let client = ApiClient::login().await;
    let site = create_site(&client).await;
    let (binding, response) = CreatedResource::create(&client, "/api/bindings", binding_payload()).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let collection = format!("{}/bindings", site.path());
    let relationship = format!("{}/{}", collection, binding.id);
    let port = binding.payload["port"].as_u64().unwrap() as u16;

    assert_eq!(client.get(&collection).await.json, json!([]));
    assert_eq!(options_status(port).await, 404);
    assert_eq!(client.put(&relationship, None).await.status, 200);
    assert_eq!(client.put(&relationship, None).await.status, 200);
    assert_eq!(client.get(&collection).await.json, json!([binding.id]));
    assert_eq!(options_status(port).await, 200);
    assert_eq!(client.delete(site.path()).await.status, 409);

    let mut updated = binding.payload.clone();
    updated["port"] = json!(common::api_fixtures::free_port());
    assert_eq!(client.put("/api/bindings", Some(&updated)).await.status, 200);
    assert_eq!(client.get(&collection).await.json, json!([binding.id]));
    assert_eq!(options_status(updated["port"].as_u64().unwrap() as u16).await, 200);
    assert_eq!(client.delete(&relationship).await.status, 200);
    assert_eq!(client.delete(&relationship).await.status, 200);
    assert_eq!(options_status(updated["port"].as_u64().unwrap() as u16).await, 404);
    assert_eq!(client.get(&collection).await.json, json!([]));
    assert_eq!(client.put(&format!("{}/missing", collection), None).await.status, 404);
    assert_eq!(client.put(&format!("/api/sites/missing/bindings/{}", binding.id), None).await.status, 404);
    assert_eq!(client.get("/api/sites/missing/bindings").await.status, 404);
    assert_eq!(client.put(&relationship, None).await.status, 200);
    assert_eq!(client.delete(binding.path()).await.status, 200);
    assert_eq!(client.get(&collection).await.json, json!([]));
    updated["port"] = json!(common::api_fixtures::free_port());
    assert_eq!(client.post("/api/bindings", Some(&updated)).await.status, 201);
    assert_eq!(options_status(updated["port"].as_u64().unwrap() as u16).await, 404);
}

#[tokio::test]
async fn site_binding_relationships_require_authentication_and_csrf() {
    let anonymous = ApiClient::unauthenticated();
    let collection = "/api/sites/some-site/bindings";
    let relationship = "/api/sites/some-site/bindings/some-binding";
    assert_eq!(anonymous.get(collection).await.status, 401);
    assert_eq!(anonymous.put(relationship, None).await.status, 401);
    assert_eq!(anonymous.delete(relationship).await.status, 401);
    let no_csrf = ApiClient::login().await.without_csrf();
    assert_eq!(no_csrf.put(relationship, None).await.status, 403);
    assert_eq!(no_csrf.delete(relationship).await.status, 403);
}

async fn server_certificate(port: u16, hostname: &str) -> Vec<u8> {
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions().unwrap()
        .dangerous().with_custom_certificate_verifier(Arc::new(common::api_client::AcceptAllVerifier))
        .with_no_client_auth();
    let stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    let name = rustls::pki_types::ServerName::try_from(hostname.to_string()).unwrap();
    let tls = TlsConnector::from(Arc::new(config)).connect(name, stream).await.unwrap();
    tls.get_ref().1.peer_certificates().unwrap()[0].as_ref().to_vec()
}

#[tokio::test]
async fn relationship_changes_refresh_tls_certificates() {
    let client = ApiClient::login().await;
    let mut payload = site_payload();
    let hostname = format!("{}.example", uuid::Uuid::new_v4().simple());
    payload["hostnames"] = json!([hostname]);
    let (site, response) = CreatedResource::create(&client, SITES, payload).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let mut payload = binding_payload();
    payload["is_tls"] = json!(true);
    let (binding, response) = CreatedResource::create(&client, "/api/bindings", payload).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let port = binding.payload["port"].as_u64().unwrap() as u16;
    let relationship = format!("{}/bindings/{}", site.path(), binding.id);
    let initial = server_certificate(port, &hostname).await;
    assert_eq!(client.put(&relationship, None).await.status, 200);
    let attached = timeout(Duration::from_secs(10), async {
        loop {
            let cert = server_certificate(port, &hostname).await;
            if cert != initial { break cert; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("TLS certificate was not refreshed after attaching");
    let saved = client.get(site.path()).await.json;
    let path = saved["tls_cert_path"].as_str().unwrap();
    let mut pem = std::io::BufReader::new(std::fs::File::open(path).unwrap());
    assert_eq!(rustls_pemfile::certs(&mut pem).next().unwrap().unwrap().as_ref(), attached);

    let mut second_payload = binding_payload();
    second_payload["is_tls"] = json!(true);
    let (second, response) = CreatedResource::create(&client, "/api/bindings", second_payload).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let second_port = second.payload["port"].as_u64().unwrap() as u16;
    let second_relationship = format!("{}/bindings/{}", site.path(), second.id);
    assert_eq!(client.put(&second_relationship, None).await.status, 200);
    timeout(Duration::from_secs(10), async {
        loop {
            if server_certificate(second_port, &hostname).await == attached { break; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("Second TLS binding did not use the site's certificate");
    assert_eq!(client.delete(&relationship).await.status, 200);
    timeout(Duration::from_secs(10), async {
        loop {
            if server_certificate(port, &hostname).await != attached { break; }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }).await.expect("TLS certificate was not refreshed after detaching");
    assert_eq!(server_certificate(second_port, &hostname).await, attached);
}
