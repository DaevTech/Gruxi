use crate::{
    configuration::site::Site,
    error::gruxi_error::GruxiError, http::request_response::{gruxi_request::GruxiRequest, gruxi_response::GruxiResponse},
};

// Trait that processors must implement
#[allow(async_fn_in_trait)]
pub trait ProcessorTrait {
    // Sanitize
    fn sanitize(&mut self);

    // Validate and return a list of errors if any
    fn validate(&self) -> Result<(), Vec<String>>;

    // Returns the type of the processor as a string, e.g. "php", "static", "proxy"
    fn get_type(&self) -> String;

    // Reurns the default pretty name of the processor, such as "PHP Processor", "Static File Processor", etc
    fn get_default_pretty_name(&self) -> String;

    // Handle an incoming request (details would depend on the actual implementation)
    async fn handle_request(&self, gruxi_request: &mut GruxiRequest, site: &Site) -> Result<GruxiResponse, GruxiError>;
}
