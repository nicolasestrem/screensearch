//! OCR Processor Orchestrator
//!
//! This module provides the high-level OCR processing orchestrator that:
//! - Consumes frames from the capture queue
//! - Runs OCR on each frame asynchronously
//! - Filters low-confidence results
//! - Stores results in the database with frame metadata
//! - Provides error handling and retry logic
//! - Tracks performance metrics
//!
//! # Architecture
//!
//! The `OcrProcessor` acts as a bridge between the screen capture pipeline
//! and the database storage layer. It operates in a continuous loop, processing
//! frames as they become available.
//!
//! # Example
//!
//! ```no_run
//! use screen_capture::{OcrProcessor, OcrProcessorConfig, CaptureEngine, CaptureConfig};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     // Initialize database (pseudo-code)
//!     // let db = screen_db::DatabaseManager::new("screen_memories.db").await?;
//!
//!     let config = OcrProcessorConfig::default();
//!     let processor = OcrProcessor::new(config).await?;
//!
//!     // Start processing frames
//!     // processor.start(capture_engine, db).await?;
//!
//!     Ok(())
//! }
//! ```

use crate::{CaptureError, CapturedFrame, OcrEngine, OcrResult, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::interval;

/// Configuration for OCR processor
#[derive(Debug, Clone)]
pub struct OcrProcessorConfig {
    /// Minimum confidence threshold for storing OCR results (0.0 - 1.0)
    /// Results below this threshold are discarded
    pub min_confidence: f32,

    /// Number of concurrent OCR processing tasks
    pub worker_threads: usize,

    /// Maximum retry attempts for failed OCR operations
    pub max_retries: u32,

    /// Backoff duration between retries (milliseconds)
    pub retry_backoff_ms: u64,

    /// Whether to store frames with no text detected
    pub store_empty_frames: bool,

    /// Channel buffer size for frame queue
    pub channel_buffer_size: usize,

    /// Enable performance metrics logging
    pub enable_metrics: bool,

    /// Metrics reporting interval (seconds)
    pub metrics_interval_secs: u64,
}

impl Default for OcrProcessorConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.7,
            worker_threads: 2,
            max_retries: 3,
            retry_backoff_ms: 1000,
            store_empty_frames: false,
            channel_buffer_size: 100,
            enable_metrics: true,
            metrics_interval_secs: 60,
        }
    }
}

/// Performance metrics for OCR processing
#[derive(Debug, Clone, Default)]
pub struct OcrMetrics {
    /// Total frames processed
    pub frames_processed: Arc<AtomicU64>,

    /// Total OCR errors encountered
    pub errors: Arc<AtomicU64>,

    /// Total text regions extracted
    pub regions_extracted: Arc<AtomicU64>,

    /// Total processing time in milliseconds
    pub total_processing_time_ms: Arc<AtomicU64>,

    /// Frames with no text detected
    pub empty_frames: Arc<AtomicU64>,

    /// Frames filtered by confidence threshold
    pub filtered_frames: Arc<AtomicU64>,
}

impl OcrMetrics {
    /// Create new metrics tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a processed frame
    pub fn record_frame(&self, result: &OcrResult, had_error: bool) {
        self.frames_processed.fetch_add(1, Ordering::Relaxed);
        self.total_processing_time_ms
            .fetch_add(result.processing_time_ms, Ordering::Relaxed);
        self.regions_extracted
            .fetch_add(result.regions.len() as u64, Ordering::Relaxed);

        if had_error {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }

        if result.regions.is_empty() {
            self.empty_frames.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record a filtered frame
    pub fn record_filtered(&self) {
        self.filtered_frames.fetch_add(1, Ordering::Relaxed);
    }

    /// Get average processing time per frame
    pub fn avg_processing_time_ms(&self) -> f64 {
        let frames = self.frames_processed.load(Ordering::Relaxed);
        if frames == 0 {
            return 0.0;
        }
        let total = self.total_processing_time_ms.load(Ordering::Relaxed);
        total as f64 / frames as f64
    }

    /// Get success rate (frames without errors / total frames)
    pub fn success_rate(&self) -> f64 {
        let frames = self.frames_processed.load(Ordering::Relaxed);
        if frames == 0 {
            return 1.0;
        }
        let errors = self.errors.load(Ordering::Relaxed);
        1.0 - (errors as f64 / frames as f64)
    }

    /// Log current metrics
    pub fn log_metrics(&self) {
        let frames = self.frames_processed.load(Ordering::Relaxed);
        let _errors = self.errors.load(Ordering::Relaxed);
        let regions = self.regions_extracted.load(Ordering::Relaxed);
        let empty = self.empty_frames.load(Ordering::Relaxed);
        let filtered = self.filtered_frames.load(Ordering::Relaxed);

        tracing::info!(
            "OCR Metrics: frames={}, regions={}, avg_time={:.1}ms, success_rate={:.2}%, empty={}, filtered={}",
            frames,
            regions,
            self.avg_processing_time_ms(),
            self.success_rate() * 100.0,
            empty,
            filtered
        );
    }
}

/// Frame processing result
#[derive(Debug)]
pub struct ProcessedFrame {
    /// Original captured frame
    pub frame: CapturedFrame,

    /// OCR extraction result
    pub ocr_result: OcrResult,

    /// Frame ID in database (set after insertion)
    pub frame_id: Option<i64>,
}

/// OCR processor orchestrator
///
/// Manages the OCR processing pipeline, consuming frames from capture
/// and producing OCR results for database storage.
pub struct OcrProcessor {
    config: OcrProcessorConfig,
    ocr_engine: OcrEngine,
    running: Arc<AtomicBool>,
    metrics: OcrMetrics,
}

impl OcrProcessor {
    /// Create a new OCR processor
    ///
    /// # Errors
    ///
    /// Returns error if OCR engine initialization fails
    pub async fn new(config: OcrProcessorConfig) -> Result<Self> {
        tracing::info!("Initializing OCR processor with config: {:?}", config);

        let ocr_engine = OcrEngine::new().await?;
        let metrics = OcrMetrics::new();

        Ok(Self {
            config,
            ocr_engine,
            running: Arc::new(AtomicBool::new(false)),
            metrics,
        })
    }

    /// Get current metrics
    pub fn metrics(&self) -> &OcrMetrics {
        &self.metrics
    }

    /// Check if processor is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Process a single frame with OCR
    ///
    /// This method:
    /// 1. Runs OCR on the frame image
    /// 2. Filters results by confidence threshold
    /// 3. Returns processed frame ready for database insertion
    ///
    /// # Arguments
    ///
    /// * `frame` - The captured frame to process
    ///
    /// # Returns
    ///
    /// `ProcessedFrame` containing OCR results, or `None` if filtered out
    pub async fn process_frame(&self, frame: CapturedFrame) -> Result<Option<ProcessedFrame>> {
        let frame_timestamp = frame.timestamp;
        tracing::debug!("Processing frame from {}", frame_timestamp);

        // Attempt OCR with retry logic
        let ocr_result = self.process_with_retry(&frame.image).await?;

        // Check if frame should be stored
        let should_store = if ocr_result.regions.is_empty() {
            self.config.store_empty_frames
        } else {
            // Check if any region meets confidence threshold
            ocr_result
                .regions
                .iter()
                .any(|r| r.confidence >= self.config.min_confidence)
        };

        if !should_store {
            tracing::trace!(
                "Frame filtered: empty={}, below_confidence={}",
                ocr_result.regions.is_empty(),
                !ocr_result.regions.is_empty()
            );
            self.metrics.record_filtered();
            return Ok(None);
        }

        // Record metrics
        self.metrics.record_frame(&ocr_result, false);

        Ok(Some(ProcessedFrame {
            frame,
            ocr_result,
            frame_id: None,
        }))
    }

    /// Start processing frames from a channel
    ///
    /// This spawns worker tasks that continuously consume frames from the
    /// provided channel and process them with OCR. Results are sent to
    /// the output channel for database storage.
    ///
    /// # Arguments
    ///
    /// * `input_rx` - Receiver for captured frames
    /// * `output_tx` - Sender for processed frames
    ///
    /// # Returns
    ///
    /// A join handle for the processing task
    pub fn start_processing(
        self: Arc<Self>,
        mut input_rx: mpsc::Receiver<CapturedFrame>,
        output_tx: mpsc::Sender<ProcessedFrame>,
    ) -> tokio::task::JoinHandle<()> {
        self.running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            tracing::info!("OCR processor started");

            // Start metrics reporting task
            let metrics_handle = if self.config.enable_metrics {
                let metrics = self.metrics.clone();
                let interval_secs = self.config.metrics_interval_secs;
                Some(tokio::spawn(async move {
                    let mut ticker = interval(Duration::from_secs(interval_secs));
                    loop {
                        ticker.tick().await;
                        metrics.log_metrics();
                    }
                }))
            } else {
                None
            };

            // Process frames
            while self.running.load(Ordering::SeqCst) {
                match input_rx.recv().await {
                    Some(frame) => {
                        match self.process_frame(frame).await {
                            Ok(Some(processed)) => {
                                if let Err(e) = output_tx.send(processed).await {
                                    tracing::error!("Failed to send processed frame: {}", e);
                                    break;
                                }
                            }
                            Ok(None) => {
                                // Frame was filtered out
                            }
                            Err(e) => {
                                tracing::error!("OCR processing error: {}", e);
                                self.metrics.errors.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                    None => {
                        tracing::info!("Input channel closed, stopping processor");
                        break;
                    }
                }
            }

            // Cleanup
            if let Some(handle) = metrics_handle {
                handle.abort();
            }

            self.running.store(false, Ordering::SeqCst);
            tracing::info!("OCR processor stopped");
        })
    }

    /// Stop the processor
    pub fn stop(&self) {
        tracing::info!("Stopping OCR processor");
        self.running.store(false, Ordering::SeqCst);
    }

    /// Process image with retry logic
    async fn process_with_retry(&self, image: &image::RgbaImage) -> Result<OcrResult> {
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match self.ocr_engine.process_image(image).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < self.config.max_retries {
                        tracing::warn!(
                            "OCR attempt {} failed, retrying in {}ms",
                            attempt + 1,
                            self.config.retry_backoff_ms
                        );
                        tokio::time::sleep(Duration::from_millis(self.config.retry_backoff_ms))
                            .await;
                    }
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| CaptureError::OcrError("All retry attempts exhausted".to_string())))
    }
}

/// Builder for OcrProcessor with fluent API
pub struct OcrProcessorBuilder {
    config: OcrProcessorConfig,
}

impl OcrProcessorBuilder {
    /// Create a new builder with default configuration
    pub fn new() -> Self {
        Self {
            config: OcrProcessorConfig::default(),
        }
    }

    /// Set minimum confidence threshold
    pub fn min_confidence(mut self, threshold: f32) -> Self {
        self.config.min_confidence = threshold.clamp(0.0, 1.0);
        self
    }

    /// Set number of worker threads
    pub fn worker_threads(mut self, threads: usize) -> Self {
        self.config.worker_threads = threads.max(1);
        self
    }

    /// Set maximum retry attempts
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.config.max_retries = retries;
        self
    }

    /// Enable or disable storing empty frames
    pub fn store_empty_frames(mut self, enable: bool) -> Self {
        self.config.store_empty_frames = enable;
        self
    }

    /// Enable or disable metrics
    pub fn enable_metrics(mut self, enable: bool) -> Self {
        self.config.enable_metrics = enable;
        self
    }

    /// Build the OcrProcessor
    pub async fn build(self) -> Result<OcrProcessor> {
        OcrProcessor::new(self.config).await
    }
}

impl Default for OcrProcessorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_config_default() {
        let config = OcrProcessorConfig::default();
        assert_eq!(config.min_confidence, 0.7);
        assert_eq!(config.worker_threads, 2);
        assert_eq!(config.max_retries, 3);
        assert!(!config.store_empty_frames);
    }

    #[test]
    fn test_metrics_initial_state() {
        let metrics = OcrMetrics::new();
        assert_eq!(metrics.frames_processed.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.errors.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.avg_processing_time_ms(), 0.0);
        assert_eq!(metrics.success_rate(), 1.0);
    }

    #[test]
    fn test_metrics_record_frame() {
        let metrics = OcrMetrics::new();
        let result = OcrResult::empty((1920, 1080));

        metrics.record_frame(&result, false);
        assert_eq!(metrics.frames_processed.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.empty_frames.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.errors.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_success_rate() {
        let metrics = OcrMetrics::new();

        // Process 10 frames, 2 with errors
        for i in 0..10 {
            let result = OcrResult::empty((1920, 1080));
            metrics.record_frame(&result, i < 2);
        }

        assert_eq!(metrics.frames_processed.load(Ordering::Relaxed), 10);
        assert_eq!(metrics.errors.load(Ordering::Relaxed), 2);
        assert!((metrics.success_rate() - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_builder_fluent_api() {
        let config = OcrProcessorBuilder::new()
            .min_confidence(0.8)
            .worker_threads(4)
            .max_retries(5)
            .store_empty_frames(true)
            .enable_metrics(false)
            .config;

        assert_eq!(config.min_confidence, 0.8);
        assert_eq!(config.worker_threads, 4);
        assert_eq!(config.max_retries, 5);
        assert!(config.store_empty_frames);
        assert!(!config.enable_metrics);
    }

    #[test]
    fn test_builder_confidence_clamping() {
        let config = OcrProcessorBuilder::new()
            .min_confidence(1.5) // Should clamp to 1.0
            .config;

        assert_eq!(config.min_confidence, 1.0);

        let config = OcrProcessorBuilder::new()
            .min_confidence(-0.5) // Should clamp to 0.0
            .config;

        assert_eq!(config.min_confidence, 0.0);
    }

    #[tokio::test]
    async fn test_processor_creation() {
        let config = OcrProcessorConfig::default();
        match OcrProcessor::new(config).await {
            Ok(processor) => {
                assert!(!processor.is_running());
                assert_eq!(
                    processor.metrics().frames_processed.load(Ordering::Relaxed),
                    0
                );
            }
            Err(e) => {
                // May fail in CI without Windows OCR support
                tracing::warn!("Processor creation failed (expected in CI): {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_process_frame_empty_image() {
        if let Ok(processor) = OcrProcessor::new(OcrProcessorConfig::default()).await {
            let frame = CapturedFrame {
                timestamp: Utc::now(),
                monitor_index: 0,
                image: image::RgbaImage::new(100, 100),
                active_window: Some("Test".to_string()),
                active_process: Some("test.exe".to_string()),
            };

            match processor.process_frame(frame).await {
                Ok(result) => {
                    // Empty frame should be filtered out by default
                    assert!(result.is_none());
                }
                Err(e) => {
                    tracing::warn!("Processing failed: {}", e);
                }
            }
        }
    }

    #[tokio::test]
    async fn test_processor_channels() {
        if let Ok(processor) = OcrProcessor::new(OcrProcessorConfig::default()).await {
            let (input_tx, input_rx) = mpsc::channel(10);
            let (output_tx, _output_rx) = mpsc::channel(10);

            let processor = Arc::new(processor);
            let handle = processor.clone().start_processing(input_rx, output_tx);

            // Send a test frame
            let frame = CapturedFrame {
                timestamp: Utc::now(),
                monitor_index: 0,
                image: image::RgbaImage::new(100, 100),
                active_window: None,
                active_process: None,
            };

            input_tx.send(frame).await.ok();
            drop(input_tx); // Close channel to stop processor

            // Wait a moment for processing
            tokio::time::sleep(Duration::from_millis(100)).await;

            processor.stop();
            handle.await.ok();
        }
    }
}
