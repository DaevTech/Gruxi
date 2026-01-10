use crate::http::request_response::grux_body::GruxBody;
use crate::http::request_response::body_error::{BodyError, box_err};
use http::response::Parts;
use http_body_util::BodyExt;
use http_body_util::Full;
use http_body_util::combinators::BoxBody;
use hyper::Response;
use hyper::body::{Body, Bytes};
use std::collections::HashMap;

// Wrapper around hyper responses
#[derive(Debug)]
pub struct GruxResponse {
    // Parts of the original request
    parts: Parts,
    body: GruxBody,
    // Calculated data cache, such as remote_ip, hostname etc
    pub calculated_data: HashMap<String, String>,
}

impl GruxResponse {
    // Created new empty response with given status code
    pub fn new_empty_with_status(status_code: u16) -> Self {
        let response = Response::builder().status(status_code).body(Bytes::new()).unwrap();

        // Convert to Response<Incoming> compatible format
        let body_size_hint = 0;
        let (parts, _body) = response.into_parts();
        let body = GruxBody::Buffered(Bytes::new());

        let mut calculated_data = HashMap::new();
        calculated_data.insert("body_size_hint".to_string(), body_size_hint.to_string());

        Self { parts, body, calculated_data }
    }

    pub fn new_with_bytes<T: Into<Bytes>>(status_code: u16, body_bytes: T) -> Self {
        let mut response = GruxResponse::new_empty_with_status(status_code);
        response.body = GruxBody::Buffered(body_bytes.into());
        response
    }

    pub fn new_with_body(status_code: u16, body: BoxBody<hyper::body::Bytes, BodyError>) -> Self {
        let mut response = GruxResponse::new_empty_with_status(status_code);
        response.body = GruxBody::StreamingBoxed(body);
        response
    }

    // Created new streaming response from hyper Response<Incoming>
    pub fn from_hyper(hyper_response: Response<hyper::body::Incoming>) -> Self {
        let body_size_hint = hyper_response.body().size_hint().upper().unwrap_or(0);

        let (parts, body) = hyper_response.into_parts();
        let body = GruxBody::Streaming(body);

        // Calculated data cache, such as remote_ip, hostname etc
        let mut calculated_data = HashMap::new();
        calculated_data.insert("body_size_hint".to_string(), body_size_hint.to_string());

        Self { parts, body, calculated_data }
    }

    // Created new streaming response from hyper Response<Incoming>
    pub async fn from_hyper_bytes(hyper_response: Response<BoxBody<hyper::body::Bytes, hyper::Error>>) -> Self {
        let body_size_hint = hyper_response.body().size_hint().upper().unwrap_or(0);

        let (parts, body) = hyper_response.into_parts();

        let collected_result = body.collect().await;
        let bytes = match collected_result {
            Ok(c) => c.to_bytes(),
            Err(_) => Bytes::new(),
        };
        let body = GruxBody::Buffered(bytes);

        // Calculated data cache, such as remote_ip, hostname etc
        let mut calculated_data = HashMap::new();
        calculated_data.insert("body_size_hint".to_string(), body_size_hint.to_string());

        Self { parts, body, calculated_data }
    }

    pub fn headers_mut(&mut self) -> &mut http::HeaderMap {
        &mut self.parts.headers
    }

    pub fn headers(&self) -> &http::HeaderMap {
        &self.parts.headers
    }

    pub fn get_header(&self, header_name: &str) -> Option<&http::header::HeaderValue> {
        self.parts.headers.get(header_name)
    }

    pub fn get_body_size(&mut self) -> u64 {
        if let Some(body_size_hint) = self.calculated_data.get("body_size_hint") {
            return body_size_hint.parse().unwrap_or(0);
        }
        0
    }

    pub fn get_status(&self) -> u16 {
        self.parts.status.as_u16()
    }

    // Returns the full body bytes. Beware this consumes the internal body bytes
    pub async fn get_body_bytes(&mut self) -> Bytes {
        match &mut self.body {
            GruxBody::Buffered(bytes) => bytes.clone(),
            GruxBody::Streaming(incoming_body) => {
                let body = incoming_body.collect().await;
                match body {
                    Ok(bytes) => bytes.to_bytes(),
                    Err(_) => Bytes::new(),
                }
            }
            GruxBody::StreamingBoxed(boxed_body) => {
                let body = boxed_body.collect().await;
                match body {
                    Ok(bytes) => bytes.to_bytes(),
                    Err(_) => Bytes::new(),
                }
            }
        }
    }

    // Convert GruxResponse back into a hyper Response
    pub fn into_hyper(self) -> Response<BoxBody<Bytes, BodyError>> {
        let body: BoxBody<Bytes, BodyError> = match self.body {
            GruxBody::Buffered(bytes) => BoxBody::new(
                Full::new(bytes).map_err(|never| -> BodyError { match never {} }),
            ),
            GruxBody::Streaming(incoming) => BoxBody::new(incoming.map_err(box_err)),
            GruxBody::StreamingBoxed(boxed_body) => boxed_body,
        };

        let response = Response::from_parts(self.parts, body);
        response
    }

    pub fn set_body(&mut self, body: GruxBody) {
        self.body = body;
        let length = match &self.body {
            GruxBody::Buffered(bytes) => bytes.len() as u64,
            GruxBody::Streaming(_) => 0,
            GruxBody::StreamingBoxed(_) => 0,
        };
        self.calculated_data.insert("body_size_hint".to_string(), length.to_string());
    }
}
