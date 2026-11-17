use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use arc_swap::ArcSwap;

use crate::{
    config::{
        binding_site_relation::BindingSiteRelationship,
        configuration::Configuration,
        load_configuration::load_sites,
        site::Site,
    },
    core::database_connection::get_database_connection,
    error::{
        gruxi_error::GruxiError,
        gruxi_error_enums::{DatabaseError, GruxiErrorKind, GruxiRuntimeError::RequestHandling, SiteError},
    },
    http::site_match::site_matcher::find_best_match_site,
    warn,
};

#[derive(Debug)]
pub struct SiteRuntime {
    pub site: Site,
}

pub struct SiteManager {
    pub sites: Mutex<HashMap<String, Arc<ArcSwap<SiteRuntime>>>>,
    pub binding_to_sites_lookup: HashMap<String, Vec<Arc<ArcSwap<SiteRuntime>>>>,
}

impl SiteManager {
    pub fn new() -> Self {
        let site_manager = SiteManager {
            sites: Mutex::new(HashMap::new()),
            binding_to_sites_lookup: HashMap::new(),
        };
        site_manager
    }

    pub fn get_configuration_sites(&self) -> Result<Vec<Site>, GruxiError> {
        let _sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        let connection = get_database_connection().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::ConnectionFailed), e))?;
        load_sites(&connection).map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), e))
    }

    pub fn create_site(&self, site: Site) -> Result<(), GruxiError> {
        self.change_site(site, true)
    }

    pub fn update_site(&self, site: Site) -> Result<(), GruxiError> {
        self.change_site(site, false)
    }

    fn change_site(&self, site: Site, create: bool) -> Result<(), GruxiError> {
        let mut sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        let connection = get_database_connection().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::ConnectionFailed), e))?;
        self.change_site_in_connection(&connection, &mut sites, site, create)
    }

    fn change_site_in_connection(&self, connection: &sqlite::Connection, sites: &mut HashMap<String, Arc<ArcSwap<SiteRuntime>>>, site: Site, create: bool) -> Result<(), GruxiError> {
        if create && sites.contains_key(&site.id) {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::Conflict)));
        }
        site.save(connection, create)?;
        if let Some(slot) = sites.get(&site.id) {
            slot.store(Arc::new(SiteRuntime { site }));
        } else {
            sites.insert(site.id.clone(), Arc::new(ArcSwap::from_pointee(SiteRuntime { site })));
        }
        Ok(())
    }

    pub fn delete_site(&self, id: &str) -> Result<(), GruxiError> {
        let mut sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        let connection = get_database_connection().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::ConnectionFailed), e))?;
        self.delete_site_in_connection(&connection, &mut sites, id)
    }

    fn delete_site_in_connection(&self, connection: &sqlite::Connection, sites: &mut HashMap<String, Arc<ArcSwap<SiteRuntime>>>, id: &str) -> Result<(), GruxiError> {
        Site::delete(connection, id)?;
        sites.remove(id);
        Ok(())
    }

    pub fn init(&mut self, configuration: &Configuration) -> Result<(), GruxiError> {
        match self.load_all_sites(configuration) {
            Ok(_) => (),
            Err(e) => {
                warn!("Failed to load all sites: {:?}", e);
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn get_site_for_request(&self, binding_id: &str, hostname: &str) -> Result<Arc<SiteRuntime>, GruxiError> {
        if let Some(sites) = self.binding_to_sites_lookup.get(binding_id) {
            if let Some(site_runtime) = find_best_match_site(sites, hostname) {
                return Ok(Arc::clone(&site_runtime));
            }
        }
        Err(GruxiError::new(
            GruxiErrorKind::Runtime(RequestHandling),
            format!("No site found for binding ID {} for hostname {}", binding_id, hostname),
        ))
    }

    fn load_all_sites(&mut self, configuration: &Configuration) -> Result<(), GruxiError> {
        let connection = match get_database_connection() {
            Ok(conn) => conn,
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::ConnectionFailed),
                    format!("Failed to get database connection: {}", e),
                ));
            }
        };
        self.load_sites_from_connection(&connection, configuration)
    }

    fn load_sites_from_connection(&mut self, connection: &sqlite::Connection, configuration: &Configuration) -> Result<(), GruxiError> {
        let sites = match load_sites(connection) {
            Ok(sites) => sites,
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::QueryFailed),
                    format!("Failed to load sites from database: {}", e),
                ));
            }
        };

        // For each site, we add it a hashmap with site id being the key
        for site in sites {
            let site_runtime = SiteRuntime { site };
            self.sites
                .get_mut()
                .map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?
                .insert(site_runtime.site.id.clone(), Arc::new(ArcSwap::new(Arc::new(site_runtime))));
        }

        // Now we load the binding<>site relationship, so we known which requests should be routed to which site
        let relationships = match self.get_sites_bindings_from_db(connection) {
            Ok(rels) => rels,
            Err(e) => {
                return Err(GruxiError::new(
                    GruxiErrorKind::Database(DatabaseError::QueryFailed),
                    format!("Failed to load binding-site relationships from database: {}", e),
                ));
            }
        };

        self.add_site_relationships(relationships);

        // Admin and telemetry sites are generated in memory rather than stored in the database.
        for binding in configuration.bindings.iter().filter(|binding| binding.is_admin || binding.is_telemetry) {
            for relationship in configuration.binding_sites.iter().filter(|relationship| relationship.binding_id == binding.id) {
                if let Some(site) = configuration.sites.iter().find(|site| site.id == relationship.site_id) {
                    self.sites
                        .get_mut()
                        .map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?
                        .entry(site.id.clone())
                        .or_insert_with(|| Arc::new(ArcSwap::from_pointee(SiteRuntime { site: site.clone() })));
                }
                self.add_site_relationships(vec![relationship.clone()]);
            }
        }
        Ok(())
    }

    fn add_site_relationships(&mut self, relationships: Vec<BindingSiteRelationship>) {
        for rel in relationships {
            if let Some(site_runtime) = self.sites.get_mut().expect("site manager lock poisoned").get(&rel.site_id) {
                // Found the binding's associated site,
                self.binding_to_sites_lookup.entry(rel.binding_id.clone()).or_default().push(Arc::clone(site_runtime));
            } else {
                warn!("Binding with ID {} has no associated site with ID {} - skipping", rel.binding_id, rel.site_id);
            }
        }
    }

    fn get_sites_bindings_from_db(&self, connection: &sqlite::Connection) -> Result<Vec<BindingSiteRelationship>, sqlite::Error> {
        let mut relationships = Vec::new();
        let mut statement = connection.prepare("SELECT binding_id, site_id FROM binding_sites")?;
        while let sqlite::State::Row = statement.next()? {
            let binding_id: String = statement.read(0)?;
            let site_id: String = statement.read(1)?;
            relationships.push(BindingSiteRelationship { binding_id, site_id });
        }
        Ok(relationships)
    }
}

static SITE_MANAGER_SINGLETON: OnceLock<SiteManager> = OnceLock::new();

pub fn init_site_manager(configuration: &Configuration) -> Result<(), GruxiError> {
    let mut manager = SiteManager::new();
    manager.init(configuration)?;
    SITE_MANAGER_SINGLETON
        .set(manager)
        .map_err(|_| GruxiError::new(GruxiErrorKind::Internal("site manager already initialized"), "Site manager already initialized".to_string()))
}

pub fn get_site_manager() -> &'static SiteManager {
    SITE_MANAGER_SINGLETON.get().expect("site manager must be initialized before serving requests")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::binding::Binding, database::database_schema::DatabaseSchema};

    #[test]
    fn loads_database_and_generated_sites_for_matching() {
        let connection = sqlite::open(":memory:").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        connection.execute("INSERT INTO sites (id, hostnames) VALUES ('site-1', 'example.com')").unwrap();
        connection.execute("INSERT INTO binding_sites (binding_id, site_id) VALUES ('binding-1', 'site-1')").unwrap();

        let mut configuration = Configuration::new();
        for is_admin in [true, false] {
            let mut binding = Binding::new();
            binding.is_admin = is_admin;
            binding.is_telemetry = !is_admin;
            let site = Site::new();
            configuration.binding_sites.push(BindingSiteRelationship {
                binding_id: binding.id.clone(),
                site_id: site.id.clone(),
            });
            configuration.bindings.push(binding);
            configuration.sites.push(site);
        }

        let mut manager = SiteManager::new();
        manager.load_sites_from_connection(&connection, &configuration).unwrap();
        assert_eq!(manager.get_site_for_request("binding-1", "example.com").unwrap().site.id, "site-1");
        for (binding, site) in configuration.bindings.iter().zip(&configuration.sites) {
            assert_eq!(manager.get_site_for_request(&binding.id, "example.com").unwrap().site.id, site.id);
        }
        assert!(manager.get_site_for_request("unknown", "example.com").is_err());
    }

    #[test]
    fn reports_missing_relationship_table() {
        let connection = sqlite::open(":memory:").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        connection.execute("DROP TABLE binding_sites").unwrap();

        let mut manager = SiteManager::new();
        let error = manager.load_sites_from_connection(&connection, &Configuration::new()).unwrap_err();
        assert!(matches!(error.kind, GruxiErrorKind::Database(DatabaseError::QueryFailed)));
        assert!(error.message.contains("binding-site relationships"));
    }

    #[tokio::test]
    async fn binding_lookup_shares_site_updates() {
        let mut manager = SiteManager::new();
        let site = Site::new();
        let site_id = site.id.clone();
        manager.sites.get_mut().unwrap().insert(site_id.clone(), Arc::new(ArcSwap::new(Arc::new(SiteRuntime { site }))));
        manager.add_site_relationships(vec![
            BindingSiteRelationship {
                binding_id: "first".into(),
                site_id: site_id.clone(),
            },
            BindingSiteRelationship {
                binding_id: "second".into(),
                site_id: site_id.clone(),
            },
            BindingSiteRelationship {
                binding_id: "missing".into(),
                site_id: "unknown".into(),
            },
        ]);

        assert!(!manager.binding_to_sites_lookup.contains_key("missing"));
        let mut updated_site = manager.sites.lock().unwrap()[&site_id].load().site.clone();
        updated_site.is_enabled = false;
        manager.sites.lock().unwrap()[&site_id].store(Arc::new(SiteRuntime { site: updated_site }));

        for binding_id in ["first", "second"] {
            let site = manager.binding_to_sites_lookup[binding_id][0].load();
            assert_eq!(site.site.id, site_id);
            assert!(!site.site.is_enabled);
        }
    }

    #[test]
    fn changes_preserve_relationships_and_runtime_slots() {
        let connection = sqlite::open(":memory:").unwrap();
        connection.execute("PRAGMA foreign_keys=ON").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        connection.execute("INSERT INTO bindings (id, ip, port) VALUES ('binding-1', '127.0.0.1', 1234)").unwrap();
        connection.execute("INSERT INTO sites (id, hostnames) VALUES ('site-1', 'example.com')").unwrap();
        connection.execute("INSERT INTO binding_sites (binding_id, site_id) VALUES ('binding-1', 'site-1')").unwrap();
        let mut manager = SiteManager::new();
        manager.load_sites_from_connection(&connection, &Configuration::new()).unwrap();

        let mut updated = manager.get_site_for_request("binding-1", "example.com").unwrap().site.clone();
        updated.hostnames = vec!["changed.example".into()];
        {
            let mut sites = manager.sites.lock().unwrap();
            manager.change_site_in_connection(&connection, &mut sites, updated.clone(), false).unwrap();
            assert!(matches!(manager.delete_site_in_connection(&connection, &mut sites, "site-1"), Err(GruxiError { kind: GruxiErrorKind::Site(SiteError::Referenced), .. })));
        }
        assert!(manager.get_site_for_request("binding-1", "example.com").is_err());
        assert_eq!(manager.get_site_for_request("binding-1", "changed.example").unwrap().site.id, "site-1");
        assert_eq!(load_sites(&connection).unwrap()[0].hostnames, updated.hostnames);
        assert_eq!(manager.get_sites_bindings_from_db(&connection).unwrap().len(), 1);
    }

    #[test]
    fn failed_database_change_does_not_change_runtime() {
        let connection = sqlite::open(":memory:").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        connection.execute("INSERT INTO sites (id, hostnames) VALUES ('site-1', 'example.com')").unwrap();
        let mut manager = SiteManager::new();
        manager.load_sites_from_connection(&connection, &Configuration::new()).unwrap();
        connection
            .execute("CREATE TRIGGER reject_update BEFORE UPDATE ON sites BEGIN SELECT RAISE(FAIL, 'rejected'); END")
            .unwrap();
        let mut site = load_sites(&connection).unwrap().remove(0);
        site.hostnames = vec!["changed.example".into()];
        let mut sites = manager.sites.lock().unwrap();
        assert!(matches!(manager.change_site_in_connection(&connection, &mut sites, site, false), Err(GruxiError { kind: GruxiErrorKind::Database(DatabaseError::QueryFailed), .. })));
        assert_eq!(sites["site-1"].load().site.hostnames, vec!["example.com"]);
        assert_eq!(load_sites(&connection).unwrap()[0].hostnames, vec!["example.com"]);
    }
}
