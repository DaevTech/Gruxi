use crate::warn;
use rustls::{ClientConfig, RootCertStore};

pub fn tls_config() -> ClientConfig {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut roots = RootCertStore::empty();

    let native_certs_result = rustls_native_certs::load_native_certs();
    for err in &native_certs_result.errors {
        warn!("Failed to load native certificate: {}", err);
    }
    for cert in native_certs_result.certs {
        if let Err(e) = roots.add(cert) {
            warn!("Failed to add native cert to root store: {}", e);
        }
    }

    // Extend with webpki-roots
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    ClientConfig::builder().with_root_certificates(roots).with_no_client_auth()
}
