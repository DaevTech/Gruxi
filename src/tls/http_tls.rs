use crate::core::running_state_manager::get_running_state_manager;
use crate::file::app_paths::get_app_paths;
use crate::tls::shared_acme_manager::{get_shared_acme_domains, get_shared_acme_manager_async};
use crate::{debug, error, info, warn};
use rand;
use rustls::crypto::aws_lc_rs;
use rustls_acme::ResolvesServerCertAcme;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::io::BufReader;
use std::path::Path;
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
    let app_paths = get_app_paths();
    let dir = app_paths.certificates_dir.display().to_string();
    fs::create_dir_all(&dir).await.map_err(|e| format!("Failed to create certs directory '{}': {}", dir, e))?;

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

// Unified certificate resolver that combines ACME certificates with manual/fallback certificates.
// This allows serving both ACME-acquired certificates and manually configured certificates
// from the same TLS binding.
#[derive(Debug)]
pub struct UnifiedCertResolver {
    /// The ACME resolver handles TLS-ALPN-01 challenges and serves ACME-acquired certificates
    acme_resolver: Option<std::sync::Arc<ResolvesServerCertAcme>>,
    /// SNI-based resolver for manually configured certificates
    sni_resolver: ResolvesServerCertUsingSni,
    /// Fallback certificate when no SNI match is found
    fallback_cert: Option<std::sync::Arc<RustlsCertifiedKey>>,
    /// Domains that are managed by ACME (should not use manual certs)
    acme_domains: std::collections::HashSet<String>,
}

impl UnifiedCertResolver {
    pub fn new(acme_resolver: Option<std::sync::Arc<ResolvesServerCertAcme>>, acme_domains: std::collections::HashSet<String>) -> Self {
        Self {
            acme_resolver,
            sni_resolver: ResolvesServerCertUsingSni::new(),
            fallback_cert: None,
            acme_domains,
        }
    }

    pub fn add_manual_cert(&mut self, hostname: &str, cert: RustlsCertifiedKey) -> Result<(), rustls::Error> {
        self.sni_resolver.add(hostname, cert)
    }

    pub fn set_fallback(&mut self, cert: std::sync::Arc<RustlsCertifiedKey>) {
        self.fallback_cert = Some(cert);
    }

    /// Check if a domain is managed by ACME
    fn is_acme_domain(&self, domain: &str) -> bool {
        self.acme_domains.contains(&domain.to_lowercase())
    }
}

impl ResolvesServerCert for UnifiedCertResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<std::sync::Arc<RustlsCertifiedKey>> {
        // Check if this is an ACME TLS-ALPN-01 challenge
        // This must be checked first and handled by the ACME resolver
        let is_acme_challenge = rustls_acme::is_tls_alpn_challenge(&client_hello);

        if is_acme_challenge {
            // For ACME challenges, delegate to the ACME resolver
            if let Some(ref acme_resolver) = self.acme_resolver {
                return acme_resolver.resolve(client_hello);
            }
            // No ACME resolver, but got an ACME challenge - this shouldn't happen in normal operation
            return None;
        }

        // For regular TLS connections, get the SNI name first (before consuming client_hello)
        let sni_name = client_hello.server_name().map(|s| s.to_lowercase());

        // Check if this domain is managed by ACME
        if let Some(ref domain) = sni_name {
            if self.is_acme_domain(domain) {
                // For ACME-managed domains, try the ACME resolver
                if let Some(ref acme_resolver) = self.acme_resolver {
                    // The ACME resolver will return the ACME-acquired certificate for non-challenge requests
                    if let Some(cert) = acme_resolver.resolve(client_hello) {
                        return Some(cert);
                    }
                }
                // If ACME resolver returns None, fall through to fallback
            } else {
                // Not an ACME domain, try the manual SNI resolver
                if let Some(cert) = self.sni_resolver.resolve(client_hello) {
                    return Some(cert);
                }
            }
        } else {
            // No SNI provided, try the SNI resolver anyway (it might have a default)
            if let Some(cert) = self.sni_resolver.resolve(client_hello) {
                return Some(cert);
            }
        }

        // If no match found, use the fallback certificate
        self.fallback_cert.clone()
    }
}

/// Helper function to get domains that are ACME-enabled for a binding
pub async fn get_acme_domains_for_binding(binding: &Binding) -> std::collections::HashSet<String> {
    let mut domains = std::collections::HashSet::new();

    if !binding.is_tls {
        return domains;
    }

    let running_state = get_running_state_manager().await.get_running_state();
    let binding_site_cache = running_state.get_binding_site_cache();
    let sites = binding_site_cache.get_sites_for_binding(&binding.id);

    for site in sites.iter().filter(|s| s.is_enabled && s.tls_automatic_enabled) {
        for hostname in &site.hostnames {
            let h = hostname.trim().to_lowercase();
            if h.is_empty() || h == "*" || h.contains('*') || h == "localhost" || !h.contains('.') {
                continue;
            }
            domains.insert(h);
        }
    }

    domains
}

/// Build a unified certificate resolver that handles both ACME and manual certificates.
/// Uses the shared ACME manager if available.
pub async fn build_unified_cert_resolver(binding: &Binding, acme_resolver: Option<std::sync::Arc<ResolvesServerCertAcme>>) -> Result<UnifiedCertResolver, Box<dyn std::error::Error + Send + Sync>> {
    // Get ACME domains from the shared manager if available, otherwise use binding-specific lookup
    let acme_domains = {
        let shared_domains = get_shared_acme_domains().await;
        if !shared_domains.is_empty() {
            shared_domains
        } else {
            get_acme_domains_for_binding(binding).await
        }
    };

    debug!("Building unified cert resolver for {}:{} with {} ACME domains", binding.ip, binding.port, acme_domains.len());

    let mut resolver = UnifiedCertResolver::new(acme_resolver, acme_domains.clone());
    let mut fallback_certificate: Option<std::sync::Arc<RustlsCertifiedKey>> = None;
    let mut cert_added = false;

    // Get sites for this binding
    let running_state = get_running_state_manager().await.get_running_state();
    let binding_site_cache = running_state.get_binding_site_cache();
    let sites = binding_site_cache.get_sites_for_binding(&binding.id);

    // Skip sites that are disabled or have ACME enabled - they'll be handled by the ACME resolver
    for site in sites.iter().filter(|s| s.is_enabled && !s.tls_automatic_enabled) {
        // Determine SANs for this site
        let mut sans: Vec<String> = site.hostnames.iter().cloned().filter(|h| !h.trim().is_empty() && h != "*").collect();
        let has_wildcard = site.hostnames.contains(&"*".to_string());

        if sans.is_empty() || has_wildcard {
            // For wildcard sites or empty hostnames, generate a cert for common local addresses
            sans.clear();
            sans.push("localhost".to_string());

            // Add machine's hostname if available
            if let Ok(hostname) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
                if !hostname.is_empty() && !sans.contains(&hostname) {
                    sans.push(hostname.to_lowercase());
                }
            }
        }

        // Load or generate certificate, if possible
        let mut certificates = None;

        // Try to load from disk first
        if certificates.is_none() {
            let certificates_from_disk = get_certificates_from_disk(site);
            certificates = match certificates_from_disk {
                Some(certificates) => Some(certificates),
                None => {
                    // If paths were specified but loading failed, log as error and ignore site
                    // We dont want to fallback to other methods if user explicitly set paths
                    // And also, we dont want to overwrite it with a self signed cert
                    if !site.tls_cert_path.is_empty() && !site.tls_key_path.is_empty() {
                        error!(
                            "Site: '{}' with hostnames '{:?}' Failed to load TLS certificates from disk paths - Administrator is needed to fix this issue",
                            site.id, site.hostnames
                        );
                        continue;
                    }
                    None
                }
            };
        }

        // Secondary, we try to load content from config
        if certificates.is_none() {
            let certificates_from_config = get_certificates_from_config(site);
            certificates = match certificates_from_config {
                Some(certificates) => Some(certificates),
                None => {
                    // If cert content were specified but loading failed, log as error and ignore site
                    // We dont want to fallback to other methods if user explicitly set content of certificates
                    // And also, we dont want to overwrite it with a self signed cert
                    if !site.tls_cert_content.is_empty() && !site.tls_key_content.is_empty() {
                        error!(
                            "Site: '{}' with hostnames '{:?}' Failed to load TLS certificates from content fields - Administrator is needed to fix this issue",
                            site.id, site.hostnames
                        );
                        continue;
                    }
                    None
                }
            };
        }

        // Final attempt is to create self-signed certificates
        if certificates.is_none() {
            let generated_certs = generate_self_signed_certificate_and_persist(site, sans.clone(), binding.is_admin).await;
            if let Some(certs) = generated_certs {
                info!("Generated self-signed certificate, because of missing certificates, for site '{}' with hostnames: {:?}", site.id, sans);
                certificates = Some(certs);
            }
        }

        // Extract cert chain and private key, if any could be found/created
        let (cert_chain, priv_key) = match certificates {
            Some(certs) => certs,
            None => {
                warn!("No valid TLS certificate could be obtained for site '{}' with hostnames {:?}", site.id, site.hostnames);
                continue;
            }
        };

        // Build certified key
        let signing_key = aws_lc_rs::sign::any_supported_type(&priv_key).map_err(|e| format!("Unsupported private key type: {}", e))?;
        let certified = RustlsCertifiedKey::new(cert_chain, signing_key);
        let certified_arc = std::sync::Arc::new(certified);

        // Set as fallback if this is the first certificate
        if fallback_certificate.is_none() {
            fallback_certificate = Some(certified_arc.clone());
        }

        // Add certificate for each hostname
        for name in &sans {
            match resolver.add_manual_cert(name, certified_arc.as_ref().clone()) {
                Ok(()) => {
                    debug!("Added manual cert for hostname '{}'", name);
                    cert_added = true;
                }
                Err(e) => {
                    debug!("Failed to add SNI name '{}': {:?}", name, e);
                }
            }
        }

        // For wildcard sites, add localhost
        if has_wildcard {
            if !sans.contains(&"localhost".to_string()) {
                if let Err(e) = resolver.add_manual_cert("localhost", certified_arc.as_ref().clone()) {
                    debug!("Failed to add localhost for wildcard site: {:?}", e);
                } else {
                    cert_added = true;
                }
            }
        }
    }

    // If no manual certs were added but we have ACME domains, that's fine
    // If no certs at all, generate a fallback
    if !cert_added && acme_domains.is_empty() {
        // Generate a fallback self-signed cert
        let rcgen::CertifiedKey { cert, signing_key } =
            rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).map_err(|e| format!("Failed to generate fallback self-signed cert: {}", e))?;
        let cert_der = CertificateDer::from(cert.der().to_vec());
        let key_der = PrivateKeyDer::try_from(signing_key.serialize_der()).map_err(|e| format!("Invalid key DER: {}", e))?;
        let signing_key = aws_lc_rs::sign::any_supported_type(&key_der).map_err(|e| format!("Unsupported private key type: {}", e))?;
        let certified = RustlsCertifiedKey::new(vec![cert_der], signing_key);
        let certified_arc = std::sync::Arc::new(certified);

        if fallback_certificate.is_none() {
            fallback_certificate = Some(certified_arc.clone());
        }

        if let Err(e) = resolver.add_manual_cert("localhost", certified_arc.as_ref().clone()) {
            warn!("Failed to add fallback certificate for localhost: {:?}", e);
        }
    }

    // Set fallback certificate
    if let Some(fallback_cert) = fallback_certificate {
        resolver.set_fallback(fallback_cert);
    }

    Ok(resolver)
}

fn generate_self_signed_certificate(sans: Vec<String>) -> Option<(Vec<CertificateDer<'static>>, String, PrivateKeyDer<'static>, String)> {
    // Generate self-signed certificate
    debug!("Generating self-signed certificate for site with hostnames: {:?}", sans);
    let gen_key_result = rcgen::generate_simple_self_signed(sans);
    let gen_key = match gen_key_result {
        Ok(k) => k,
        Err(e) => {
            warn!("Failed to generate self-signed cert: {}", e);
            return None;
        }
    };
    let cert = gen_key.cert;
    let signing_key = gen_key.signing_key;

    let cert_pem = cert.pem();
    let key_pem = signing_key.serialize_pem();

    let mut cert_cursor = std::io::Cursor::new(cert_pem.as_bytes());
    let mut key_cursor = std::io::Cursor::new(key_pem.as_bytes());

    let certs_result: Result<Vec<CertificateDer<'static>>, _> = rustls_pemfile::certs(&mut cert_cursor).collect();
    let cert_chain = match certs_result {
        Ok(certs) => certs,
        Err(e) => {
            warn!("Failed to parse generated TLS cert PEM content: {}", e);
            return None;
        }
    };

    let key_result = rustls_pemfile::private_key(&mut key_cursor);
    let priv_key = match key_result {
        Ok(Some(key)) => key,
        Ok(None) => {
            warn!("No private key found in generated PEM content");
            return None;
        }
        Err(e) => {
            warn!("Failed to parse generated TLS key PEM content: {}", e);
            return None;
        }
    };

    Some((cert_chain, cert_pem, priv_key, key_pem))
}

async fn generate_self_signed_certificate_and_persist(site: &Site, sans: Vec<String>, is_admin: bool) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let (cert_chain, cert_pem, priv_key, key_pem) = match generate_self_signed_certificate(sans) {
        Some(certs) => certs,
        None => {
            return None;
        }
    };

    // Persist generated cert/key to disk
    match persist_generated_tls_for_site(site, &cert_pem, &key_pem, is_admin).await {
        Ok(cert_paths) => {
            debug!("Successfully persisted generated certificate to: {:?}", cert_paths);
        }
        Err(e) => {
            warn!("Failed to persist generated certificate (will continue with in-memory cert): {}", e);
        }
    }

    Some((cert_chain, priv_key))
}

fn get_certificates_from_config(site: &Site) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    // Check if fields are filled out
    if site.tls_cert_content.is_empty() || site.tls_key_content.is_empty() {
        return None;
    }

    // Parse from content strings
    let mut cert_cursor = std::io::Cursor::new(site.tls_cert_content.as_bytes());
    let mut key_cursor = std::io::Cursor::new(site.tls_key_content.as_bytes());

    let certs_result: Result<Vec<CertificateDer<'static>>, _> = rustls_pemfile::certs(&mut cert_cursor).collect();
    let cert_chain = match certs_result {
        Ok(certs) => certs,
        Err(e) => {
            warn!("Site: '{}' Failed to parse TLS cert PEM content: {}", site.id, e);
            return None;
        }
    };

    let key_result = rustls_pemfile::private_key(&mut key_cursor);
    let priv_key = match key_result {
        Ok(Some(key)) => key,
        Ok(None) => {
            warn!("Site: '{}' No private key found in PEM content", site.id);
            return None;
        }
        Err(e) => {
            warn!("Site: '{}' Failed to parse TLS key PEM content: {}", site.id, e);
            return None;
        }
    };

    Some((cert_chain, priv_key))
}

fn get_certificates_from_disk(site: &Site) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    // Load certificates from disk if paths are provided
    if site.tls_cert_path.is_empty() || site.tls_key_path.is_empty() {
        return None;
    }

    // Load from PEM files
    let path_test = Path::new(&site.tls_cert_path);
    let cert_file_result = if path_test.is_absolute() {
        std::fs::File::open(&site.tls_cert_path)
    } else {
        // If relative path, we assume it's relative to the certificates directory
        let app_paths = get_app_paths();
        let full_path = app_paths.certificates_dir.join(&site.tls_cert_path);
        std::fs::File::open(full_path)
    };

    let cert_file = match cert_file_result {
        Ok(f) => f,
        Err(e) => {
            warn!("Site: '{}' Failed to open TLS cert file '{}': {}", site.id, site.tls_cert_path, e);
            return None;
        }
    };

    let key_file_result = if Path::new(&site.tls_key_path).is_absolute() {
        std::fs::File::open(&site.tls_key_path)
    } else {
        let app_paths = get_app_paths();
        let full_path = app_paths.certificates_dir.join(&site.tls_key_path);
        std::fs::File::open(full_path)
    };
    let key_file = match key_file_result {
        Ok(f) => f,
        Err(e) => {
            warn!("Site: '{}' Failed to open TLS key file '{}': {}", site.id, site.tls_key_path, e);
            return None;
        }
    };

    let mut cert_reader = BufReader::new(cert_file);
    let mut key_reader = BufReader::new(key_file);

    let certs_result: Result<Vec<CertificateDer<'static>>, _> = rustls_pemfile::certs(&mut cert_reader).collect();
    let cert_chain = match certs_result {
        Ok(certs) => certs,
        Err(e) => {
            warn!("Site: '{}' Failed to parse TLS cert file '{}': {}", site.id, site.tls_cert_path, e);
            return None;
        }
    };

    let key_result = rustls_pemfile::private_key(&mut key_reader);
    let priv_key = match key_result {
        Ok(Some(key)) => key,
        Ok(None) => {
            warn!("Site: '{}' No private key found in '{}'", site.id, site.tls_key_path);
            return None;
        }
        Err(e) => {
            warn!("Site: '{}' Failed to parse TLS key file '{}': {}", site.id, site.tls_key_path, e);
            return None;
        }
    };

    return Some((cert_chain, priv_key));
}

/// Build a unified TLS acceptor that handles both ACME and manual certificates.
/// Uses the shared ACME manager if available, ensuring only one ACME client exists globally.
/// Returns the TlsAcceptor only (ACME polling is handled by the shared manager).
pub async fn build_unified_tls_acceptor(binding: &Binding) -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let provider = rustls::crypto::aws_lc_rs::default_provider();

    // Get the shared ACME resolver if available (already initialized during server startup)
    let acme_resolver = get_shared_acme_manager_async().await;
    let has_acme = acme_resolver.is_some();

    // Build the unified cert resolver with ACME and manual certs
    let unified_resolver = build_unified_cert_resolver(binding, acme_resolver).await?;

    // Build ServerConfig with our unified resolver
    let mut server_config = RustlsServerConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .map_err(|_| "Protocol versions unavailable")?
        .with_no_client_auth()
        .with_cert_resolver(std::sync::Arc::new(unified_resolver));

    // Enable ALPN for HTTP/2 and HTTP/1.1, and add ACME TLS-ALPN-01 protocol if ACME is enabled
    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    if has_acme {
        // TLS-ALPN-01 protocol identifier for ACME challenges
        server_config.alpn_protocols.push(b"acme-tls/1".to_vec());
    }

    let tls_acceptor = TlsAcceptor::from(std::sync::Arc::new(server_config));

    Ok(tls_acceptor)
}
