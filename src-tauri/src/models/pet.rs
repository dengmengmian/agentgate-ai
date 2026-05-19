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
