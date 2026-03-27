pub mod models;
pub mod client;
pub mod local_model;

use anyhow::Result;
use async_trait::async_trait;
use image::DynamicImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionAnalysis {
    pub description: String,
    pub visible_text: Vec<String>,
    pub activity_type: String,
    pub app_hint: Option<String>,
    pub confidence: f32,
}

#[async_trait]
pub trait VisionModel: Send + Sync {
    async fn analyze(&self, image: &DynamicImage, context: &str) -> Result<VisionAnalysis>;
}
