use std::{collections::HashMap, sync::{Arc, OnceLock}};

use arc_swap::ArcSwap;

use crate::{
    config::{binding_site_relation::BindingSiteRelationship, configuration::Configuration, site::{HeaderKV, Site}}, core::database_connection::get_database_connection, error::{
        gruxi_error::GruxiError, gruxi_error_enums::{DatabaseError, GruxiErrorKind, GruxiRuntimeError::RequestHandling},
    }, http::site_match::site_matcher::find_best_match_site, util::string_manipulation::{parse_comma_separated_list, parse_key_value_pairs}, warn,
};

#[derive(Debug)]
pub struct SiteRuntime {
    pub site: Site,
}

pub struct SiteManager {
    pub sites: HashMap<String, Arc<ArcSwap<SiteRuntime>>>,
    pub binding_to_sites_lookup: HashMap<String, Vec<Arc<ArcSwap<SiteRuntime>>>>,
}

impl SiteManager {
    pub fn new() -> Self {
        let site_manager = SiteManager { sites: HashMap::new(), binding_to_sites_lookup: HashMap::new() };
        site_manager
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
        let sites = match self.get_sites_from_db(connection) {
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
            self.sites.insert(site_runtime.site.id.clone(), Arc::new(ArcSwap::new(Arc::new(site_runtime))));
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
                    self.sites.entry(site.id.clone()).or_insert_with(|| Arc::new(ArcSwap::from_pointee(SiteRuntime { site: site.clone() })));
                }
                self.add_site_relationships(vec![relationship.clone()]);
            }
        }
        Ok(())
    }

    fn add_site_relationships(&mut self, relationships: Vec<BindingSiteRelationship>) {
        for rel in relationships {
            if let Some(site_runtime) = self.sites.get(&rel.site_id) {
                // Found the binding's associated site,
                self.binding_to_sites_lookup.entry(rel.binding_id.clone()).or_default().push(Arc::clone(site_runtime));
            } else {
                warn!("Binding with ID {} has no associated site with ID {} - skipping", rel.binding_id, rel.site_id);
            }
        }
    }

    fn get_sites_from_db(&self, connection: &sqlite::Connection) -> Result<Vec<Site>, sqlite::Error> {
        // Load sites from the database here
        let mut statement = connection.prepare(
            "SELECT id, is_default, is_enabled, hostnames, tls_cert_path, tls_cert_content, tls_key_path, \
             tls_key_content, request_handlers, rewrite_functions, access_log_enabled, access_log_file, \
             extra_headers, tls_automatic_enabled, force_tls, force_tls_port, canonical_host FROM sites",
        )?;

        let mut sites = Vec::new();
        while let sqlite::State::Row = statement.next()? {
            let site_id: String = statement.read(0)?;
            let is_default: i64 = statement.read(1)?;
            let is_enabled: i64 = statement.read(2)?;

            // Hostnames is comma separated
            let hostnames_str: String = statement.read(3)?;
            let hostnames = parse_comma_separated_list(&hostnames_str, true);

            let tls_cert_path: String = statement.read(4).ok().unwrap_or_default();
            let tls_cert_content: String = statement.read(5).ok().unwrap_or_default();
            let tls_key_path: String = statement.read(6).ok().unwrap_or_default();
            let tls_key_content: String = statement.read(7).ok().unwrap_or_default();

            // Request handlers is comma separated
            let request_handlers_str: String = statement.read(8)?;
            let request_handlers: Vec<String> = parse_comma_separated_list(&request_handlers_str, false);

            // Rewrite functions is comma separated
            let rewrite_functions_str: String = statement.read(9)?;
            let rewrite_functions: Vec<String> = parse_comma_separated_list(&rewrite_functions_str, false);

            // Access log
            let access_log_enabled: i64 = statement.read(10)?;
            let access_log_file: String = statement.read(11)?;

            // Optional extra_headers column (comma separated key=value)
            let extra_headers_str: String = statement.read(12).ok().unwrap_or_default();
            let extra_headers_pairs = parse_key_value_pairs(&extra_headers_str);
            let extra_headers: Vec<HeaderKV> = extra_headers_pairs.into_iter().map(|(k, v)| HeaderKV { key: k, value: v }).collect();

            // TLS Automatic Enabled (added in schema version 4)
            let tls_automatic_enabled: i64 = statement.read(13)?;

            // Force TLS and Canonical host (added in schema version 7)
            let force_tls: i64 = statement.read(14)?;
            let force_tls_port: i64 = statement.read(15)?;
            let canonical_host: String = statement.read(16)?;

            sites.push(Site {
                id: site_id,
                hostnames,
                is_default: is_default != 0,
                is_enabled: is_enabled != 0,
                tls_automatic_enabled: tls_automatic_enabled != 0,
                tls_cert_path,
                tls_cert_content,
                tls_key_path,
                tls_key_content,
                request_handlers,
                rewrite_functions,
                access_log_enabled: access_log_enabled != 0,
                access_log_file,
                extra_headers,
                force_tls: force_tls != 0,
                force_tls_port: force_tls_port as u16,
                canonical_host,
            });
        }

        Ok(sites)
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
            configuration.binding_sites.push(BindingSiteRelationship { binding_id: binding.id.clone(), site_id: site.id.clone() });
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
        manager.sites.insert(site_id.clone(), Arc::new(ArcSwap::new(Arc::new(SiteRuntime { site }))));
        manager.add_site_relationships(vec![
            BindingSiteRelationship { binding_id: "first".into(), site_id: site_id.clone() },
            BindingSiteRelationship { binding_id: "second".into(), site_id: site_id.clone() },
            BindingSiteRelationship { binding_id: "missing".into(), site_id: "unknown".into() },
        ]);

        assert!(!manager.binding_to_sites_lookup.contains_key("missing"));
        let mut updated_site = manager.sites[&site_id].load().site.clone();
        updated_site.is_enabled = false;
        manager.sites[&site_id].store(Arc::new(SiteRuntime { site: updated_site }));

        for binding_id in ["first", "second"] {
            let site = manager.binding_to_sites_lookup[binding_id][0].load();
            assert_eq!(site.site.id, site_id);
            assert!(!site.site.is_enabled);
        }
    }
}
