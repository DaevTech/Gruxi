use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(unused)]
pub struct BindingSiteRelationship {
    pub binding_id: String,
    pub site_id: String,
}
