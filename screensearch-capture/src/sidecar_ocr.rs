//! PP-OCRv5 client for the managed local quality sidecar.

use crate::{CaptureError, OcrResult, Result, TextRegion};
use image::codecs::jpeg::JpegEncoder;
use image::RgbaImage;
use reqwest::multipart::{Form, Part};
use serde::Deserialize;
use std::time::{Duration, Instant};

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
        let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
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
        let regions = payload
            .lines
            .into_iter()
            .filter(|line| !line.text.trim().is_empty())
            .map(|line| {
                TextRegion::new(
                    line.text,
                    line.bbox[0],
                    line.bbox[1],
                    line.bbox[2],
                    line.bbox[3],
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
