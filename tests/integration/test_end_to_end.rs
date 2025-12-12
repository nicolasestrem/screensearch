//! End-to-end integration test
//!
//! Tests the complete pipeline: capture → OCR → database → API

use anyhow::Result;
use screen_api::ApiConfig;
use screen_capture::{CaptureConfig, CaptureEngine, OcrProcessor, OcrProcessorConfig};
use screen_db::{DatabaseConfig, DatabaseManager, FrameFilter, Pagination};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn test_full_pipeline() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    // Create temporary database
    let db_path = format!("test_e2e_{}.db", std::process::id());
    let db_config = DatabaseConfig::new(&db_path);
    let db = Arc::new(DatabaseManager::with_config(db_config).await?);

    // Initialize capture engine (will generate test frames)
    let capture_config = CaptureConfig {
        interval_ms: 1000,
        max_frames_buffer: 5,
        ..Default::default()
    };
    let mut capture_engine = CaptureEngine::new(capture_config)?;
    capture_engine.start()?;

    // Initialize OCR processor
    let ocr_config = OcrProcessorConfig {
        min_confidence: 0.5,
        worker_threads: 1,
        store_empty_frames: true,
        ..Default::default()
    };
    let ocr_processor = Arc::new(OcrProcessor::new(ocr_config).await?);

    // Create processing pipeline
    let (frame_tx, frame_rx) = mpsc::unbounded_channel();
    let (processed_tx, mut processed_rx) = mpsc::unbounded_channel();

    // Start OCR processing
    let _ocr_handle = ocr_processor.start_processing(frame_rx, processed_tx);

    // Simulate frame capture and processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Poll for a few frames
    for _ in 0..3 {
        if let Some(frame) = capture_engine.try_get_frame() {
            let _ = frame_tx.send(frame);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Drop sender to close channel
    drop(frame_tx);

    // Wait for processed frames
    let mut frame_count = 0;
    while let Some(processed) = processed_rx.recv().await {
        // Store in database
        let new_frame = screen_db::NewFrame {
            timestamp: processed.frame.timestamp,
            device_name: format!("monitor-{}", processed.frame.monitor_index),
            file_path: format!("test_frame_{}.png", frame_count),
            monitor_index: processed.frame.monitor_index as i32,
            width: processed.frame.image.width() as i32,
            height: processed.frame.image.height() as i32,
            offset_index: 0,
            chunk_id: None,
            active_window: processed.frame.active_window,
            active_process: processed.frame.active_process,
            browser_url: None,
            focused: Some(true),
        };

        let frame_id = db.insert_frame(new_frame).await?;
        assert!(frame_id > 0);

        frame_count += 1;
    }

    // Query database to verify storage
    let frames = db.get_frames(FrameFilter::default(), Pagination::default()).await?;
    assert!(frames.len() > 0, "Expected frames to be stored in database");

    // Cleanup
    capture_engine.stop()?;
    std::fs::remove_file(&db_path).ok();

    Ok(())
}

#[tokio::test]
async fn test_database_query_pipeline() -> Result<()> {
    // Initialize tracing for test
    let _ = tracing_subscriber::fmt::try_init();

    // Create temporary database
    let db_path = format!("test_query_{}.db", std::process::id());
    let db_config = DatabaseConfig::new(&db_path);
    let db = DatabaseManager::with_config(db_config).await?;

    // Insert test frame
    let new_frame = screen_db::NewFrame {
        timestamp: chrono::Utc::now(),
        device_name: "test-monitor".to_string(),
        file_path: "test.png".to_string(),
        monitor_index: 0,
        width: 1920,
        height: 1080,
        offset_index: 0,
        chunk_id: None,
        active_window: Some("Test Window".to_string()),
        active_process: Some("test.exe".to_string()),
        browser_url: None,
        focused: Some(true),
    };

    let frame_id = db.insert_frame(new_frame).await?;
    assert!(frame_id > 0);

    // Insert OCR text
    let ocr_text = screen_db::NewOcrText {
        frame_id,
        text: "Hello World Test".to_string(),
        text_json: serde_json::json!({"confidence": 0.95}).to_string(),
        ocr_engine: "test".to_string(),
        focused: true,
    };

    let ocr_id = db.insert_ocr_text(ocr_text).await?;
    assert!(ocr_id > 0);

    // Query frames
    let frames = db.get_frames(FrameFilter::default(), Pagination::default()).await?;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].device_name, "test-monitor");

    // Search OCR text
    let results = db.search_ocr_text(
        "Hello",
        FrameFilter::default(),
        Pagination::default()
    ).await?;
    assert!(results.len() > 0);

    // Cleanup
    std::fs::remove_file(&db_path).ok();

    Ok(())
}
