use crate::{
    configuration::site::Site,
    error::{
        gruxi_error::GruxiError,
        gruxi_error_enums::{GruxiErrorKind, StaticFileProcessorError},
    },
    file::file_util::{check_path_secure, get_full_file_path},
    http::{
        http_util::{resolve_web_root_and_path_and_get_file},
        request_handlers::processor_trait::ProcessorTrait,
        request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
    },
    logging::syslog::{error, trace},
};
use hyper::{header::HeaderValue};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticFileProcessor {
    pub id: String,                            // Unique identifier for the processor
    pub web_root: String,                      // Web root directory for static files
    pub web_root_index_file_list: Vec<String>, // List of index files to look for in directories
}

impl StaticFileProcessor {
    pub fn new(web_root: String, web_root_index_file_list: Vec<String>) -> Self {
        let id = Uuid::new_v4().to_string();
        Self {
            id,
            web_root,
            web_root_index_file_list,
        }
    }
}

impl ProcessorTrait for StaticFileProcessor {
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

        // Validate index file list
        for (idx, file) in self.web_root_index_file_list.iter().enumerate() {
            if file.trim().is_empty() {
                errors.push(format!("Index file at position {} cannot be empty", idx + 1));
            }
        }

        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }

    async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site) -> Result<GruxiResponse, GruxiError> {
        // First, check if there is a specific file requested
        let web_root_result = get_full_file_path(&self.web_root);
        if let Err(e) = web_root_result {
            error(format!("Failed to get full web root path: {} for site: {:?}", e, site));
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
        }
        let web_root = web_root_result.unwrap();
        let mut path = gruxi_request.get_path().clone();

        // Get the cached file, if it exists
        let file_data_result = resolve_web_root_and_path_and_get_file(&web_root, &path).await;
        if let Err(e) = file_data_result {
            // If we fail to get the file, return cant/wont handle
            error(format!("We could not get data on the file: {}, so we cannot handle with static file processor", e));
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
        }
        let mut file_data = file_data_result.unwrap();
        let mut file_path = file_data.meta.file_path.clone();

        // If the file/dir does not exist, we check if we have a rewrite function that allows us to rewrite to the index file
        if !file_data.meta.exists {
            trace(format!("File does not exist: {}", file_path));
            if site.get_rewrite_functions_hashmap().contains_key("OnlyWebRootIndexForSubdirs") {
                trace(format!("[OnlyWebRootIndexForSubdirs] Rewriting request path {} to root dir due to rewrite function", path));
                // We rewrite the path to just "/" which will make it serve the index file
                path = "/".to_string();

                // Get the cached file, if it exists
                let file_data_result = resolve_web_root_and_path_and_get_file(&web_root, &path).await;
                if let Err(e) = file_data_result {
                    trace(format!(
                        "File does not exist, even after rewrite function is applied: {}, so we cannot handle with static file processor",
                        file_path
                    ));
                    return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
                }
                file_data = file_data_result.unwrap();
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
                // Get the cached file, if it exists
                let file_data_result = resolve_web_root_and_path_and_get_file(&file_path, &file).await;
                if let Err(_) = file_data_result {
                    trace(format!("Index files in dir does not exist: {}", file_path));
                    continue;
                }
                file_data = file_data_result.unwrap();
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

        // Do a safety check of the path, make sure it's still under the web root and not blocked
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
        response.headers_mut().insert(hyper::header::CONTENT_TYPE, HeaderValue::from_str(&file_data.meta.mime_type).unwrap());

        // Set content encoding if gzipped
        if compression == "gzip" {
            response.headers_mut().insert(hyper::header::CONTENT_ENCODING, HeaderValue::from_str("gzip").unwrap());
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
