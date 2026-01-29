// ============================================================================
// SHARED ACME MANAGER
// ============================================================================
//
// This module provides a single, shared ACME client instance for all TLS bindings.
// Instead of creating one ACME client per binding (which would cause rate-limiting
// issues and duplicate certificate requests), we create one shared manager that:
//   - Holds a single AcmeConfig and AcmeState
//   - Collects all ACME-enabled domains across all bindings
//   - Provides a shared resolver (Arc<ResolvesServerCertAcme>) to all bindings
//   - Runs a single background task to poll for certificate updates
//   - Responds to shutdown/stop_services/reload_configuration triggers
// ============================================================================

use crate::core::running_state_manager::get_running_state_manager;
use crate::core::triggers::get_trigger_handler;
use crate::logging::syslog::{debug, trace};
use rustls_acme::caches::DirCache;
use rustls_acme::{AcmeConfig, ResolvesServerCertAcme};
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

/// Global singleton for the shared ACME manager (can be reset on configuration reload)
static SHARED_ACME_MANAGER: RwLock<Option<SharedAcmeManager>> = RwLock::const_new(None);

/// Holds the shared ACME state and resolver that can be used across all TLS bindings
pub struct SharedAcmeManager {
    /// The ACME resolver used to resolve certificates for ACME-managed domains
    resolver: Arc<ResolvesServerCertAcme>,
    /// All domains managed by this ACME instance
    domains: std::collections::HashSet<String>,
    /// Cancellation token for the polling task
    polling_cancel_token: CancellationToken,
}

impl SharedAcmeManager {
    /// Get the shared ACME resolver
    pub fn resolver(&self) -> Arc<ResolvesServerCertAcme> {
        self.resolver.clone()
    }

    /// Check if a domain is managed by ACME
    #[allow(dead_code)]
    pub fn is_acme_domain(&self, domain: &str) -> bool {
        self.domains.contains(&domain.to_lowercase())
    }

    /// Get all ACME-managed domains
    pub fn domains(&self) -> &std::collections::HashSet<String> {
        &self.domains
    }
}

/// Clear and shutdown the shared ACME manager. This should be called before
/// reinitializing on configuration reload, or during shutdown.
pub async fn shutdown_shared_acme_manager() {
    let mut manager = SHARED_ACME_MANAGER.write().await;
    if let Some(existing) = manager.take() {
        debug("Shutting down shared ACME manager".to_string());
        existing.polling_cancel_token.cancel();
    }
}

/// Initialize or reinitialize the shared ACME manager. This should be called during
/// server startup and on configuration reload. It will shut down any existing manager
/// before creating a new one.
///
/// Returns Ok(()) if initialization succeeded (or ACME is not configured).
pub async fn initialize_shared_acme_manager() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // First, shut down any existing manager
    shutdown_shared_acme_manager().await;

    // Create new manager
    let new_manager = create_shared_acme_manager().await?;

    // Store the new manager
    let mut manager = SHARED_ACME_MANAGER.write().await;
    *manager = new_manager;

    Ok(())
}

/// Get the shared ACME manager if it has been initialized
pub async fn get_shared_acme_manager_async() -> Option<Arc<ResolvesServerCertAcme>> {
    let manager = SHARED_ACME_MANAGER.read().await;
    manager.as_ref().map(|m| m.resolver())
}

/// Get ACME domains from the shared manager
pub async fn get_shared_acme_domains() -> std::collections::HashSet<String> {
    let manager = SHARED_ACME_MANAGER.read().await;
    manager.as_ref().map(|m| m.domains().clone()).unwrap_or_default()
}

/// Internal function to create the shared ACME manager
async fn create_shared_acme_manager() -> Result<Option<SharedAcmeManager>, Box<dyn std::error::Error + Send + Sync>> {
    let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration().await;

    let tls_settings = &config.core.tls_settings;

    // ACME requires an account email to create/register the account.
    if tls_settings.account_email.trim().is_empty() {
        debug("ACME not enabled: no account email configured".to_string());
        return Ok(None);
    }

    // Collect all ACME-enabled domains across all TLS bindings
    let mut all_domains: BTreeSet<String> = BTreeSet::new();

    let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
    let binding_site_cache = running_state.get_binding_site_cache();

    for binding in &config.bindings {
        if !binding.is_tls {
            continue;
        }

        let sites = binding_site_cache.get_sites_for_binding(&binding.id);

        for site in sites.iter().filter(|s| s.is_enabled && s.tls_automatic_enabled) {
            for hostname in &site.hostnames {
                let h = hostname.trim().to_lowercase();
                if h.is_empty() || h == "*" {
                    continue;
                }

                // Wildcards require DNS-01, which rustls-acme does not support.
                if h.contains('*') {
                    continue;
                }

                // Avoid obviously-non-public hostnames.
                if h == "localhost" {
                    continue;
                }

                // Minimal sanity: must look like a DNS name.
                if !h.contains('.') {
                    continue;
                }

                all_domains.insert(h);
            }
        }
    }

    if all_domains.is_empty() {
        debug("ACME not enabled: no valid domains found with tls_automatic_enabled".to_string());
        return Ok(None);
    }

    let cache_dir ="certs/cache".to_string();

    // Ensure cache directory exists.
    fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| format!("Failed to create ACME cache directory '{}': {}", cache_dir, e))?;

    let provider = rustls::crypto::aws_lc_rs::default_provider();

    let mut acme_config = AcmeConfig::new_with_provider(all_domains.iter().cloned().collect::<Vec<_>>(), provider.into())
        .cache_with_boxed_err(DirCache::new(cache_dir.clone()))
        .directory_lets_encrypt(!tls_settings.use_staging_server);

    // rustls-acme requires `mailto:` prefix.
    acme_config = acme_config.contact_push(format!("mailto:{}", tls_settings.account_email.trim()));

    trace(format!(
        "ACME initialized (staging={}, cache_dir='{}') for {} domains: {:?}",
        tls_settings.use_staging_server,
        cache_dir,
        all_domains.len(),
        all_domains
    ));

    // Create the ACME state - this is the single instance that will handle all certificate operations
    let acme_state = acme_config.state();
    let resolver = acme_state.resolver();

    // Create a cancellation token for the polling task
    let polling_cancel_token = CancellationToken::new();

    // Spawn a single background task to poll the ACME state for certificate updates
    spawn_acme_polling_task(acme_state, polling_cancel_token.clone());

    let domains_set: std::collections::HashSet<String> = all_domains.into_iter().collect();

    Ok(Some(SharedAcmeManager {
        resolver,
        domains: domains_set,
        polling_cancel_token,
    }))
}

/// Spawn a background task that polls the ACME state for certificate acquisition and renewal.
/// The task will stop when the cancellation token is cancelled or when shutdown/stop_services triggers fire.
fn spawn_acme_polling_task(
    mut acme_state: rustls_acme::AcmeState<Box<dyn std::fmt::Debug>, Box<dyn std::fmt::Debug>>,
    cancel_token: CancellationToken,
) {
    tokio::spawn(async move {
        trace("ACME background polling task started".to_string());

        // Get shutdown and stop_services triggers
        let triggers = get_trigger_handler();
        let shutdown_token = triggers
            .get_trigger("shutdown")
            .map(|t| {
                // We need to clone the token from inside the RwLock
                // Use try_read to avoid blocking, fall back to a new token if locked
                t.try_read().map(|guard| guard.clone()).unwrap_or_else(|_| CancellationToken::new())
            })
            .unwrap_or_else(|| CancellationToken::new());

        let stop_services_token = triggers
            .get_trigger("stop_services")
            .map(|t| {
                t.try_read().map(|guard| guard.clone()).unwrap_or_else(|_| CancellationToken::new())
            })
            .unwrap_or_else(|| CancellationToken::new());

        // Poll the ACME state to handle certificate acquisition and renewal
        loop {
            tokio::select! {
                // Check for cancellation (from manager shutdown)
                _ = cancel_token.cancelled() => {
                    debug("ACME polling task cancelled by manager shutdown".to_string());
                    break;
                }
                // Check for shutdown trigger
                _ = shutdown_token.cancelled() => {
                    debug("ACME polling task stopping due to shutdown signal".to_string());
                    break;
                }
                // Check for stop_services trigger
                _ = stop_services_token.cancelled() => {
                    debug("ACME polling task stopping due to stop_services signal".to_string());
                    break;
                }
                // Poll for ACME events
                event = acme_state.next() => {
                    match event {
                        Some(Ok(ok)) => {
                            trace(format!("ACME event: {:?}", ok));
                        }
                        Some(Err(err)) => {
                            debug(format!("ACME error: {:?}", err));
                        }
                        None => {
                            // Stream ended
                            debug("ACME event stream ended".to_string());
                            break;
                        }
                    }
                }
            }
        }

        debug("ACME background polling task ended".to_string());
    });
}
