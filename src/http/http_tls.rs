use crate::core::running_state_manager::get_running_state_manager;
use crate::logging::syslog::{debug, warn};
use rand;
use rustls::crypto::aws_lc_rs;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::io::BufReader;
use tls_listener::rustls as tokio_rustls;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::server::ResolvesServerCertUsingSni;
use tokio_rustls::rustls::server::{ClientHello, ResolvesServerCert};
use tokio_rustls::rustls::sign::CertifiedKey as RustlsCertifiedKey;
use tokio_rustls::rustls::{self, ServerConfig as RustlsServerConfig};

use crate::configuration::binding::Binding;
use crate::configuration::site::Site;
use crate::core::database_connection::get_database_connection;

// Persist generated cert/key to disk and update configuration for a specific site
pub async fn persist_generated_tls_for_site(site: &Site, cert_pem: &str, key_pem: &str, is_admin: bool) -> Result<(String, String), Box<dyn std::error::Error + Send + Sync>> {
    // Ensure target directory exists with appropriate permissions
    let dir = "certs";
    fs::create_dir_all(dir).await.map_err(|e| format!("Failed to create certs directory '{}': {}", dir, e))?;

    // Generate a random number for this cert
    let random_number: u32 = rand::random();

    let cert_path = format!("{}/{}.crt.pem", dir, random_number);
    let key_path = format!("{}/{}.key.pem", dir, random_number);

    // Write files atomically: write to temp then rename
    let cert_tmp = format!("{}.tmp", &cert_path);
    let key_tmp = format!("{}.tmp", &key_path);

    {
        let mut f = fs::File::create(&cert_tmp).await.map_err(|e| format!("Failed to create temp cert file '{}': {}", cert_tmp, e))?;
        f.write_all(cert_pem.as_bytes()).await.map_err(|e| format!("Failed to write cert data to '{}': {}", cert_tmp, e))?;
        f.flush().await.map_err(|e| format!("Failed to flush cert file '{}': {}", cert_tmp, e))?;
    }
    fs::rename(&cert_tmp, &cert_path)
        .await
        .map_err(|e| format!("Failed to rename temp cert file '{}' to '{}': {}", cert_tmp, cert_path, e))?;

    {
        let mut f = fs::File::create(&key_tmp).await.map_err(|e| format!("Failed to create temp key file '{}': {}", key_tmp, e))?;
        f.write_all(key_pem.as_bytes()).await.map_err(|e| format!("Failed to write key data to '{}': {}", key_tmp, e))?;
        f.flush().await.map_err(|e| format!("Failed to flush key file '{}': {}", key_tmp, e))?;
    }
    fs::rename(&key_tmp, &key_path)
        .await
        .map_err(|e| format!("Failed to rename temp key file '{}' to '{}': {}", key_tmp, key_path, e))?;

    // Update configuration in DB so future runs use persisted files
    let connection = get_database_connection()?;

    // Update the fields in the database directly
    if is_admin {
        // For admin portal, update the configuration table
        let sql_update = format!(
            "UPDATE server_settings SET setting_value = '{}' WHERE setting_key = 'admin_portal_tls_certificate_path';",
            cert_path.clone()
        );
        connection
            .execute(sql_update.as_str())
            .map_err(|e| format!("Failed to update admin portal TLS paths in database: {}", e))?;
        let sql_update = format!("UPDATE server_settings SET setting_value = '{}' WHERE setting_key = 'admin_portal_tls_key_path';", key_path.clone());
        connection
            .execute(sql_update.as_str())
            .map_err(|e| format!("Failed to update admin portal TLS paths in database: {}", e))?;
        return Ok((cert_path, key_path));
    } else {
        // For regular site, update the sites table
        let sql_update = format!(
            "UPDATE sites SET tls_cert_path = '{}', tls_key_path = '{}' WHERE id = '{}';",
            cert_path.clone(),
            key_path.clone(),
            site.id
        );
        connection.execute(sql_update.as_str()).map_err(|e| format!("Failed to update site TLS paths in database: {}", e))?;
    }

    Ok((cert_path, key_path))
}

// Custom certificate resolver that provides fallback when SNI doesn't match
#[derive(Debug)]
struct FallbackCertResolver {
    sni_resolver: ResolvesServerCertUsingSni,
    fallback_cert: Option<std::sync::Arc<RustlsCertifiedKey>>,
}

impl FallbackCertResolver {
    fn new(sni_resolver: ResolvesServerCertUsingSni) -> Self {
        Self { sni_resolver, fallback_cert: None }
    }

    fn with_fallback(mut self, cert: std::sync::Arc<RustlsCertifiedKey>) -> Self {
        self.fallback_cert = Some(cert);
        self
    }
}

impl ResolvesServerCert for FallbackCertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<std::sync::Arc<RustlsCertifiedKey>> {
        // First try the SNI resolver
        if let Some(cert) = self.sni_resolver.resolve(client_hello) {
            return Some(cert);
        }

        // If SNI doesn't match, use fallback certificate
        self.fallback_cert.clone()
    }
}

// Build a TLS acceptor that selects certificates per-site using SNI
pub async fn build_tls_acceptor(binding: &Binding) -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let provider = rustls::crypto::aws_lc_rs::default_provider();

    // Create SNI resolver
    let mut resolver = ResolvesServerCertUsingSni::new();
    let mut have_default = false;
    let mut site_added = false;
    let mut fallback_certificate: Option<std::sync::Arc<RustlsCertifiedKey>> = None;

    // Get the running state
    let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
    let binding_site_cache = running_state.get_binding_site_cache();
    let sites = binding_site_cache.get_sites_for_binding(&binding.id);

    for site in sites.iter().filter(|s| s.is_enabled) {
        // Determine SANs: handle wildcard sites specially
        let mut sans: Vec<String> = site.hostnames.iter().cloned().filter(|h| !h.trim().is_empty() && h != "*").collect();
        let has_wildcard = site.hostnames.contains(&"*".to_string());

        if sans.is_empty() || has_wildcard {
            // For wildcard sites or empty hostnames, generate a cert that works with common local addresses
            sans.clear();
            sans.extend(vec![
                "localhost".to_string(),
                //     "127.0.0.1".to_string(),
                //     "::1".to_string(),
            ]);

            // Add the machine's hostname if available
            if let Ok(hostname) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
                if !hostname.is_empty() && !sans.contains(&hostname) {
                    sans.push(hostname.to_lowercase());
                }
            }
        }

        let (cert_chain, priv_key) = if site.tls_cert_path.len() > 0 && site.tls_key_path.len() > 0 {
            // Load from PEM files
            let cert_file = std::fs::File::open(&site.tls_cert_path).map_err(|e| format!("Failed to open TLS cert file {}: {}", site.tls_cert_path, e))?;
            let key_file = std::fs::File::open(&site.tls_key_path).map_err(|e| format!("Failed to open TLS key file {}: {}", site.tls_key_path, e))?;

            let mut cert_reader = BufReader::new(cert_file);
            let mut key_reader = BufReader::new(key_file);

            let certs: Result<Vec<CertificateDer<'static>>, _> = rustls_pemfile::certs(&mut cert_reader).collect();
            let cert_chain = certs.map_err(|e| format!("Failed to parse TLS cert file {}: {}", site.tls_cert_path, e))?;

            let key_result = rustls_pemfile::private_key(&mut key_reader).map_err(|e| format!("Failed to parse TLS key file {}: {}", site.tls_key_path, e))?;
            let priv_key = key_result.ok_or_else(|| format!("No private key found in {}", site.tls_key_path))?;

            (cert_chain, priv_key)
        } else if site.tls_cert_content.len() > 0 && site.tls_key_content.len() > 0 {
            // Parse from content strings
            let mut cert_cursor = std::io::Cursor::new(site.tls_cert_content.as_bytes());
            let mut key_cursor = std::io::Cursor::new(site.tls_key_content.as_bytes());

            let certs: Result<Vec<CertificateDer<'static>>, _> = rustls_pemfile::certs(&mut cert_cursor).collect();
            let cert_chain = certs.map_err(|e| format!("Failed to parse TLS cert PEM content: {}", e))?;

            let key_result = rustls_pemfile::private_key(&mut key_cursor).map_err(|e| format!("Failed to parse TLS key PEM content: {}", e))?;
            let priv_key = key_result.ok_or_else(|| "No private key found in PEM content".to_string())?;

            (cert_chain, priv_key)
        } else {
            // Generate self-signed cert with comprehensive SAN list
            debug(format!("Generating self-signed certificate for site with hostnames: {:?}", sans));
            let rcgen::CertifiedKey { cert, signing_key } = rcgen::generate_simple_self_signed(sans.clone()).map_err(|e| format!("Failed to generate self-signed cert: {}", e))?;
            let cert_pem = cert.pem();
            let key_pem = signing_key.serialize_pem();

            let mut cert_cursor = std::io::Cursor::new(cert_pem.as_bytes());
            let mut key_cursor = std::io::Cursor::new(key_pem.as_bytes());

            let certs: Result<Vec<CertificateDer<'static>>, _> = rustls_pemfile::certs(&mut cert_cursor).collect();
            let cert_chain = certs.map_err(|e| format!("Failed to parse generated TLS cert PEM content: {}", e))?;

            let key_result = rustls_pemfile::private_key(&mut key_cursor).map_err(|e| format!("Failed to parse generated TLS key PEM content: {}", e))?;
            let priv_key = key_result.ok_or_else(|| "No private key found in generated PEM content".to_string())?;

            // Persist generated cert/key to disk and update the site configuration
            match persist_generated_tls_for_site(site, &cert_pem, &key_pem, binding.is_admin).await {
                Ok(cert_paths) => {
                    debug(format!("Successfully persisted generated certificate to: {:?}", cert_paths));
                }
                Err(e) => {
                    warn(format!("Failed to persist generated certificate (will continue with in-memory cert): {}", e));
                }
            }

            (cert_chain, priv_key)
        };

        if cert_chain.is_empty() {
            warn(format!("No valid certificates found in TLS cert for site with hostnames {:?}", site.hostnames));
            continue;
        }

        // Build a signing key and certified key for rustls
        let signing_key = aws_lc_rs::sign::any_supported_type(&priv_key).map_err(|e| format!("Unsupported private key type for: {}", e))?;
        let certified = RustlsCertifiedKey::new(cert_chain.clone(), signing_key);
        let certified_arc = std::sync::Arc::new(certified);

        // Use the first certificate as fallback for cases where SNI doesn't match
        if fallback_certificate.is_none() {
            fallback_certificate = Some(certified_arc.clone());
        }

        // Add each SAN as a mapping
        for name in &sans {
            // Accept wildcard names like "*.example.com" if provided
            match resolver.add(name, certified_arc.as_ref().clone()) {
                Ok(()) => {
                    site_added = true;
                }
                Err(e) => {
                    debug(format!("Failed to add SNI name '{}': {:?}", name, e));
                }
            }
        }

        // For wildcard sites, also add some common IP addresses and variations
        if has_wildcard {
            let additional_names = vec![
                //   "127.0.0.1",
                //   "::1",
                "localhost",
            ];

            for name in additional_names {
                if !sans.contains(&name.to_string()) {
                    match resolver.add(name, certified_arc.as_ref().clone()) {
                        Ok(()) => {
                            site_added = true;
                        }
                        Err(e) => {
                            debug(format!("Failed to add additional SNI name '{}': {:?}", name, e));
                        }
                    }
                }
            }
        } // If site is default or hostname includes wildcard "*", set as default cert
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
        let signing_key = aws_lc_rs::sign::any_supported_type(&key_der).map_err(|e| format!("Unsupported private key type for rustls: {}", e))?;
        let certified = RustlsCertifiedKey::new(vec![cert_der], signing_key);

        let certified_arc = std::sync::Arc::new(certified);

        // Use this as fallback if we don't have one yet
        if fallback_certificate.is_none() {
            fallback_certificate = Some(certified_arc.clone());
        }

        // Add the fallback certificate to the resolver
        if let Err(e) = resolver.add("localhost", certified_arc.as_ref().clone()) {
            warn(format!("Failed to add fallback certificate for localhost: {:?}", e));
        } else {
            site_added = true;
        }
    }

    if !site_added {
        return Err("No valid TLS certificates could be configured for this binding".into());
    }

    // Create a fallback certificate resolver that can handle cases where SNI doesn't match
    let mut fallback_resolver = FallbackCertResolver::new(resolver);
    if let Some(fallback_cert) = fallback_certificate {
        fallback_resolver = fallback_resolver.with_fallback(fallback_cert);
    }

    let mut server_config = RustlsServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .map_err(|_| "Protocol versions unavailable")?
        .with_no_client_auth()
        .with_cert_resolver(std::sync::Arc::new(fallback_resolver));

    // Enable ALPN for HTTP/2 and HTTP/1.1 (prefer h2)
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    Ok(TlsAcceptor::from(std::sync::Arc::new(server_config)))
}
