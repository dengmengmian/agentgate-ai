use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfigView {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub icon: String,
    pub config_path: String,
    pub description: String,
    pub config_exists: bool,
}
