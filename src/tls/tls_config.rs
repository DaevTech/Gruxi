use rustls::{ClientConfig, RootCertStore};

pub fn tls_config() -> ClientConfig {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let mut roots = RootCertStore::empty();

    let native_certs_result = rustls_native_certs::load_native_certs();
    for cert in native_certs_result.certs {
        roots.add(cert).expect("failed to add cert to root store");
    }

    // Extend with webpki-roots
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    let config = ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();

    config
}
