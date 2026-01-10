use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use std::fmt::Debug;

use crate::http::request_response::body_error::BodyError;

pub enum GruxBody {
    Buffered(Bytes),
    Streaming(hyper::body::Incoming),
    StreamingBoxed(BoxBody<Bytes, BodyError>),
}

impl Debug for GruxBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GruxBody::Buffered(bytes) => write!(f, "GruxBody::Buffered(len={})", bytes.len()),
            GruxBody::Streaming(_) => write!(f, "GruxBody::Streaming(...)"),
            GruxBody::StreamingBoxed(_) => write!(f, "GruxBody::StreamingBoxed(...)"),
        }
    }
}