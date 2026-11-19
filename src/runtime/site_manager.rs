use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use arc_swap::ArcSwap;
use tokio::sync::watch;

use crate::{
    config::{binding::Binding, binding_site_relation::BindingSiteRelationship, configuration::Configuration, load_configuration::load_sites, site::Site},
    core::database_connection::get_database_connection,
    error::{
        gruxi_error::GruxiError,
        gruxi_error_enums::{DatabaseError, GruxiErrorKind, GruxiRuntimeError::RequestHandling, SiteError},
    },
    http::site_match::site_matcher::find_best_match_site,
    http::request_handlers::processors::processor_manager::ProcessorManager,
    warn,
};

pub struct SiteRuntime {
    pub site: Site,
    pub processors: Arc<ProcessorManager>,
}

impl SiteRuntime {
    pub fn new(mut site: Site) -> Self {
        for handler in &mut site.request_handlers {
            handler.processor.initialize();
        }
        let processors = Arc::new(ProcessorManager::from_site(&site));
        Self { site, processors }
    }
}

pub struct SiteManager {
    pub sites: Mutex<HashMap<String, Arc<ArcSwap<SiteRuntime>>>>,
    binding_to_sites_lookup: ArcSwap<HashMap<String, Vec<Arc<ArcSwap<SiteRuntime>>>>>,
    changes: watch::Sender<u64>,
}

impl SiteManager {
    pub fn new() -> Self {
        let site_manager = SiteManager {
            sites: Mutex::new(HashMap::new()),
            binding_to_sites_lookup: ArcSwap::from_pointee(HashMap::new()),
            changes: watch::channel(0).0,
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
        self.change_site_in_connection(&connection, &mut sites, site, create)?;
        self.notify_change();
        Ok(())
    }

    fn change_site_in_connection(&self, connection: &sqlite::Connection, sites: &mut HashMap<String, Arc<ArcSwap<SiteRuntime>>>, site: Site, create: bool) -> Result<(), GruxiError> {
        if create && sites.contains_key(&site.id) {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::Conflict)));
        }
        let previous = sites.get(&site.id).map(|slot| slot.load_full());
        let mut site = site;
        for handler in &mut site.request_handlers {
            handler.processor.initialize();
        }
        let same_processors = if let Some(previous) = &previous {
            let serialize = |site: &Site| serde_json::to_vec(&site.request_handlers.iter().map(|handler| &handler.processor).collect::<Vec<_>>())
                .map_err(|error| GruxiError::new(GruxiErrorKind::Internal("failed to compare site processors"), error.to_string()));
            serialize(&site)? == serialize(&previous.site)?
        } else {
            false
        };
        let processors = match previous {
            Some(previous) if same_processors => Arc::clone(&previous.processors),
            _ => Arc::new(ProcessorManager::from_site(&site)),
        };
        let runtime = SiteRuntime { site, processors };
        runtime.site.save(connection, create)?;
        if let Some(slot) = sites.get(&runtime.site.id) {
            slot.store(Arc::new(runtime));
        } else {
            sites.insert(runtime.site.id.clone(), Arc::new(ArcSwap::from_pointee(runtime)));
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
        let lookup = self.binding_to_sites_lookup.load();
        if let Some(sites) = lookup.get(binding_id) {
            if let Some(site_runtime) = find_best_match_site(sites, hostname) {
                return Ok(Arc::clone(&site_runtime));
            }

        }
        Err(GruxiError::new(
            GruxiErrorKind::Runtime(RequestHandling),
            format!("No site found for binding ID {} for hostname {}", binding_id, hostname),
        ))
    }

    pub fn get_site_by_id(&self, site_id: &str) -> Result<Arc<SiteRuntime>, GruxiError> {
        let sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        sites.get(site_id).map(|site| site.load_full()).ok_or_else(|| {
            GruxiError::new(GruxiErrorKind::Site(SiteError::NotFound), format!("Site '{}' not found", site_id))
        })
    }

    pub fn get_sites_for_binding(&self, binding_id: &str) -> Vec<Arc<SiteRuntime>> {
        let lookup = self.binding_to_sites_lookup.load();
        lookup.get(binding_id).into_iter().flatten().map(|site| site.load_full()).collect()
    }

    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.changes.subscribe()
    }

    fn notify_change(&self) {
        self.changes.send_modify(|version| *version = version.wrapping_add(1));
    }

    pub fn notify_bindings_changed(&self) {
        self.notify_change();
    }

    pub fn get_binding_ids_for_site(&self, site_id: &str) -> Result<Vec<String>, GruxiError> {
        let _sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        let connection = get_database_connection().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::ConnectionFailed), e))?;
        Self::binding_ids_in_connection(&connection, site_id)
    }

    fn binding_ids_in_connection(connection: &sqlite::Connection, site_id: &str) -> Result<Vec<String>, GruxiError> {
        if !Site::exists(connection, site_id)? {
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::NotFound)));
        }
        let query = || -> Result<Vec<String>, sqlite::Error> {
            let mut statement = connection.prepare("SELECT binding_id FROM binding_sites WHERE site_id = ? ORDER BY binding_id")?;
            statement.bind((1, site_id))?;
            let mut ids = Vec::new();
            while let sqlite::State::Row = statement.next()? {
                ids.push(statement.read(0)?);
            }
            Ok(ids)
        };
        query().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), e.to_string()))
    }

    pub fn change_relationship(&self, site_id: &str, binding_id: &str, attach: bool) -> Result<(), GruxiError> {
        let sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        let connection = get_database_connection().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::ConnectionFailed), e))?;
        self.change_relationship_in_connection(&connection, &sites, site_id, binding_id, attach)
    }

    fn change_relationship_in_connection(
        &self,
        connection: &sqlite::Connection,
        sites: &HashMap<String, Arc<ArcSwap<SiteRuntime>>>,
        site_id: &str,
        binding_id: &str,
        attach: bool,
    ) -> Result<(), GruxiError> {
        let operation = || -> Result<(), GruxiError> {
            if !Site::exists(connection, site_id)? {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::NotFound)));
            }
            let mut exists = connection.prepare("SELECT EXISTS(SELECT 1 FROM bindings WHERE id = ?)").map_err(db_error)?;
            exists.bind((1, binding_id)).map_err(db_error)?;
            exists.next().map_err(db_error)?;
            if exists.read::<i64, _>(0).map_err(db_error)? == 0 {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::Site(SiteError::BindingNotFound)));
            }
            drop(exists);
            let site = sites
                .get(site_id)
                .ok_or_else(|| GruxiError::new(GruxiErrorKind::Internal("site missing from runtime"), site_id.to_string()))?;
            let sql = if attach {
                "INSERT INTO binding_sites (binding_id, site_id) VALUES (?, ?) ON CONFLICT(binding_id, site_id) DO NOTHING"
            } else {
                "DELETE FROM binding_sites WHERE binding_id = ? AND site_id = ?"
            };
            let mut statement = connection.prepare(sql).map_err(db_error)?;
            statement.bind((1, binding_id)).map_err(db_error)?;
            statement.bind((2, site_id)).map_err(db_error)?;
            statement.next().map_err(db_error)?;
            let mut lookup = (**self.binding_to_sites_lookup.load()).clone();
            if attach {
                let entries = lookup.entry(binding_id.to_string()).or_default();
                if !entries.iter().any(|entry| entry.load().site.id == site_id) {
                    entries.push(Arc::clone(site));
                }
            } else if let Some(entries) = lookup.get_mut(binding_id) {
                entries.retain(|entry| entry.load().site.id != site_id);
            }
            self.binding_to_sites_lookup.store(Arc::new(lookup));
            self.notify_change();
            Ok(())
        };
        operation()
    }

    pub fn delete_binding(&self, binding: &Binding) -> Result<(), GruxiError> {
        let _sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        binding.delete()?;
        let mut lookup = (**self.binding_to_sites_lookup.load()).clone();
        lookup.remove(&binding.id);
        self.binding_to_sites_lookup.store(Arc::new(lookup));
        self.notify_change();
        Ok(())
    }

    pub fn record_generated_tls_paths(&self, original: &Arc<SiteRuntime>, is_admin: bool, cert_path: &str, key_path: &str) -> Result<(), GruxiError> {
        let sites = self.sites.lock().map_err(|e| GruxiError::new(GruxiErrorKind::Internal("site manager lock poisoned"), e.to_string()))?;
        let slot = sites
            .get(&original.site.id)
            .ok_or_else(|| GruxiError::new(GruxiErrorKind::Internal("site missing from runtime"), original.site.id.clone()))?;
        if !Arc::ptr_eq(&slot.load_full(), original) {
            return Err(GruxiError::new(GruxiErrorKind::Internal("site changed during certificate generation"), original.site.id.clone()));
        }
        let connection = get_database_connection().map_err(|e| GruxiError::new(GruxiErrorKind::Database(DatabaseError::ConnectionFailed), e))?;
        if is_admin {
            connection.execute("BEGIN IMMEDIATE").map_err(db_error)?;
            let result = (|| -> Result<(), sqlite::Error> {
                for (key, value) in [("admin_portal_tls_certificate_path", cert_path), ("admin_portal_tls_key_path", key_path)] {
                    let mut statement = connection.prepare("UPDATE server_settings SET setting_value = ? WHERE setting_key = ?")?;
                    statement.bind((1, value))?;
                    statement.bind((2, key))?;
                    statement.next()?;
                }
                connection.execute("COMMIT")
            })();
            if let Err(error) = result {
                if let Err(rollback) = connection.execute("ROLLBACK") {
                    warn!("Failed to roll back generated certificate paths: {}", rollback);
                }
                return Err(db_error(error));
            }
        } else {
            let mut statement = connection.prepare("UPDATE sites SET tls_cert_path = ?, tls_key_path = ? WHERE id = ?").map_err(db_error)?;
            statement.bind((1, cert_path)).map_err(db_error)?;
            statement.bind((2, key_path)).map_err(db_error)?;
            statement.bind((3, original.site.id.as_str())).map_err(db_error)?;
            statement.next().map_err(db_error)?;
        }
        let mut site = original.site.clone();
        site.tls_cert_path = cert_path.to_string();
        site.tls_key_path = key_path.to_string();
        slot.store(Arc::new(SiteRuntime { site, processors: Arc::clone(&original.processors) }));
        Ok(())
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
            let site_runtime = SiteRuntime::new(site);
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
                        .or_insert_with(|| Arc::new(ArcSwap::from_pointee(SiteRuntime::new(site.clone()))));
                }
                self.add_site_relationships(vec![relationship.clone()]);
            }
        }
        Ok(())
    }

    fn add_site_relationships(&mut self, relationships: Vec<BindingSiteRelationship>) {
        let mut lookup = (**self.binding_to_sites_lookup.load()).clone();
        for rel in relationships {
            if let Some(site_runtime) = self.sites.get_mut().expect("site manager lock poisoned").get(&rel.site_id) {
                // Found the binding's associated site,
                lookup.entry(rel.binding_id.clone()).or_default().push(Arc::clone(site_runtime));
            } else {
                warn!("Binding with ID {} has no associated site with ID {} - skipping", rel.binding_id, rel.site_id);
            }
        }
        self.binding_to_sites_lookup.store(Arc::new(lookup));
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

fn db_error(error: sqlite::Error) -> GruxiError {
    GruxiError::new(GruxiErrorKind::Database(DatabaseError::QueryFailed), error.to_string())
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
        manager.sites.get_mut().unwrap().insert(site_id.clone(), Arc::new(ArcSwap::new(Arc::new(SiteRuntime::new(site)))));
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

        assert!(!manager.binding_to_sites_lookup.load().contains_key("missing"));
        let mut updated_site = manager.sites.lock().unwrap()[&site_id].load().site.clone();
        updated_site.is_enabled = false;
        manager.sites.lock().unwrap()[&site_id].store(Arc::new(SiteRuntime::new(updated_site)));

        for binding_id in ["first", "second"] {
            let lookup = manager.binding_to_sites_lookup.load();
            let site = lookup[binding_id][0].load();
            assert_eq!(site.site.id, site_id);
            assert!(!site.site.is_enabled);
        }
    }

    #[test]
    fn site_update_publishes_handlers_with_site_snapshot() {
        use crate::config::request_handler::{Processor, RequestHandler};
        use crate::http::request_handlers::processors::static_files_processor::StaticFileProcessor;

        let connection = sqlite::open(":memory:").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        let manager = SiteManager::new();
        let mut sites = manager.sites.lock().unwrap();
        let mut site = Site::new();
        let mut handler = RequestHandler::new();
        handler.processor = Processor::Static(StaticFileProcessor::new("./www-default".into(), vec![]));
        handler.name = "before".into();
        site.request_handlers.push(handler);
        manager.change_site_in_connection(&connection, &mut sites, site.clone(), true).unwrap();
        let original = sites[&site.id].load_full();
        site.request_handlers[0].name = "after".into();
        manager.change_site_in_connection(&connection, &mut sites, site.clone(), false).unwrap();
        let updated = sites[&site.id].load_full();
        assert_eq!(original.site.request_handlers[0].name, "before");
        assert_eq!(updated.site.request_handlers[0].name, "after");
        assert!(Arc::ptr_eq(&original.processors, &updated.processors));
        if let Processor::Static(processor) = &mut site.request_handlers[0].processor {
            processor.web_root = "./www-testing".into();
        }
        manager.change_site_in_connection(&connection, &mut sites, site.clone(), false).unwrap();
        assert!(!Arc::ptr_eq(&updated.processors, &sites[&site.id].load_full().processors));
    }

    #[tokio::test]
    async fn proxy_load_balancers_follow_site_snapshots() {
        use crate::config::request_handler::{Processor, RequestHandler};
        use crate::http::request_handlers::processors::proxy_processor::ProxyProcessor;

        let mut processor = ProxyProcessor::new();
        processor.health_check_path.clear();
        processor.upstream_servers = vec!["http://old.example".into()];
        let id = processor.id.clone();
        let mut handler = RequestHandler::new();
        handler.processor = Processor::Proxy(processor);
        let mut site = Site::new();
        site.request_handlers.push(handler);
        let original = SiteRuntime::new(site.clone());
        if let Processor::Proxy(processor) = &mut site.request_handlers[0].processor {
            processor.upstream_servers = vec!["http://new.example".into()];
        }
        let updated = SiteRuntime::new(site);
        assert_eq!(original.processors.load_balancer_registry.get_next_server(&id).await.as_deref(), Some("http://old.example"));
        assert_eq!(updated.processors.load_balancer_registry.get_next_server(&id).await.as_deref(), Some("http://new.example"));
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
            assert!(matches!(
                manager.delete_site_in_connection(&connection, &mut sites, "site-1"),
                Err(GruxiError {
                    kind: GruxiErrorKind::Site(SiteError::Referenced),
                    ..
                })
            ));
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
        assert!(matches!(
            manager.change_site_in_connection(&connection, &mut sites, site, false),
            Err(GruxiError {
                kind: GruxiErrorKind::Database(DatabaseError::QueryFailed),
                ..
            })
        ));
        assert_eq!(sites["site-1"].load().site.hostnames, vec!["example.com"]);
        assert_eq!(load_sites(&connection).unwrap()[0].hostnames, vec!["example.com"]);
    }

    #[test]
    fn relationships_update_database_and_runtime_and_survive_reload() {
        let connection = sqlite::open(":memory:").unwrap();
        connection.execute("PRAGMA foreign_keys=ON").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        connection
            .execute("INSERT INTO bindings (id, ip, port) VALUES ('binding-1', '127.0.0.1', 1234), ('binding-2', '127.0.0.1', 1235)")
            .unwrap();
        connection.execute("INSERT INTO sites (id, hostnames) VALUES ('site-1', 'example.com')").unwrap();
        let mut manager = SiteManager::new();
        manager.load_sites_from_connection(&connection, &Configuration::new()).unwrap();

        {
            let sites = manager.sites.lock().unwrap();
            for binding in ["binding-1", "binding-1", "binding-2"] {
                manager.change_relationship_in_connection(&connection, &sites, "site-1", binding, true).unwrap();
            }
        }
        assert_eq!(SiteManager::binding_ids_in_connection(&connection, "site-1").unwrap(), vec!["binding-1", "binding-2"]);
        assert_eq!(manager.get_sites_for_binding("binding-1").len(), 1);
        assert_eq!(manager.get_site_for_request("binding-1", "example.com").unwrap().site.id, "site-1");
        let mut reloaded = SiteManager::new();
        reloaded.load_sites_from_connection(&connection, &Configuration::new()).unwrap();
        assert_eq!(reloaded.get_site_for_request("binding-2", "example.com").unwrap().site.id, "site-1");

        {
            let sites = manager.sites.lock().unwrap();
            for _ in 0..2 {
                manager.change_relationship_in_connection(&connection, &sites, "site-1", "binding-1", false).unwrap();
            }
            assert!(manager.change_relationship_in_connection(&connection, &sites, "site-1", "missing", true).is_err());
            assert!(manager.change_relationship_in_connection(&connection, &sites, "missing", "binding-2", true).is_err());
        }
        assert!(manager.get_site_for_request("binding-1", "example.com").is_err());
        assert_eq!(manager.get_site_for_request("binding-2", "example.com").unwrap().site.id, "site-1");
        assert_eq!(SiteManager::binding_ids_in_connection(&connection, "site-1").unwrap(), vec!["binding-2"]);
    }

    #[test]
    fn failed_relationship_write_does_not_change_runtime() {
        let connection = sqlite::open(":memory:").unwrap();
        for sql in DatabaseSchema::new().init_sql {
            connection.execute(sql).unwrap();
        }
        connection.execute("INSERT INTO bindings (id, ip, port) VALUES ('binding-1', '127.0.0.1', 1234)").unwrap();
        connection.execute("INSERT INTO sites (id, hostnames) VALUES ('site-1', 'example.com')").unwrap();
        let mut manager = SiteManager::new();
        manager.load_sites_from_connection(&connection, &Configuration::new()).unwrap();
        connection
            .execute("CREATE TRIGGER reject_relation BEFORE INSERT ON binding_sites BEGIN SELECT RAISE(FAIL, 'rejected'); END")
            .unwrap();
        let sites = manager.sites.lock().unwrap();
        assert!(manager.change_relationship_in_connection(&connection, &sites, "site-1", "binding-1", true).is_err());
        assert!(manager.get_sites_for_binding("binding-1").is_empty());
        assert!(SiteManager::binding_ids_in_connection(&connection, "site-1").unwrap().is_empty());
    }
}
