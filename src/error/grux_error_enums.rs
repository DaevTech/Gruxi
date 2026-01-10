#[derive(Debug)]
pub enum GruxErrorKind {
    ProxyProcessor(ProxyProcessorError),
    StaticFileProcessor(StaticFileProcessorError),
    PHPProcessor(PHPProcessorError),
    HttpRequestValidation(u16), // HTTP status code for request validation errors
    FastCgi(FastCgiError),
    Internal(&'static str),
    AdminApi(AdminApiError)
}

#[derive(Debug)]
pub enum ProxyProcessorError {
    ConnectionFailed,
    InvalidRequest,
    InvalidResponse,
    UpstreamUnavailable,
    UpstreamTimeout,
    Internal,
}

#[derive(Debug)]
pub enum StaticFileProcessorError {
    PathError(std::io::Error),
    FileNotFound,
    FileBlockedDueToSecurity(String),
    Internal,
}

#[derive(Debug)]
pub enum PHPProcessorError {
    Connection,
    PathError(std::io::Error),
    FileNotFound,
    Timeout,
    Internal,
}

#[derive(Debug)]
pub enum FastCgiError {
    Initialization,
    Connection(std::io::Error),
    Communication(std::io::Error),
    ConnectionPermitAcquisition,
    Timeout,
    InvalidResponse,
    Internal, // Internal processing errors, that should not happen
}

#[derive(Debug)]
pub enum AdminApiError {
    NoRouteMatched,
    InvalidRequest,
}
