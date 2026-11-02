use crate::core::running_state_manager::get_running_state_manager;
use crate::core::triggers::get_trigger_handler;
use crate::file::app_paths::get_app_paths;
use crate::{debug, trace, warn};
use rustls_acme::caches::DirCache;
use rustls_acme::{AcmeConfig, ResolvesServerCertAcme};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, RwLock as StdRwLock};
use std::time::Duration;
use tokio::fs;
use tokio::sync::Mutex;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

/// Each hostname has its own independent ACME state (own order, own cached certificate).
/// A failing or pending host therefore never affects the certificates served for other hosts.
struct HostEntry {
    cancel_token: CancellationToken,
}

struct ManagerState {
    /// Account settings the running states were created with (email, staging). A change restarts all hosts.
    account_key: String,
    hosts: HashMap<String, HostEntry>,
}

static MANAGER_STATE: Mutex<Option<ManagerState>> = Mutex::const_new(None);

/// Per-hostname resolvers, readable synchronously from the TLS handshake path.
/// Updated incrementally so existing hosts keep serving their certificate during reloads.
static HOST_RESOLVERS: StdRwLock<Option<HashMap<String, Arc<ResolvesServerCertAcme>>>> = StdRwLock::new(None);

/// Get the ACME resolver for a hostname, if the host is ACME-managed.
pub fn get_acme_resolver_for_host(hostname: &str) -> Option<Arc<ResolvesServerCertAcme>> {
    let guard = HOST_RESOLVERS.read().ok()?;
    guard.as_ref()?.get(&hostname.to_lowercase()).cloned()
}

/// Whether ACME is configured (account email set), so bindings should accept the ACME TLS-ALPN protocol.
pub fn is_acme_configured() -> bool {
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration();
    !config.core.tls_settings.account_email.trim().is_empty()
}

/// Shut down all ACME states and drop their resolvers. Called on shutdown.
pub async fn shutdown_shared_acme_manager() {
    let mut state = MANAGER_STATE.lock().await;
    if let Some(existing) = state.take() {
        debug!("Shutting down shared ACME manager");
        for (_, host) in existing.hosts {
            host.cancel_token.cancel();
        }
    }
    if let Ok(mut resolvers) = HOST_RESOLVERS.write() {
        *resolvers = None;
    }
}

/// Initialize or reconcile the ACME hosts with the current configuration.
/// Hosts that are unchanged keep their running state and certificate; only added and removed hosts are touched.
pub async fn initialize_shared_acme_manager() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration();
    let tls_settings = &config.core.tls_settings;

    // ACME requires an account email to create/register the account.
    if tls_settings.account_email.trim().is_empty() {
        debug!("ACME not enabled: no account email configured");
        shutdown_shared_acme_manager().await;
        return Ok(());
    }

    let desired = collect_acme_domains().await;
    if desired.is_empty() {
        debug!("ACME not enabled: no valid domains found with tls_automatic_enabled");
        shutdown_shared_acme_manager().await;
        return Ok(());
    }

    let account_key = format!("{}|{}", tls_settings.account_email.trim().to_lowercase(), tls_settings.use_staging_server);

    let app_paths = get_app_paths();
    let cache_dir = app_paths.certificates_dir.join("cache").display().to_string();
    fs::create_dir_all(&cache_dir)
        .await
        .map_err(|e| format!("Failed to create ACME cache directory '{}': {}", cache_dir, e))?;

    let mut state_guard = MANAGER_STATE.lock().await;

    // Account settings changed: restart everything.
    if state_guard.as_ref().is_some_and(|s| s.account_key != account_key) {
        if let Some(old) = state_guard.take() {
            for (_, host) in old.hosts {
                host.cancel_token.cancel();
            }
        }
        if let Ok(mut resolvers) = HOST_RESOLVERS.write() {
            *resolvers = None;
        }
    }

    let state = state_guard.get_or_insert_with(|| ManagerState {
        account_key: account_key.clone(),
        hosts: HashMap::new(),
    });

    // Remove hosts that are no longer configured.
    let removed: Vec<String> = state.hosts.keys().filter(|h| !desired.contains(*h)).cloned().collect();
    for host in removed {
        if let Some(entry) = state.hosts.remove(&host) {
            entry.cancel_token.cancel();
        }
        if let Ok(mut resolvers) = HOST_RESOLVERS.write() {
            if let Some(map) = resolvers.as_mut() {
                map.remove(&host);
            }
        }
        debug!("ACME host removed: {}", host);
    }

    // Start states for new hosts only.
    let mut started = 0u64;
    for host in desired.iter() {
        if state.hosts.contains_key(host) {
            continue;
        }

        let provider = rustls::crypto::aws_lc_rs::default_provider();
        let acme_config = AcmeConfig::new_with_provider(vec![host.clone()], provider.into())
            .cache_with_boxed_err(DirCache::new(cache_dir.clone()))
            .directory_lets_encrypt(!tls_settings.use_staging_server)
            // rustls-acme requires `mailto:` prefix.
            .contact_push(format!("mailto:{}", tls_settings.account_email.trim()));

        let acme_state = acme_config.state();
        let resolver = acme_state.resolver();

        if let Ok(mut resolvers) = HOST_RESOLVERS.write() {
            resolvers.get_or_insert_with(HashMap::new).insert(host.clone(), resolver);
        }

        let cancel_token = CancellationToken::new();
        // Small stagger avoids concurrent first-time account creation racing in the shared cache.
        let start_delay = Duration::from_millis((started * 500).min(10_000));
        started += 1;
        spawn_acme_polling_task(host.clone(), acme_state, cancel_token.clone(), start_delay);
        state.hosts.insert(host.clone(), HostEntry { cancel_token });

        trace!(
            "ACME host added: {} (staging={}, cache_dir='{}')",
            host, tls_settings.use_staging_server, cache_dir
        );
    }

    Ok(())
}

/// Get ACME-managed hostnames
#[allow(dead_code)]
pub async fn get_shared_acme_domains() -> HashSet<String> {
    let state = MANAGER_STATE.lock().await;
    state.as_ref().map(|s| s.hosts.keys().cloned().collect()).unwrap_or_default()
}

/// Collect all ACME-enabled hostnames across all TLS bindings
async fn collect_acme_domains() -> BTreeSet<String> {
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let config = cached_configuration.get_configuration();

    let mut all_domains: BTreeSet<String> = BTreeSet::new();

    let running_state = get_running_state_manager().await.get_running_state();
    let binding_site_cache = running_state.get_binding_site_cache();

    for binding in &config.bindings {
        if !binding.is_tls {
            continue;
        }

        let sites = binding_site_cache.get_sites_for_binding(&binding.id);

        for site in sites.iter().filter(|s| s.is_enabled && s.tls_automatic_enabled) {
            for hostname in &site.hostnames {
                let h = hostname.trim().to_lowercase();
                // Wildcards require DNS-01, which rustls-acme does not support.
                // Also skip non-public names: must look like a DNS name.
                if h.is_empty() || h.contains('*') || h == "localhost" || !h.contains('.') {
                    continue;
                }
                all_domains.insert(h);
            }
        }
    }

    all_domains
}

/// Spawn a background task that drives certificate acquisition and renewal for a single host.
/// The task stops when its cancellation token is cancelled or on shutdown. It deliberately ignores stop_services,
/// which fires on every config reload; hosts are reconciled incrementally by initialize_shared_acme_manager instead.
fn spawn_acme_polling_task(host: String, mut acme_state: rustls_acme::AcmeState<Box<dyn std::fmt::Debug>, Box<dyn std::fmt::Debug>>, cancel_token: CancellationToken, start_delay: Duration) {
    tokio::spawn(async move {
        trace!("ACME polling task started for {}", host);

        let triggers = get_trigger_handler();
        let shutdown_token = triggers.get_token("shutdown").await.unwrap_or_default();

        if !start_delay.is_zero() {
            tokio::select! {
                _ = tokio::time::sleep(start_delay) => {}
                _ = cancel_token.cancelled() => return,
                _ = shutdown_token.cancelled() => return,
            }
        }

        loop {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    debug!("ACME polling task for {} cancelled", host);
                    break;
                }
                _ = shutdown_token.cancelled() => {
                    debug!("ACME polling task for {} stopping due to shutdown signal", host);
                    break;
                }
                event = acme_state.next() => {
                    match event {
                        Some(Ok(ok)) => {
                            trace!("ACME event for {}: {:?}", host, ok);
                        }
                        Some(Err(err)) => {
                            // The previous certificate (if any) stays in the resolver; rustls-acme retries with backoff.
                            warn!("ACME error for {}: {:?}", host, err);
                        }
                        None => {
                            debug!("ACME event stream for {} ended", host);
                            break;
                        }
                    }
                }
            }
        }

        debug!("ACME polling task for {} ended", host);
    });
}
