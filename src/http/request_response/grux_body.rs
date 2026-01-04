use hyper::body::Bytes;
use std::fmt::Debug;

pub enum GruxBody {
    Buffered(Bytes),
    Streaming(hyper::body::Incoming),
}

impl Debug for GruxBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GruxBody::Buffered(bytes) => write!(f, "GruxBody::Buffered(len={})", bytes.len()),
            GruxBody::Streaming(_) => write!(f, "GruxBody::Streaming(...)"),
        }
    }
}