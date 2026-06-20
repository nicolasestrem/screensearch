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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// Import workspace crates
use screensearch_api::{ApiConfig, ApiServer};
use screensearch_capture::{
    CaptureConfig, CaptureEngine, DiffMethod, OcrProcessor, OcrProcessorConfig,
};
use screensearch_db::{DatabaseConfig, DatabaseManager};

// Version and update checking modules
mod update_checker;
mod version;

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
    #[serde(default = "default_embeddings_settings")]
    embeddings: EmbeddingsSettings,
}

fn default_embeddings_settings() -> EmbeddingsSettings {
    EmbeddingsSettings {
        enabled: false,
        batch_size: 50,
        image_enabled: false,
    }
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
    #[serde(default = "default_diff_method")]
    diff_method: String,
    max_frames_buffer: usize,
    monitor_indices: Vec<usize>,
    include_cursor: bool,
    draw_border: bool,
}

fn default_diff_method() -> String {
    "pixel".to_string()
}

/// Map the `diff_method` config string to a `DiffMethod`. Unknown values fall
/// back to `Pixel`, whose semantics match the default `diff_threshold`.
fn parse_diff_method(value: &str) -> DiffMethod {
    match value.trim().to_ascii_lowercase().as_str() {
        "histogram" => DiffMethod::Histogram,
        "ssim" => DiffMethod::Ssim,
        _ => DiffMethod::Pixel,
    }
}

#[derive(Debug, Clone, Deserialize)]
struct OcrSettings {
    /// OCR language hint (BCP-47 tag, e.g. `"en-US"`; empty = user profile).
    #[serde(default = "default_ocr_language")]
    language: String,
    min_confidence: f32,
    worker_threads: usize,
    max_retries: u32,
    retry_backoff_ms: u64,
    store_empty_frames: bool,
    channel_buffer_size: usize,
    enable_metrics: bool,
    metrics_interval_secs: u64,
}

fn default_ocr_language() -> String {
    // Empty = use the user's Windows profile languages.
    String::new()
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
    log_file: String,
    #[allow(dead_code)]
    max_log_size_mb: u64,
    log_rotation_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct EmbeddingsSettings {
    enabled: bool,
    batch_size: i64,
    /// Enable the optional in-process image-embedding index (visual recall).
    /// Off by default: loads extra models and adds per-frame image-embedding CPU.
    #[serde(default)]
    image_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            capture: CaptureSettings {
                interval_ms: 3000,
                enable_frame_diff: true,
                diff_threshold: 0.006,
                diff_method: default_diff_method(),
                max_frames_buffer: 30,
                monitor_indices: Vec::new(),
                include_cursor: true,
                draw_border: false,
            },
            ocr: OcrSettings {
                language: default_ocr_language(),
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
                // Captures are read by machines (OCR runs on the full-res frame
                // *before* this resize; the vision model re-decodes this stored
                // JPEG). 1280px keeps app/layout legible for vision while cutting
                // disk use and — crucially — the vision encoder's per-frame cost
                // on ultrawide (3440-wide) multi-monitor captures.
                max_width: 1280,
            },
            embeddings: default_embeddings_settings(),
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
            diff_method: parse_diff_method(&self.capture.diff_method),
            max_frames_buffer: self.capture.max_frames_buffer,
            include_cursor: self.capture.include_cursor,
            draw_border: self.capture.draw_border,
        }
    }

    /// Convert to OcrProcessorConfig
    fn ocr_config(&self) -> OcrProcessorConfig {
        OcrProcessorConfig {
            language: self.ocr.language.clone(),
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
        // Use AppData for database in production, current directory in development
        let db_path = if cfg!(debug_assertions) {
            // Development: use relative path
            self.database.path.clone()
        } else {
            // Production: use AppData
            if let Some(data_dir) = dirs::data_local_dir() {
                let app_dir = data_dir.join("screensearch");
                if let Err(e) = std::fs::create_dir_all(&app_dir) {
                    warn!("Could not create AppData directory: {}", e);
                    self.database.path.clone()
                } else {
                    app_dir
                        .join(&self.database.path)
                        .to_string_lossy()
                        .to_string()
                }
            } else {
                warn!("Could not determine AppData directory, using relative path");
                self.database.path.clone()
            }
        };

        DatabaseConfig {
            path: db_path,
            max_connections: self.database.max_connections,
            min_connections: self.database.min_connections,
            acquire_timeout_secs: self.database.acquire_timeout_secs,
            enable_wal: self.database.enable_wal,
            cache_size_kb: self.database.cache_size_kb,
        }
    }

    /// Convert to ApiConfig with the correct database path
    fn api_config(&self, db_path: &str) -> ApiConfig {
        ApiConfig {
            host: self.api.host.clone(),
            port: self.api.port,
            database_path: db_path.to_string(),
        }
    }
}

/// Initialize tracing/logging subsystem
fn init_tracing(
    config: &LoggingSettings,
) -> Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.level));

    if config.log_to_file {
        // Use AppData for logs in production, current directory in development
        let (log_dir, log_filename) = if cfg!(debug_assertions) {
            // Development: use relative path from config
            let log_path = PathBuf::from(&config.log_file);
            let dir = log_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            let filename = log_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("screensearch")
                .to_string();
            (dir, filename)
        } else {
            // Production: use LocalAppData
            let dir = if let Some(data_dir) = dirs::data_local_dir() {
                let log_dir = data_dir.join("ScreenSearch").join("logs");
                if let Err(e) = std::fs::create_dir_all(&log_dir) {
                    warn!("Could not create log directory: {}", e);
                    PathBuf::from(".")
                } else {
                    log_dir
                }
            } else {
                warn!("Could not determine LocalAppData directory, using current directory");
                PathBuf::from(".")
            };
            let filename = PathBuf::from(&config.log_file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("screensearch")
                .to_string();
            (dir, filename)
        };

        // Create rolling file appender with daily rotation
        let file_appender = RollingFileAppender::builder()
            .rotation(Rotation::DAILY)
            .filename_prefix(&log_filename)
            .filename_suffix("log")
            .max_log_files(config.log_rotation_count as usize)
            .build(&log_dir)
            .context("Failed to create rolling file appender")?;

        let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);

        // Log to both stdout and file
        let stdout_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_line_number(true);

        let file_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_line_number(true)
            .with_ansi(false)
            .with_writer(non_blocking_file);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(stdout_layer)
            .with(file_layer)
            .init();

        info!(
            "File logging enabled: {:?}",
            log_dir.join(format!("{}.log", log_filename))
        );
        info!(
            "Log rotation: {} files, daily rotation",
            config.log_rotation_count
        );

        Ok(Some(guard))
    } else {
        // Console-only logging
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_line_number(true);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();

        Ok(None)
    }
}

use crossbeam::channel::Receiver;
use tray_icon::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    TrayIconBuilder,
};
use winit::event_loop::{ControlFlow, EventLoop};

struct App {
    config: AppConfig,
    shutdown_tx: broadcast::Sender<()>,
}

struct EventLoopState {
    _tray_icon: tray_icon::TrayIcon,
    menu_items: (MenuItem, MenuItem),
    menu_channel: &'static Receiver<tray_icon::menu::MenuEvent>,
    tray_channel: &'static Receiver<tray_icon::TrayIconEvent>,
    app_task: std::thread::JoinHandle<()>,
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
    api_url: String,
}

impl App {
    fn new(config: AppConfig) -> Self {
        let (shutdown_tx, _) = broadcast::channel(10);
        Self {
            config,
            shutdown_tx,
        }
    }

    async fn run_with_signal(
        &self,
        mut external_shutdown: tokio::sync::mpsc::Receiver<()>,
    ) -> Result<()> {
        info!("Starting ScreenSearch v{}", env!("CARGO_PKG_VERSION"));
        info!("Configuration loaded: {:?}", self.config);

        // Initialize database
        info!("Initializing database...");
        let db_config = self.config.database_config();
        let db = Arc::new(
            DatabaseManager::with_config(db_config.clone())
                .await
                .context("Failed to initialize database")?,
        );

        // Initialize OCR processor (native Windows OCR, in-process)
        let ocr_config = self.config.ocr_config();
        let ocr_processor = Arc::new(OcrProcessor::new(ocr_config).await?);

        // Load persisted runtime settings (monitor selection + capture interval)
        // so a choice made in the UI survives restarts. These override the
        // config.toml defaults. Empty/unparseable monitors means "all monitors".
        let persisted_settings = db.get_settings().await.ok();
        let initial_monitors: Vec<usize> = persisted_settings
            .as_ref()
            .and_then(|s| serde_json::from_str(&s.monitors).ok())
            .unwrap_or_else(|| self.config.capture.monitor_indices.clone());
        let initial_interval_ms = persisted_settings
            .as_ref()
            .map(|s| (s.capture_interval.max(1) as u64) * 1000)
            .unwrap_or(self.config.capture.interval_ms);

        // Initialize capture engine using the persisted monitor selection.
        let mut capture_config = self.config.capture_config();
        capture_config.monitor_indices = initial_monitors.clone();
        // Base config (other capture fields) reused when the monitor set changes.
        let base_capture_config = capture_config.clone();
        let mut capture_engine = CaptureEngine::new(capture_config)?;

        // Shared capture interval
        let capture_interval_ms = Arc::new(AtomicU64::new(initial_interval_ms));

        // Channel that carries live monitor-selection changes to the capture task.
        let (monitor_config_tx, mut monitor_config_rx) =
            tokio::sync::watch::channel(initial_monitors);

        // Initialize API server with the same database path
        let api_config = self.config.api_config(&db_config.path);
        let api_server = ApiServer::new(
            api_config.clone(),
            Arc::clone(&capture_interval_ms),
            monitor_config_tx,
        )
        .await?;
        api_server
            .set_embeddings_enabled(self.config.embeddings.enabled)
            .await?;

        // Keep the worker alive so the API toggle can enable indexing later.
        let worker_config = screensearch_api::workers::embedding_worker::EmbeddingWorkerConfig {
            enabled: true,
            batch_size: self.config.embeddings.batch_size,
            interval_secs: 60,
        };
        if let Err(e) = api_server.start_embedding_worker(worker_config).await {
            warn!("Embedding worker did not start yet: {}", e);
        }

        // Optional image-embedding index (visual recall). The worker loop is
        // always started but stays idle (no models loaded) until enabled via the
        // `image_embeddings_enabled` flag, which config seeds and the API toggles.
        api_server
            .set_image_embeddings_enabled(self.config.embeddings.image_enabled)
            .await?;
        let image_worker_config =
            screensearch_api::workers::image_embedding_worker::ImageEmbeddingWorkerConfig {
                batch_size: self.config.embeddings.batch_size,
                interval_secs: 60,
            };
        if let Err(e) = api_server
            .start_image_embedding_worker(image_worker_config)
            .await
        {
            warn!("Image embedding worker did not start yet: {}", e);
        }

        // Start vision analysis worker (shares AppState to drive the unified
        // local llama-server with --mmproj for on-device vision).
        api_server.start_vision_worker();

        let (frame_tx, frame_rx) = tokio::sync::mpsc::channel(100);
        let (processed_tx, mut processed_rx) = tokio::sync::mpsc::channel(100);

        let app_config_clone = self.config.clone();
        let db_clone = Arc::clone(&db);
        let ocr_clone = Arc::clone(&ocr_processor);

        let mut shutdown_rx1 = self.shutdown_tx.subscribe();
        let mut shutdown_rx2 = self.shutdown_tx.subscribe();
        let mut shutdown_rx3 = self.shutdown_tx.subscribe();
        let mut shutdown_rx4 = self.shutdown_tx.subscribe();

        capture_engine.start()?;

        let capture_interval_clone = Arc::clone(&capture_interval_ms);
        let capture_handle = tokio::spawn(async move {
            let mut current_interval_ms = capture_interval_clone.load(Ordering::Relaxed);
            // Ensure minimum interval of 500ms
            if current_interval_ms < 500 {
                current_interval_ms = 500;
            }

            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_millis(current_interval_ms));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            // Mark the initial monitor selection as seen so `changed()` only fires
            // on real updates, not once at startup.
            monitor_config_rx.borrow_and_update();

            loop {
                tokio::select! {
                    _ = monitor_config_rx.changed() => {
                        let new_monitors = monitor_config_rx.borrow_and_update().clone();
                        info!("Reconfiguring capture for monitors: {:?}", new_monitors);
                        // Stop the current engine; its detached per-monitor threads
                        // exit on their next loop check and drain into the queue we
                        // are about to drop. Build a fresh engine (new queue + flag).
                        let _ = capture_engine.stop();
                        let mut new_config = base_capture_config.clone();
                        new_config.monitor_indices = new_monitors;
                        match CaptureEngine::new(new_config) {
                            Ok(mut engine) => match engine.start() {
                                Ok(()) => capture_engine = engine,
                                Err(e) => error!("Failed to restart capture engine: {}", e),
                            },
                            Err(e) => error!("Failed to rebuild capture engine: {}", e),
                        }
                    }
                    _ = interval.tick() => {
                        // Check for interval update
                        let new_interval_ms = capture_interval_clone.load(Ordering::Relaxed);
                        // Ensure minimum interval of 500ms
                        let new_interval_ms = if new_interval_ms < 500 { 500 } else { new_interval_ms };

                        if new_interval_ms != current_interval_ms {
                             current_interval_ms = new_interval_ms;
                             interval = tokio::time::interval(tokio::time::Duration::from_millis(current_interval_ms));
                             interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                             // First tick of new interval fires immediately, which is fine
                        }

                        // Drain captured frames without ever blocking this select
                        // loop. `send().await` would park here whenever OCR is
                        // backed up (the channel fills because a frame can take
                        // tens of seconds on CPU), and a parked loop cannot react
                        // to monitor reconfiguration or shutdown — which makes
                        // toggling a monitor appear to freeze capture entirely.
                        // `try_send` instead applies backpressure by leaving
                        // frames in the engine queue (which drops oldest when
                        // full), so the loop stays responsive.
                        while let Some(frame) = capture_engine.try_get_frame() {
                           use tokio::sync::mpsc::error::TrySendError;
                           match frame_tx.try_send(frame) {
                               Ok(()) => {}
                               // OCR not keeping up: stop draining this tick and
                               // retry next tick; surplus frames age out of the
                               // capture queue rather than blocking reconfig.
                               Err(TrySendError::Full(_)) => break,
                               Err(TrySendError::Closed(_)) => break,
                           }
                        }
                    }
                    _ = shutdown_rx1.recv() => {
                        let _ = capture_engine.stop();
                        break;
                    }
                }
            }
        });

        let ocr_handle = ocr_clone.start_processing(frame_rx, processed_tx);
        let ocr_shutdown = tokio::spawn(async move {
            let _ = shutdown_rx2.recv().await;
        });

        let db_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(processed) = processed_rx.recv() => {
                         let storage_config = &app_config_clone.storage;
                         if let Err(e) = store_processed_frame(&db_clone, processed, storage_config).await {
                             error!("Failed to save frame: {}", e);
                         }
                    }
                    _ = shutdown_rx3.recv() => break,
                }
            }
        });

        let api_handle = tokio::spawn(async move {
            if let Err(e) = api_server.run().await {
                error!("{}", e);
            }
            let _ = shutdown_rx4.recv().await;
        });

        let mut shutdown_rx6 = self.shutdown_tx.subscribe();
        tokio::spawn(async move {
            let _ = shutdown_rx6.recv().await;
        });

        if self.config.api.auto_open_browser {
            let url = format!("http://{}:{}", api_config.host, api_config.port);
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                let _ = webbrowser::open(&url);
            });
        }

        // Check for updates in background
        tokio::spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            if let Some(update) = update_checker::check_updates().await {
                info!("========================================");
                info!("UPDATE AVAILABLE!");
                info!("Current version: {}", version::VERSION);
                info!("Latest version: {}", update.version);
                info!("Download: {}", update.download_url);
                info!("========================================");
            }
        });

        tokio::select! {
            _ = signal::ctrl_c() => info!("Ctrl+C"),
            _ = external_shutdown.recv() => info!("External Shutdown"),
        }

        info!("Initiating graceful shutdown...");

        // Shutdown LlamaServer if running (via API server state)
        // Note: The server shutdown is handled through AppState.shutdown_llama_server()
        // which is called by the API handlers or when the state is dropped.
        // We just need to ensure the shutdown broadcast is sent.

        let _ = self.shutdown_tx.send(());
        let _ = tokio::join!(
            capture_handle,
            ocr_handle,
            ocr_shutdown,
            db_handle,
            api_handle
        );

        info!("All services stopped. Shutdown complete.");

        Ok(())
    }
}

impl winit::application::ApplicationHandler for EventLoopState {
    fn resumed(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        info!("Event loop resumed - tray icon active");
    }

    fn window_event(
        &mut self,
        _event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        _event: winit::event::WindowEvent,
    ) {
        // No windows in tray-only app
    }

    fn new_events(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _cause: winit::event::StartCause,
    ) {
        event_loop.set_control_flow(ControlFlow::Wait);

        // Process menu events
        while let Ok(event) = self.menu_channel.try_recv() {
            if event.id == self.menu_items.0.id() {
                // Open Interface
                info!("Opening web interface");
                let _ = webbrowser::open(&self.api_url);
            } else if event.id == self.menu_items.1.id() {
                // Quit
                info!("Quit requested from tray menu");
                let _ = self.shutdown_tx.blocking_send(());
                event_loop.exit();
            }
        }

        // Process tray icon events (left-click handling)
        while let Ok(tray_event) = self.tray_channel.try_recv() {
            match tray_event {
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    ..
                } => {
                    info!("Tray icon left-clicked");
                    let _ = webbrowser::open(&self.api_url);
                }
                tray_icon::TrayIconEvent::DoubleClick {
                    button: tray_icon::MouseButton::Left,
                    ..
                } => {
                    info!("Tray icon double-clicked");
                    let _ = webbrowser::open(&self.api_url);
                }
                _ => {}
            }
        }

        // Check if app task finished
        if self.app_task.is_finished() {
            info!("Application task completed");
            event_loop.exit();
        }
    }

    fn exiting(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop) {
        info!("Event loop exiting");
    }
}

async fn store_processed_frame(
    db: &DatabaseManager,
    processed: screensearch_capture::ProcessedFrame,
    config: &StorageSettings,
) -> Result<i64> {
    use image::DynamicImage;
    use screensearch_db::{NewFrame, NewOcrText};

    let mut image = DynamicImage::ImageRgba8(processed.frame.image.clone());

    if config.max_width > 0 && image.width() > config.max_width {
        let n_width = config.max_width;
        let n_height = (image.height() as f64 * (n_width as f64 / image.width() as f64)) as u32;
        image = image.resize(n_width, n_height, image::imageops::FilterType::Lanczos3);
    }

    let (ext, format) =
        if config.format.to_lowercase() == "jpeg" || config.format.to_lowercase() == "jpg" {
            ("jpg", image::ImageOutputFormat::Jpeg(config.jpeg_quality))
        } else {
            ("png", image::ImageOutputFormat::Png)
        };

    let timestamp_str = processed.frame.timestamp.format("%Y%m%d_%H%M%S_%3f");
    let image_filename = format!(
        "frame_{}_{}.{}",
        processed.frame.monitor_index, timestamp_str, ext
    );

    // Use AppData for captures in production, current directory in development
    let captures_dir = if cfg!(debug_assertions) {
        PathBuf::from("captures")
    } else {
        if let Some(data_dir) = dirs::data_local_dir() {
            data_dir.join("screensearch").join("captures")
        } else {
            PathBuf::from("captures")
        }
    };

    let image_path = captures_dir.join(&image_filename);

    if let Some(parent) = image_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut file = std::fs::File::create(&image_path)?;
    image
        .write_to(&mut file, format)
        .context("Failed to save frame image")?;

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

    for region in processed.ocr_result.regions {
        let ocr_text = NewOcrText {
            frame_id,
            text: region.text.clone(),
            text_json: Some(
                serde_json::json!({
                    "provider": processed.ocr_result.provider.clone(),
                    "language": processed.ocr_result.language.clone(),
                    "orientation_degrees": processed.ocr_result.orientation_degrees,
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

        db.insert_ocr_text(ocr_text).await?;
    }

    Ok(frame_id)
}

fn main() -> Result<()> {
    let config = AppConfig::load().unwrap_or_else(|_| AppConfig::default());
    let _log_guard = init_tracing(&config.logging)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to build Tokio runtime")?;

    let event_loop = EventLoop::new().context("Failed to build EventLoop")?;

    let tray_menu = Menu::new();
    let open_item = MenuItem::new("Open Interface", true, None);
    let quit_item = MenuItem::new("Quit ScreenSearch", true, None);

    tray_menu.append_items(&[&open_item, &PredefinedMenuItem::separator(), &quit_item])?;

    let icon = match image::load_from_memory(include_bytes!("../assets/icon.png")) {
        Ok(img) => {
            let rgba = img.into_rgba8();
            let (width, height) = rgba.dimensions();
            let rgba_vec = rgba.into_raw();
            tray_icon::Icon::from_rgba(rgba_vec, width, height).unwrap_or_else(|_| {
                // Fallback to white square if dimensions invalid
                tray_icon::Icon::from_rgba(vec![255u8; 4 * 32 * 32], 32, 32).unwrap()
            })
        }
        Err(e) => {
            error!("Failed to load embedded tray icon: {}", e);
            // Fallback to white square
            tray_icon::Icon::from_rgba(vec![255u8; 4 * 32 * 32], 32, 32).unwrap()
        }
    };

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("ScreenSearch")
        .with_icon(icon)
        .build()
        .context("Failed to build TrayIcon")?;

    let app = App::new(config.clone());
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel(1);

    // Start app in background thread
    let app_task = std::thread::spawn(move || {
        runtime.block_on(async move {
            if let Err(e) = app.run_with_signal(shutdown_rx).await {
                error!("App error: {}", e);
            }
        });
    });

    // Get event channels
    let menu_channel = tray_icon::menu::MenuEvent::receiver();
    let tray_channel = tray_icon::TrayIconEvent::receiver();
    let api_url = format!("http://{}:{}", config.api.host, config.api.port);

    // Create event loop state
    let mut event_loop_state = EventLoopState {
        _tray_icon: tray_icon,
        menu_items: (open_item, quit_item),
        menu_channel,
        tray_channel,
        app_task,
        shutdown_tx,
        api_url,
    };

    info!("System Tray initialized. Running event loop...");

    // Use new ApplicationHandler API
    event_loop.run_app(&mut event_loop_state)?;

    Ok(())
}
