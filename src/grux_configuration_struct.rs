use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Server {
    pub bindings: Vec<Binding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Binding {
    pub ip: String,
    pub port: u16,
    pub is_admin: bool,
    pub sites: Vec<Sites>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Sites {
    pub hostnames: Vec<String>,
    pub is_default: bool,
    pub is_enabled: bool,
    pub is_ssl: bool,
    pub is_ssl_required: bool,
    pub web_root: String,
    pub web_root_index_file_list: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Configuration {
    pub servers: Vec<Server>,
    pub admin_site: AdminSite,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AdminSite {
    pub is_admin_portal_enabled: bool,
    pub admin_portal_ip: String,
    pub admin_portal_port: u16,
    pub admin_portal_web_root: String,
    pub admin_portal_index_file: String,
}

impl Configuration {
    pub fn new() -> Self {
        let default_site = Sites {
            hostnames: vec!["*".to_string()],
            is_default: true,
            is_enabled: true,
            is_ssl: false,
            is_ssl_required: false,
            web_root: "./www-default/".to_string(),
            web_root_index_file_list: vec!["index.html".to_string()],
        };

        let default_binding = Binding {
            ip: "0.0.0.0".to_string(),
            port: 80,
            is_admin: false,
            sites: vec![default_site],
        };

        let default_server = Server { bindings: vec![default_binding] };

        let admin_site = AdminSite {
            is_admin_portal_enabled: true,
            admin_portal_ip: "0.0.0.0".to_string(),
            admin_portal_port: 8000,
            admin_portal_web_root: "./www-admin/".to_string(),
            admin_portal_index_file: "index.html".to_string(),
        };

        Configuration {
            servers: vec![default_server],
            admin_site,
        }
    }
}
