//! Screen Capture Module
//!
//! This crate provides screen capture and OCR functionality for the ScreenSearch project.
//! It uses Windows-specific APIs to capture screen content and perform OCR processing.
//!
//! # Features
//!
//! - Multi-monitor screen capture
//! - Frame differencing to skip unchanged content
//! - Windows OCR API integration
//! - Efficient image processing pipeline
//!
//! # Example
//!
//! ```no_run
//! use screen_capture::{CaptureConfig, ScreenCapture};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = CaptureConfig::default();
//!     let mut capture = ScreenCapture::new(config)?;
//!
//!     // Start capture loop
//!     capture.start().await?;
//!
//!     Ok(())
//! }
//! ```

use thiserror::Error;

pub mod capture;
pub mod frame_diff;
pub mod monitor;
pub mod ocr;
pub mod ocr_processor;
pub mod window_context;

pub use capture::{CaptureConfig, CaptureEngine, ScreenCapture};
pub use frame_diff::FrameDiffer;
pub use monitor::MonitorInfo;
pub use ocr::{OcrEngine, OcrResult, TextRegion};
pub use ocr_processor::{
    OcrMetrics, OcrProcessor, OcrProcessorBuilder, OcrProcessorConfig, ProcessedFrame,
};
pub use window_context::WindowContext;

/// Errors that can occur during screen capture operations
#[derive(Error, Debug)]
pub enum CaptureError {
    #[error("Failed to initialize capture: {0}")]
    InitializationError(String),

    #[error("Failed to capture screen: {0}")]
    ScreenCaptureError(String),

    #[error("OCR processing failed: {0}")]
    OcrError(String),

    #[error("Invalid monitor index: {0}")]
    InvalidMonitor(usize),

    #[error("Image processing error: {0}")]
    ImageProcessingError(String),

    #[error("Windows API error: {0}")]
    WindowsApiError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type alias for capture operations
pub type Result<T> = std::result::Result<T, CaptureError>;

/// Represents a captured frame with metadata
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// Timestamp when the frame was captured
    pub timestamp: chrono::DateTime<chrono::Utc>,

    /// Monitor index (0-based)
    pub monitor_index: usize,

    /// Raw image data
    pub image: image::RgbaImage,

    /// Active window title at capture time
    pub active_window: Option<String>,

    /// Active process name
    pub active_process: Option<String>,
}

/// OCR result with text and bounding boxes
#[derive(Debug, Clone)]
pub struct OcrTextResult {
    /// Extracted text
    pub text: String,

    /// Bounding box coordinates (x, y, width, height)
    pub bounds: (u32, u32, u32, u32),

    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_captured_frame_creation() {
        let frame = CapturedFrame {
            timestamp: chrono::Utc::now(),
            monitor_index: 0,
            image: image::RgbaImage::new(1920, 1080),
            active_window: Some("Test Window".to_string()),
            active_process: Some("test.exe".to_string()),
        };

        assert_eq!(frame.monitor_index, 0);
        assert_eq!(frame.image.width(), 1920);
        assert_eq!(frame.image.height(), 1080);
    }

    #[test]
    fn test_ocr_text_result() {
        let result = OcrTextResult {
            text: "Hello, World!".to_string(),
            bounds: (100, 200, 300, 50),
            confidence: 0.95,
        };

        assert_eq!(result.text, "Hello, World!");
        assert_eq!(result.bounds, (100, 200, 300, 50));
        assert!(result.confidence > 0.9);
    }
}
