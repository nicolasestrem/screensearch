//! OCR provider — native Windows OCR (WinRT).
//!
//! ScreenSearch performs OCR fully in-process with the Windows `Media.Ocr` API,
//! which is fast (~70-80ms/frame) and requires no model download or sidecar.
//! This thin wrapper exists so the processing pipeline can stay agnostic to the
//! concrete engine.

use crate::{OcrEngine, OcrResult, Result};
use image::RgbaImage;

pub struct OcrProviderEngine {
    engine: OcrEngine,
}

impl OcrProviderEngine {
    /// Create the native Windows OCR engine.
    ///
    /// `language` is an optional BCP-47 tag (e.g. `"en-US"`). When empty, the
    /// user's profile languages are used.
    pub async fn new(language: String) -> Result<Self> {
        let engine = if language.trim().is_empty() {
            OcrEngine::new().await?
        } else {
            OcrEngine::new_with_language(&language).await?
        };
        Ok(Self { engine })
    }

    pub async fn process_image(&self, image: &RgbaImage) -> Result<OcrResult> {
        self.engine.process_image(image).await
    }
}
