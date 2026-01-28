use std::time::Duration;

use crate::error::gruxi_error::GruxiError;
use crate::error::gruxi_error_enums::{GruxiErrorKind, PHPProcessorError};
use crate::external_connections::fastcgi::FastCgi;
use crate::file::normalized_path::NormalizedPath;
use crate::http::http_util::resolve_web_root_and_path_and_get_file;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::logging::syslog::{debug, error, trace};
use crate::{
    configuration::site::Site,
    core::running_state_manager::get_running_state_manager,
    http::{http_util::empty_response_with_status, request_handlers::processor_trait::ProcessorTrait, request_response::gruxi_request::GruxiRequest},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PHPProcessor {
    pub id: String, // Unique identifier for the processor
    // Can either be served by a local PHP-CGI executable or via FastCGI (PHP-FPM or similar)
    pub served_by_type: String,      // How it is handled, by Gruxi handled "win-php-cgi" or "php-fpm"
    pub php_cgi_handler_id: String,  // Optional ID of the PHP-CGI handler to use, if user has selected "win-php-cgi" as the type
    pub fastcgi_ip_and_port: String, // Optional IP and port to connect to FastCGI handler, if user has selected "php-fpm" as the type
    // Request timeout, that may be different from the global timeout
    pub request_timeout: u32, // Seconds
    // Web root
    pub local_web_root: String,   // local location for the web root
    pub fastcgi_web_root: String, // Relevant for "php-fpm" type, for web-root rewriting when passing to FastCGI handler
    // Server software spoofing [fastcgi:SERVER_SOFTWARE] (some PHP frameworks check for this in stupid ways - Looking at you, WordPress!)
    pub server_software_spoof: String, // Spoofed server software string

    // Calculated fields (not serialized)
    #[serde(skip)]
    normalized_local_web_root: Option<NormalizedPath>,
    #[serde(skip)]
    normalized_fastcgi_web_root: Option<NormalizedPath>,
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
            server_software_spoof: "".to_string(),
            normalized_local_web_root: None,
            normalized_fastcgi_web_root: None,
        }
    }
}

impl ProcessorTrait for PHPProcessor {
    fn initialize(&mut self) {
        // Check and normalize web roots if not already done
        if self.normalized_local_web_root.is_none() {
            let normalized_path_result = NormalizedPath::new(&self.local_web_root, "");
            self.normalized_local_web_root = match normalized_path_result {
                Ok(path) => Some(path),
                Err(_) => {
                    error(format!("Failed to normalize local web root path: {}", self.local_web_root));
                    return;
                }
            };
        }
        if self.normalized_fastcgi_web_root.is_none() {
            let normalized_path_result = NormalizedPath::new(&self.fastcgi_web_root, "");
            self.normalized_fastcgi_web_root = match normalized_path_result {
                Ok(path) => Some(path),
                Err(_) => {
                    error(format!("Failed to normalize FastCGI web root path: {}", self.fastcgi_web_root));
                    return;
                }
            };
        }
    }

    fn sanitize(&mut self) {
        // Trim strings
        self.id = self.id.trim().to_string();
        self.served_by_type = self.served_by_type.trim().to_string();
        self.php_cgi_handler_id = self.php_cgi_handler_id.trim().to_string();
        self.fastcgi_ip_and_port = self.fastcgi_ip_and_port.trim().to_string();
        self.local_web_root = self.local_web_root.trim().to_string();
        self.fastcgi_web_root = self.fastcgi_web_root.trim().to_string();
        self.server_software_spoof = self.server_software_spoof.trim().to_string();
    }

    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Id should be a uuid
        if Uuid::parse_str(&self.id).is_err() {
            errors.push(format!("PHP Processor: Invalid ID, must be a valid UUID: {}", self.id));
        }

        // served_by_type should be either "win-php-cgi" or "php-fpm"
        if self.served_by_type != "win-php-cgi" && self.served_by_type != "php-fpm" {
            errors.push(format!("PHP Processor: Invalid served_by_type, must be either 'win-php-cgi' or 'php-fpm': {}", self.served_by_type));
        }

        // PHP-CGI handler ID must be set if served_by_type is "win-php-cgi"
        if self.served_by_type == "win-php-cgi" && self.php_cgi_handler_id.trim().is_empty() {
            errors.push("PHP Processor: PHP CGI handler must be set when served by PHP CGI on Windows.".to_string());
        }

        // fastcgi_ip_and_port must be set if served_by_type is "php-fpm"
        if self.served_by_type == "php-fpm" && self.fastcgi_ip_and_port.trim().is_empty() {
            errors.push("PHP Processor: FastCGI IP and port must be set when served by PHP-FPM.".to_string());
        }

        // Request time must be greater than 0
        if self.request_timeout < 1 {
            errors.push("PHP Processor: Request timeout must be greater than 0.".to_string());
        }

        // Local web root must be set
        if self.local_web_root.is_empty() {
            errors.push("PHP Processor: Local web root must be set.".to_string());
        }

        // FastCGI web root must be set
        if self.served_by_type == "php-fpm" && self.fastcgi_web_root.is_empty() {
            errors.push("PHP Processor: FastCGI web root must be set to web root served by PHP-FPM.".to_string());
        }

        // Validate that local web root can be normalized
        let normalized_local_web_root_result = NormalizedPath::new(&self.local_web_root, "");
        if normalized_local_web_root_result.is_err() {
            errors.push(format!("Local web root path is invalid: '{}' - Check strange characters and path format", self.local_web_root));
        }

        // Validate that fastcgi web root can be normalized
        if self.served_by_type == "php-fpm" {
            let normalized_fastcgi_web_root_result = NormalizedPath::new(&self.fastcgi_web_root, "");
            if normalized_fastcgi_web_root_result.is_err() {
                errors.push(format!("FastCGI web root path is invalid: '{}' - Check strange characters and path format", self.fastcgi_web_root));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site) -> Result<GruxiResponse, GruxiError> {
        // Get our web roots, based on normalized paths, so we know they are safe
        let local_web_root_option = self.normalized_local_web_root.as_ref();
        let local_web_root = match local_web_root_option {
            Some(path) => path.get_full_path(),
            None => {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::Internal)));
            }
        };
        let fastcgi_web_root_option = self.normalized_fastcgi_web_root.as_ref();
        let fastcgi_web_root = match fastcgi_web_root_option {
            Some(path) => path.get_full_path(),
            None => {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::Internal)));
            }
        };

        let mut path = gruxi_request.get_path().clone();

        // Get the file, if it exists
        let normalized_path_result = NormalizedPath::new(&local_web_root, &path);
        let normalized_path = match normalized_path_result {
            Ok(path) => path,
            Err(_) => {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::FileNotFound)));
            }
        };

        let file_data_result = resolve_web_root_and_path_and_get_file(&normalized_path).await;
        let mut file_data = match file_data_result {
            Ok(data) => data,
            Err(e) => {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::PathError(e))));
            }
        };
        let mut file_path = file_data.meta.file_path.clone();

        // If the file/dir does not exist, we check if we have a rewrite function that allows us to rewrite to the index file
        if !file_data.meta.exists {
            trace(format!("File does not exist: {}", file_path));
            if site.get_rewrite_functions_hashmap().contains_key("OnlyWebRootIndexForSubdirs") {
                trace(format!("[OnlyWebRootIndexForSubdirs] Rewriting request path {} to root dir due to rewrite function", path));
                // We rewrite the path to just "/" which will make it serve the index file
                path = "/index.php".to_string();

                // Check if the index file exists
                let normalized_path_result = NormalizedPath::new(&local_web_root, &path);
                let normalized_path = match normalized_path_result {
                    Ok(path) => path,
                    Err(_) => {
                        return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::FileNotFound)));
                    }
                };

                let file_data_result = resolve_web_root_and_path_and_get_file(&normalized_path).await;
                let file_data = match file_data_result {
                    Ok(data) => data,
                    Err(e) => {
                        return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::PathError(e))));
                    }
                };
                file_path = file_data.meta.file_path.clone();
            } else {
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::FileNotFound)));
            }
        }

        let mut uri_is_a_dir_with_index_file_inside = false;
        if file_data.meta.is_directory {
            // If it's a directory, we will try to check if there is an index.php file inside
            trace(format!("File is a directory: {}", file_path));

            let normalized_path_result = NormalizedPath::new(&file_path, "/index.php");
            let normalized_path = match normalized_path_result {
                Ok(path) => path,
                Err(_) => {
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::FileNotFound)));
                }
            };

            let file_data_result = resolve_web_root_and_path_and_get_file(&normalized_path).await;
            file_data = match file_data_result {
                Ok(data) => data,
                Err(_) => {
                    return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
                }
            };

            if file_data.meta.exists == false {
                trace(format!("Index files in dir does not exist: {}", file_path));
                return Ok(empty_response_with_status(hyper::StatusCode::NOT_FOUND));
            }

            file_path = file_data.meta.file_path.clone();
            trace(format!("Found index file: {}", file_path));
            uri_is_a_dir_with_index_file_inside = true;
        }

        // Now get the IP and port to connect to
        let connect_ip_and_port_result = self.get_ip_and_port().await;
        let connect_ip_and_port = match connect_ip_and_port_result {
            Ok(ip_and_port) => ip_and_port,
            Err(_) => {
                // Cannot determine how to connect to the PHP handler, so we cannot handle
                error(format!("PHP Processor: Cannot determine how to connect to PHP handler for processor ID: {}", self.id));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::Connection)));
            }
        };

        // Figure out if we have a connection semaphore to use
        if !self.php_cgi_handler_id.trim().is_empty() {
            let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
            let external_system_handler = running_state.get_external_system_handler();

            let semaphore_option = external_system_handler.get_connection_semaphore(&self.php_cgi_handler_id);
            let connection_semaphore = match semaphore_option {
                Some(semaphore) => semaphore,
                None => {
                    error(format!("PHP Processor: Cannot find connection semaphore for PHP-CGI handler ID: {}", self.php_cgi_handler_id));
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::Internal)));
                }
            };
            gruxi_request.set_connection_semaphore(connection_semaphore);
        }

        // So now we have everything we need to handle the request, so we pass it to the FastCGI handler
        trace(format!("Serving PHP request via FastCGI at {} and full file path: {}", &connect_ip_and_port, &file_path));

        gruxi_request.add_calculated_data("fastcgi_connect_ip_and_port", &connect_ip_and_port);
        gruxi_request.add_calculated_data("fastcgi_script_file", &file_path);
        gruxi_request.add_calculated_data("fastcgi_uri_is_a_dir_with_index_file_inside", if uri_is_a_dir_with_index_file_inside { "true" } else { "false" });
        gruxi_request.add_calculated_data("fastcgi_local_web_root", &local_web_root);
        gruxi_request.add_calculated_data("fastcgi_web_root", &fastcgi_web_root);
        gruxi_request.add_calculated_data("fastcgi_override_server_software", &self.server_software_spoof);

        // Process the FastCGI request with timeout
        match tokio::time::timeout(Duration::from_secs(self.request_timeout as u64), FastCgi::process_fastcgi_request(gruxi_request)).await {
            Ok(response) => match response {
                Ok(resp) => {
                    trace("PHP Request completed successfully".to_string());
                    return Ok(resp);
                }
                Err(err) => {
                    error("PHP Request processing via FastCGI failed".to_string());
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::FastCgi(err)));
                }
            },
            Err(_) => {
                debug(format!("PHP Request timed out - Timeout: {} seconds - Request: {:?}", self.request_timeout, gruxi_request));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::PHPProcessor(PHPProcessorError::Timeout)));
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
            // Served by local PHP-CGI executable managed by Gruxi, so this means we use the local_web_root as web root and the php_cgi_handler_id to find the port to connect to with fastcgi

            // Get the running state
            let running_state = get_running_state_manager().await.get_running_state_unlocked().await;
            let external_system_handler = running_state.get_external_system_handler();
            let php_cgi_port_result = external_system_handler.get_port_for_php_cgi(&self.php_cgi_handler_id);
            let php_cgi_port = match php_cgi_port_result {
                Ok(port) => port,
                Err(_) => {
                    error(format!("PHP Processor: Cannot find port for PHP-CGI handler ID: {}", self.php_cgi_handler_id));
                    return Err(());
                }
            };

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
