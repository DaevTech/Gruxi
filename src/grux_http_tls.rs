use crate::grux_configuration::{get_current_configuration_from_db, save_configuration};
use crate::grux_configuration_struct::*;
use log::{info, warn};
use rustls_pki_types::pem::PemObject; // for from_pem_file, etc.
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tls_listener::rustls as tokio_rustls;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::crypto::ring::sign as ring_sign;
use tokio_rustls::rustls::server::ResolvesServerCertUsingSni;
use tokio_rustls::rustls::sign::CertifiedKey as RustlsCertifiedKey;
use tokio_rustls::rustls::{self, ServerConfig as RustlsServerConfig};

// Persist generated cert/key to disk and update configuration for a specific site
pub async fn persist_generated_tls_for_site(binding: &Binding, site: &Sites, cert_pem: &str, key_pem: &str) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Ensure target directory exists
    let dir = "certs";
    fs::create_dir_all(dir).await?;

    // Sanitize for filename
    let mut ip_part = binding.ip.replace(':', "_");
    if ip_part.is_empty() {
        ip_part = "unknown".to_string();
    }
    let site_part = site.hostnames.iter().find(|h| h.as_str() != "*").cloned().unwrap_or_else(|| "any".to_string()).replace('*', "star");
    let cert_path = format!("{}/site_{}_{}_{}.crt.pem", dir, ip_part, binding.port, site_part);
    let key_path = format!("{}/site_{}_{}_{}.key.pem", dir, ip_part, binding.port, site_part);

    // Write files atomically: write to temp then rename
    let cert_tmp = format!("{}.tmp", &cert_path);
    let key_tmp = format!("{}.tmp", &key_path);

    {
        let mut f = fs::File::create(&cert_tmp).await?;
        f.write_all(cert_pem.as_bytes()).await?;
        f.flush().await?;
    }
    fs::rename(&cert_tmp, &cert_path).await?;

    {
        let mut f = fs::File::create(&key_tmp).await?;
        f.write_all(key_pem.as_bytes()).await?;
        f.flush().await?;
    }
    fs::rename(&key_tmp, &key_path).await?;

    // Update configuration in DB so future runs use persisted files
    // Best-effort; failures shouldn't block startup
    match get_current_configuration_from_db() {
        Ok(mut cfg) => {
            let mut updated = false;
            // Update matching binding and site
            for server in cfg.servers.iter_mut() {
                for b in server.bindings.iter_mut() {
                    if b.ip == binding.ip && b.port == binding.port && b.is_admin == binding.is_admin {
                        b.is_tls = true;
                        for s in b.sites.iter_mut() {
                            if s.web_root == site.web_root && s.hostnames == site.hostnames {
                                s.tls_cert_path = Some(cert_path.clone());
                                s.tls_key_path = Some(key_path.clone());
                                updated = true;
                            }
                        }
                    }
                }
            }
            if updated {
                if let Err(e) = save_configuration(&cfg) {
                    warn!("Failed to persist TLS paths to configuration: {}", e);
                } else {
                    info!("Persisted generated TLS certificate and key to configuration.");
                }
            }
        }
        Err(e) => warn!("Failed to load configuration for TLS path persistence: {}", e),
    }

    Ok((cert_path, key_path))
}

// Build a TLS acceptor that selects certificates per-site using SNI
pub async fn build_tls_acceptor(binding: &Binding) -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let provider = rustls::crypto::ring::default_provider();

    // Create SNI resolver
    let mut resolver = ResolvesServerCertUsingSni::new();
    let mut have_default = false;
    let mut site_added = false;

    for site in &binding.sites {
        if !site.is_enabled {
            continue;
        }

        // Determine SANs: filter out wildcard-only
        let mut sans: Vec<String> = site.hostnames.iter().cloned().filter(|h| !h.trim().is_empty() && h != "*").collect();
        if sans.is_empty() {
            sans.push("localhost".to_string());
        }

        // Load from PEM if both provided; else generate
        let (cert_chain, priv_key, maybe_pem): (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>, Option<(String, String)>) = match (&site.tls_cert_path, &site.tls_key_path) {
            (Some(cert_path), Some(key_path)) => match (CertificateDer::from_pem_file(cert_path), PrivateKeyDer::from_pem_file(key_path)) {
                (Ok(cert), Ok(key)) => (vec![cert.into_owned()], key, None),
                (cerr, kerr) => {
                    warn!("Failed to load TLS materials for site (cert: {:?}, key: {:?}); generating self-signed", cerr.err(), kerr.err());
                    let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(sans.clone()).map_err(|e| format!("Failed to generate self-signed cert: {}", e))?;
                    let cert_pem = cert.pem();
                    let key_pem = signing_key.serialize_pem();
                    (
                        vec![CertificateDer::from(cert.der().to_vec())],
                        PrivateKeyDer::try_from(signing_key.serialize_der()).map_err(|e| format!("Invalid key DER from rcgen: {}", e))?,
                        Some((cert_pem, key_pem)),
                    )
                }
            },
            _ => {
                let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(sans.clone()).map_err(|e| format!("Failed to generate self-signed cert: {}", e))?;
                let cert_pem = cert.pem();
                let key_pem = signing_key.serialize_pem();
                (
                    vec![CertificateDer::from(cert.der().to_vec())],
                    PrivateKeyDer::try_from(signing_key.serialize_der()).map_err(|e| format!("Invalid key DER from rcgen: {}", e))?,
                    Some((cert_pem, key_pem)),
                )
            }
        };

        // Persist if generated
        if let Some((cert_pem, key_pem)) = maybe_pem {
            let _ = persist_generated_tls_for_site(binding, site, &cert_pem, &key_pem).await;
        }

        // Build a signing key and certified key for rustls
        let signing_key = ring_sign::any_supported_type(&priv_key).map_err(|e| format!("Unsupported private key type for rustls: {}", e))?;
        let certified = RustlsCertifiedKey::new(cert_chain.clone(), signing_key);

        // Add each SAN as a mapping
        for name in &sans {
            // Accept wildcard names like "*.example.com" if provided
            match resolver.add(name, certified.clone()) {
                Ok(()) => {
                    site_added = true;
                }
                Err(e) => warn!("Failed to add SNI name '{}': {:?}", name, e),
            }
        }

        // If site is default or hostname includes wildcard "*", set as default cert
        if site.is_default && !have_default {
            // No explicit default setter; rely on SNI match. Keep note to add a fallback later.
            have_default = true;
        }
    }

    if !site_added {
        // As a last resort, generate a single default cert
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).map_err(|e| format!("Failed to generate fallback self-signed cert: {}", e))?;
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivateKeyDer::try_from(signing_key.serialize_der()).map_err(|e| format!("Invalid key DER: {}", e))?;
        let signing_key = ring_sign::any_supported_type(&key_der).map_err(|e| format!("Unsupported private key type for rustls: {}", e))?;
        let _certified = RustlsCertifiedKey::new(vec![cert_der], signing_key);
        // No API for explicit default here; omit.
    }

    let mut server_config = RustlsServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .map_err(|_| "Protocol versions unavailable")?
        .with_no_client_auth()
        .with_cert_resolver(std::sync::Arc::new(resolver));

    // Enable http/1.1 ALPN
    server_config.alpn_protocols = vec![b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(std::sync::Arc::new(server_config)))
}
