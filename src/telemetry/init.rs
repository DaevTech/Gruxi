use uuid::Uuid;

use crate::config::{
    binding::Binding,
    binding_site_relation::BindingSiteRelationship,
    configuration::Configuration,
    site::Site,
};

pub fn add_telemetry_to_configuration(configuration: &mut Configuration) {
    let telemetry_binding = Binding {
        id: Uuid::new_v4().to_string(),
        ip: "0.0.0.0".to_string(),
        port: 8001,
        is_admin: false,
        is_telemetry: true,
        is_tls: true,
    };

    // Reuse TLS config from admin portal
    let admin_portal = &configuration.core.admin_portal;
    let tls_automatic_enabled = admin_portal.tls_automatic_enabled;
    let tls_cert_path = admin_portal.get_tls_certificate_path();
    let tls_key_path = admin_portal.get_tls_key_path();

    let telemetry_hostnames = if tls_automatic_enabled {
        let domain = &admin_portal.domain_name;
        if !domain.is_empty() { vec![domain.clone()] } else { vec!["*".to_string()] }
    } else {
        vec!["*".to_string()]
    };

    let telemetry_site = Site {
        id: Uuid::new_v4().to_string(),
        hostnames: telemetry_hostnames,
        is_default: true,
        is_enabled: true,
        tls_automatic_enabled,
        tls_cert_path,
        tls_cert_content: "".to_string(),
        tls_key_path,
        tls_key_content: "".to_string(),
        request_handlers: vec![],
        rewrite_functions: vec![],
        extra_headers: vec![],
        access_log_enabled: true,
        access_log_file: "telemetry_access.log".to_string(),
        force_tls: false,
        force_tls_port: 8001,
        canonical_host: "".to_string(),
    };

    configuration.binding_sites.push(BindingSiteRelationship {
        binding_id: telemetry_binding.id.clone(),
        site_id: telemetry_site.id.clone(),
    });
    configuration.sites.push(telemetry_site);
    configuration.bindings.push(telemetry_binding);
}
