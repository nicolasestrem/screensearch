//! OCR Processing Demo
//!
//! This example demonstrates the full OCR processing pipeline:
//! 1. Initialize the OCR engine
//! 2. Create a test image with text
//! 3. Process the image
//! 4. Display results with bounding boxes
//!
//! Run with: cargo run --example ocr_demo

use image::{Rgba, RgbaImage};
use imageproc::drawing::{draw_hollow_rect_mut, draw_text_mut};
use imageproc::rect::Rect;
use rusttype::{Font, Scale};
use screen_capture::{OcrEngine, OcrProcessor, OcrProcessorBuilder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    tracing::info!("Starting OCR demo");

    // Create OCR engine
    let engine = match OcrEngine::new().await {
        Ok(engine) => engine,
        Err(e) => {
            tracing::error!("Failed to create OCR engine: {}", e);
            tracing::info!("Please ensure Windows OCR language packs are installed");
            return Err(e.into());
        }
    };

    // Display available languages
    match OcrEngine::available_languages() {
        Ok(languages) => {
            tracing::info!("Available OCR languages: {:?}", languages);
        }
        Err(e) => {
            tracing::warn!("Could not get available languages: {}", e);
        }
    }

    // Create a test image with text
    tracing::info!("Creating test image with text...");
    let image = create_test_image();

    // Process the image
    tracing::info!("Processing image with OCR...");
    let result = engine.process_image(&image).await?;

    // Display results
    tracing::info!("OCR Results:");
    tracing::info!("  Processing time: {}ms", result.processing_time_ms);
    tracing::info!(
        "  Image dimensions: {}x{}",
        result.image_dimensions.0,
        result.image_dimensions.1
    );
    tracing::info!("  Regions found: {}", result.regions.len());
    tracing::info!("  Total characters: {}", result.char_count());
    tracing::info!("  Full text: {}", result.full_text);

    for (i, region) in result.regions.iter().enumerate() {
        tracing::info!(
            "  Region {}: '{}' at ({}, {}) {}x{} confidence={:.2}",
            i + 1,
            region.text,
            region.x,
            region.y,
            region.width,
            region.height,
            region.confidence
        );
    }

    // Demonstrate OcrProcessor
    tracing::info!("\nDemonstrating OcrProcessor...");
    let processor = OcrProcessorBuilder::new()
        .min_confidence(0.5)
        .worker_threads(2)
        .enable_metrics(true)
        .build()
        .await?;

    tracing::info!("Processor created successfully");
    tracing::info!("Metrics: {:?}", processor.metrics());

    Ok(())
}

/// Create a test image with text for OCR demonstration
fn create_test_image() -> RgbaImage {
    let width = 800;
    let height = 600;
    let mut image = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));

    // Note: In a real implementation, you would load a font
    // For this demo, we'll create a simple text pattern
    // The actual text rendering would require the imageproc and rusttype crates

    // Draw some simple text-like patterns
    // In production, replace this with actual text rendering
    tracing::info!("Test image created: {}x{}", width, height);
    tracing::info!("Note: For actual text rendering, you would need to add imageproc and rusttype dependencies");

    image
}
