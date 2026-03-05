use uuid::Uuid;

use crate::{configuration::{binding::Binding, binding_site_relation::BindingSiteRelationship, configuration::Configuration, request_handler::RequestHandler, site::{HeaderKV, Site}}, core::admin_user::create_default_admin_user, error, file::app_paths::get_app_paths, http::request_handlers::processors::static_files_processor::StaticFileProcessor};
use crate::http::request_handlers::processor_trait::ProcessorTrait;

pub fn initialize_admin_site() -> Result<(), ()>{
    // Check if there is at least one admin user
    let connection_result = crate::core::database_connection::get_database_connection();
    let connection = match connection_result {
        Ok(conn) => conn,
        Err(e) => {
            error!("Failed to get database connection: {}", e);
            return Err(());
        }
    };

    let admin_user_result = create_default_admin_user(&connection);
    match admin_user_result {
        Ok(_) => (),
        Err(e) => {
            error!("Failed to create default admin user: {}", e);
            return Err(());
        }
    };

    Ok(())
}

pub fn add_admin_portal_to_configuration(configuration: &mut Configuration) {
    let admin_binding = Binding {
        id: Uuid::new_v4().to_string(),
        ip: "0.0.0.0".to_string(),
        port: 8000,
        is_admin: true,
        is_tls: true,
    };

    // Static file processor for admin site
    let app_paths = get_app_paths();
    let mut request_static_processor = StaticFileProcessor::new(app_paths.default_admin_portal_dir.to_string_lossy().to_string(), vec!["index.html".to_string()]);
    request_static_processor.initialize();

    // Request handler for admin site
    let request_handler = RequestHandler {
        id: Uuid::new_v4().to_string(),
        is_enabled: true,
        name: "Static File Handler".to_string(),
        processor_type: "static".to_string(),
        processor_id: request_static_processor.id.clone(),
        url_match: vec!["*".to_string()],
    };

    // Get the admin portal configuration
    // If automatic TLS is enabled and a domain is configured, use that domain
    // Otherwise use wildcard to match any hostname
    let admin_hostnames = if configuration.core.admin_portal.tls_automatic_enabled {
        let domain = &configuration.core.admin_portal.domain_name;
        if !domain.is_empty() { vec![domain.clone()] } else { vec!["*".to_string()] }
    } else {
        vec!["*".to_string()]
    };

    let admin_site = Site {
        id: Uuid::new_v4().to_string(),
        hostnames: admin_hostnames,
        is_default: true,
        is_enabled: true,
        tls_automatic_enabled: configuration.core.admin_portal.tls_automatic_enabled,
        tls_cert_path: configuration.core.admin_portal.get_tls_certificate_path(),
        tls_cert_content: "".to_string(),
        tls_key_path: configuration.core.admin_portal.get_tls_key_path(),
        tls_key_content: "".to_string(),
        request_handlers: vec![request_handler.id.clone()],
        rewrite_functions: vec![],
        extra_headers: vec![
            HeaderKV {
                key: "X-Content-Type-Options".to_string(),
                value: "nosniff".to_string(),
            },
            HeaderKV {
                key: "X-Frame-Options".to_string(),
                value: "DENY".to_string(),
            },
            HeaderKV {
                key: "Strict-Transport-Security".to_string(),
                value: "max-age=31536000; includeSubDomains".to_string(),
            },
            HeaderKV {
                key: "Cache-Control".to_string(),
                value: "no-store".to_string(),
            },
            HeaderKV {
                key: "Content-Security-Policy".to_string(),
                value: "default-src 'self'; script-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none';".to_string(),
            },
            HeaderKV {
                key: "Permissions-Policy".to_string(),
                value: "camera=(), microphone=(), geolocation=()".to_string(),
            },
            HeaderKV {
                key: "Referrer-Policy".to_string(),
                value: "no-referrer".to_string(),
            },
        ],
        access_log_enabled: true,
        access_log_file: app_paths.logs_dir.to_string_lossy().to_string() + "/admin-portal-access.log",
    };

    // Admin site
    configuration.binding_sites.push(BindingSiteRelationship {
        binding_id: admin_binding.id.clone(),
        site_id: admin_site.id.clone(),
    });
    configuration.sites.push(admin_site);
    configuration.request_handlers.push(request_handler);
    configuration.static_file_processors.push(request_static_processor);

    configuration.bindings.push(admin_binding);
}
