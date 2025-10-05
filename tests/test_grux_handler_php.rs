use grux::grux_external_request_handlers::grux_handler_php::PHPHandler;
use grux::grux_external_request_handlers::ExternalRequestHandler;

#[test]
fn test_php_handler_creation() {
    let handler = PHPHandler::new(
        "php-cgi.exe".to_string(),
        "127.0.0.1:9000".to_string(),
        30,
        2,
        "./www-default".to_string(),
        vec![],
        vec![]
    );

    assert_eq!(handler.get_handler_type(), "php");
    assert_eq!(handler.get_file_matches(), vec![".php".to_string()]);
}

#[test]
fn test_php_handler_with_single_process() {
    let handler = PHPHandler::new(
        "echo".to_string(), // Use 'echo' as a test executable
        "".to_string(), // Empty string means use internal PHP-CGI process
        30,
        2,
        "./www-default".to_string(),
        vec![],
        vec![]
    );

    // Test that handler can be created and will use single internal process
    assert_eq!(handler.get_max_concurrent_requests(), 2);
    assert_eq!(handler.get_handler_type(), "php");
    assert_eq!(handler.get_file_matches(), vec![".php".to_string()]);

    // Start and stop the handler (internal process management is now hidden)
    handler.start();
    handler.stop();
}

#[test]
fn test_php_handler_lifecycle() {
    let handler = PHPHandler::new(
        "echo".to_string(), // Use 'echo' as a test executable
        "127.0.0.1:9000".to_string(),
        30,
        1,
        "./www-default".to_string(),
        vec![],
        vec![]
    );

    // Test that we can call start and stop methods
    handler.start();
    handler.stop();

    // Just verify the handler was created properly
    assert_eq!(handler.get_max_concurrent_requests(), 1);
}

#[test]
fn test_php_handler_concurrent_processing() {
    let handler = PHPHandler::new(
        "echo".to_string(),
        "127.0.0.1:9000".to_string(),
        30,
        3,
        "./www-default".to_string(),
        vec![],
        vec![]
    );

    // Test that handler can be created with multiple concurrent requests
    assert_eq!(handler.get_max_concurrent_requests(), 3);

    // Start and stop the handler
    handler.start();
    handler.stop();
}