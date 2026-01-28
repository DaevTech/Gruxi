use crate::{
    configuration::site::Site,
    error::{
        gruxi_error::GruxiError,
        gruxi_error_enums::{GruxiErrorKind, StaticFileProcessorError},
    },
    file::{file_util::check_path_secure, normalized_path::NormalizedPath},
    http::{
        http_util::resolve_web_root_and_path_and_get_file,
        request_handlers::processor_trait::ProcessorTrait,
        request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
    },
    logging::syslog::{error, trace},
};
use hyper::header::HeaderValue;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticFileProcessor {
    pub id: String,                            // Unique identifier for the processor
    pub web_root: String,                      // Web root directory for static files
    pub web_root_index_file_list: Vec<String>, // List of index files to look for in directories

    // Calculated fields (not serialized)
    #[serde(skip)]
    normalized_web_root: Option<NormalizedPath>,
}

impl StaticFileProcessor {
    pub fn new(web_root: String, web_root_index_file_list: Vec<String>) -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            web_root,
            web_root_index_file_list,
            normalized_web_root: None,
        }
    }
}

impl ProcessorTrait for StaticFileProcessor {
    fn initialize(&mut self) {
        // Check and normalize web root if not already done
        if self.normalized_web_root.is_none() {
            let normalized_path_result = NormalizedPath::new(&self.web_root, "");
            self.normalized_web_root = match normalized_path_result {
                Ok(path) => Some(path),
                Err(_) => {
                    error(format!("Failed to normalize web root path: {}", self.web_root));
                    return;
                }
            };
        }
    }

    fn sanitize(&mut self) {
        // Trim whitespace from web root
        self.web_root = self.web_root.trim().to_string();

        // Convert backslashes to forward slashes in web root (for Windows paths)
        self.web_root = self.web_root.replace("\\", "/");

        // Trim whitespace from each index file and remove empty entries
        self.web_root_index_file_list = self.web_root_index_file_list.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();

        // For index files, remove any non-allowed characters (basic sanitization)
        for file in &mut self.web_root_index_file_list {
            *file = file.replace("..", ""); // Prevent directory traversal
            *file = file.replace("\\", "/"); // Normalize slashes
            *file = file.replace("//", "/"); // Remove double slashes
        }
    }

    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Validate web root
        if self.web_root.trim().is_empty() {
            errors.push("Web root cannot be empty".to_string());
        }

        // Validate that web root can be normalized
        let normalized_path_result = NormalizedPath::new(&self.web_root, "");
        if normalized_path_result.is_err() {
            errors.push(format!("Web root path is invalid: '{}' - Check strange characters and path format", self.web_root));
        }

        // Validate index file list
        for (idx, file) in self.web_root_index_file_list.iter().enumerate() {
            if file.trim().is_empty() {
                errors.push(format!("Index file at position {} cannot be empty", idx + 1));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site) -> Result<GruxiResponse, GruxiError> {
        // Check and normalize web root if not already done
        if self.normalized_web_root.is_none() {
            error(format!("StaticFileProcessor web root is not initialized as expected for id: '{}'", self.id));
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
        }

        // Get our web root and requested path
        let web_root_option = self.normalized_web_root.as_ref();
        let web_root = match web_root_option {
            None => {
                error(format!("StaticFileProcessor web root is not initialized as expected for id: '{}'", self.id));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
            Some(web_root) => web_root.get_full_path(),
        };

        let mut path = gruxi_request.get_path().clone();

        // Get the file, if it exists
        let normalized_path_result = NormalizedPath::new(&web_root, &path);
        if let Err(_) = normalized_path_result {
            trace(format!("Failed or rejected to normalize request path: {}", path));
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
        }
        let normalized_path = match normalized_path_result {
            Ok(path) => path,
            Err(_) => {
                trace(format!("Failed or rejected to normalize request path: {}", path));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        };

        let file_data_result = resolve_web_root_and_path_and_get_file(&normalized_path).await;
        if let Err(e) = file_data_result {
            // If we fail to get the file, return cant/wont handle
            trace(format!("We could not get data on the file: {}, so we cannot handle with static file processor", e));
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
        }
        let mut file_data = match file_data_result {
            Ok(data) => data,
            Err(e) => {
                trace(format!("We could not get data on the file: {}, so we cannot handle with static file processor", e));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
            }
        };
        let mut file_path = file_data.meta.file_path.clone();

        // If the file/dir does not exist, we check if we have a rewrite function that allows us to rewrite to the index file
        if !file_data.meta.exists {
            trace(format!("File does not exist: {}", file_path));
            if site.get_rewrite_functions_hashmap().contains_key("OnlyWebRootIndexForSubdirs") {
                trace(format!("[OnlyWebRootIndexForSubdirs] Rewriting request path {} to root dir due to rewrite function", path));
                // We rewrite the path to just "/" which will make it serve the index file
                path = "/".to_string();

                // Get the cached file, if it exists
                let normalized_path_result = NormalizedPath::new(&web_root, &path);
                let normalized_path = match normalized_path_result {
                    Ok(path) => path,
                    Err(_) => {
                        trace(format!("Failed or rejected to normalize request path: {}", path));
                        return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
                    }
                };

                let file_data_result = resolve_web_root_and_path_and_get_file(&normalized_path).await;
                file_data = match file_data_result {
                    Ok(data) => data,
                    Err(e) => {
                        trace(format!("We could not get data on the file: {}, so we cannot handle with static file processor", e));
                        return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
                    }
                };
                file_path = file_data.meta.file_path.clone();
            } else {
                trace(format!(
                    "File does not exist and no rewrite function is applied: {}, so we cannot handle with static file processor",
                    file_path
                ));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        }

        if file_data.meta.is_directory {
            // If it's a directory, we will try to return the index file
            trace(format!("File is a directory: {}", file_path));

            // Check if we can find a index file in the directory
            let mut found_index = false;
            for file in &self.web_root_index_file_list {
                // Get the file, if it exists
                let normalized_path_result = NormalizedPath::new(&file_path, &file);
                let normalized_path = match normalized_path_result {
                    Ok(path) => path,
                    Err(_) => {
                        trace(format!("Failed to normalize path: {} and file: {}", file_path, file));
                        continue;
                    }
                };

                let file_data_result = resolve_web_root_and_path_and_get_file(&normalized_path).await;
                file_data = match file_data_result {
                    Ok(data) => data,
                    Err(_) => {
                        trace(format!("Index files in dir does not exist: {}", file_path));
                        continue;
                    }
                };

                if file_data.meta.exists == false {
                    trace(format!("Index files in dir does not exist: {}", file_path));
                    continue;
                }

                file_path = file_data.meta.file_path.clone();
                trace(format!("Found index file: {}", file_path));
                found_index = true;
                break;
            }

            if !found_index {
                trace(format!("Did not find index file: {}", file_path));
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        }

        // Do a safety check of the path, make sure it's still under the web root and not blocked file extension
        if !check_path_secure(&web_root, &file_path).await {
            trace(format!("File path is not secure: {}", file_path));
            // We should probably not reveal that the file is blocked, so we return a 404
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileBlockedDueToSecurity(
                file_path,
            ))));
        }

        // Get a stream of the file content, based on the accept-encoding header
        let (stream, compression) = file_data.get_content_stream(gruxi_request).await;

        let mut response = GruxiResponse::new_with_body(hyper::StatusCode::OK.as_u16(), stream);

        // Set content type
        let header_value = HeaderValue::from_str(&file_data.meta.mime_type);
        match header_value {
            Err(e) => {
                error(format!(
                    "Failed to set content type header for file: {} with mime type: {}. Error: {}",
                    file_path, file_data.meta.mime_type, e
                ));
            }
            Ok(value) => {
                response.headers_mut().insert(hyper::header::CONTENT_TYPE, value);
            }
        }

        // Set content encoding if gzipped
        if compression == "gzip" {
            let header_value = HeaderValue::from_str("gzip");
            match header_value {
                Err(e) => {
                    error(format!("Failed to set content encoding header for file: {} with gzip. Error: {}", file_path, e));
                }
                Ok(value) => {
                    response.headers_mut().insert(hyper::header::CONTENT_ENCODING, value);
                }
            }
        }

        Ok(response)
    }

    fn get_type(&self) -> String {
        "static".to_string()
    }

    fn get_default_pretty_name(&self) -> String {
        "Static File Processor".to_string()
    }
}
