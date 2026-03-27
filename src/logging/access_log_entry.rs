use chrono::DateTime;
use crate::http::request_response::gruxi_request::GruxiRequest;

const LOG_TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%z";

pub struct AccessLogEntry {
    pub gruxi_request: GruxiRequest,
    pub site_id: String,
    pub log_time: DateTime<chrono::Utc>,
    pub status_code: u16,
    pub response_size: u64,
}

impl AccessLogEntry {
    pub fn format_for_log(&self) -> String {
        let log_entry = format!(
            "{} - - [{}] \"{} {} {}\" {} {}",
            self.gruxi_request.get_remote_ip_pretty(),
            self.log_time.format(LOG_TIMESTAMP_FORMAT),
            self.gruxi_request.get_http_method(),
            self.gruxi_request.get_path_and_query(),
            self.gruxi_request.get_http_version(),
            self.status_code,
            self.response_size
        );
        log_entry
    }
}
