use grux::external_request_handlers::{external_request_handlers::ExternalRequestHandler, php_handler::PHPHandler};



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

    // Start and stop the handler
    handler.start();
    handler.stop();
}

#[test]
fn test_fastcgi_binary_response_parsing() {
    // Test that the parse_fastcgi_response function correctly handles binary data
    // This simulates a FastCGI response with binary content in the body

    // Create a mock FastCGI STDOUT record with binary data
    let mut fastcgi_response = Vec::new();

    // FastCGI header: version=1, type=6 (STDOUT), request_id=1, content_length, padding=0, reserved=0
    let binary_content = vec![0x00, 0x01, 0x02, 0x03, 0xFF, 0xFE, 0xFD]; // Some binary data
    let headers = b"Content-Type: application/octet-stream\r\n\r\n";
    let full_content = [headers.as_slice(), &binary_content].concat();

    fastcgi_response.push(1); // version
    fastcgi_response.push(6); // type: FCGI_STDOUT
    fastcgi_response.extend(&1u16.to_be_bytes()); // request_id
    fastcgi_response.extend(&(full_content.len() as u16).to_be_bytes()); // content_length
    fastcgi_response.push(0); // padding_length
    fastcgi_response.push(0); // reserved
    fastcgi_response.extend(&full_content);

    // Add FCGI_END_REQUEST record
    fastcgi_response.push(1); // version
    fastcgi_response.push(3); // type: FCGI_END_REQUEST
    fastcgi_response.extend(&1u16.to_be_bytes()); // request_id
    fastcgi_response.extend(&8u16.to_be_bytes()); // content_length (8 bytes for end request)
    fastcgi_response.push(0); // padding_length
    fastcgi_response.push(0); // reserved
    fastcgi_response.extend(&[0u8; 8]); // end request body

    // Parse the response using our updated function
    let parsed_response = PHPHandler::parse_fastcgi_response(&fastcgi_response);

    // Verify the binary data is preserved
    assert!(parsed_response.len() > 0);
    assert!(parsed_response.windows(binary_content.len()).any(|w| w == binary_content.as_slice()));
}