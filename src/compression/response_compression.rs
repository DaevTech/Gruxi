use crate::core::running_state::RunningState;
use crate::http::request_response::gruxi_body::GruxiBody::Buffered;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::{debug, trace};
use flate2::write::GzEncoder;
use http::HeaderValue;
use hyper::body::Bytes;
use std::io::Write;

pub async fn maybe_compress_response(request: &GruxiRequest, response: &mut GruxiResponse, running_state: &RunningState) {
    let mut should_compress = false;
    let content_encoding_header_option = response.headers().get(hyper::header::CONTENT_ENCODING);
    trace!("Checking if response should be compressed. Content-Encoding header: {:?}", content_encoding_header_option);

    match content_encoding_header_option {
        Some(_) => {
            // If content encoding is already present, we skip compression
            trace!("Content-Encoding header is already present, skipping compression");
            return;
        }
        None => {
            let headers = response.headers();
            // If cache control header has no-transform, we skip compression
            let cache_control_header_option = headers.get(hyper::header::CACHE_CONTROL);
            if let Some(cache_control_header) = cache_control_header_option
                && cache_control_header.to_str().unwrap_or("").contains("no-transform")
            {
                // If no-transform is present, we skip compression
                trace!("Cache-Control header contains no-transform, skipping compression");
                return;
            }
            // If content range is present, we skip compression
            let content_range_header_option = headers.get(hyper::header::CONTENT_RANGE);
            if content_range_header_option.is_some() {
                // If content range is present, we skip compression
                trace!("Content-Range header is present, skipping compression");
                return;
            }

            // If content encoding is not present, we consider compressing if it's a compressible type and size
            let content_type_header_option = headers.get(hyper::header::CONTENT_TYPE);
            if let Some(content_type_header) = content_type_header_option
                && request.check_accepted_encoding("gzip")
            {
                let content_length = response.get_body_size();
                trace!(
                    "Compression check: Content-Type header: {}, Content-Length: {}",
                    content_type_header.to_str().unwrap_or(""),
                    content_length
                );
                if running_state.file_reader_cache.should_compress(content_type_header.to_str().unwrap_or(""), content_length) {
                    trace!("Content is eligible for compression, will compress response");
                    should_compress = true;
                }
            }
        }
    }

    if should_compress {
        trace!("Compressing response");
        compress_response(response, running_state).await;
    }
}

pub async fn compress_response(response: &mut GruxiResponse, running_state: &RunningState) {
    // We hit the access counter for this resource, which will help us determine what to keep in the compression cache
    let resource_id = response.get_resource_id();
    let mut should_cache = false;

    // If we have a resource ID, we record the access and check if we have a cached compressed version
    if !resource_id.is_empty() {
        let hits_ceiling = running_state.get_access_counters().compression_access_counter.record_access(&resource_id);
        trace!("Recorded access for resource ID {}. Hits ceiling: {}", resource_id, hits_ceiling);
        should_cache = hits_ceiling;

        if should_cache && let Some(cached_compressed_response) = running_state.get_compression_cache().get(&resource_id).await {
            trace!("Found cached compressed response for resource ID {}, using cached version", resource_id);
            response.set_body(Buffered(cached_compressed_response));
            response.headers_mut().insert("Content-Encoding", HeaderValue::from_static("gzip"));
            response.headers_mut().insert("Vary", HeaderValue::from_static("Accept-Encoding"));
            return;
        }
    }

    // If we do not have a cached compressed version, we proceed to compress the response
    let body_bytes = response.get_body_bytes().await;

    let compressed_bytes = match compress_content(&body_bytes) {
        Ok(bytes) => Bytes::from(bytes),
        Err(e) => {
            // If compression fails, we just return without modifying the response
            debug!("Gzip compression failed: {}", e);
            return;
        }
    };

    if should_cache {
        if !resource_id.is_empty() {
            trace!("Caching compressed response for resource ID {}", resource_id);
            running_state.get_compression_cache().insert(resource_id.to_string(), compressed_bytes.clone()).await;
        }
    }

    response.set_body(Buffered(compressed_bytes));
    response.headers_mut().insert("Content-Encoding", HeaderValue::from_static("gzip"));
    response.headers_mut().insert("Vary", HeaderValue::from_static("Accept-Encoding"));
}

// Compress content using gzip
pub fn compress_content(content: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut compressed_bytes = Vec::with_capacity(content.len() / 2);
    let mut encoder = GzEncoder::new(&mut compressed_bytes, flate2::Compression::new(2));
    encoder.write_all(content)?;
    encoder.finish()?;
    Ok(compressed_bytes)
}
