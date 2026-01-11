use crate::error::gruxi_error_enums::*;

#[derive(Debug)]
pub struct GruxiError {
    pub kind: GruxiErrorKind,
    pub message: String,
}

impl GruxiError {
    pub fn new(kind: GruxiErrorKind, message: String) -> Self {
        Self { kind, message }
    }

    pub fn new_with_kind_only(kind: GruxiErrorKind) -> Self {
        Self { kind, message: String::new() }
    }

    pub fn get_http_status_code(&self) -> u16 {
        match self.kind {
            GruxiErrorKind::HttpRequestValidation(status_code) => status_code,
            _ => 500, // Default to Internal Server Error for other error kinds
        }
    }
}
