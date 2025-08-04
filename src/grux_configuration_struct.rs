use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Server {
    pub bindings: Vec<Binding>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct Binding {
    pub ip: String,
    pub port: String,
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
            port: "80".to_string(),
            sites: vec![default_site],
        };

        let default_server = Server { bindings: vec![default_binding] };

        Configuration { servers: vec![default_server] }
    }
}
