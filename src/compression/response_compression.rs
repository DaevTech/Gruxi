use crate::debug;
use crate::file::file_reader_structs::FileReaderCache;
use crate::http::request_response::gruxi_body::GruxiBody::Buffered;
use crate::http::request_response::gruxi_request::GruxiRequest;
use crate::http::request_response::gruxi_response::GruxiResponse;
use flate2::write::GzEncoder;
use http::HeaderValue;
use hyper::body::Bytes;
use std::io::Write;

pub async fn maybe_compress_response(request: &GruxiRequest, response: &mut GruxiResponse, file_reader_cache: &FileReaderCache) {
    let mut should_compress = false;
    let content_encoding_header_option = response.headers().get(hyper::header::CONTENT_ENCODING);

    match content_encoding_header_option {
        Some(_) => {
            // If content encoding is already present, we skip compression
            return;
        }
        None => {
            // If cache control header has no-transform, we skip compression
            let cache_control_header_option = response.headers().get(hyper::header::CACHE_CONTROL);
            if let Some(cache_control_header) = cache_control_header_option
                && cache_control_header.to_str().unwrap_or("").contains("no-transform")
            {
                // If no-transform is present, we skip compression
                return;
            }
            // If content range is present, we skip compression
            let content_range_header_option = response.headers().get(hyper::header::CONTENT_RANGE);
            if content_range_header_option.is_some() {
                // If content range is present, we skip compression
                return;
            }

            // If content encoding is not present, we consider compressing if it's a compressible type and size
            let content_type_header_option = response.get_header(hyper::header::CONTENT_TYPE.as_str());
            if let Some(content_type_header) = content_type_header_option {
                if request.check_accepted_encoding("gzip") {
                    let content_length = response.get_body_size();
                    if file_reader_cache.should_compress(content_type_header.to_str().unwrap_or(""), content_length) {
                        should_compress = true;
                    }
                }
            }
        }
    }

    if should_compress {
        compress_response(response).await;
    }
}

pub async fn compress_response(response: &mut GruxiResponse) {
    // Perform gzip compression on the response body
    let body_bytes = response.get_body_bytes().await;
    let mut gzipped_bytes = Vec::new();
    match compress_content(&body_bytes, &mut gzipped_bytes) {
        Ok(_) => {}
        Err(e) => {
            // If compression fails, we just return without modifying the response
            debug!("Gzip compression failed: {}", e);
            return;
        }
    }

    response.set_body(Buffered(Bytes::from(gzipped_bytes)));
    response.headers_mut().insert("Content-Encoding", HeaderValue::from_static("gzip"));
    response.headers_mut().insert("Vary", HeaderValue::from_static("Accept-Encoding"));
}

/// Compress content using gzip
pub fn compress_content(content: &[u8], gzip_content: &mut Vec<u8>) -> Result<(), std::io::Error> {
    let mut encoder = GzEncoder::new(gzip_content, flate2::Compression::default());
    encoder.write_all(content)?;
    encoder.finish()?;
    Ok(())
}
