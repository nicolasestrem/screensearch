//! LlamaServer Process Manager
//!
//! Auto-manages a llama.cpp server process for local LLM inference.
//! Features:
//! - Lazy startup on first AI request
//! - Automatic shutdown after idle timeout (TTL)
//! - Health monitoring and auto-restart on crash
//! - Port fallback if default port is busy

use crate::download::{get_bin_dir, llama_server_exists};
use crate::error::{LlmError, Result};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, info, warn};

/// Default port for llama-server (avoids common 8080 conflicts)
pub const DEFAULT_LLAMA_PORT: u16 = 31130;

/// Ports to try if default is busy
const PORT_FALLBACKS: [u16; 3] = [31130, 31131, 31132];

/// Default idle timeout before shutting down the server
pub const DEFAULT_IDLE_TTL_SECS: u64 = 300; // 5 minutes

/// Maximum restart attempts before giving up
const MAX_RESTART_ATTEMPTS: u32 = 3;

/// Health check timeout for CPU mode (model loading can take 60-90s on slower systems)
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 120;

/// GPU health-check timeout is scaled by model size: loading a multi-GB model
/// plus a ~1 GB vision projector into VRAM over Vulkan (with first-run shader
/// compilation) routinely takes longer than a fixed short timeout. A timeout
/// that is too aggressive caused a silent, false fallback to CPU — the root of
/// "answer generation does not use the GPU". Base + per-GB, capped.
const GPU_HEALTH_CHECK_BASE_SECS: u64 = 60;
const GPU_HEALTH_CHECK_SECS_PER_GB: u64 = 25;
const GPU_HEALTH_CHECK_MAX_SECS: u64 = 300;

/// Compute the GPU-mode health-check timeout from the on-disk size of the model
/// (and projector, if any). Falls back to the base timeout when sizes are
/// unknown.
fn gpu_health_timeout_secs(config: &LlamaServerConfig) -> u64 {
    let size_of = |p: &std::path::Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let mut bytes = size_of(&config.model_path);
    if let Some(mmproj) = &config.mmproj_path {
        bytes += size_of(mmproj);
    }
    let gb = bytes / 1_000_000_000;
    (GPU_HEALTH_CHECK_BASE_SECS + GPU_HEALTH_CHECK_SECS_PER_GB * gb).min(GPU_HEALTH_CHECK_MAX_SECS)
}

/// Health check poll interval
const HEALTH_CHECK_POLL_MS: u64 = 1000;

/// Server status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerStatus {
    /// Server is not running
    Stopped,
    /// Server is starting up (loading model)
    Starting,
    /// Server is running and ready for requests
    Running,
    /// Server encountered an error
    Error(String),
}

impl ServerStatus {
    pub fn as_str(&self) -> &str {
        match self {
            ServerStatus::Stopped => "stopped",
            ServerStatus::Starting => "starting",
            ServerStatus::Running => "running",
            ServerStatus::Error(_) => "error",
        }
    }
}

/// Configuration for the LlamaServer
#[derive(Debug, Clone)]
pub struct LlamaServerConfig {
    /// Path to the GGUF model file
    pub model_path: PathBuf,
    /// Optional multimodal projector (mmproj) GGUF. When set, the server is
    /// started with `--mmproj`, enabling vision input (image_url) for models
    /// like Gemma 4. `None` = text-only server.
    pub mmproj_path: Option<PathBuf>,
    /// Port to listen on
    pub port: u16,
    /// Host to bind to (always 127.0.0.1 for security)
    pub host: String,
    /// Idle timeout before shutdown
    pub idle_ttl: Duration,
    /// Number of threads for inference (0 = auto)
    pub threads: usize,
    /// Context length
    pub context_length: usize,
    /// Enable GPU acceleration
    pub use_gpu: bool,
    /// Number of layers to offload to GPU (0 = none, 99 = all)
    /// Systems with limited VRAM may need lower values
    pub gpu_layers: u32,
}

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            mmproj_path: None,
            port: DEFAULT_LLAMA_PORT,
            host: "127.0.0.1".to_string(),
            idle_ttl: Duration::from_secs(DEFAULT_IDLE_TTL_SECS),
            threads: 0,
            context_length: 8192,
            use_gpu: true,
            gpu_layers: 99, // Offload all layers by default
        }
    }
}

/// Manages the llama.cpp server lifecycle
pub struct LlamaServer {
    /// Current child process (if running)
    child: Mutex<Option<Child>>,
    /// Configuration
    config: RwLock<LlamaServerConfig>,
    /// Current status
    status: RwLock<ServerStatus>,
    /// Active port (may differ from config if fallback was used)
    active_port: RwLock<u16>,
    /// Last request timestamp for TTL tracking
    last_request: RwLock<Instant>,
    /// Number of restart attempts
    restart_count: RwLock<u32>,
    /// Which acceleration mode the running server actually started in:
    /// `Some(true)` = GPU (Vulkan), `Some(false)` = CPU fallback, `None` = not
    /// running. Surfaced to the UI so a silent CPU fallback is visible.
    gpu_active: RwLock<Option<bool>>,
    /// Flag to signal shutdown
    shutting_down: AtomicBool,
    /// Lock to serialize start attempts (prevents race condition)
    starting_lock: tokio::sync::Mutex<()>,
    /// HTTP client for health checks
    client: reqwest::Client,
}

impl LlamaServer {
    /// Create a new LlamaServer instance
    pub fn new(config: LlamaServerConfig) -> Self {
        let port = config.port;
        Self {
            child: Mutex::new(None),
            config: RwLock::new(config),
            status: RwLock::new(ServerStatus::Stopped),
            active_port: RwLock::new(port),
            last_request: RwLock::new(Instant::now()),
            restart_count: RwLock::new(0),
            gpu_active: RwLock::new(None),
            shutting_down: AtomicBool::new(false),
            starting_lock: tokio::sync::Mutex::new(()),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Get the current server status
    pub async fn status(&self) -> ServerStatus {
        self.status.read().await.clone()
    }

    /// Get the active port (the port the server is actually listening on)
    pub async fn active_port(&self) -> u16 {
        *self.active_port.read().await
    }

    /// Get the endpoint URL for the running server
    pub async fn endpoint(&self) -> String {
        let port = self.active_port().await;
        format!("http://127.0.0.1:{}", port)
    }

    /// Get idle duration (time since last request)
    pub async fn idle_duration(&self) -> Duration {
        self.last_request.read().await.elapsed()
    }

    /// Update the last request timestamp (call this on each inference request)
    pub async fn touch(&self) {
        *self.last_request.write().await = Instant::now();
    }

    /// Update the configuration (requires restart to take effect)
    pub async fn update_config(&self, config: LlamaServerConfig) {
        *self.config.write().await = config;
    }

    /// The model + optional projector this server is configured to run. Used to
    /// decide whether a cached server can be reused or must be rebuilt (e.g.
    /// when vision is toggled and the unified model/projector changes).
    pub async fn current_models(&self) -> (PathBuf, Option<PathBuf>) {
        let config = self.config.read().await;
        (config.model_path.clone(), config.mmproj_path.clone())
    }

    /// Update idle TTL
    pub async fn set_idle_ttl(&self, ttl: Duration) {
        self.config.write().await.idle_ttl = ttl;
    }

    /// Which acceleration mode the running server started in: `Some(true)` =
    /// GPU (Vulkan), `Some(false)` = CPU, `None` = not currently running.
    pub async fn gpu_active(&self) -> Option<bool> {
        *self.gpu_active.read().await
    }

    /// Start the server if not already running.
    /// Uses a lock to serialize concurrent start attempts.
    pub async fn ensure_started(&self) -> Result<()> {
        // Acquire starting lock to prevent race conditions between concurrent requests
        let _lock = self.starting_lock.lock().await;

        // Check if already running (after acquiring lock)
        let status = self.status().await;
        if status == ServerStatus::Running {
            self.touch().await;
            return Ok(());
        }

        // Check if starting (shouldn't happen with lock, but defensive)
        if status == ServerStatus::Starting {
            // Wait for startup to complete
            return self.wait_for_ready().await;
        }

        // Start the server
        self.start().await
    }

    /// Start the server
    pub async fn start(&self) -> Result<()> {
        if self.shutting_down.load(Ordering::SeqCst) {
            return Err(LlmError::ModelInitError(
                "Server is shutting down".to_string(),
            ));
        }

        // Set status to starting
        *self.status.write().await = ServerStatus::Starting;

        // Find llama-server binary
        let bin_dir = get_bin_dir();
        if !llama_server_exists(&bin_dir) {
            let err = "llama-server not found. Download required.".to_string();
            *self.status.write().await = ServerStatus::Error(err.clone());
            return Err(LlmError::ModelNotFound(err));
        }

        let llama_server_path = bin_dir.join(crate::download::LLAMA_SERVER_FILENAME);
        let config = self.config.read().await.clone();

        // Verify model exists
        if !config.model_path.exists() {
            let err = format!("Model not found at {:?}", config.model_path);
            *self.status.write().await = ServerStatus::Error(err.clone());
            return Err(LlmError::ModelNotFound(err));
        }

        // Find an available port
        let port = self.find_available_port().await?;
        *self.active_port.write().await = port;

        // Try GPU mode first (if enabled), fall back to CPU if it fails
        if config.use_gpu {
            info!(
                "Starting llama-server on port {} with GPU acceleration (Vulkan)",
                port
            );

            match self
                .start_with_mode(&llama_server_path, &config, port, true)
                .await
            {
                Ok(()) => {
                    info!(
                        "llama-server started successfully with GPU on port {}",
                        port
                    );
                    *self.gpu_active.write().await = Some(true);
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        "GPU mode failed ({}), falling back to CPU-only mode. \
                        This is common in VirtualBox or systems without Vulkan support.",
                        e
                    );
                    // GPU failed, try CPU mode
                }
            }
        }

        // Start in CPU-only mode
        info!("Starting llama-server on port {} in CPU-only mode", port);

        match self
            .start_with_mode(&llama_server_path, &config, port, false)
            .await
        {
            Ok(()) => {
                info!(
                    "llama-server started successfully with CPU on port {}",
                    port
                );
                *self.gpu_active.write().await = Some(false);
                Ok(())
            }
            Err(e) => {
                error!("llama-server failed to start in CPU mode: {}", e);
                *self.status.write().await = ServerStatus::Error(e.to_string());
                Err(e)
            }
        }
    }

    /// Start the server with a specific mode (GPU or CPU)
    async fn start_with_mode(
        &self,
        llama_server_path: &std::path::Path,
        config: &LlamaServerConfig,
        port: u16,
        use_gpu: bool,
    ) -> Result<()> {
        // Build command
        let mut cmd = Command::new(llama_server_path);
        cmd.arg("-m")
            .arg(&config.model_path)
            .arg("--port")
            .arg(port.to_string())
            .arg("--host")
            .arg(&config.host)
            .arg("-c")
            .arg(config.context_length.to_string())
            // Use each model's embedded chat template (Jinja). Required for
            // correct formatting of modern instruct models and to surface
            // reasoning/"thinking" output from models like Qwen3.5 / Gemma 4.
            .arg("--jinja");

        // Load a multimodal projector when configured so the same server can
        // also answer vision (image_url) requests (Option B: one unified
        // gemma-4 server serves both text and vision).
        if let Some(mmproj) = &config.mmproj_path {
            info!("Enabling vision: loading mmproj projector {:?}", mmproj);
            cmd.arg("--mmproj").arg(mmproj);
        }

        // Add GPU flag if enabled (use configured number of layers)
        if use_gpu {
            cmd.arg("-ngl").arg(config.gpu_layers.to_string());
        }

        // Add threads if specified
        if config.threads > 0 {
            cmd.arg("-t").arg(config.threads.to_string());
        }

        // Capture llama-server's stdout/stderr to a log file so Vulkan init,
        // the GPU layer-offload summary, and any silent GPU→CPU fallback are
        // observable (previously sent to /dev/null, which hid all of it).
        //
        // Open in *append* mode (not truncate): when GPU mode fails and CPU mode
        // is retried, this runs twice — truncating would wipe the GPU failure
        // output that explains the fallback. A header marks each attempt. Falls
        // back to discarding output only if the log file can't be opened.
        let log_path = get_bin_dir().join("llama-server.log");
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(mut file) => {
                use std::io::Write;
                let _ = writeln!(
                    file,
                    "\n=== llama-server start: {} mode (port {}) ===",
                    if use_gpu { "GPU (Vulkan)" } else { "CPU" },
                    port
                );
                match file.try_clone() {
                    Ok(err_file) => {
                        cmd.stdout(Stdio::from(file));
                        cmd.stderr(Stdio::from(err_file));
                    }
                    // Couldn't duplicate the handle: keep stdout on the log,
                    // discard stderr rather than panicking.
                    Err(_) => {
                        cmd.stdout(Stdio::from(file));
                        cmd.stderr(Stdio::null());
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Could not open llama-server log at {:?} ({}); discarding output",
                    log_path, e
                );
                cmd.stdout(Stdio::null());
                cmd.stderr(Stdio::null());
            }
        }

        // Spawn the process
        let child = cmd.spawn().map_err(|e| {
            // Check for missing DLL errors on Windows
            #[cfg(windows)]
            {
                // Windows error codes for missing DLL:
                // 0xc0000135 = STATUS_DLL_NOT_FOUND
                // 0x7e = ERROR_MOD_NOT_FOUND (126)
                let raw_error = e.raw_os_error();
                if raw_error == Some(126) {
                    return LlmError::MissingDependency(
                        "VCRUNTIME140.dll or MSVCP140.dll not found. \
                         The Visual C++ 2015-2022 Redistributable is required for AI features"
                            .to_string(),
                    );
                }
            }

            let err = format!("Failed to spawn llama-server: {}", e);
            LlmError::ModelInitError(err)
        })?;

        *self.child.lock().await = Some(child);

        // GPU mode: scale the timeout by model size so a large model loading
        // into VRAM isn't mistaken for a GPU failure and bounced to CPU.
        let timeout_secs = if use_gpu {
            gpu_health_timeout_secs(config)
        } else {
            HEALTH_CHECK_TIMEOUT_SECS
        };

        // Wait for server to be ready
        match self.wait_for_health_with_timeout(port, timeout_secs).await {
            Ok(()) => {
                *self.status.write().await = ServerStatus::Running;
                *self.restart_count.write().await = 0;
                self.touch().await;
                Ok(())
            }
            Err(e) => {
                // Check if process exited immediately (could be DLL error)
                #[cfg(windows)]
                {
                    let mut child_guard = self.child.lock().await;
                    if let Some(ref mut child) = *child_guard {
                        if let Ok(Some(exit_status)) = child.try_wait() {
                            // Windows exit code 0xc0000135 = -1073741515 = STATUS_DLL_NOT_FOUND
                            if let Some(code) = exit_status.code() {
                                if code == -1073741515 || code == 0xc0000135u32 as i32 {
                                    child_guard.take();
                                    *self.status.write().await = ServerStatus::Error(
                                        "Missing Visual C++ Runtime".to_string(),
                                    );
                                    return Err(LlmError::MissingDependency(
                                        "VCRUNTIME140.dll not found. Install Visual C++ 2015-2022 Redistributable".to_string()
                                    ));
                                }
                            }
                        }
                    }
                }

                // Kill the process if health check failed
                self.force_stop().await;
                Err(e)
            }
        }
    }

    /// Stop the server gracefully
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping llama-server...");

        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            // Try graceful shutdown first
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;
                if let Some(pid) = child.id() {
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
            }

            #[cfg(windows)]
            {
                // On Windows, just kill it
                let _ = child.kill().await;
            }

            // Wait for process to exit
            let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;

            // Force kill if still running
            let _ = child.kill().await;
        }

        *self.status.write().await = ServerStatus::Stopped;
        *self.gpu_active.write().await = None;
        info!("llama-server stopped");
        Ok(())
    }

    /// Force stop without waiting
    async fn force_stop(&self) {
        let mut child_guard = self.child.lock().await;
        if let Some(mut child) = child_guard.take() {
            let _ = child.kill().await;
        }
        *self.status.write().await = ServerStatus::Stopped;
        *self.gpu_active.write().await = None;
    }

    /// Shutdown the server and prevent restarts
    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        let _ = self.stop().await;
    }

    /// Check if the server should be stopped due to idle timeout
    pub async fn check_idle_timeout(&self) -> bool {
        let status = self.status().await;
        if status != ServerStatus::Running {
            return false;
        }

        let config = self.config.read().await;
        let idle_duration = self.idle_duration().await;

        if idle_duration > config.idle_ttl {
            info!(
                "Server idle for {:?}, exceeds TTL of {:?}. Shutting down.",
                idle_duration, config.idle_ttl
            );
            let _ = self.stop().await;
            return true;
        }

        false
    }

    /// Attempt to restart the server after a crash
    pub async fn handle_crash(&self) -> Result<()> {
        let mut restart_count = self.restart_count.write().await;

        if *restart_count >= MAX_RESTART_ATTEMPTS {
            let err = format!("Server crashed {} times, giving up", MAX_RESTART_ATTEMPTS);
            *self.status.write().await = ServerStatus::Error(err.clone());
            return Err(LlmError::ModelInitError(err));
        }

        *restart_count += 1;
        warn!(
            "Server crashed, attempting restart ({}/{})",
            *restart_count, MAX_RESTART_ATTEMPTS
        );

        // Clear the dead child process
        *self.child.lock().await = None;

        // Wait a bit before restarting
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Try to start again
        drop(restart_count); // Release lock before calling start
        self.start().await
    }

    /// Wait for the server to become ready
    async fn wait_for_ready(&self) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(HEALTH_CHECK_TIMEOUT_SECS);

        while start.elapsed() < timeout {
            let status = self.status().await;
            match status {
                ServerStatus::Running => return Ok(()),
                ServerStatus::Error(e) => return Err(LlmError::ModelInitError(e)),
                ServerStatus::Stopped => {
                    return Err(LlmError::ModelInitError(
                        "Server stopped unexpectedly".to_string(),
                    ))
                }
                ServerStatus::Starting => {
                    tokio::time::sleep(Duration::from_millis(HEALTH_CHECK_POLL_MS)).await;
                }
            }
        }

        Err(LlmError::Timeout(HEALTH_CHECK_TIMEOUT_SECS))
    }

    /// Wait for server health endpoint to respond using a specific timeout.
    ///
    /// # Arguments
    ///
    /// * `port` - The port to check health on
    /// * `timeout_secs` - Maximum time to wait in seconds before returning error
    ///
    /// # Errors
    ///
    /// Returns `LlmError::Timeout` if server doesn't respond within `timeout_secs`.
    /// Returns `LlmError::ModelInitError` if the server process exits unexpectedly.
    async fn wait_for_health_with_timeout(&self, port: u16, timeout_secs: u64) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let health_url = format!("http://127.0.0.1:{}/health", port);

        info!(
            "Waiting for llama-server health check at {} (timeout: {}s)",
            health_url, timeout_secs
        );

        let mut attempt = 0;

        while start.elapsed() < timeout {
            attempt += 1;
            match self.client.get(&health_url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        info!(
                            "llama-server health check passed after {:?} ({} attempts)",
                            start.elapsed(),
                            attempt
                        );
                        return Ok(());
                    } else if status.as_u16() == 503 {
                        // 503 = model still loading, this is expected
                        if attempt % 10 == 1 {
                            info!(
                                "llama-server loading model... (attempt {}, elapsed: {:?})",
                                attempt,
                                start.elapsed()
                            );
                        }
                    } else {
                        warn!(
                            "Health check returned unexpected status {}, retrying...",
                            status
                        );
                    }
                }
                Err(e) => {
                    if attempt % 10 == 1 {
                        debug!("Health check connection error (attempt {}): {}", attempt, e);
                    }
                }
            }

            // Check if process crashed
            let mut child_guard = self.child.lock().await;
            if let Some(ref mut child) = *child_guard {
                match child.try_wait() {
                    Ok(Some(exit_status)) => {
                        error!(
                            "llama-server process exited unexpectedly with status: {}",
                            exit_status
                        );
                        return Err(LlmError::ModelInitError(format!(
                            "Server process exited with status: {}",
                            exit_status
                        )));
                    }
                    Ok(None) => {
                        // Still running, continue waiting
                    }
                    Err(e) => {
                        return Err(LlmError::ModelInitError(format!(
                            "Failed to check process status: {}",
                            e
                        )));
                    }
                }
            }
            drop(child_guard);

            tokio::time::sleep(Duration::from_millis(HEALTH_CHECK_POLL_MS)).await;
        }

        error!(
            "llama-server health check timed out after {}s ({} attempts)",
            timeout_secs, attempt
        );
        Err(LlmError::Timeout(timeout_secs))
    }
    async fn find_available_port(&self) -> Result<u16> {
        let config = self.config.read().await;
        let preferred_port = config.port;
        drop(config);

        // Try preferred port first
        if self.is_port_available(preferred_port).await {
            return Ok(preferred_port);
        }

        // Try fallback ports
        for &port in &PORT_FALLBACKS {
            if port != preferred_port && self.is_port_available(port).await {
                warn!("Port {} busy, using fallback port {}", preferred_port, port);
                return Ok(port);
            }
        }

        Err(LlmError::ModelInitError(format!(
            "No available ports. Tried: {:?}",
            PORT_FALLBACKS
        )))
    }

    /// Check if a port is available
    async fn is_port_available(&self, port: u16) -> bool {
        use tokio::net::TcpListener;
        TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
    }

    /// Check if the server process is still running
    pub async fn is_process_alive(&self) -> bool {
        let mut child_guard = self.child.lock().await;
        if let Some(ref mut child) = *child_guard {
            match child.try_wait() {
                Ok(Some(_)) => false, // Process exited
                Ok(None) => true,     // Still running
                Err(_) => false,      // Error checking status
            }
        } else {
            false
        }
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        // Attempt synchronous cleanup
        // Note: This is best-effort since we can't do async in Drop
        if let Ok(mut child) = self.child.try_lock() {
            if let Some(mut c) = child.take() {
                let _ = c.start_kill();
            }
        }
    }
}

/// Spawns a background task to monitor server health and handle TTL
pub fn spawn_server_monitor(server: Arc<LlamaServer>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let check_interval = Duration::from_secs(30);

        loop {
            tokio::time::sleep(check_interval).await;

            // Check if server should be stopped due to idle
            if server.check_idle_timeout().await {
                continue;
            }

            // Check if server process crashed
            let status = server.status().await;
            if status == ServerStatus::Running && !server.is_process_alive().await {
                warn!("Server process crashed, attempting restart...");
                if let Err(e) = server.handle_crash().await {
                    error!("Failed to restart server: {}", e);
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = LlamaServerConfig::default();
        assert_eq!(config.port, DEFAULT_LLAMA_PORT);
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.idle_ttl, Duration::from_secs(DEFAULT_IDLE_TTL_SECS));
    }

    #[test]
    fn test_server_status_as_str() {
        assert_eq!(ServerStatus::Stopped.as_str(), "stopped");
        assert_eq!(ServerStatus::Starting.as_str(), "starting");
        assert_eq!(ServerStatus::Running.as_str(), "running");
        assert_eq!(ServerStatus::Error("test".to_string()).as_str(), "error");
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = LlamaServerConfig::default();
        let server = LlamaServer::new(config);

        assert_eq!(server.status().await, ServerStatus::Stopped);
        assert_eq!(server.active_port().await, DEFAULT_LLAMA_PORT);
    }

    #[tokio::test]
    async fn test_idle_tracking() {
        let config = LlamaServerConfig::default();
        let server = LlamaServer::new(config);

        server.touch().await;
        let idle1 = server.idle_duration().await;

        tokio::time::sleep(Duration::from_millis(100)).await;

        let idle2 = server.idle_duration().await;
        assert!(idle2 > idle1);
    }
    #[tokio::test]
    async fn test_ensure_started_idempotency() {
        let config = LlamaServerConfig::default();
        let server = LlamaServer::new(config);

        // Manually set status to running
        *server.status.write().await = ServerStatus::Running;

        // ensure_started should return Ok immediately without trying to start
        let result = server.ensure_started().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_for_health_timeout() {
        let config = LlamaServerConfig::default();
        let server = LlamaServer::new(config);

        // Use a random port that nothing is listening on
        let port = 54321;

        // Should timeout quickly (1 second)
        // Note: We use a short timeout for the test
        let start = Instant::now();
        let result = server.wait_for_health_with_timeout(port, 1).await;
        let duration = start.elapsed();

        match result {
            Err(LlmError::Timeout(secs)) => assert_eq!(secs, 1),
            _ => panic!("Expected timeout error, got {:?}", result),
        }

        // Verify it took at least 1 second
        assert!(duration >= Duration::from_secs(1));
    }
}
