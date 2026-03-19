use http::HeaderValue;

use crate::{
    config::{binding::Binding, site::Site},
    debug,
    http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
};

pub async fn validate_request(gruxi_request: &mut GruxiRequest, binding: &Binding, site: &Site) -> Result<(), GruxiResponse> {
    // Here we can add any request validation logic if needed
    let cached_configuration = crate::config::cached_configuration::get_cached_configuration();
    let configuration = cached_configuration.get_configuration().await;

    // Validation for HTTP/1.1 only
    if gruxi_request.get_http_version() == "HTTP/1.1" {
        // [HTTP1.1] Requires a Host header
        if !gruxi_request.get_headers().contains_key("Host") {
            debug!("Missing Host header for HTTP/1.1 request: {:?}", gruxi_request);
            return Err(GruxiResponse::new_empty_with_status(hyper::StatusCode::BAD_REQUEST.as_u16()));
        }

        // [HTTP1.1] If there is multiple host headers, we return a 400 error
        if gruxi_request.get_headers().get_all("Host").iter().count() > 1 {
            debug!("Multiple Host headers for HTTP/1.1 request: {:?}", gruxi_request);
            return Err(GruxiResponse::new_empty_with_status(hyper::StatusCode::BAD_REQUEST.as_u16()));
        }
    }

    // [HTTP1.1 and later] Basic validation: check for valid method
    let http_method = gruxi_request.get_http_method();
    if http_method != "GET"
        && http_method != "POST"
        && http_method != "HEAD"
        && http_method != "PUT"
        && http_method != "DELETE"
        && http_method != "OPTIONS"
        && http_method != "TRACE"
        && http_method != "CONNECT"
        && http_method != "PATCH"
    {
        // Return a error for unsupported method
        debug!("Unsupported HTTP method for request: {:?}", gruxi_request);
        return Err(GruxiResponse::new_empty_with_status(hyper::StatusCode::NOT_IMPLEMENTED.as_u16()));
    }

    // Protect our server from overly large bodies
    let max_body_size = configuration.core.server_settings.max_body_size;
    if max_body_size > 0 && (http_method == "POST" || http_method == "PUT") {
        // Check Content-Length header if present
        if let Some(content_length_header) = gruxi_request.get_headers().get("Content-Length")
            && let Ok(content_length_str) = content_length_header.to_str()
                && let Ok(content_length) = content_length_str.parse::<u64>()
                    && content_length > max_body_size {
                        debug!("Payload too large for request based on Content-Length header: {:?}", gruxi_request);
                        return Err(GruxiResponse::new_empty_with_status(hyper::StatusCode::PAYLOAD_TOO_LARGE.as_u16()));
                    }

        // Also check the expected body size
        if gruxi_request.get_body_size() > max_body_size {
            debug!("Payload too large for request based on actual body size: {:?}", gruxi_request);
            return Err(GruxiResponse::new_empty_with_status(hyper::StatusCode::PAYLOAD_TOO_LARGE.as_u16()));
        }
    }

    // Check if we need to enforce TLS for this site
    if site.force_tls && !binding.is_tls {
        let mut resp = GruxiResponse::new_empty_with_status(hyper::StatusCode::PERMANENT_REDIRECT.as_u16());
        let host = gruxi_request.get_hostname();
        let path_and_query = gruxi_request.get_path_and_query();
        let location = format!("https://{}:{}{}", host, site.force_tls_port, path_and_query);
        if let Ok(location_header_value) = HeaderValue::from_str(&location) {
            resp.headers_mut().insert("Location", location_header_value);
        }
        return Err(resp);
    }

    // Check if we need to enforce a canonical hostname for this site
    if !site.canonical_host.is_empty() && gruxi_request.get_hostname() != site.canonical_host {
        let mut resp = GruxiResponse::new_empty_with_status(hyper::StatusCode::PERMANENT_REDIRECT.as_u16());
        let path_and_query = gruxi_request.get_path_and_query();
        let scheme = gruxi_request.get_scheme();
        let port = gruxi_request.get_server_port();
        let location = format!("{}://{}:{}{}", scheme, site.canonical_host, port, path_and_query);
        if let Ok(location_header_value) = HeaderValue::from_str(&location) {
            resp.headers_mut().insert("Location", location_header_value);
        }
        return Err(resp);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use http::Request;
    use hyper::body::Bytes;

    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_force_tls_redirect() {
        let mut site = Site::new();
        site.force_tls = true;
        site.force_tls_port = 8443;

        let raw_request = Request::builder()
            .method("GET")
            .uri("http://example.com/test/?yes=no")
            .header("Host", "example.com")
            .body(Bytes::new())
            .unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let mut binding = Binding::new();
        binding.is_tls = false;
        binding.port = 80;

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_err());
        let response = result.err().unwrap();
        assert_eq!(response.get_status(), hyper::StatusCode::PERMANENT_REDIRECT.as_u16());
        assert_eq!(response.headers().get("Location").unwrap().to_str().unwrap(), "https://example.com:8443/test/?yes=no");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_force_tls_no_redirect_std_port() {
        let mut site = Site::new();
        site.force_tls = true;
        site.force_tls_port = 443;

        let raw_request = Request::builder()
            .method("GET")
            .uri("https://example.com/test")
            .header("Host", "example.com")
            .body(Bytes::new())
            .unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let mut binding = Binding::new();
        binding.is_tls = true;
        binding.port = 443;

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_force_tls_no_redirect_alt_port() {
        let mut site = Site::new();
        site.force_tls = true;
        site.force_tls_port = 8443;

        let raw_request = Request::builder()
            .method("GET")
            .uri("https://example.com:8443/test")
            .header("Host", "example.com")
            .body(Bytes::new())
            .unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let mut binding = Binding::new();
        binding.is_tls = true;
        binding.port = 8443;

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_canonical_host_redirect_no_redirect() {
        let mut site = Site::new();
        site.canonical_host = "example.com".to_string();

        let raw_request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .header("Host", "example.com")
            .body(Bytes::new())
            .unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let binding = Binding::new();

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_ok());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_canonical_host_redirect_success() {
        let mut site = Site::new();
        site.canonical_host = "www.example.com".to_string();

        let raw_request = Request::builder()
            .method("GET")
            .uri("http://example.com/test")
            .header("Host", "example.com")
            .body(Bytes::new())
            .unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let binding = Binding::new();

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_err());
        let response = result.err().unwrap();
        assert_eq!(response.get_status(), hyper::StatusCode::PERMANENT_REDIRECT.as_u16());
        assert_eq!(response.headers().get("Location").unwrap().to_str().unwrap(), "http://www.example.com:80/test");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_canonical_host_redirect_success_tls_standard_port() {
        let mut site = Site::new();
        site.canonical_host = "www.example.com".to_string();

        let raw_request = Request::builder().method("GET").uri("https://example.com/").header("Host", "example.com").body(Bytes::new()).unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let mut binding = Binding::new();
        binding.is_tls = true;
        binding.port = 443;

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_err());
        let response = result.err().unwrap();
        assert_eq!(response.get_status(), hyper::StatusCode::PERMANENT_REDIRECT.as_u16());
        assert_eq!(response.headers().get("Location").unwrap().to_str().unwrap(), "https://www.example.com:443/");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_canonical_host_redirect_success_tls_alt_port() {
        let mut site = Site::new();
        site.canonical_host = "www.example.com".to_string();

        let raw_request = Request::builder()
            .method("GET")
            .uri("https://example.com:8443/test/tester/?yes=no")
            .header("Host", "example.com")
            .body(Bytes::new())
            .unwrap();
        let mut request = GruxiRequest::new(raw_request);
        let mut binding = Binding::new();
        binding.is_tls = true;
        binding.port = 8443;

        let result = validate_request(&mut request, &binding, &site).await;
        assert!(result.is_err());
        let response = result.err().unwrap();
        assert_eq!(response.get_status(), hyper::StatusCode::PERMANENT_REDIRECT.as_u16());
        assert_eq!(response.headers().get("Location").unwrap().to_str().unwrap(), "https://www.example.com:8443/test/tester/?yes=no");
    }
}
