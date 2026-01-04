use crate::error::grux_error_enums::*;

#[derive(Debug)]
pub struct GruxError {
    pub kind: GruxErrorKind,
    pub message: String,
}

impl GruxError {
    pub fn new(kind: GruxErrorKind, message: String) -> Self {
        Self { kind, message }
    }

    pub fn new_with_kind_only(kind: GruxErrorKind) -> Self {
        Self { kind, message: String::new() }
    }

    pub fn get_http_status_code(&self) -> u16 {
        match self.kind {
            GruxErrorKind::HttpRequestValidation(status_code) => status_code,
            _ => 500, // Default to Internal Server Error for other error kinds
        }
    }
}
