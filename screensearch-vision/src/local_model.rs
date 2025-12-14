use crate::{VisionAnalysis, VisionModel};
use anyhow::Result;
use async_trait::async_trait;
use image::DynamicImage;

pub struct LocalClient {
    // Placeholder for model state (weights, config, etc.)
}

impl LocalClient {
    pub fn new() -> Result<Self> {
        // TODO: Load model weights (Moondream2 / TinyLlama)
        Ok(Self {})
    }
}

#[async_trait]
impl VisionModel for LocalClient {
    async fn analyze(&self, _image: &DynamicImage, _context: &str) -> Result<VisionAnalysis> {
        // TODO: Implement actual inference using candle-transformers
        // For now, return a placeholder or error to indicate it's not ready
        Err(anyhow::anyhow!("Local model inference not yet implemented. Please use Ollama/External provider."))
    }
}
