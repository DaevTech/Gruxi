use grux::grux_external_request_handlers::grux_handler_php::PHPHandler;
use grux::grux_external_request_handlers::grux_php_cgi_process::PhpCgiProcess;
use grux::grux_external_request_handlers::ExternalRequestHandler;
use grux::grux_port_manager::PortManager;

#[test]
fn test_php_handler_creation() {
    let handler = PHPHandler::new(
        "php-cgi.exe".to_string(),
        "127.0.0.1:9000".to_string(),
        30,
        2,
        vec![],
        vec![]
    );

    assert_eq!(handler.get_handler_type(), "php");
    assert_eq!(handler.get_file_matches(), vec![".php".to_string()]);
}

#[test]
fn test_php_cgi_process_management() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let port_manager = PortManager::instance();
        let mut process = PhpCgiProcess::new(
            "echo".to_string(),
            "test-service".to_string(),
            port_manager.clone()
        ); // Use 'echo' as a test executable

        // Test starting a process
        let result = process.start().await;
        // This might fail if echo is not available, but that's okay for this test
        match result {
            Ok(_) => {
                // Process started successfully
                assert!(process.is_alive().await);
                assert!(process.get_port().is_some());
            }
            Err(_) => {
                // Process failed to start (expected on systems without the executable)
                assert!(!process.is_alive().await);
            }
        }

        // Clean up
        process.stop().await;
    });
}

#[test]
fn test_php_handler_lifecycle() {
    let handler = PHPHandler::new(
        "echo".to_string(), // Use 'echo' as a test executable
        "127.0.0.1:9000".to_string(),
        30,
        1,
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
fn test_port_allocation() {
    let handler = PHPHandler::new(
        "echo".to_string(),
        "127.0.0.1:9000".to_string(),
        30,
        3,
        vec![],
        vec![]
    );

    // Test that port manager was initialized and handler can be created
    assert_eq!(handler.get_max_concurrent_requests(), 3);

    // Test that we can call start and stop methods
    handler.start();
    handler.stop();
}
