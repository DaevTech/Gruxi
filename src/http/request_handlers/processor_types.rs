use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ProcessorType {
    StaticFileProcessor,
    ProxyProcessor,
    PHPProcessor,
}