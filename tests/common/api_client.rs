#![allow(dead_code)]

use rustls::{
    ClientConfig, DigitallySignedStruct, Error as TlsError, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use serde_json::Value;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{Duration, timeout};
use tokio_rustls::TlsConnector;

const SESSION_COOKIE: &str = "__Host-gruxi_admin_session";
const CSRF_COOKIE: &str = "__Host-gruxi_admin_csrf";
const CSRF_HEADER: &str = "X-CSRF-Token";
const IO_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
struct AcceptAllVerifier;

impl ServerCertVerifier for AcceptAllVerifier {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, TlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, TlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

/// Parsed API response. `json` is `Value::Null` when the body is empty or not JSON.
#[derive(Debug, Clone)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub json: Value,
}

impl ApiResponse {
    /// The `error` message of a `{ "error": "..." }` response, if any.
    pub fn error(&self) -> Option<&str> {
        self.json.get("error").and_then(|v| v.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct ApiClient {
    host: String,
    port: u16,
    session_token: Option<String>,
    csrf_token: Option<String>,
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

impl ApiClient {
    /// A client without any session. Useful for testing authentication failures.
    pub fn unauthenticated() -> Self {
        let _ = rustls::crypto::CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider());
        Self {
            host: env_or("GRUXI_TEST_ADMIN_HOST", "127.0.0.1"),
            port: env_or("GRUXI_TEST_ADMIN_PORT", "8000").parse().expect("GRUXI_TEST_ADMIN_PORT must be a port number"),
            session_token: None,
            csrf_token: None,
        }
    }

    /// Get an authenticated client without any credentials or server settings.
    ///
    /// The session is created directly in the database through the library's
    /// `create_unauthenticated_admin_session()` (the running server validates sessions against the
    /// same database; run the tests from the repository root so the same `db/` is used).
    /// The CSRF check is a double-submit (cookie must equal header), so we generate our own token.
    pub async fn login() -> Self {
        let session = tokio::task::spawn_blocking(gruxi::core::admin_user::create_unauthenticated_admin_session)
            .await
            .expect("Session creation task panicked")
            .expect("Could not create an admin session in the database. Run from the repository root with an initialized db/gruxi.db");

        let mut client = Self::unauthenticated();
        client.session_token = Some(session.token);
        client.csrf_token = Some(format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple()));
        client
    }

    /// Same session, but without the CSRF header/cookie. Useful for testing CSRF protection.
    pub fn without_csrf(&self) -> Self {
        Self { csrf_token: None, ..self.clone() }
    }

    pub async fn get(&self, path: &str) -> ApiResponse {
        self.request("GET", path, None).await
    }

    pub async fn post(&self, path: &str, body: Option<&Value>) -> ApiResponse {
        self.request("POST", path, body).await
    }

    pub async fn put(&self, path: &str, body: Option<&Value>) -> ApiResponse {
        self.request("PUT", path, body).await
    }

    pub async fn delete(&self, path: &str) -> ApiResponse {
        self.request("DELETE", path, None).await
    }

    /// Send a request with the raw body text (for invalid JSON tests).
    pub async fn request_raw(&self, method: &str, path: &str, body: &str) -> ApiResponse {
        self.send(method, path, Some(body.to_string())).await
    }

    pub async fn request(&self, method: &str, path: &str, body: Option<&Value>) -> ApiResponse {
        self.send(method, path, body.map(|b| b.to_string())).await
    }

    async fn send(&self, method: &str, path: &str, body: Option<String>) -> ApiResponse {
        let mut request = format!("{} {} HTTP/1.1\r\nHost: {}:{}\r\nAccept: application/json\r\nConnection: close\r\n", method, path, self.host, self.port);

        let mut cookies = Vec::new();
        if let Some(token) = &self.session_token {
            cookies.push(format!("{}={}", SESSION_COOKIE, token));
        }
        if let Some(token) = &self.csrf_token {
            cookies.push(format!("{}={}", CSRF_COOKIE, token));
            request.push_str(&format!("{}: {}\r\n", CSRF_HEADER, token));
        }
        if !cookies.is_empty() {
            request.push_str(&format!("Cookie: {}\r\n", cookies.join("; ")));
        }

        let body = body.unwrap_or_default();
        if !body.is_empty() || method == "POST" || method == "PUT" {
            request.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
        }
        request.push_str("\r\n");
        request.push_str(&body);

        let raw = self.exchange(request.as_bytes()).await;
        parse_response(&raw)
    }

    async fn exchange(&self, request: &[u8]) -> Vec<u8> {
        let config = ClientConfig::builder().dangerous().with_custom_certificate_verifier(Arc::new(AcceptAllVerifier)).with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(config));

        let tcp = timeout(IO_TIMEOUT, TcpStream::connect((self.host.as_str(), self.port)))
            .await
            .expect("Timed out connecting to admin portal")
            .expect("Could not connect to admin portal. Is Gruxi running?");
        let server_name = ServerName::try_from("localhost".to_string()).unwrap();
        let mut stream = timeout(IO_TIMEOUT, connector.connect(server_name, tcp)).await.expect("TLS handshake timed out").expect("TLS handshake failed");

        stream.write_all(request).await.expect("Failed to write request");

        // Read until the response is complete (don't rely on the server closing the connection)
        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            if response_complete(&data) {
                break;
            }
            match timeout(IO_TIMEOUT, stream.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => data.extend_from_slice(&buf[..n]),
                // Servers may close without a TLS close_notify; use what we have
                Ok(Err(_)) => break,
                Err(_) => panic!("Timed out reading response"),
            }
        }
        data
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn split_head(data: &[u8]) -> Option<(String, &[u8])> {
    let pos = find(data, b"\r\n\r\n")?;
    Some((String::from_utf8_lossy(&data[..pos]).to_string(), &data[pos + 4..]))
}

fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines().skip(1).find_map(|l| {
        let (n, v) = l.split_once(':')?;
        n.trim().eq_ignore_ascii_case(name).then(|| v.trim().to_string())
    })
}

fn response_complete(data: &[u8]) -> bool {
    let Some((head, body)) = split_head(data) else { return false };
    if header_value(&head, "transfer-encoding").map(|v| v.to_ascii_lowercase().contains("chunked")).unwrap_or(false) {
        return body.ends_with(b"0\r\n\r\n");
    }
    match header_value(&head, "content-length").and_then(|v| v.parse::<usize>().ok()) {
        Some(len) => body.len() >= len,
        None => false,
    }
}

fn decode_chunked(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(pos) = find(body, b"\r\n") {
        let size_text = String::from_utf8_lossy(&body[..pos]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        if size == 0 || body.len() < pos + 2 + size {
            break;
        }
        out.extend_from_slice(&body[pos + 2..pos + 2 + size]);
        body = &body[(pos + 2 + size + 2).min(body.len())..];
    }
    out
}

fn parse_response(raw: &[u8]) -> ApiResponse {
    let (head, body_bytes) = split_head(raw).unwrap_or_else(|| (String::from_utf8_lossy(raw).to_string(), &[]));
    let status = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':').map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string())))
        .collect();

    let chunked = header_value(&head, "transfer-encoding").map(|v| v.to_ascii_lowercase().contains("chunked")).unwrap_or(false);
    let body = if chunked { decode_chunked(body_bytes) } else { body_bytes.to_vec() };
    let body = String::from_utf8_lossy(&body).to_string();
    let json = serde_json::from_str(&body).unwrap_or(Value::Null);

    ApiResponse { status, headers, body, json }
}

/// All `(name, value)` pairs from `Set-Cookie` headers.
fn set_cookies(response: &ApiResponse) -> Vec<(String, String)> {
    response
        .headers
        .iter()
        .filter(|(n, _)| n == "set-cookie")
        .filter_map(|(_, v)| {
            let (name, rest) = v.split_once('=')?;
            Some((name.to_string(), rest.split(';').next().unwrap_or("").to_string()))
        })
        .collect()
}
