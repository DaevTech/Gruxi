use crate::{configuration::site::Site, logging::syslog::trace};

// Find a best match site for the requested hostname, comparing case-insensitively
pub fn find_best_match_site<'a>(sites: &'a Vec<Site>, requested_hostname: &str) -> Option<&'a Site> {
    let requested_hostname_lower = requested_hostname.to_lowercase();
    let mut site = sites.iter().find(|s| s.hostnames.iter().any(|h| h.to_string() == requested_hostname_lower) && s.is_enabled);

    // We check for star hostnames
    if site.is_none() {
        site = sites.iter().find(|s| s.hostnames.iter().any(|h| h.to_string() == "*") && s.is_enabled);
    }

    // If we cant find a matching site, we see if there is a default one
    if site.is_none() {
        site = sites.iter().find(|s| s.is_default && s.is_enabled);
    }

    // If we still cant find a proper site, we return None
    if site.is_none() {
        trace(format!("No matching site found for requested hostname: {}", requested_hostname));
        return None;
    }

    site
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configuration::site::Site;

    #[test]
    fn test_site_matcher_simple_case_insensitive() {
        let mut site1 = Site::new();
        site1.id = 1;
        site1.hostnames = vec!["grux.eu".to_string(), "gruxi.org".to_string(), "othersite.com".to_string()];
        site1.is_default = false;
        site1.is_enabled = true;

        let mut site2 = Site::new();
        site2.id = 2;
        site2.hostnames = vec!["*".to_string()];
        site2.is_default = false;
        site2.is_enabled = true;

        let mut site3 = Site::new();
        site3.id = 3;
        site3.hostnames = vec!["*".to_string()];
        site3.is_default = true;
        site3.is_enabled = true;

        let sites = vec![site1, site2, site3];

        // Exact match
        let matched_site = find_best_match_site(&sites, "grux.eu").unwrap();
        assert_eq!(matched_site.id, 1);
        let matched_site = find_best_match_site(&sites, "GRUX.eu").unwrap();
        assert_eq!(matched_site.id, 1);
        let matched_site = find_best_match_site(&sites, "grux.EU").unwrap();
        assert_eq!(matched_site.id, 1);

        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.id, 1);
        let matched_site = find_best_match_site(&sites, "GRUXI.ORG").unwrap();
        assert_eq!(matched_site.id, 1);

        // Wildcard match for rest, none should hit the default, as we have a wildcard site
        let matched_site = find_best_match_site(&sites, "unknown.com").unwrap();
        assert_eq!(matched_site.id, 2);
        let matched_site = find_best_match_site(&sites, "anotherunknown.com").unwrap();
        assert_eq!(matched_site.id, 2);
        let matched_site = find_best_match_site(&sites, "GRUXI.CoM").unwrap();
        assert_eq!(matched_site.id, 2);
    }

    #[test]
    fn test_site_matcher_partial_match() {
        let mut site1 = Site::new();
        site1.id = 1;
        site1.hostnames = vec!["grux.eu".to_string(), "gruxi.org".to_string(), "othersite.com".to_string()];
        site1.is_default = false;
        site1.is_enabled = true;

        let mut site2 = Site::new();
        site2.id = 2;
        site2.hostnames = vec!["www.grux.eu".to_string()];
        site2.is_default = false;
        site2.is_enabled = true;

        // grux.eu should match site1, www.grux.eu should match site2
        let sites = vec![site1, site2];
        let matched_site = find_best_match_site(&sites, "grux.eu").unwrap();
        assert_eq!(matched_site.id, 1);
        let matched_site = find_best_match_site(&sites, "www.grux.eu").unwrap();
        assert_eq!(matched_site.id, 2);
    }

    #[test]
    fn test_site_matcher_disabled_sites() {
        let mut site1 = Site::new();
        site1.id = 1;
        site1.hostnames = vec!["grux.eu".to_string(), "gruxi.org".to_string(), "othersite.com".to_string()];
        site1.is_default = true;
        site1.is_enabled = false;

        let mut site2 = Site::new();
        site2.id = 2;
        site2.hostnames = vec!["gruxi.org".to_string()];
        site2.is_default = false;
        site2.is_enabled = true;

        // grux.eu should not match site1 as it is disabled, gruxi.org should match site2
        let sites = vec![site1, site2];
        let matched_site = find_best_match_site(&sites, "grux.eu");
        assert!(matched_site.is_none());
        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.id, 2);
    }

    #[test]
    fn test_site_matcher_default_sites() {
        let mut site1 = Site::new();
        site1.id = 1;
        site1.hostnames = vec!["grux.eu".to_string(), "othersite.com".to_string()];
        site1.is_default = true;
        site1.is_enabled = true;

        let mut site2 = Site::new();
        site2.id = 2;
        site2.hostnames = vec!["gruxi.org".to_string()];
        site2.is_default = true;
        site2.is_enabled = true;

        // unknown.com should match site1 as default, gruxi.org should match site2
        let sites = vec![site1, site2];

        let matched_site = find_best_match_site(&sites, "unknown.com").unwrap();
        assert_eq!(matched_site.id, 1);
        let matched_site = find_best_match_site(&sites, "UnKnoWN.com").unwrap();
        assert_eq!(matched_site.id, 1);

        let matched_site = find_best_match_site(&sites, "gruxi.org").unwrap();
        assert_eq!(matched_site.id, 2);
        let matched_site = find_best_match_site(&sites, "GruXi.Org").unwrap();
        assert_eq!(matched_site.id, 2);
    }
}
