//! PP-OCRv5 client for the managed local quality sidecar.

use crate::{CaptureError, OcrResult, Result, TextRegion};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::RgbaImage;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::{Duration, Instant};

/// Cap the longest side of frames sent to OCR. Detection cost grows with pixel
/// count, and ultrawide/4K frames are far larger than PP-OCRv5 needs to read
/// on-screen text. Downscaling here is the single biggest OCR speedup; returned
/// boxes are mapped back to original frame coordinates.
const MAX_OCR_LONGEST_SIDE: u32 = 2000;

#[derive(Debug, Clone)]
pub struct SidecarOcrConfig {
    pub url: String,
    pub token: Option<String>,
    pub language: String,
}

#[derive(Debug, Deserialize)]
struct SidecarOcrResponse {
    provider: String,
    language: Option<String>,
    orientation_degrees: Option<f32>,
    lines: Vec<SidecarLine>,
}

#[derive(Debug, Deserialize)]
struct SidecarLine {
    text: String,
    confidence: f32,
    bbox: [u32; 4],
}

pub struct SidecarOcrEngine {
    config: SidecarOcrConfig,
    client: reqwest::Client,
}

impl SidecarOcrEngine {
    pub fn new(config: SidecarOcrConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|error| CaptureError::OcrError(error.to_string()))?;
        Ok(Self { config, client })
    }

    pub async fn health_check(&self) -> Result<()> {
        let response = self
            .authorized(
                self.client
                    .get(format!("{}/health", self.config.url.trim_end_matches('/'))),
            )
            .send()
            .await
            .map_err(|error| CaptureError::OcrError(error.to_string()))?;
        if !response.status().is_success() {
            return Err(CaptureError::OcrError(format!(
                "OCR sidecar health check returned {}",
                response.status()
            )));
        }
        Ok(())
    }

    pub async fn process_image(&self, image: &RgbaImage) -> Result<OcrResult> {
        let started = Instant::now();
        let (orig_width, orig_height) = image.dimensions();
        let longest = orig_width.max(orig_height);
        // Scale factor applied before OCR (<= 1.0). Returned boxes are divided
        // by this to recover original-frame coordinates.
        let scale = if longest > MAX_OCR_LONGEST_SIDE {
            MAX_OCR_LONGEST_SIDE as f32 / longest as f32
        } else {
            1.0
        };

        let dynamic = image::DynamicImage::ImageRgba8(image.clone());
        let rgb = if scale < 1.0 {
            let new_width = ((orig_width as f32 * scale).round() as u32).max(1);
            let new_height = ((orig_height as f32 * scale).round() as u32).max(1);
            dynamic
                .resize_exact(new_width, new_height, FilterType::Triangle)
                .to_rgb8()
        } else {
            dynamic.to_rgb8()
        };
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 85)
            .encode(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                image::ColorType::Rgb8,
            )
            .map_err(|error| CaptureError::ImageProcessingError(error.to_string()))?;

        let image_part = Part::bytes(jpeg)
            .file_name("capture.jpg")
            .mime_str("image/jpeg")
            .map_err(|error| CaptureError::OcrError(error.to_string()))?;
        let form = Form::new()
            .text("language", self.config.language.clone())
            .part("image", image_part);
        let response = self
            .authorized(
                self.client
                    .post(format!("{}/v1/ocr", self.config.url.trim_end_matches('/'))),
            )
            .multipart(form)
            .send()
            .await
            .map_err(|error| CaptureError::OcrError(error.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(CaptureError::OcrError(format!(
                "PP-OCRv5 request failed ({status}): {body}"
            )));
        }

        let payload: SidecarOcrResponse = response
            .json()
            .await
            .map_err(|error| CaptureError::OcrError(error.to_string()))?;
        // Map boxes from the (possibly downscaled) sent image back to the
        // original frame so downstream highlight overlays line up.
        let inverse_scale = 1.0 / scale;
        let restore = |value: u32| (value as f32 * inverse_scale).round() as u32;
        let regions = payload
            .lines
            .into_iter()
            .filter(|line| !line.text.trim().is_empty())
            .map(|line| {
                TextRegion::new(
                    line.text,
                    restore(line.bbox[0]),
                    restore(line.bbox[1]),
                    restore(line.bbox[2]),
                    restore(line.bbox[3]),
                    line.confidence.clamp(0.0, 1.0),
                )
            })
            .collect();
        let mut result = OcrResult::new(
            regions,
            image.dimensions(),
            started.elapsed().as_millis() as u64,
        );
        result.provider = payload.provider;
        result.language = payload.language;
        result.orientation_degrees = payload.orientation_degrees;
        Ok(result)
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.config.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }
}
