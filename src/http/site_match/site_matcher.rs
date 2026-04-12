use crate::{config::site::Site, trace};

/// Find a best match site for the requested hostname using direct comparison.
/// Expects hostname to already be lowercase, as site hostnames are stored lowercase
/// to avoid repeated lowercasing on each request.
///
/// Match priority: exact hostname > wildcard ("*") > is_default
pub fn find_best_match_site<'a>(sites: &'a [Site], requested_hostname: &str) -> Option<&'a Site> {
    let mut wildcard_site: Option<&Site> = None;
    let mut default_site: Option<&Site> = None;

    for site in sites.iter().filter(|s| s.is_enabled) {
        if site.hostnames.iter().any(|h| h == requested_hostname) {
            return Some(site);
        }
        if wildcard_site.is_none() && site.hostnames.iter().any(|h| h == "*") {
            wildcard_site = Some(site);
        }
        if default_site.is_none() && site.is_default {
            default_site = Some(site);
        }
    }

    let result = wildcard_site.or(default_site);
    if result.is_none() {
        trace!("No matching site found for requested hostname: {}", requested_hostname);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::site::Site;

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

        let sites = vec![site1.clone(), site2.clone(), site3.clone()];

        // Exact match
        let matched_site = find_best_match_site(&sites, "grux.eu").unwrap();
        assert_eq!(matched_site.id, site1.id);
        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.id, site1.id);

        // Wildcard match for rest, none should hit the default, as we have a wildcard site
        let matched_site = find_best_match_site(&sites, "unknown.com").unwrap();
        assert_eq!(matched_site.id, site2.id);
        let matched_site = find_best_match_site(&sites, "anotherunknown.com").unwrap();
        assert_eq!(matched_site.id, site2.id);
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
        let sites = vec![site1.clone(), site2.clone()];

        let matched_site = find_best_match_site(&sites, "grux.eu").unwrap();
        assert_eq!(matched_site.id, site1.id);
        let matched_site = find_best_match_site(&sites, "www.grux.eu").unwrap();
        assert_eq!(matched_site.id, site2.id);
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
        let sites = vec![site1.clone(), site2.clone()];

        let matched_site = find_best_match_site(&sites, "grux.eu");
        assert!(matched_site.is_none());
        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.id, site2.id);
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
        let sites = vec![site1.clone(), site2.clone()];

        let matched_site = find_best_match_site(&sites, "unknown.com").unwrap();
        assert_eq!(matched_site.id, site1.id);
        let matched_site = find_best_match_site(&sites, "UnKnoWN.com").unwrap();
        assert_eq!(matched_site.id, site1.id);

        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.id, site2.id);
    }
}
