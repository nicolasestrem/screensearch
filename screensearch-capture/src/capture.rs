//! Screen capture implementation using Windows Graphics Capture API
//!
//! This module provides the core screen capture functionality using the modern
//! Windows Graphics Capture API for hardware-accelerated, efficient screen recording.

use crate::{CaptureError, CapturedFrame, FrameDiffer, MonitorInfo, Result, WindowContext};
use crossbeam::queue::ArrayQueue;
use image::RgbaImage;
use screenshots::Screen;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tokio::sync::mpsc;

/// Configuration for screen capture
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Capture interval in milliseconds
    pub interval_ms: u64,

    /// Monitor indices to capture (empty = all monitors)
    pub monitor_indices: Vec<usize>,

    /// Whether to perform frame differencing
    pub enable_frame_diff: bool,

    /// Frame difference threshold (0.0 - 1.0)
    /// Default 0.006 means skip if < 0.6% change
    pub diff_threshold: f32,

    /// Maximum frames to buffer in memory
    pub max_frames_buffer: usize,

    /// Whether to include cursor in capture
    pub include_cursor: bool,

    /// Whether to draw border around captured window
    pub draw_border: bool,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            interval_ms: 3000, // 3 seconds
            monitor_indices: Vec::new(),
            enable_frame_diff: true,
            diff_threshold: 0.006, // 0.6% change threshold
            max_frames_buffer: 30,
            include_cursor: true,
            draw_border: false,
        }
    }
}

/// Main screen capture interface
pub struct ScreenCapture {
    config: CaptureConfig,
    running: Arc<AtomicBool>,
}

impl ScreenCapture {
    /// Create a new screen capture instance
    pub fn new(config: CaptureConfig) -> Result<Self> {
        Ok(Self {
            config,
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start the capture loop with a frame callback
    pub async fn start<F>(&mut self, mut callback: F) -> Result<()>
    where
        F: FnMut(CapturedFrame) -> Result<()> + Send + 'static,
    {
        tracing::info!("Starting screen capture with config: {:?}", self.config);
        self.running.store(true, Ordering::SeqCst);

        let monitors = if self.config.monitor_indices.is_empty() {
            MonitorInfo::enumerate()?
        } else {
            self.config
                .monitor_indices
                .iter()
                .map(|&idx| MonitorInfo::by_index(idx))
                .collect::<Result<Vec<_>>>()?
        };

        tracing::info!("Capturing {} monitor(s)", monitors.len());

        // Create channels for communication
        let (tx, mut rx) = mpsc::unbounded_channel::<CapturedFrame>();

        // Spawn capture tasks for each monitor
        let mut handles = Vec::new();
        for monitor in monitors {
            let config = self.config.clone();
            let running = self.running.clone();
            let tx = tx.clone();

            let handle = tokio::task::spawn_blocking(move || {
                Self::capture_monitor_loop(monitor, config, running, tx)
            });

            handles.push(handle);
        }

        // Drop the original sender so the receiver can detect when all senders are dropped
        drop(tx);

        // Process frames from all monitors
        while let Some(frame) = rx.recv().await {
            if let Err(e) = callback(frame) {
                tracing::error!("Frame callback error: {}", e);
            }
        }

        // Wait for all capture tasks to complete
        for handle in handles {
            if let Err(e) = handle.await {
                tracing::error!("Capture task error: {}", e);
            }
        }

        Ok(())
    }

    /// Capture a single frame from a specific monitor
    pub async fn capture_frame(&self, monitor_index: usize) -> Result<CapturedFrame> {
        let monitor = MonitorInfo::by_index(monitor_index)?;

        tokio::task::spawn_blocking(move || Self::capture_single_frame(monitor))
            .await
            .map_err(|e| CaptureError::ScreenCaptureError(format!("Task join error: {}", e)))?
    }

    /// Stop the capture loop
    pub async fn stop(&mut self) -> Result<()> {
        tracing::info!("Stopping screen capture");
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Check if capture is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Capture loop for a single monitor
    fn capture_monitor_loop(
        monitor: MonitorInfo,
        config: CaptureConfig,
        running: Arc<AtomicBool>,
        tx: mpsc::UnboundedSender<CapturedFrame>,
    ) -> Result<()> {
        tracing::info!(
            "Starting capture loop for monitor {} ({}x{})",
            monitor.index,
            monitor.width,
            monitor.height
        );

        let mut differ = if config.enable_frame_diff {
            Some(FrameDiffer::new(config.diff_threshold))
        } else {
            None
        };

        let interval = Duration::from_millis(config.interval_ms);

        while running.load(Ordering::SeqCst) {
            let capture_start = std::time::Instant::now();

            match Self::capture_single_frame(monitor.clone()) {
                Ok(frame) => {
                    // Check if frame has changed
                    let should_process = if let Some(ref mut differ) = differ {
                        differ.has_changed(&frame.image)
                    } else {
                        true
                    };

                    if should_process {
                        tracing::debug!(
                            "Frame captured from monitor {} (changed: {})",
                            monitor.index,
                            should_process
                        );

                        if let Err(e) = tx.send(frame) {
                            tracing::error!("Failed to send frame: {}", e);
                            break;
                        }
                    } else {
                        tracing::trace!("Frame skipped (no significant change)");
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to capture frame from monitor {}: {}",
                        monitor.index,
                        e
                    );
                }
            }

            // Sleep for the remaining interval time
            let elapsed = capture_start.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }

        tracing::info!("Capture loop stopped for monitor {}", monitor.index);
        Ok(())
    }

    /// Capture a single frame from a monitor
    fn capture_single_frame(monitor: MonitorInfo) -> Result<CapturedFrame> {
        // Get window context before capture
        let window_context = WindowContext::capture().ok();

        // Get all screens and find matching monitor
        let screens = Screen::all().map_err(|e| {
            CaptureError::ScreenCaptureError(format!("Failed to enumerate screens: {}", e))
        })?;

        let screen = screens.get(monitor.index).ok_or_else(|| {
            CaptureError::ScreenCaptureError(format!("Monitor {} not found", monitor.index))
        })?;

        // Capture the screen
        let captured_image = screen.capture().map_err(|e| {
            CaptureError::ScreenCaptureError(format!("Screen capture failed: {}", e))
        })?;

        // Convert from screenshots::Image to image::RgbaImage
        let width = captured_image.width();
        let height = captured_image.height();
        let rgba_data = captured_image.into_raw();

        let image = RgbaImage::from_raw(width, height, rgba_data).ok_or_else(|| {
            CaptureError::ScreenCaptureError("Failed to create RgbaImage".to_string())
        })?;

        tracing::debug!(
            "Captured frame from monitor {} ({}x{})",
            monitor.index,
            width,
            height
        );

        Ok(CapturedFrame {
            timestamp: chrono::Utc::now(),
            monitor_index: monitor.index,
            image,
            active_window: window_context.as_ref().map(|w| w.window_title.clone()),
            active_process: window_context.as_ref().map(|w| w.process_name.clone()),
        })
    }
}

/// Lower-level capture engine for more control
pub struct CaptureEngine {
    config: CaptureConfig,
    frame_queue: Arc<ArrayQueue<CapturedFrame>>,
    running: Arc<AtomicBool>,
}

impl CaptureEngine {
    /// Create a new capture engine
    pub fn new(config: CaptureConfig) -> Result<Self> {
        let queue_size = config.max_frames_buffer;
        Ok(Self {
            config,
            frame_queue: Arc::new(ArrayQueue::new(queue_size)),
            running: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Start capturing frames (non-blocking)
    pub fn start(&mut self) -> Result<()> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Err(CaptureError::InitializationError(
                "Already running".to_string(),
            ));
        }

        tracing::info!("Starting capture engine");

        // Enumerate monitors
        let monitors = if self.config.monitor_indices.is_empty() {
            MonitorInfo::enumerate()?
        } else {
            self.config
                .monitor_indices
                .iter()
                .map(|&idx| MonitorInfo::by_index(idx))
                .collect::<Result<Vec<_>>>()?
        };

        tracing::info!("Capture engine will capture {} monitor(s)", monitors.len());

        // Spawn capture thread for each monitor
        for monitor in monitors {
            let config = self.config.clone();
            let running = self.running.clone();
            let queue = self.frame_queue.clone();

            std::thread::spawn(move || Self::capture_loop(monitor, config, running, queue));
        }

        Ok(())
    }

    /// Background capture loop for a single monitor
    fn capture_loop(
        monitor: MonitorInfo,
        config: CaptureConfig,
        running: Arc<AtomicBool>,
        queue: Arc<ArrayQueue<CapturedFrame>>,
    ) {
        tracing::info!(
            "Starting capture loop for monitor {} ({}x{})",
            monitor.index,
            monitor.width,
            monitor.height
        );

        let mut differ = if config.enable_frame_diff {
            Some(FrameDiffer::new(config.diff_threshold))
        } else {
            None
        };

        let interval = Duration::from_millis(config.interval_ms);

        while running.load(Ordering::SeqCst) {
            let capture_start = std::time::Instant::now();

            match Self::capture_single_frame(monitor.clone()) {
                Ok(frame) => {
                    let should_process = if let Some(ref mut differ) = differ {
                        differ.has_changed(&frame.image)
                    } else {
                        true
                    };

                    if should_process {
                        tracing::debug!(
                            "Frame captured from monitor {} (changed: {})",
                            monitor.index,
                            should_process
                        );

                        if queue.push(frame).is_err() {
                            tracing::warn!("Frame queue full, dropping oldest frame");
                            let _ = queue.pop();
                        }
                    } else {
                        tracing::trace!("Frame skipped (no significant change)");
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to capture frame from monitor {}: {}",
                        monitor.index,
                        e
                    );
                }
            }

            let elapsed = capture_start.elapsed();
            if elapsed < interval {
                std::thread::sleep(interval - elapsed);
            }
        }

        tracing::info!("Capture loop stopped for monitor {}", monitor.index);
    }

    /// Capture a single frame from a monitor
    fn capture_single_frame(monitor: MonitorInfo) -> Result<CapturedFrame> {
        // Get window context before capture
        let window_context = WindowContext::capture().ok();

        // Get all screens and find matching monitor
        let screens = Screen::all().map_err(|e| {
            CaptureError::ScreenCaptureError(format!("Failed to enumerate screens: {}", e))
        })?;

        let screen = screens.get(monitor.index).ok_or_else(|| {
            CaptureError::ScreenCaptureError(format!("Monitor {} not found", monitor.index))
        })?;

        // Capture the screen
        let captured_image = screen.capture().map_err(|e| {
            CaptureError::ScreenCaptureError(format!("Screen capture failed: {}", e))
        })?;

        // Convert from screenshots::Image to image::RgbaImage
        let width = captured_image.width();
        let height = captured_image.height();
        let rgba_data = captured_image.into_raw();

        let image = RgbaImage::from_raw(width, height, rgba_data).ok_or_else(|| {
            CaptureError::ScreenCaptureError("Failed to create RgbaImage".to_string())
        })?;

        tracing::debug!(
            "Captured frame from monitor {} ({}x{})",
            monitor.index,
            width,
            height
        );

        Ok(CapturedFrame {
            timestamp: chrono::Utc::now(),
            monitor_index: monitor.index,
            image,
            active_window: window_context.as_ref().map(|w| w.window_title.clone()),
            active_process: window_context.as_ref().map(|w| w.process_name.clone()),
        })
    }

    /// Stop capturing frames
    pub fn stop(&mut self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        tracing::info!("Stopping capture engine");
        Ok(())
    }

    /// Get the next captured frame (non-blocking)
    pub fn try_get_frame(&self) -> Option<CapturedFrame> {
        self.frame_queue.pop()
    }

    /// Check if engine is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the number of frames in the queue
    pub fn frame_count(&self) -> usize {
        self.frame_queue.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_config_default() {
        let config = CaptureConfig::default();
        assert_eq!(config.interval_ms, 3000);
        assert!(config.enable_frame_diff);
        assert_eq!(config.diff_threshold, 0.006);
        assert_eq!(config.max_frames_buffer, 30);
    }

    #[test]
    fn test_screen_capture_new() {
        let config = CaptureConfig::default();
        let capture = ScreenCapture::new(config);
        assert!(capture.is_ok());
    }

    #[test]
    fn test_capture_engine_new() {
        let config = CaptureConfig::default();
        let engine = CaptureEngine::new(config);
        assert!(engine.is_ok());

        let mut engine = engine.unwrap();
        assert!(!engine.is_running());

        let result = engine.start();
        assert!(result.is_ok());
        assert!(engine.is_running());

        engine.stop().unwrap();
        assert!(!engine.is_running());
    }

    #[tokio::test]
    async fn test_capture_single_frame() {
        // This test requires a display and may fail in headless environments
        let monitors = MonitorInfo::enumerate();
        if let Ok(monitors) = monitors {
            if !monitors.is_empty() {
                let config = CaptureConfig::default();
                let capture = ScreenCapture::new(config).unwrap();

                match capture.capture_frame(0).await {
                    Ok(frame) => {
                        assert_eq!(frame.monitor_index, 0);
                        assert!(frame.image.width() > 0);
                        assert!(frame.image.height() > 0);
                        tracing::info!(
                            "Captured frame: {}x{}",
                            frame.image.width(),
                            frame.image.height()
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Could not capture frame (may be expected in CI): {}", e);
                    }
                }
            }
        }
    }
}
