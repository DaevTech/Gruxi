use arc_swap::ArcSwap;
use std::sync::Arc;

use crate::runtime::site_manager::SiteRuntime;

/// Find a best match site for the requested hostname using direct comparison.
/// Expects hostname to already be lowercase, as site hostnames are stored lowercase
/// to avoid repeated lowercasing on each request.
///
/// Match priority: exact hostname > wildcard ("*") > is_default
pub fn find_best_match_site(sites: &[Arc<ArcSwap<SiteRuntime>>], hostname: &str) -> Option<Arc<SiteRuntime>> {
    let mut wildcard = None;
    let mut default = None;

    for slot in sites {
        let runtime = slot.load_full();
        let site = &runtime.site;

        if !site.is_enabled {
            continue;
        }
        if site.hostnames.iter().any(|h| h == hostname) {
            return Some(runtime);
        }
        if wildcard.is_none() && site.hostnames.iter().any(|h| h == "*") {
            wildcard = Some(Arc::clone(&runtime));
        }
        if default.is_none() && site.is_default {
            default = Some(runtime);
        }
    }

    wildcard.or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::site::Site;

    fn site_runtimes(sites: Vec<Site>) -> Vec<Arc<ArcSwap<SiteRuntime>>> {
        sites.into_iter().map(|site| Arc::new(ArcSwap::from_pointee(SiteRuntime::new(site)))).collect()
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_site_matcher_simple() {
        let mut site1 = Site::new();
        site1.hostnames = vec!["grux.eu".to_string(), "gruxi.org".to_string(), "othersite.com".to_string()];
        site1.is_default = false;
        site1.is_enabled = true;

        let mut site2 = Site::new();
        site2.hostnames = vec!["*".to_string()];
        site2.is_default = false;
        site2.is_enabled = true;

        let mut site3 = Site::new();
        site3.hostnames = vec!["*".to_string()];
        site3.is_default = true;
        site3.is_enabled = true;

        let sites = site_runtimes(vec![site1.clone(), site2.clone(), site3.clone()]);

        // Exact match
        let matched_site = find_best_match_site(&sites, "grux.eu").unwrap();
        assert_eq!(matched_site.site.id, site1.id);
        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.site.id, site1.id);

        // Wildcard match for rest, none should hit the default, as we have a wildcard site
        let matched_site = find_best_match_site(&sites, "unknown.com").unwrap();
        assert_eq!(matched_site.site.id, site2.id);
        let matched_site = find_best_match_site(&sites, "anotherunknown.com").unwrap();
        assert_eq!(matched_site.site.id, site2.id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_site_matcher_partial_match() {
        let mut site1 = Site::new();
        site1.hostnames = vec!["grux.eu".to_string(), "gruxi.org".to_string(), "othersite.com".to_string()];
        site1.is_default = false;
        site1.is_enabled = true;

        let mut site2 = Site::new();
        site2.hostnames = vec!["www.grux.eu".to_string()];
        site2.is_default = false;
        site2.is_enabled = true;

        // grux.eu should match site1, www.grux.eu should match site2
        let sites = site_runtimes(vec![site1.clone(), site2.clone()]);

        let matched_site = find_best_match_site(&sites, "grux.eu").unwrap();
        assert_eq!(matched_site.site.id, site1.id);
        let matched_site = find_best_match_site(&sites, "www.grux.eu").unwrap();
        assert_eq!(matched_site.site.id, site2.id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_site_matcher_disabled_sites() {
        let mut site1 = Site::new();
        site1.hostnames = vec!["grux.eu".to_string(), "gruxi.org".to_string(), "othersite.com".to_string()];
        site1.is_default = true;
        site1.is_enabled = false;

        let mut site2 = Site::new();
        site2.hostnames = vec!["gruxi.org".to_string()];
        site2.is_default = false;
        site2.is_enabled = true;

        // grux.eu should not match site1 as it is disabled, gruxi.org should match site2
        let sites = site_runtimes(vec![site1.clone(), site2.clone()]);

        let matched_site = find_best_match_site(&sites, "grux.eu");
        assert!(matched_site.is_none());
        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.site.id, site2.id);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_site_matcher_default_sites() {
        let mut site1 = Site::new();
        site1.hostnames = vec!["grux.eu".to_string(), "othersite.com".to_string()];
        site1.is_default = true;
        site1.is_enabled = true;

        let mut site2 = Site::new();
        site2.hostnames = vec!["gruxi.org".to_string()];
        site2.is_default = true;
        site2.is_enabled = true;

        // unknown.com should match site1 as default, gruxi.org should match site2
        let sites = site_runtimes(vec![site1.clone(), site2.clone()]);

        let matched_site = find_best_match_site(&sites, "unknown.com").unwrap();
        assert_eq!(matched_site.site.id, site1.id);
        let matched_site = find_best_match_site(&sites, "UnKNoWN.com").unwrap();
        assert_eq!(matched_site.site.id, site1.id);

        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.site.id, site2.id);
    }

    #[test]
    fn matched_runtime_remains_a_snapshot_after_update() {
        let mut original = Site::new();
        original.hostnames = vec!["original.example".to_string()];
        original.is_enabled = true;
        let sites = site_runtimes(vec![original.clone()]);

        let matched = find_best_match_site(&sites, "original.example").unwrap();
        let mut updated = original.clone();
        updated.hostnames = vec!["updated.example".to_string()];
        sites[0].store(Arc::new(SiteRuntime::new(updated)));

        assert_eq!(matched.site.hostnames, original.hostnames);
        assert!(find_best_match_site(&sites, "original.example").is_none());
        assert_eq!(find_best_match_site(&sites, "updated.example").unwrap().site.id, original.id);
    }
}
