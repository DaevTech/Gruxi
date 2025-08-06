use http_body_util::combinators::BoxBody;
use hyper::{Request, Response};
use hyper::body::Bytes;
use crate::grux_configuration_struct::Sites;
use crate::grux_http_util::{full};


pub fn handle_login_request(_req: &Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Here we can handle the login requests
    let mut resp = Response::new(full("Login page not implemented yet"));
    *resp.status_mut() = hyper::StatusCode::OK;
    Ok(resp)
}

pub fn handle_logout_request(_req: &Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Here we can handle the logout requests
    let mut resp = Response::new(full("Logout page not implemented yet"));
    *resp.status_mut() = hyper::StatusCode::OK;
    Ok(resp)
}

pub fn admin_get_configuration_endpoint(_req: &Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Here we can handle the get configuration requests
    let mut resp = Response::new(full("Get configuration endpoint not implemented yet"));
    *resp.status_mut() = hyper::StatusCode::OK;
    Ok(resp)
}

pub fn admin_post_configuration_endpoint(_req: &Request<hyper::body::Incoming>, _admin_site: &Sites) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // Here we can handle the post configuration requests
    let mut resp = Response::new(full("Post configuration endpoint not implemented yet"));
    *resp.status_mut() = hyper::StatusCode::OK;
    Ok(resp)
}
