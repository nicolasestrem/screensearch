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
            // Always prefer the sidecar without gating on an initial health
            // check. The sidecar is launched non-blocking and is usually still
            // cold-starting at this point; gating here would permanently demote
            // OCR to the Windows engine. Instead `process_image` tries the
            // sidecar first and falls back per-request, so OCR upgrades to
            // PP-OCRv5 automatically once the sidecar reports healthy.
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
