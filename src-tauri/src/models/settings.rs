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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_config_view_roundtrip() {
        let v = ToolConfigView {
            id: "codex".into(),
            name: "Codex".into(),
            slug: "codex".into(),
            icon: "terminal".into(),
            config_path: "/path".into(),
            description: "desc".into(),
            config_exists: true,
        };
        let json = serde_json::to_string(&v).unwrap();
        assert!(json.contains("Codex"));
        let de: ToolConfigView = serde_json::from_str(&json).unwrap();
        assert!(de.config_exists);
    }
}
