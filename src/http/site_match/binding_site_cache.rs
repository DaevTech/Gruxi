use std::{collections::HashMap, sync::Arc};

use dashmap::DashMap;

use crate::configuration::{binding::Binding, binding_site_relation::BindingSiteRelationship, cached_configuration::get_cached_configuration, site::Site};

pub struct BindingSiteCache {
    binding_to_sites: DashMap<String, Arc<Vec<Site>>>,
}

impl BindingSiteCache {
    pub fn new() -> Self {
        BindingSiteCache { binding_to_sites: DashMap::new() }
    }

    pub async fn init(&self) {
        // Get the configuration
        let cached_configuration = get_cached_configuration();
        let configuration = cached_configuration.get_configuration().await;
        self.populate_cache(&configuration.bindings, &configuration.sites, &configuration.binding_sites);
    }

    fn populate_cache(&self, bindings: &Vec<Binding>, sites: &Vec<Site>, binding_sites: &Vec<BindingSiteRelationship>) {
        // Clear existing cache
        self.binding_to_sites.clear();

        // Build a map of binding ID to sites
        let unique_binding_ids: Vec<String> = bindings.iter().map(|b| b.id.clone()).collect();

        // Generate hashmap with site id to Site for quick lookup
        let site_map: HashMap<String, Site> = sites.iter().filter(|site| site.is_enabled).map(|site| (site.id.clone(), site.clone())).collect();

        // For each binding, find associated sites
        for binding_id in unique_binding_ids {
            // For each binding, we get a list of sites associated with it and fetch the actual Site objects
            let associated_sites: Vec<Site> = binding_sites
                .iter()
                .filter(|rel| rel.binding_id == binding_id)
                .filter_map(|rel| site_map.get(&rel.site_id).cloned())
                .collect();

            // Insert into the cache
            self.binding_to_sites.insert(binding_id, Arc::new(associated_sites));
        }
    }

    pub fn get_sites_for_binding(&self, binding_id: &str) -> Arc<Vec<Site>> {
        self.binding_to_sites.get(binding_id).map(|entry| Arc::clone(&entry)).unwrap_or_else(|| Arc::new(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_populate_binding_site_cache_simple() {
        let binding1 = Binding::new();
        let binding2 = Binding::new();

        let site1 = Site::new();
        let site2 = Site::new();
        let site3 = Site::new();
        let site4 = Site::new();

        let rel1 = BindingSiteRelationship {
            binding_id: binding1.id.clone(),
            site_id: site1.id.clone(),
        };
        let rel2 = BindingSiteRelationship {
            binding_id: binding1.id.clone(),
            site_id: site2.id.clone(),
        };
        let rel3 = BindingSiteRelationship {
            binding_id: binding2.id.clone(),
            site_id: site3.id.clone(),
        };
        let rel4 = BindingSiteRelationship {
            binding_id: binding1.id.clone(),
            site_id: site4.id.clone(),
        };

        let cache = BindingSiteCache::new();
        cache.populate_cache(
            &vec![binding1.clone(), binding2.clone()],
            &vec![site1.clone(), site2.clone(), site3.clone(), site4.clone()],
            &vec![rel1, rel2, rel3, rel4],
        );
        let sites_for_binding1 = cache.get_sites_for_binding(&binding1.id);
        let sites_for_binding2 = cache.get_sites_for_binding(&binding2.id);
        assert_eq!(sites_for_binding1.len(), 3);
        assert_eq!(sites_for_binding2.len(), 1);
        assert!(sites_for_binding1.iter().any(|s| s.id == site1.id));
        assert!(sites_for_binding1.iter().any(|s| s.id == site2.id));
        assert!(sites_for_binding1.iter().any(|s| s.id == site4.id));
        assert!(sites_for_binding2.iter().any(|s| s.id == site3.id));
    }
}
