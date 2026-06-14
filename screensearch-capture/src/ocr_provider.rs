//! OCR provider selection and explicit fallback behavior.

use crate::sidecar_ocr::{SidecarOcrConfig, SidecarOcrEngine};
use crate::{OcrEngine, OcrResult, Result};
use image::RgbaImage;

pub struct OcrProviderEngine {
    preferred: PreferredProvider,
    windows_fallback: Option<OcrEngine>,
}

enum PreferredProvider {
    Windows(OcrEngine),
    PpOcr(SidecarOcrEngine),
}

impl OcrProviderEngine {
    pub async fn new(
        provider: &str,
        sidecar_url: String,
        sidecar_token: Option<String>,
        language: String,
        fallback_to_windows: bool,
    ) -> Result<Self> {
        if matches!(provider, "ppocr" | "ppocr-v5" | "sidecar") {
            let sidecar = SidecarOcrEngine::new(SidecarOcrConfig {
                url: sidecar_url,
                token: sidecar_token,
                language,
            })?;
            match sidecar.health_check().await {
                Ok(()) => {
                    let windows_fallback = if fallback_to_windows {
                        Some(OcrEngine::new().await?)
                    } else {
                        None
                    };
                    return Ok(Self {
                        preferred: PreferredProvider::PpOcr(sidecar),
                        windows_fallback,
                    });
                }
                Err(error) if fallback_to_windows => {
                    tracing::warn!(
                        "PP-OCRv5 sidecar unavailable; using Windows OCR fallback: {}",
                        error
                    );
                }
                Err(error) => return Err(error),
            }
        }

        Ok(Self {
            preferred: PreferredProvider::Windows(OcrEngine::new().await?),
            windows_fallback: None,
        })
    }

    pub async fn process_image(&self, image: &RgbaImage) -> Result<OcrResult> {
        match &self.preferred {
            PreferredProvider::Windows(engine) => engine.process_image(image).await,
            PreferredProvider::PpOcr(engine) => match engine.process_image(image).await {
                Ok(result) => Ok(result),
                Err(error) => match &self.windows_fallback {
                    Some(fallback) => {
                        tracing::warn!(
                            "PP-OCRv5 request failed; retrying with Windows OCR: {}",
                            error
                        );
                        fallback.process_image(image).await
                    }
                    None => Err(error),
                },
            },
        }
    }
}
