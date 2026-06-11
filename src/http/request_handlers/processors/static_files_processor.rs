use crate::error;
use crate::file::file_entry::ContentResult;
use crate::http::http_util::trailing_slash_check;
use crate::{
    config::site::Site,
    error::{
        gruxi_error::GruxiError,
        gruxi_error_enums::{GruxiErrorKind, StaticFileProcessorError},
    },
    file::{file_util::check_path_secure, normalized_path::NormalizedPath},
    http::{
        caching::{
            etag::handle_conditional_headers,
            range::{accept_ranges_bytes, format_content_range_unsatisfiable},
        },
        http_server::ConnectionContext,
        request_handlers::processor_trait::ProcessorTrait,
        request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
    },
    trace, warn,
};
use http::HeaderName;
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
            let normalized_path_result = NormalizedPath::new(&self.web_root, "", true);
            self.normalized_web_root = match normalized_path_result {
                Ok(path) => Some(path),
                Err(_) => {
                    error!("Failed to normalize web root path: {}", self.web_root);
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
        let normalized_path_result = NormalizedPath::new(&self.web_root, "", true);
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

    async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site, connection_context: &ConnectionContext) -> Result<GruxiResponse, GruxiError> {
        // Check and normalize web root if not already done
        if self.normalized_web_root.is_none() {
            error!("StaticFileProcessor web root is not initialized as expected for id: '{}'", self.id);
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
        }

        // Get our web root and requested path
        let web_root_option = self.normalized_web_root.as_ref();
        let web_root = match web_root_option {
            None => {
                error!("StaticFileProcessor web root is not initialized as expected for id: '{}'", self.id);
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
            Some(web_root) => web_root.get_full_path(),
        };

        let mut path = gruxi_request.get_path().to_string();

        // Get the file, if it exists
        let normalized_path_result = NormalizedPath::new(web_root, &path, false);
        if normalized_path_result.is_err() {
            trace!("Failed or rejected to normalize request path: {}", path);
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
        }
        let mut normalized_path = match normalized_path_result {
            Ok(path) => path,
            Err(_) => {
                trace!("Failed or rejected to normalize request path: {}", path);
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        };

        let file_reader_cache = connection_context.running_state.get_file_reader_cache();

        let file_data_result = file_reader_cache.get_file(normalized_path.get_full_path()).await;
        let mut file_data = match file_data_result.meta.exists {
            true => file_data_result,
            false => {
                trace!("File does not exist: {}", normalized_path.get_full_path());
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        };


        // Make sure the trailing slash logic is correct
        let trailing_slash_result = trailing_slash_check(file_data.clone(), &path);
        match trailing_slash_result {
            Ok(_) => {}
            Err(response) => {
                // If there is some problem that could be handled, we get a response back, that we just return to the user
                return Ok(response);
            }
        }

        // If the file/dir does not exist, we check if we have a rewrite function that allows us to rewrite to the index file
        if !file_data.meta.exists {
            trace!("File does not exist: {}", file_data.meta.file_path);
            if site.has_rewrite_function("OnlyWebRootIndexForSubdirs") {
                trace!("[OnlyWebRootIndexForSubdirs] Rewriting request path {} to root dir due to rewrite function", path);
                // We rewrite the path to just "/" which will make it serve the index file
                path = "/".to_string();

                // Get the cached file, if it exists
                let normalized_path_result = normalized_path.set_path(&path, true);
                match normalized_path_result {
                    Ok(_) => {}
                    Err(_) => {
                        trace!("Failed or rejected to normalize request path: {}", path);
                        return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
                    }
                };

                let file_data_result = file_reader_cache.get_file(normalized_path.get_full_path()).await;
                file_data = match file_data_result.meta.exists {
                    true => file_data_result,
                    false => {
                        trace!("File does not exist: {}", normalized_path.get_full_path());
                        return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
                    }
                };
            } else {
                trace!(
                    "File does not exist and no rewrite function is applied: {}, so we cannot handle with static file processor",
                    normalized_path.get_full_path()
                );
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        }

        if file_data.meta.is_directory {
            // If it's a directory, we will try to return the index file
            trace!("File is a directory: {}", normalized_path.get_full_path());

            // Check if we can find a index file in the directory
            let mut found_index = false;
            for file in &self.web_root_index_file_list {
                // Get the file, if it exists
                let file_index_to_check = format!("{}{}", normalized_path.get_path(), file);
                let normalized_path_result = normalized_path.set_path(&file_index_to_check, true);
                match normalized_path_result {
                    Ok(_) => {}
                    Err(_) => {
                        trace!("Failed to normalize path: {}", normalized_path.get_full_path());
                        continue;
                    }
                };

                let file_data_result = file_reader_cache.get_file(normalized_path.get_full_path()).await;
                file_data = match file_data_result.meta.exists {
                    true => file_data_result,
                    false => {
                        trace!("Index files in dir does not exist: {}", normalized_path.get_full_path());
                        continue;
                    }
                };

                trace!("Found index file: {}", normalized_path.get_full_path());
                found_index = true;
                break;
            }

            if !found_index {
                trace!("Did not find index file: {}", normalized_path.get_full_path());
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        }

        // Do a safety check of the path, make sure it's still under the web root and not blocked file extension
        if !check_path_secure(web_root, &normalized_path, &connection_context.configuration.core.server_settings.blocked_file_patterns).await {
            trace!("File path is not secure: {}", normalized_path.get_full_path());
            // We should probably not reveal that the file is blocked, so we return a 404
            return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileBlockedDueToSecurity(
                normalized_path.get_full_path().to_string(),
            ))));
        }

        // Get a stream of the file content, based on the accept-encoding header and range requests
        let (stream, content_result) = file_data.get_content_stream(gruxi_request).await;

        // Determine response status and handle range-specific logic
        let (status_code, content_type_override, content_range_header, encoding) = match content_result {
            ContentResult::SingleRange { content_range } => {
                // Single range - 206 Partial Content
                (206, None, Some(content_range), None)
            }
            ContentResult::MultipartRange { content_type } => {
                // Multiple ranges - 206 Partial Content with multipart/byteranges
                (206, Some(content_type), None, None)
            }
            ContentResult::RangeNotSatisfiable => {
                // 416 Range Not Satisfiable
                let mut response = GruxiResponse::new_empty_with_status(416);
                // Add Content-Range header with unsatisfiable indicator
                let content_range = format_content_range_unsatisfiable(file_data.meta.length);
                if let Ok(header_value) = HeaderValue::from_str(&content_range) {
                    response.headers_mut().insert(hyper::header::CONTENT_RANGE, header_value);
                }
                // Add Accept-Ranges header
                response.headers_mut().insert(hyper::header::ACCEPT_RANGES, accept_ranges_bytes());
                return Ok(response);
            }
            ContentResult::Full { encoding } => {
                // Normal 200 OK response
                (200, None, None, encoding)
            }
            ContentResult::Error => {
                // If there was an error getting the content, we return a 404 Not Found to avoid revealing information
                trace!("Error getting content stream for file: {}", normalized_path.get_full_path());
                return Err(GruxiError::new_with_kind_only(GruxiErrorKind::StaticFileProcessor(StaticFileProcessorError::FileNotFound)));
            }
        };

        let mut response = GruxiResponse::new_with_body(status_code, stream);

        // Set a resource id on the response, for caching
        response.set_resource_id(normalized_path.get_full_path().to_string());

        // Handle conditional headers like If-*-Match and If-*Modified-Since if we have an ETag
        // Only for full content requests (status 200)
        if status_code == 200
            && let Some(etag) = file_data.meta.etag_header.as_ref()
            && handle_conditional_headers(gruxi_request, &mut response, etag, &file_data.meta.last_modified)
        {
            // If we handled a conditional request, return the response as is (304 Not Modified)
            return Ok(response);
        }

        // Set Content-Range header for single range responses
        if let Some(content_range) = content_range_header
            && let Ok(header_value) = HeaderValue::from_str(&content_range)
        {
            response.headers_mut().insert(hyper::header::CONTENT_RANGE, header_value);
        }

        // Set content type (override for multipart ranges)
        let content_type = content_type_override.as_deref().unwrap_or(&file_data.meta.mime_type);
        let header_value = HeaderValue::from_str(content_type);
        match header_value {
            Err(e) => {
                warn!(
                    "Failed to set content type header for file: {} with mime type: {}. Error: {}",
                    normalized_path.get_full_path(),
                    content_type,
                    e
                );
            }
            Ok(value) => {
                response.headers_mut().insert(hyper::header::CONTENT_TYPE, value);
            }
        }

        // Set content encoding if gzipped (only for non-range requests)
        if let Some(encoding) = encoding
            && encoding == "gzip"
        {
            let header_value = HeaderValue::from_str("gzip");
            match header_value {
                Err(e) => {
                    warn!("Failed to set content encoding header for file: {} with gzip. Error: {}", normalized_path.get_full_path(), e);
                }
                Ok(value) => {
                    response.headers_mut().insert(hyper::header::CONTENT_ENCODING, value);
                }
            }
        }

        // Always add Accept-Ranges header to indicate range request support
        response.headers_mut().insert(hyper::header::ACCEPT_RANGES, accept_ranges_bytes());

        // Set ETag header, if available
        StaticFileProcessor::add_caching_headers(file_data.meta.etag_header.as_ref(), hyper::header::ETAG, &mut response, normalized_path.get_full_path());

        // Set Last-Modified header, if available
        StaticFileProcessor::add_caching_headers(
            file_data.meta.last_modified_header.as_ref(),
            hyper::header::LAST_MODIFIED,
            &mut response,
            normalized_path.get_full_path(),
        );

        // Set Expires header, if available
        StaticFileProcessor::add_caching_headers(file_data.meta.expires_header.as_ref(), hyper::header::EXPIRES, &mut response, normalized_path.get_full_path());

        // Set cache-control header, if available
        StaticFileProcessor::add_caching_headers(
            file_data.meta.cache_control_header.as_ref(),
            hyper::header::CACHE_CONTROL,
            &mut response,
            normalized_path.get_full_path(),
        );

        Ok(response)
    }

    fn get_type(&self) -> String {
        "static".to_string()
    }

    fn get_default_pretty_name(&self) -> String {
        "Static File Processor".to_string()
    }
}

impl StaticFileProcessor {
    fn add_caching_headers(header_value_option: Option<&String>, header_name: HeaderName, response: &mut GruxiResponse, file_path: &str) {
        if let Some(header_value_string) = header_value_option {
            let header_value = HeaderValue::from_str(header_value_string);
            match header_value {
                Err(e) => {
                    warn!("Failed to set header: {} for file: {} with value: {}. Error: {}", header_name, file_path, header_value_string, e);
                }
                Ok(value) => {
                    response.headers_mut().insert(header_name, value);
                }
            }
        }
    }
}
