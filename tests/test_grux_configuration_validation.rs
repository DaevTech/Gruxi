use grux::grux_configuration_struct::*;

#[test]
fn test_request_handler_validation_valid() {
    let handler = RequestHandler {
        id: "php_handler".to_string(),
        is_enabled: true,
        name: "PHP Handler".to_string(),
        handler_type: "php".to_string(),
        request_timeout: 30,
        concurrent_threads: 10,
        file_match: vec![".php".to_string(), ".php".to_string()],
        executable: "php-cgi.exe".to_string(),
        ip_and_port: "127.0.0.1:9000".to_string(),
        other_webroot: "./www-default".to_string(),
        extra_handler_config: vec![("option1".to_string(), "value1".to_string())],
        extra_environment: vec![("ENV_VAR".to_string(), "value".to_string())],
    };

    let result = handler.validate();
    assert!(result.is_ok(), "Valid handler should pass validation");
}

#[test]
fn test_request_handler_validation_empty_id() {
    let mut handler = create_valid_handler();
    handler.id = "".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("ID cannot be empty")));
}

#[test]
fn test_request_handler_validation_short_id() {
    let mut handler = create_valid_handler();
    handler.id = "ab".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("must be at least 3 characters long")));
}

#[test]
fn test_request_handler_validation_invalid_id_characters() {
    let mut handler = create_valid_handler();
    handler.id = "test@handler".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("can only contain alphanumeric characters")));
}

#[test]
fn test_request_handler_validation_empty_name() {
    let mut handler = create_valid_handler();
    handler.name = "".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("name cannot be empty")));
}

#[test]
fn test_request_handler_validation_invalid_handler_type() {
    let mut handler = create_valid_handler();
    handler.handler_type = "invalid_type".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Unknown handler type")));
}

#[test]
fn test_request_handler_validation_zero_timeout() {
    let mut handler = create_valid_handler();
    handler.request_timeout = 0;

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("timeout cannot be 0")));
}

#[test]
fn test_request_handler_validation_excessive_timeout() {
    let mut handler = create_valid_handler();
    handler.request_timeout = 4000;

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("cannot exceed 3600 seconds")));
}

#[test]
fn test_request_handler_validation_excessive_concurrent_requests() {
    let mut handler = create_valid_handler();
    handler.concurrent_threads = 2000;

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("cannot exceed 1000")));
}

#[test]
fn test_request_handler_validation_empty_file_match() {
    let mut handler = create_valid_handler();
    handler.file_match = vec![];

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("File match patterns cannot be empty")));
}

#[test]
fn test_request_handler_validation_invalid_file_match_pattern() {
    let mut handler = create_valid_handler();
    handler.file_match = vec!["php".to_string()]; // Missing . or *

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("should start with")));
}

#[test]
fn test_request_handler_validation_empty_executable() {
    let mut handler = create_valid_handler();
    handler.executable = "".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Executable path cannot be empty")));
}

#[test]
fn test_request_handler_validation_invalid_ip_port_format() {
    let mut handler = create_valid_handler();
    handler.ip_and_port = "127.0.0.1".to_string(); // Missing port

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("must be in format 'IP:PORT'")));
}

#[test]
fn test_request_handler_validation_invalid_ip() {
    let mut handler = create_valid_handler();
    handler.ip_and_port = "999.999.999.999:9000".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Invalid IP address")));
}

#[test]
fn test_request_handler_validation_invalid_port() {
    let mut handler = create_valid_handler();
    handler.ip_and_port = "127.0.0.1:abc".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Invalid port")));
}

#[test]
fn test_request_handler_validation_zero_port() {
    let mut handler = create_valid_handler();
    handler.ip_and_port = "127.0.0.1:0".to_string();

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Port cannot be 0")));
}

#[test]
fn test_request_handler_validation_empty_extra_config_key() {
    let mut handler = create_valid_handler();
    handler.extra_handler_config = vec![("".to_string(), "value".to_string())];

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("config key") && e.contains("cannot be empty")));
}

#[test]
fn test_request_handler_validation_empty_env_var_key() {
    let mut handler = create_valid_handler();
    handler.extra_environment = vec![("".to_string(), "value".to_string())];

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Environment variable key") && e.contains("cannot be empty")));
}

#[test]
fn test_request_handler_validation_invalid_env_var_name() {
    let mut handler = create_valid_handler();
    handler.extra_environment = vec![("INVALID-VAR".to_string(), "value".to_string())];

    let result = handler.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("can only contain alphanumeric characters and underscores")));
}

#[test]
fn test_configuration_full_validation() {
    let default_site = Site {
        hostnames: vec!["*".to_string()],
        is_default: true,
        is_enabled: true,
        web_root: "./www-default".to_string(),
        web_root_index_file_list: vec!["index.html".to_string()],
        enabled_handlers: vec!["php_handler".to_string()],
        tls_cert_path: None,
        tls_key_path: None,
        rewrite_functions: vec![],
    };

    let binding = Binding {
        ip: "127.0.0.1".to_string(),
        port: 8080,
        is_admin: false,
        is_tls: false,
        sites: vec![default_site],
    };

    let server = Server {
        bindings: vec![binding],
    };

    let file_cache = FileCache {
        is_enabled: true,
        cache_item_size: 1024,
        cache_max_size_per_file: 1000000,
        cache_item_time_between_checks: 60,
        cleanup_thread_interval: 300,
        max_item_lifetime: 3600,
        forced_eviction_threshold: 80,
    };

    let gzip = Gzip {
        is_enabled: true,
        compressible_content_types: vec!["text/html".to_string()],
    };

    let core = Core {
        file_cache,
        gzip,
    };

    let admin_site = AdminSite {
        is_admin_portal_enabled: true,
    };

    let config = Configuration {
        servers: vec![server],
        admin_site,
        core,
        request_handlers: vec![create_valid_handler()],
    };

    let result = config.validate();
    assert!(result.is_ok(), "Valid configuration should pass validation: {:?}", result);
}

fn create_valid_handler() -> RequestHandler {
    RequestHandler {
        id: "php_handler".to_string(),
        is_enabled: true,
        name: "PHP Handler".to_string(),
        handler_type: "php".to_string(),
        request_timeout: 30,
        concurrent_threads: 10,
        file_match: vec![".php".to_string()],
        executable: "php-cgi.exe".to_string(),
        ip_and_port: "127.0.0.1:9000".to_string(),
        other_webroot: "./www-default".to_string(),
        extra_handler_config: vec![],
        extra_environment: vec![],
    }
}
