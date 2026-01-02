use std::sync::Arc;

use hyper::body::Incoming;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use crate::http::request_handlers::processors::proxy_helpers::no_verifier::NoVerifier;
use crate::tls::tls_config::tls_config;

pub struct HttpClient {
    client_with_tls_verify: Client<HttpsConnector<HttpConnector>, hyper::body::Incoming>,
    client_without_tls_verify: Client<HttpsConnector<HttpConnector>, hyper::body::Incoming>,
}

impl HttpClient {
    pub fn new() -> Self {
        // Client with TLS certificate verification
        let tls_config_with_verify = tls_config();
        let https_with_verify = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls_config_with_verify)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

        let client_with_tls_verify: Client<_, Incoming> = Client::builder(TokioExecutor::new())
            .build::<_, Incoming>(https_with_verify);

        // Client without TLS certificate verification
        let mut tls_config_with_no_verify = tls_config();
        tls_config_with_no_verify
            .dangerous()
            .set_certificate_verifier(Arc::new(NoVerifier));

        let https_without_verify = hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls_config_with_no_verify)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();

        let client_without_tls_verify: Client<_, Incoming> = Client::builder(TokioExecutor::new())
            .build::<_, Incoming>(https_without_verify);

        Self {
            client_with_tls_verify,
            client_without_tls_verify,
        }
    }

    pub fn get_client(&self, verify_tls: bool) -> Client<HttpsConnector<HttpConnector>, Incoming> {
        if verify_tls {
            self.client_with_tls_verify.clone()
        } else {
            self.client_without_tls_verify.clone()
        }
    }
}
