use crate::file::normalized_path::NormalizedPath;

#[derive(Debug, Clone)]
pub enum GruxiRequestProcessorData {
    PhpProcessorFastCgi(PhpProcessorFastCgiData)
}

#[derive(Debug, Clone)]
pub struct PhpProcessorFastCgiData {
    pub connect_ip_and_port: String,
    pub script_file: NormalizedPath,
    pub uri_is_a_dir_with_index_file_inside: bool,
    pub local_web_root: NormalizedPath,
    pub fastcgi_web_root: NormalizedPath,
    pub server_software_spoof: String,
}
