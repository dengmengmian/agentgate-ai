use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetSettings {
    pub pet_type: String,
    pub visible: bool,
    pub pos_x: f64,
    pub pos_y: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdatePetSettingsInput {
    pub pet_type: Option<String>,
    pub visible: Option<bool>,
    pub pos_x: Option<f64>,
    pub pos_y: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pet_settings_roundtrip() {
        let s = PetSettings {
            pet_type: "cat".into(),
            visible: true,
            pos_x: 100.5,
            pos_y: 200.0,
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("cat"));
        let de: PetSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(de.pos_x, 100.5);
        assert!(de.visible);
    }

    #[test]
    fn update_pet_settings_input_deserialization() {
        let json = r#"{"pet_type":"dog","visible":false}"#;
        let input: UpdatePetSettingsInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.pet_type, Some("dog".into()));
        assert_eq!(input.visible, Some(false));
        assert!(input.pos_x.is_none());
    }
}
