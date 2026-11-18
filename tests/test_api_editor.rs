mod common;

use common::api_client::ApiClient;
use common::api_fixtures::{CreatedResource, binding_payload, site_payload};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

async fn request_site(port: u16, hostname: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(format!("GET /live.txt HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n", hostname).as_bytes()).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    String::from_utf8(response).unwrap()
}

#[tokio::test]
async fn editor_save_preserves_api_resources_and_applies_new_site_processor() {
    struct WebRoot(std::path::PathBuf);
    impl Drop for WebRoot {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(self.0.join("live.txt"));
            let _ = std::fs::remove_dir(&self.0);
        }
    }

    let client = ApiClient::login().await;
    let mut config = client.get("/config").await.json;
    for field in ["bindings", "sites", "binding_sites"] {
        assert!(config.get(field).is_none(), "/config returned {field}");
    }
    let root = WebRoot(std::env::temp_dir().join(format!("gruxi-editor-{}", uuid::Uuid::new_v4())));
    std::fs::create_dir(&root.0).unwrap();
    std::fs::write(root.0.join("live.txt"), "live-editor-test").unwrap();
    let handler_id = uuid::Uuid::new_v4().to_string();
    let processor_id = uuid::Uuid::new_v4().to_string();
    let mut site_data = site_payload();
    site_data["request_handlers"] = json!([handler_id]);
    let (site, response) = CreatedResource::create(&client, "/api/sites", site_data).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let (binding, response) = CreatedResource::create(&client, "/api/bindings", binding_payload()).await;
    assert_eq!(response.status, 201, "{}", response.body);
    let relationship = format!("{}/bindings/{}", site.path(), binding.id);
    assert_eq!(client.put(&relationship, None).await.status, 200);
    let port = binding.payload["port"].as_u64().unwrap() as u16;

    let original_gzip = config["core"]["gzip"]["is_enabled"].as_bool().unwrap();
    for (path, gzip_enabled) in [("/config", !original_gzip), ("/config/editor", original_gzip)] {
        let mut stale = config.clone();
        stale["core"]["gzip"]["is_enabled"] = json!(gzip_enabled);
        stale["bindings"] = json!([]);
        stale["sites"] = json!([]);
        stale["binding_sites"] = json!([]);
        let response = client.post(path, Some(&stale)).await;
        assert_eq!(response.status, 200, "{}", response.body);
        for field in ["bindings", "sites", "binding_sites"] {
            assert!(response.json["configuration"].get(field).is_none(), "{path} returned {field}");
        }
        assert_eq!(client.get("/config").await.json["core"]["gzip"]["is_enabled"], json!(gzip_enabled));
        assert_eq!(client.get(site.path()).await.json, site.payload);
        assert_eq!(client.get(binding.path()).await.json, binding.payload);
        assert_eq!(client.get(&format!("{}/bindings", site.path())).await.json, json!([binding.id]));
    }

    config["request_handlers"].as_array_mut().unwrap().push(json!({
        "id": handler_id, "is_enabled": true, "name": "Editor test",
        "processor_type": "static", "processor_id": processor_id, "url_match": ["*"]
    }));
    config["static_file_processors"].as_array_mut().unwrap().push(json!({
        "id": processor_id, "web_root": root.0.to_str().unwrap(), "web_root_index_file_list": ["index.html"]
    }));
    let response = client.post("/config/editor", Some(&config)).await;
    assert_eq!(response.status, 200, "{}", response.body);
    assert!(response.json["configuration"].get("sites").is_none());
    assert_eq!(client.get(&format!("{}/bindings", site.path())).await.json, json!([binding.id]));
    let response = request_site(port, "example.com").await;
    assert!(response.starts_with("HTTP/1.1 200"), "{}", response);
    assert!(response.contains("live-editor-test"), "{}", response);

    let mut updated = site.payload.clone();
    updated["request_handlers"] = json!([]);
    assert_eq!(client.put("/api/sites", Some(&updated)).await.status, 200);
    assert!(request_site(port, "example.com").await.starts_with("HTTP/1.1 404"));
    updated["request_handlers"] = json!([handler_id]);
    assert_eq!(client.put("/api/sites", Some(&updated)).await.status, 200);
    assert!(request_site(port, "example.com").await.starts_with("HTTP/1.1 200"));

    assert_eq!(client.delete(&relationship).await.status, 200);
    assert_eq!(client.delete(binding.path()).await.status, 200);
    assert_eq!(client.delete(site.path()).await.status, 200);
    let current = client.get("/config").await.json;
    assert_eq!(client.post("/config/editor", Some(&current)).await.status, 200);
}
