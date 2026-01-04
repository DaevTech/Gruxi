use std::time::Duration;

use crate::error::grux_error::GruxError;
use crate::error::grux_error_enums::{GruxErrorKind, PHPProcessorError};
use crate::external_connections::fastcgi::FastCgi;
use crate::file::file_util::get_full_file_path;
use crate::http::http_util::resolve_web_root_and_path_and_get_file;
use crate::http::request_response::grux_response::GruxResponse;
use crate::logging::syslog::{debug, error, trace};
use crate::{
    configuration::site::Site,
    core::running_state_manager::get_running_state_manager,
    http::{http_util::empty_response_with_status, request_handlers::processor_trait::ProcessorTrait, request_response::grux_request::GruxRequest},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PHPProcessor {
    pub id: String, // Unique identifier for the processor
    // Can either be served by a local PHP-CGI executable or via FastCGI (PHP-FPM or similar)
    pub served_by_type: String,      // How it is handled, by Grux handled "win-php-cgi" or "php-fpm"
    pub php_cgi_handler_id: String,  // Optional ID of the PHP-CGI handler to use, if user has selected "win-php-cgi" as the type
    pub fastcgi_ip_and_port: String, // Optional IP and port to connect to FastCGI handler, if user has selected "php-fpm" as the type
    // Request timeout, that may be different from the global timeout
    pub request_timeout: u32, // Seconds
    // Web root
    pub local_web_root: String,   // local location for the web root
    pub fastcgi_web_root: String, // Relevant for "php-fpm" type, for web-root rewriting when passing to FastCGI handler
}

impl PHPProcessor {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            served_by_type: "php-fpm".to_string(),
            php_cgi_handler_id: String::new(),
            fastcgi_ip_and_port: String::new(),
            request_timeout: 30,
            local_web_root: String::new(),
            fastcgi_web_root: String::new(),
        }
    }
}

impl ProcessorTrait for PHPProcessor {
    fn sanitize(&mut self) {
        // TODO
    }

    fn validate(&self) -> Result<(), Vec<String>> {
        let errors = Vec::new();
        // TODO

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn handle_request(&self, grux_request: &mut GruxRequest, site: &Site) -> Result<GruxResponse, GruxError> {
        // First we need to determine if and how to handle the request, based on the web root and the files that allow us to
        let web_root_result = get_full_file_path(&self.local_web_root);
        if let Err(e) = web_root_result {
            error(format!("Failed to get full web root path: {}", e));
            return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::PathError(e))));
        }
        let web_root = web_root_result.unwrap();
        let mut path = grux_request.get_path().clone();

        // Get the cached file, if it exists
        let file_data_result = resolve_web_root_and_path_and_get_file(&web_root, &path).await;
        if let Err(e) = file_data_result {
            return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::PathError(e))));
        }
        let mut file_data = file_data_result.unwrap();
        let mut file_path = file_data.file_path.clone();

        // If the file/dir does not exist, we check if we have a rewrite function that allows us to rewrite to the index file
        if !file_data.exists {
            trace(format!("File does not exist: {}", file_path));
            if site.get_rewrite_functions_hashmap().contains_key("OnlyWebRootIndexForSubdirs") {
                trace(format!("[OnlyWebRootIndexForSubdirs] Rewriting request path {} to root dir due to rewrite function", path));
                // We rewrite the path to just "/" which will make it serve the index file
                path = "/index.php".to_string();

                // Check if the index file exists
                let file_data_result = resolve_web_root_and_path_and_get_file(&web_root, &path).await;
                if let Err(e) = file_data_result {
                    return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::PathError(e))));
                }
                file_data = file_data_result.unwrap();
                file_path = file_data.file_path.clone();
            } else {
                return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::FileNotFound)));
            }
        }

        let mut uri_is_a_dir_with_index_file_inside = false;
        if file_data.is_directory {
            // If it's a directory, we will try to check if there is an index.php file inside
            trace(format!("File is a directory: {}", file_path));

            let file_data_result = resolve_web_root_and_path_and_get_file(&file_path, "/index.php").await;
            if file_data_result.is_err() {
                trace(format!("Did not find index file: {}", file_path));
                return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
            }
            file_data = file_data_result.unwrap();
            file_path = file_data.file_path.clone();
            trace(format!("Found index file: {}", file_path));
            uri_is_a_dir_with_index_file_inside = true;
        }

        // Now get the IP and port to connect to
        let connect_ip_and_port_result = self.get_ip_and_port().await;
        if connect_ip_and_port_result.is_err() {
            // Cannot determine how to connect to the PHP handler, so we cannot handle
            error(format!("PHP Processor: Cannot determine how to connect to PHP handler for processor ID: {}", self.id));
            return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::Connection)));
        }
        let connect_ip_and_port = connect_ip_and_port_result.unwrap();

        // Figure out if we have a connection semaphore to use
        if !self.php_cgi_handler_id.trim().is_empty() {
            let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
            let external_system_handler = running_state.get_external_system_handler();
            let semaphore_option = external_system_handler.get_connection_semaphore(&self.php_cgi_handler_id);
            if semaphore_option.is_none() {
                error(format!("PHP Processor: Cannot find connection semaphore for PHP-CGI handler ID: {}", self.php_cgi_handler_id));
                return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::Internal)));
            }
            let connection_semaphore = semaphore_option.unwrap();
            grux_request.set_connection_semaphore(connection_semaphore);
        }

        // So now we have everything we need to handle the request, so we pass it to the FastCGI handler
        trace(format!("Serving PHP request via FastCGI at {} and full file path: {}", &connect_ip_and_port, &file_path));

        grux_request.add_calculated_data("fastcgi_connect_ip_and_port", &connect_ip_and_port);
        grux_request.add_calculated_data("fastcgi_script_file", &file_path);
        grux_request.add_calculated_data("fastcgi_uri_is_a_dir_with_index_file_inside", if uri_is_a_dir_with_index_file_inside { "true" } else { "false" });
        grux_request.add_calculated_data("fastcgi_local_web_root", &self.local_web_root);
        grux_request.add_calculated_data("fastcgi_web_root", &self.fastcgi_web_root);

        // Process the FastCGI request with timeout
        match tokio::time::timeout(Duration::from_secs(self.request_timeout as u64), FastCgi::process_fastcgi_request(grux_request)).await {
            Ok(response) => {
                if response.is_err() {
                    error("PHP Request processing via FastCGI failed".to_string());
                    return Err(GruxError::new_with_kind_only(GruxErrorKind::FastCgi(response.err().unwrap())));
                } else {
                    trace("PHP Request completed successfully".to_string());
                }

                return Ok(response.unwrap());
            }
            Err(_) => {
                debug(format!("PHP Request timed out - Timeout: {} seconds - Request: {:?}", self.request_timeout, grux_request));
                return Err(GruxError::new_with_kind_only(GruxErrorKind::PHPProcessor(PHPProcessorError::Timeout)));
            }
        }
    }

    fn get_type(&self) -> String {
        "php".to_string()
    }

    fn get_default_pretty_name(&self) -> String {
        "PHP Processor".to_string()
    }
}

impl PHPProcessor {
    async fn get_ip_and_port(&self) -> Result<String, ()> {
        if self.served_by_type == "win-php-cgi" {
            // Served by local PHP-CGI executable managed by Grux, so this means we use the local_web_root as web root and the php_cgi_handler_id to find the port to connect to with fastcgi

            // Get the running state
            let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
            let external_system_handler = running_state.get_external_system_handler();
            let php_cgi_port_result = external_system_handler.get_port_for_php_cgi(&self.php_cgi_handler_id);
            if php_cgi_port_result.is_err() {
                // Cannot find port for the specified PHP-CGI handler, so we cannot handle
                error(format!("PHP Processor: Cannot find port for PHP-CGI handler ID: {}", self.php_cgi_handler_id));
                return Err(());
            }
            let php_cgi_port = php_cgi_port_result.unwrap();

            Ok(format!("127.0.0.1:{}", php_cgi_port))
        } else if self.served_by_type == "php-fpm" {
            // Served by external FastCGI handler, so we use the fastcgi_ip_and_port to connect to and fastcgi_web_root as web root
            Ok(self.fastcgi_ip_and_port.clone())
        } else {
            // Unknown type, so we cant and wont handle
            error(format!("PHP Processor: Unknown served_by_type: {}", self.served_by_type));
            return Err(());
        }
    }
}
