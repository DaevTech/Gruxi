use http_body_util::combinators::BoxBody;
use hyper::body::Bytes;
use std::fmt::Debug;

use crate::http::request_response::body_error::BodyError;

pub enum GruxiBody {
    Buffered(Bytes),
    Streaming(hyper::body::Incoming),
    StreamingBoxed(BoxBody<Bytes, BodyError>),
}

impl Debug for GruxiBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GruxiBody::Buffered(bytes) => write!(f, "GruxiBody::Buffered(len={})", bytes.len()),
            GruxiBody::Streaming(_) => write!(f, "GruxiBody::Streaming(...)"),
            GruxiBody::StreamingBoxed(_) => write!(f, "GruxiBody::StreamingBoxed(...)"),
        }
    }
}