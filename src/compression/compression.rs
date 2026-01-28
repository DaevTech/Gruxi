use crate::http::request_response::gruxi_body::GruxiBody::Buffered;
use crate::http::request_response::gruxi_response::GruxiResponse;
use crate::logging::syslog::debug;
use flate2::write::GzEncoder;
use http::HeaderValue;
use hyper::body::Bytes;
use std::io::Write;

pub struct Compression {}

impl Compression {
    pub fn new() -> Self {
        Compression {}
    }

    pub async fn compress_response(&self, response: &mut GruxiResponse, accepted_encodings: Vec<String>, content_encoding_header: String) {
        // We need to make sure that it is not already compressed
        if content_encoding_header.to_lowercase() == "gzip" {
            return;
        }

        // Check if gzip is accepted by the client
        if !accepted_encodings.iter().any(|enc| enc.to_lowercase() == "gzip") {
            return;
        }

        // Perform gzip compression on the response body
        let body_bytes = response.get_body_bytes().await;
        let mut gzipped_bytes = Vec::new();
        match Self::compress_content(&body_bytes, &mut gzipped_bytes) {
            Ok(_) => {}
            Err(e) => {
                // If compression fails, we just return without modifying the response
                debug(format!("Gzip compression failed: {}", e));
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
}
