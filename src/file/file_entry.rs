use std::time::SystemTime;

use crate::{
    http::{
        caching::range::{RangeParseResult, build_multipart_body, build_multipart_body_from_parts, format_content_range, get_range_header, parse_range_header, should_process_range},
        request_response::{
            body_error::{BodyError, box_err},
            gruxi_request::GruxiRequest,
        },
    },
    trace,
};

use futures::TryStreamExt;
use http_body_util::BodyExt;
use http_body_util::Full;
use http_body_util::{StreamBody, combinators::BoxBody};
use hyper::body::{Bytes, Frame};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};
use tokio_util::io::ReaderStream;

pub struct FileEntry {
    pub meta: FileMeta,
    pub content: ContentCache,
}

pub struct ContentCache {
    pub raw: Option<Bytes>,
    pub gzip: Option<Bytes>,
}

#[derive(Debug)]
pub struct FileMeta {
    pub file_path: String,
    pub is_directory: bool,
    pub exists: bool,
    pub length: u64,
    pub is_too_large_to_store: bool,
    pub mime_type: String,
    pub last_modified: SystemTime,
    // Response caching headers
    pub etag_header: Option<String>,
    pub last_modified_header: Option<String>,
    pub expires_header: Option<String>,
    pub cache_control_header: Option<String>,
}

pub enum ContentResult {
    Full { encoding: Option<String> },
    SingleRange { content_range: String },
    MultipartRange { content_type: String },
    RangeNotSatisfiable,
    Error,
}

impl FileEntry {
    /// Result type for range request handling
    /// Contains the body, encoding, optional content-range header, and status code
    pub async fn get_content_stream(&self, gruxi_request: &mut GruxiRequest) -> (BoxBody<Bytes, BodyError>, ContentResult) {
        // Check if this is a range request - clone the header value to avoid borrow issues
        let range_header_value = get_range_header(gruxi_request).and_then(|h| h.to_str().ok()).map(|s| s.to_string());

        if let Some(range_str) = range_header_value {
            // Check If-Range precondition before processing range
            if should_process_range(gruxi_request, self.meta.etag_header.as_deref(), &self.meta.last_modified) {
                let range_result = self.handle_range_request(&range_str).await;
                if let Some(result) = range_result {
                    return result;
                }
                // If range_result is None, fall through to serve full content
            }
        }

        // Serve full content (no range request or range not applicable)
        self.get_full_content_stream(gruxi_request).await
    }

    /// Handle a range request, returning None if we should serve full content instead
    async fn handle_range_request(&self, range_str: &str) -> Option<(BoxBody<Bytes, BodyError>, ContentResult)> {
        let content_length = self.meta.length;

        match parse_range_header(range_str) {
            RangeParseResult::NoRangeHeader | RangeParseResult::InvalidSyntax | RangeParseResult::UnsupportedUnit => {
                // Serve full content
                None
            }
            RangeParseResult::Ranges(ranges) => {
                // Resolve all ranges against content length
                let resolved_ranges: Vec<(u64, u64)> = ranges.iter().filter_map(|r| r.resolve(content_length)).collect();

                if resolved_ranges.is_empty() {
                    // No satisfiable ranges - this will be handled by the caller with 416 response
                    // Return a special marker (empty body with encoding indicating unsatisfiable)
                    trace!("No satisfiable ranges found");
                    return Some((empty_body(), ContentResult::RangeNotSatisfiable));
                }

                if resolved_ranges.len() == 1 {
                    // Single range - use optimized path that avoids reading entire file
                    let (start, end) = resolved_ranges[0];
                    Some(self.get_single_range_content(start, end).await)
                } else {
                    // Multiple ranges - need to build multipart response
                    // For cached content, use zero-copy slicing; for uncached, read efficiently
                    Some(self.get_multipart_range_content(&resolved_ranges).await)
                }
            }
        }
    }

    /// Get content for a single range request - optimized to avoid reading entire file
    async fn get_single_range_content(&self, start: u64, end: u64) -> (BoxBody<Bytes, BodyError>, ContentResult) {
        let content_length = self.meta.length;
        let content_range = format_content_range(start, end, content_length);

        // If content is cached, slice directly without copying
        if let Some(raw_content) = &self.content.raw {
            let start_idx = start as usize;
            let end_idx = (end + 1) as usize;
            let end_idx = end_idx.min(raw_content.len());

            if start_idx < raw_content.len() {
                // Use Bytes::slice for zero-copy
                let range_bytes = raw_content.slice(start_idx..end_idx);
                let full_body = Full::new(range_bytes).map_err(|never| -> BodyError { match never {} });
                return (BoxBody::new(full_body), ContentResult::SingleRange { content_range });
            }
        }

        // For uncached files, seek and read only the needed bytes
        let range_length = end - start + 1;
        match File::open(&self.meta.file_path).await {
            Ok(mut file) => {
                // Seek to start position
                if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                    trace!("Failed to seek file {} for range: {}", self.meta.file_path, e);
                    return (empty_body(), ContentResult::Error);
                }

                // Read only the range
                let mut buffer = vec![0u8; range_length as usize];
                match file.read_exact(&mut buffer).await {
                    Ok(_) => {
                        let range_bytes = Bytes::from(buffer);
                        let full_body = Full::new(range_bytes).map_err(|never| -> BodyError { match never {} });
                        (BoxBody::new(full_body), ContentResult::SingleRange { content_range })
                    }
                    Err(e) => {
                        trace!("Failed to read range from file {}: {}", self.meta.file_path, e);
                        (empty_body(), ContentResult::Error)
                    }
                }
            }
            Err(e) => {
                trace!("Failed to open file {} for range: {}", self.meta.file_path, e);
                (empty_body(), ContentResult::Error)
            }
        }
    }

    /// Get content for multiple range requests - builds multipart response
    async fn get_multipart_range_content(&self, resolved_ranges: &[(u64, u64)]) -> (BoxBody<Bytes, BodyError>, ContentResult) {
        let content_length = self.meta.length;

        // If content is cached, use zero-copy slicing for multipart
        if let Some(raw_content) = &self.content.raw {
            let (body_bytes, content_type) = build_multipart_body(resolved_ranges, raw_content.as_ref(), &self.meta.mime_type, content_length);
            trace!("Serving {} ranges as multipart from cache", resolved_ranges.len());
            let full_body = Full::new(body_bytes).map_err(|never| -> BodyError { match never {} });
            return (BoxBody::new(full_body), ContentResult::MultipartRange { content_type });
        }

        // For uncached files, read each range separately and build multipart
        let mut range_contents: Vec<Vec<u8>> = Vec::with_capacity(resolved_ranges.len());

        match File::open(&self.meta.file_path).await {
            Ok(mut file) => {
                for &(start, end) in resolved_ranges {
                    let range_length = (end - start + 1) as usize;

                    if let Err(e) = file.seek(std::io::SeekFrom::Start(start)).await {
                        trace!("Failed to seek file {} for multipart range: {}", self.meta.file_path, e);
                        return (empty_body(), ContentResult::Error);
                    }

                    let mut buffer = vec![0u8; range_length];
                    if let Err(e) = file.read_exact(&mut buffer).await {
                        trace!("Failed to read multipart range from file {}: {}", self.meta.file_path, e);
                        return (empty_body(), ContentResult::Error);
                    }
                    range_contents.push(buffer);
                }
            }
            Err(e) => {
                trace!("Failed to open file {} for multipart ranges: {}", self.meta.file_path, e);
                return (empty_body(), ContentResult::Error);
            }
        }

        // Build multipart body from the collected ranges
        let (body_bytes, content_type) = build_multipart_body_from_parts(resolved_ranges, &range_contents, &self.meta.mime_type, content_length);

        trace!("Serving {} ranges as multipart from disk", resolved_ranges.len());
        let full_body = Full::new(body_bytes).map_err(|never| -> BodyError { match never {} });
        (BoxBody::new(full_body), ContentResult::MultipartRange { content_type })
    }

    /// Get the full content stream
    async fn get_full_content_stream(&self, gruxi_request: &mut GruxiRequest) -> (BoxBody<Bytes, BodyError>, ContentResult) {
        if self.content.raw.is_none() && self.content.gzip.is_none() {
            trace!("No cached file data content is present, so we return from the filesystem instead (full if small and stream if big)");

            // For smaller files (<= 64 KB), return full content, otherwise stream
            if self.meta.length <= 64 * 1024 {
                // Small file, return full
                let file_bytes = match tokio::fs::read(&self.meta.file_path).await {
                    Ok(bytes) => bytes,
                    Err(e) => {
                        trace!("Failed to read file {} for full content: {}", self.meta.file_path, e);
                        return (empty_body(), ContentResult::Error);
                    }
                };
                let full_body = Full::new(Bytes::from(file_bytes)).map_err(|never| -> BodyError { match never {} });
                return (BoxBody::new(full_body), ContentResult::Full { encoding: None });
            }

            // Otherwise we stream, to maintain low memory usage by not loading the full file into memory
            let file = match File::open(&self.meta.file_path).await {
                Ok(f) => f,
                Err(e) => {
                    trace!("Failed to open file {} for streaming: {}", self.meta.file_path, e);
                    return (empty_body(), ContentResult::Error);
                }
            };

            let stream = ReaderStream::new(file).map_ok(Frame::data);
            let streambody = http_body_util::BodyExt::map_err(StreamBody::new(stream), box_err);
            return (BoxBody::new(streambody), ContentResult::Full { encoding: None });
        }

        // We prefer gzip if the client accepts it
        if gruxi_request.check_accepted_encoding("gzip")
            && let Some(gzip_content) = &self.content.gzip
        {
            trace!("Serving gzipped content from cache");
            let gzipped_bytes = gzip_content.clone();
            let boxbody = BoxBody::new(Full::new(gzipped_bytes).map_err(|never| -> BodyError { match never {} }));
            return (boxbody, ContentResult::Full { encoding: Some("gzip".to_string()) });
        }

        // Otherwise serve raw content
        if let Some(raw_content) = &self.content.raw {
            trace!("Serving raw content from cache");
            let raw_bytes = raw_content.clone();
            let boxbody = BoxBody::new(Full::new(raw_bytes).map_err(|never| -> BodyError { match never {} }));
            return (boxbody, ContentResult::Full { encoding: None });
        }

        // If nothing falls to taste, return empty
        (empty_body(), ContentResult::Error)
    }
}

fn empty_body() -> BoxBody<Bytes, BodyError> {
    BoxBody::new(Full::new(Bytes::new()).map_err(|never| -> BodyError { match never {} }))
}
