
use grux::configuration::site::Site;



#[test]
fn test_site_validation_access_log_enabled_empty_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("Access log file cannot be empty when access logging is enabled")));
}

#[test]
fn test_site_validation_access_log_enabled_directory_path() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "/logs/".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("appears to be a directory path")));
}

#[test]
fn test_site_validation_access_log_enabled_windows_directory_path() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "C:\\logs\\".to_string();

    let result = site.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(errors.iter().any(|e| e.contains("appears to be a directory path")));
}

#[test]
fn test_site_validation_access_log_disabled_empty_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = false;
    site.access_log_file = "".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Access log validation should be skipped when disabled");
}

#[test]
fn test_site_validation_access_log_enabled_valid_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "/logs/access.log".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Valid access log file should pass validation");
}

#[test]
fn test_site_validation_access_log_enabled_windows_valid_file() {
    let mut site = create_valid_site();
    site.access_log_enabled = true;
    site.access_log_file = "C:\\logs\\access.log".to_string();

    let result = site.validate();
    assert!(result.is_ok(), "Valid Windows access log file should pass validation");
}

fn create_valid_site() -> Site {
    Site {
        id: 1,
        hostnames: vec!["example.com".to_string()],
        is_default: false,
        is_enabled: true,
        web_root: "./www-default".to_string(),
        web_root_index_file_list: vec!["index.html".to_string()],
        enabled_handlers: vec![],
        tls_cert_path: "".to_string(),
        tls_cert_content: "".to_string(),
        tls_key_path: "".to_string(),
        tls_key_content: "".to_string(),
        rewrite_functions: vec![],
        access_log_enabled: false,
        access_log_file: "".to_string(),
    }
}
