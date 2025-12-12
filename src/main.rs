//! ScreenSearch - Main Binary
//!
//! Integrates all components into a single executable:
//! - Screen capture with frame differencing
//! - OCR processing pipeline
//! - SQLite database storage
//! - REST API server on localhost:3131
//! - Graceful shutdown handling

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// Import workspace crates
use screensearch_api::{ApiConfig, ApiServer};
use screensearch_capture::{CaptureConfig, CaptureEngine, OcrProcessor, OcrProcessorConfig};
use screensearch_db::{DatabaseConfig, DatabaseManager};

/// Application configuration loaded from config.toml
#[derive(Debug, Clone, Deserialize)]
struct AppConfig {
    capture: CaptureSettings,
    ocr: OcrSettings,
    api: ApiSettings,
    database: DatabaseSettings,
    /// Privacy controls configuration (flagship feature - implementation pending)
    #[allow(dead_code)]
    privacy: PrivacySettings,
    /// Performance management configuration (flagship feature - implementation pending)
    #[allow(dead_code)]
    performance: PerformanceSettings,
    logging: LoggingSettings,
    storage: StorageSettings,
}

#[derive(Debug, Clone, Deserialize)]
struct StorageSettings {
    format: String,
    jpeg_quality: u8,
    max_width: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct CaptureSettings {
    interval_ms: u64,
    enable_frame_diff: bool,
    diff_threshold: f32,
    max_frames_buffer: usize,
    monitor_indices: Vec<usize>,
    include_cursor: bool,
    draw_border: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct OcrSettings {
    /// OCR engine selection (feature pending - currently uses Windows OCR only)
    #[allow(dead_code)]
    engine: String,
    min_confidence: f32,
    worker_threads: usize,
    max_retries: u32,
    retry_backoff_ms: u64,
    store_empty_frames: bool,
    channel_buffer_size: usize,
    enable_metrics: bool,
    metrics_interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct ApiSettings {
    host: String,
    port: u16,
    /// Configurable CORS origin (feature pending)
    #[allow(dead_code)]
    cors_origin: String,
    #[serde(default = "default_auto_open_browser")]
    auto_open_browser: bool,
}

fn default_auto_open_browser() -> bool {
    true // Maintain backward compatibility - enabled by default
}

#[derive(Debug, Clone, Deserialize)]
struct DatabaseSettings {
    path: String,
    max_connections: u32,
    min_connections: u32,
    acquire_timeout_secs: u64,
    enable_wal: bool,
    cache_size_kb: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct PrivacySettings {
    /// Applications to exclude from capture (feature pending)
    #[allow(dead_code)]
    excluded_apps: Vec<String>,
    /// Pause capture when screen is locked (feature pending)
    #[allow(dead_code)]
    pause_on_lock: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct PerformanceSettings {
    /// Maximum CPU usage percentage (feature pending)
    #[allow(dead_code)]
    max_cpu_percent: u8,
    /// Maximum memory usage in MB (feature pending)
    #[allow(dead_code)]
    max_memory_mb: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct LoggingSettings {
    level: String,
    log_to_file: bool,
    /// File logging destination (feature pending - currently logs to stdout)
    #[allow(dead_code)]
    log_file: String,
    /// Maximum log file size for rotation (feature pending)
    #[allow(dead_code)]
    max_log_size_mb: u64,
    /// Number of log files to keep (feature pending)
    #[allow(dead_code)]
    log_rotation_count: u32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            capture: CaptureSettings {
                interval_ms: 3000,
                enable_frame_diff: true,
                diff_threshold: 0.006,
                max_frames_buffer: 30,
                monitor_indices: Vec::new(),
                include_cursor: true,
                draw_border: false,
            },
            ocr: OcrSettings {
                engine: "windows".to_string(),
                min_confidence: 0.7,
                worker_threads: 2,
                max_retries: 3,
                retry_backoff_ms: 1000,
                store_empty_frames: false,
                channel_buffer_size: 100,
                enable_metrics: true,
                metrics_interval_secs: 60,
            },
            api: ApiSettings {
                host: "127.0.0.1".to_string(),
                port: 3131,
                cors_origin: String::new(),
                auto_open_browser: true, // Default to enabled for backward compatibility
            },
            database: DatabaseSettings {
                path: "screensearch.db".to_string(),
                max_connections: 50,
                min_connections: 3,
                acquire_timeout_secs: 10,
                enable_wal: true,
                cache_size_kb: -2000,
            },
            privacy: PrivacySettings {
                excluded_apps: vec![
                    "1Password".to_string(),
                    "KeePass".to_string(),
                    "Bitwarden".to_string(),
                    "LastPass".to_string(),
                    "Password".to_string(),
                    "Bank".to_string(),
                ],
                pause_on_lock: true,
            },
            performance: PerformanceSettings {
                max_cpu_percent: 5,
                max_memory_mb: 500,
            },
            logging: LoggingSettings {
                level: "info".to_string(),
                log_to_file: true,
                log_file: "screensearch.log".to_string(),
                max_log_size_mb: 100,
                log_rotation_count: 5,
            },
            storage: StorageSettings {
                format: "jpeg".to_string(),
                jpeg_quality: 80,
                max_width: 1920,
            },
        }
    }
}

impl AppConfig {
    /// Load configuration from file, falling back to defaults
    fn load() -> Result<Self> {
        let config_path = PathBuf::from("config.toml");

        if config_path.exists() {
            let content =
                std::fs::read_to_string(&config_path).context("Failed to read config.toml")?;
            let config: AppConfig =
                toml::from_str(&content).context("Failed to parse config.toml")?;
            info!("Loaded configuration from config.toml");
            Ok(config)
        } else {
            warn!("config.toml not found, using default configuration");
            Ok(Self::default())
        }
    }

    /// Convert to CaptureConfig
    fn capture_config(&self) -> CaptureConfig {
        CaptureConfig {
            interval_ms: self.capture.interval_ms,
            monitor_indices: self.capture.monitor_indices.clone(),
            enable_frame_diff: self.capture.enable_frame_diff,
            diff_threshold: self.capture.diff_threshold,
            max_frames_buffer: self.capture.max_frames_buffer,
            include_cursor: self.capture.include_cursor,
            draw_border: self.capture.draw_border,
        }
    }

    /// Convert to OcrProcessorConfig
    fn ocr_config(&self) -> OcrProcessorConfig {
        OcrProcessorConfig {
            min_confidence: self.ocr.min_confidence,
            worker_threads: self.ocr.worker_threads,
            max_retries: self.ocr.max_retries,
            retry_backoff_ms: self.ocr.retry_backoff_ms,
            store_empty_frames: self.ocr.store_empty_frames,
            channel_buffer_size: self.ocr.channel_buffer_size,
            enable_metrics: self.ocr.enable_metrics,
            metrics_interval_secs: self.ocr.metrics_interval_secs,
        }
    }

    /// Convert to DatabaseConfig
    fn database_config(&self) -> DatabaseConfig {
        DatabaseConfig {
            path: self.database.path.clone(),
            max_connections: self.database.max_connections,
            min_connections: self.database.min_connections,
            acquire_timeout_secs: self.database.acquire_timeout_secs,
            enable_wal: self.database.enable_wal,
            cache_size_kb: self.database.cache_size_kb,
        }
    }

    /// Convert to ApiConfig
    fn api_config(&self) -> ApiConfig {
        ApiConfig {
            host: self.api.host.clone(),
            port: self.api.port,
            database_path: self.database.path.clone(),
        }
    }
}

/// Initialize tracing/logging subsystem
fn init_tracing(config: &LoggingSettings) -> Result<()> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_line_number(true);

    if config.log_to_file {
        // TODO: Add file rotation support using tracing-appender
        // For now, log to console
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    Ok(())
}

/// Main application state
struct App {
    config: AppConfig,
    shutdown_tx: broadcast::Sender<()>,
}

impl App {
    fn new(config: AppConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(10);
        Self {
            config,
            shutdown_tx,
        }
    }

    /// Run all services concurrently
    async fn run(&self) -> Result<()> {
        info!("Starting ScreenSearch v{}", env!("CARGO_PKG_VERSION"));
        info!("Configuration loaded: {:?}", self.config);

        // Initialize database
        info!("Initializing database...");
        let db_config = self.config.database_config();
        let db = Arc::new(
            DatabaseManager::with_config(db_config)
                .await
                .context("Failed to initialize database")?,
        );
        info!("Database initialized: {}", self.config.database.path);

        // Initialize OCR processor
        info!("Initializing OCR processor...");
        let ocr_config = self.config.ocr_config();
        let ocr_processor = Arc::new(
            OcrProcessor::new(ocr_config)
                .await
                .context("Failed to initialize OCR processor")?,
        );
        info!(
            "OCR processor initialized with {} workers",
            self.config.ocr.worker_threads
        );

        // Initialize capture engine
        info!("Initializing capture engine...");
        let capture_config = self.config.capture_config();
        let mut capture_engine =
            CaptureEngine::new(capture_config).context("Failed to initialize capture engine")?;
        info!(
            "Capture engine initialized (interval: {}ms)",
            self.config.capture.interval_ms
        );

        // Initialize API server
        info!("Initializing API server...");
        let api_config = self.config.api_config();
        let api_server = ApiServer::new(api_config.clone())
            .await
            .context("Failed to initialize API server")?;
        info!(
            "API server initialized on {}:{}",
            api_config.host, api_config.port
        );

        // Create channels for frame processing pipeline
        // Use bounded channels to match OcrProcessor::start_processing signature
        let (frame_tx, frame_rx) = tokio::sync::mpsc::channel(100); // Buffer of 100 frames
        let (processed_tx, mut processed_rx) = tokio::sync::mpsc::channel(100);
        
        // Clone config for the database loop
        let app_config_clone = self.config.clone();

        // Clone for tasks
        let db_clone = Arc::clone(&db);
        let ocr_clone = Arc::clone(&ocr_processor);
        let mut shutdown_rx1 = self.shutdown_tx.subscribe();
        let mut shutdown_rx2 = self.shutdown_tx.subscribe();
        let mut shutdown_rx3 = self.shutdown_tx.subscribe();
        let mut shutdown_rx4 = self.shutdown_tx.subscribe();

        // Start capture engine
        capture_engine
            .start()
            .context("Failed to start capture engine")?;

        // Task 1: Capture loop - Poll capture engine and send frames
        let capture_handle = tokio::spawn(async move {
            info!("Starting capture loop");
            let mut interval = tokio::time::interval(
                tokio::time::Duration::from_millis(3000), // Poll interval
            );

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        // Poll for captured frames
                        while let Some(frame) = capture_engine.try_get_frame() {
                            if frame_tx.send(frame).await.is_err() {
                                error!("Failed to send frame - channel closed");
                                break;
                            }
                        }
                    }
                    _ = shutdown_rx1.recv() => {
                        info!("Capture loop shutting down");
                        let _ = capture_engine.stop();
                        break;
                    }
                }
            }
        });

        // Task 2: OCR processing - Consume frames and perform OCR
        let ocr_handle = ocr_clone.start_processing(frame_rx, processed_tx);

        // Spawn a task to handle OCR shutdown
        let ocr_shutdown = tokio::spawn(async move {
            let _ = shutdown_rx2.recv().await;
            info!("OCR processing shutdown signal received");
        });

        // Task 3: Database insertion - Store processed frames
        let db_handle = tokio::spawn(async move {
            info!("Starting database insertion loop");

            loop {
                tokio::select! {
                    Some(processed) = processed_rx.recv() => {
                        // Store frame in database
                        let storage_config = &app_config_clone.storage;
                        match store_processed_frame(&db_clone, processed, storage_config).await {
                            Ok(_) => {},
                            Err(e) => error!("Failed to store frame: {}", e),
                        }
                    }
                    _ = shutdown_rx3.recv() => {
                        info!("Database insertion loop shutting down");
                        break;
                    }
                }
            }
        });

        // Task 4: API server
        let api_handle = tokio::spawn(async move {
            info!("Starting API server");
            if let Err(e) = api_server.run().await {
                error!("API server error: {}", e);
            }

            // Wait for shutdown (API server runs until error or shutdown)
            let _ = shutdown_rx4.recv().await;
            info!("API server shutting down");
        });

        // Task 5: Metrics reporting (if enabled)
        let metrics_handle = if self.config.ocr.enable_metrics {
            let ocr_metrics = Arc::clone(&ocr_processor);
            let interval_secs = self.config.ocr.metrics_interval_secs;
            let mut shutdown_rx5 = self.shutdown_tx.subscribe();

            Some(tokio::spawn(async move {
                let mut interval =
                    tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));

                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            ocr_metrics.metrics().log_metrics();
                        }
                        _ = shutdown_rx5.recv() => {
                            info!("Metrics reporting shutting down");
                            break;
                        }
                    }
                }
            }))
        } else {
            None
        };

        // Task 6: Cleanup loop (Storage Retention)
        let db_cleanup = Arc::clone(&db);
        let mut shutdown_rx6 = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            info!("Starting cleanup loop");
            // Check immediately on startup, then every 24 hours
            let cleanup_interval = tokio::time::Duration::from_secs(24 * 60 * 60);
          
            loop {
                 // Run cleanup logic
                 match db_cleanup.get_settings().await {
                     Ok(settings) => {
                         let retention_days = settings.retention_days;
                         if retention_days > 0 {
                             info!("Running automatic cleanup (retention: {} days)", retention_days);
                             match db_cleanup.cleanup_old_data(retention_days as i32).await {
                                 Ok(deleted) => info!("Cleanup completed: {} frames removed", deleted),
                                 Err(e) => error!("Cleanup failed: {}", e),
                             }
                         } else {
                             info!("Automatic cleanup disabled (retention_days = 0)");
                         }
                     },
                     Err(e) => error!("Failed to fetch settings for cleanup: {}", e),
                 }

                 // Wait for next interval or shutdown
                 tokio::select! {
                     _ = tokio::time::sleep(cleanup_interval) => {},
                     _ = shutdown_rx6.recv() => {
                         info!("Cleanup loop shutting down");
                         break;
                     }
                 }
            }
        });

        // Auto-open browser (if enabled in config)
        if self.config.api.auto_open_browser {
            let url = format!("http://{}:{}", api_config.host, api_config.port);
            tokio::spawn(async move {
                // Wait 2 seconds for server to be fully ready
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

                if let Err(e) = webbrowser::open(&url) {
                    warn!(
                        "Failed to auto-open browser: {}. Please navigate to {} manually",
                        e, url
                    );
                } else {
                    info!("Opened browser at {}", url);
                }
            });
        } else {
            info!(
                "Auto-open browser disabled. Navigate to http://{}:{} manually",
                api_config.host, api_config.port
            );
        }

        // Wait for shutdown signal
        info!("All services started successfully");
        info!("Press Ctrl+C to shutdown");

        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Shutdown signal received");
            }
            Err(err) => {
                error!("Unable to listen for shutdown signal: {}", err);
            }
        }

        // Broadcast shutdown to all tasks
        let _ = self.shutdown_tx.send(());

        // Wait for all tasks to complete
        info!("Waiting for services to shut down...");
        let _ = tokio::join!(
            capture_handle,
            ocr_handle,
            ocr_shutdown,
            db_handle,
            api_handle,
        );

        if let Some(handle) = metrics_handle {
            let _ = handle.await;
        }

        info!("All services stopped. Goodbye!");
        Ok(())
    }
}

/// Store a processed frame in the database
async fn store_processed_frame(
    db: &DatabaseManager,
    processed: screensearch_capture::ProcessedFrame,
    config: &StorageSettings,
) -> Result<i64> {
    use screensearch_db::{NewFrame, NewOcrText};
    use image::DynamicImage;
    use std::io::Cursor;

    // Process image: Resize -> Compress
    let mut image = DynamicImage::ImageRgba8(processed.frame.image.clone());

    // Resize if needed
    if config.max_width > 0 && image.width() > config.max_width {
        let n_width = config.max_width;
        let n_height = (image.height() as f64 * (n_width as f64 / image.width() as f64)) as u32;
        image = image.resize(n_width, n_height, image::imageops::FilterType::Lanczos3);
    }
    
    // Determine format and extension
    let (ext, format) = if config.format.to_lowercase() == "jpeg" || config.format.to_lowercase() == "jpg" {
        ("jpg", image::ImageOutputFormat::Jpeg(config.jpeg_quality))
    } else {
        ("png", image::ImageOutputFormat::Png)
    };

    let timestamp_str = processed.frame.timestamp.format("%Y%m%d_%H%M%S_%3f");
    let image_filename = format!(
        "frame_{}_{}.{}",
        processed.frame.monitor_index, timestamp_str, ext
    );
    let image_path = PathBuf::from("captures").join(&image_filename);

    // Create captures directory if it doesn't exist
    if let Some(parent) = image_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Save image with specific format/quality
    let mut file = std::fs::File::create(&image_path)?;
    image.write_to(&mut file, format).context("Failed to save frame image")?;

    // Insert frame record
    let new_frame = NewFrame {
        timestamp: processed.frame.timestamp,
        device_name: format!("monitor-{}", processed.frame.monitor_index),
        file_path: image_path.to_string_lossy().to_string(),
        monitor_index: processed.frame.monitor_index as i32,
        width: image.width() as i32,
        height: image.height() as i32,
        offset_index: 0,
        chunk_id: None,
        active_window: processed.frame.active_window,
        active_process: processed.frame.active_process,
        browser_url: None,
        focused: Some(true),
    };

    let frame_id = db
        .insert_frame(new_frame)
        .await
        .context("Failed to insert frame")?;

    // Insert OCR text records
    for region in processed.ocr_result.regions {
        let ocr_text = NewOcrText {
            frame_id,
            text: region.text.clone(),
            text_json: Some(
                serde_json::json!({
                    "confidence": region.confidence,
                    "x": region.x,
                    "y": region.y,
                    "width": region.width,
                    "height": region.height,
                })
                .to_string(),
            ),
            x: region.x as i32,
            y: region.y as i32,
            width: region.width as i32,
            height: region.height as i32,
            confidence: region.confidence,
        };

        db.insert_ocr_text(ocr_text)
            .await
            .context("Failed to insert OCR text")?;
    }

    Ok(frame_id)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load configuration
    let config = AppConfig::load().context("Failed to load configuration")?;

    // Initialize logging
    init_tracing(&config.logging).context("Failed to initialize logging")?;

    // Create and run application
    let app = App::new(config);
    app.run().await
}
