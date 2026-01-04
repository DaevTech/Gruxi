use crate::{
    configuration::site::Site,
    error::{grux_error::GruxError, grux_error_enums::{GruxErrorKind, StaticFileProcessorError}},
    file::file_util::{check_path_secure, get_full_file_path},
    http::{
        http_util::{full, resolve_web_root_and_path_and_get_file},
        request_handlers::processor_trait::ProcessorTrait,
        request_response::{grux_request::GruxRequest, grux_response::GruxResponse},
    },
    logging::syslog::{error, trace},
};
use hyper::{Response, header::HeaderValue};
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

    async fn handle_request(&self, grux_request: &mut GruxRequest, site: &Site) -> Result<GruxResponse, GruxError> {
        // First, check if there is a specific file requested
        let web_root_result = get_full_file_path(&self.web_root);
        if let Err(e) = web_root_result {
            error(format!("Failed to get full web root path: {} for site: {:?}", e, site));
            return Err(GruxError::new_with_kind_only(GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
        }
        let web_root = web_root_result.unwrap();
        let mut path = grux_request.get_path().clone();

        // Get the cached file, if it exists
        let file_data_result = resolve_web_root_and_path_and_get_file(&web_root, &path).await;
        if let Err(e) = file_data_result {
            // If we fail to get the file, return cant/wont handle
            error(format!("We could not get data on the file: {}, so we cannot handle with static file processor", e));
            return Err(GruxError::new_with_kind_only(GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
        }
        let mut file_data = file_data_result.unwrap();
        let mut file_path = file_data.file_path.clone();

        // If the file/dir does not exist, we check if we have a rewrite function that allows us to rewrite to the index file
        if !file_data.exists {
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
                    return Err(GruxError::new_with_kind_only(GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::PathError(e))));
                }
                file_data = file_data_result.unwrap();
                file_path = file_data.file_path.clone();
            } else {
                trace(format!(
                    "File does not exist and no rewrite function is applied: {}, so we cannot handle with static file processor",
                    file_path
                ));
                return Err(GruxError::new_with_kind_only(GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        }

        if file_data.is_directory {
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
                file_path = file_data.file_path.clone();
                trace(format!("Found index file: {}", file_path));
                found_index = true;
                break;
            }

            if !found_index {
                trace(format!("Did not find index file: {}", file_path));
                return Err(GruxError::new_with_kind_only(GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        }

        // Do a safety check of the path, make sure it's still under the web root and not blocked
        if !check_path_secure(&web_root, &file_path).await {
            trace(format!("File path is not secure: {}", file_path));
            // We should probably not reveal that the file is blocked, so we return a 404
            return Err(GruxError::new_with_kind_only(GruxErrorKind::StaticFileProcessor(StaticFileProcessorError::FileBlockedDueToSecurity(file_path))));
        }

        // Get configuration, as we need to check for gzip support
        let cached_configuration = crate::configuration::cached_configuration::get_cached_configuration();
        let config = cached_configuration.get_configuration().await;
        let gzip_enabled = &config.core.gzip.is_enabled;
        let gzip_compressable_mime_types = &config.core.gzip.compressible_content_types;

        // Gzip body or raw content
        let mut is_gzipped = false;
        let body_content = if file_data.gzip_content.is_empty() || !gzip_enabled || !gzip_compressable_mime_types.contains(&file_data.mime_type) {
            file_data.content
        } else {
            is_gzipped = true;
            file_data.gzip_content
        };

        let mut response = Response::new(full(body_content));
        response.headers_mut().insert("Content-Type", HeaderValue::from_str(&file_data.mime_type).unwrap());
        if is_gzipped {
            response.headers_mut().insert("Content-Encoding", HeaderValue::from_str("gzip").unwrap());
        }
        *response.status_mut() = hyper::StatusCode::OK;

        Ok(GruxResponse::from_hyper_bytes(response).await)
    }

    fn get_type(&self) -> String {
        "static".to_string()
    }

    fn get_default_pretty_name(&self) -> String {
        "Static File Processor".to_string()
    }
}
