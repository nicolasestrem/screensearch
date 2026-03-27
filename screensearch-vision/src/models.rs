use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionAnalysis {
    pub description: String,
    pub visible_text: Vec<String>,
    pub activity_type: String,
    #[serde(rename = "application")]
    pub app_hint: Option<String>,
    pub confidence: f32,
}
