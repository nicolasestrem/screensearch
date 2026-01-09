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

/// Health check timeout (model loading can take 60-90s on slower systems)
const HEALTH_CHECK_TIMEOUT_SECS: u64 = 120;

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
}

impl Default for LlamaServerConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            port: DEFAULT_LLAMA_PORT,
            host: "127.0.0.1".to_string(),
            idle_ttl: Duration::from_secs(DEFAULT_IDLE_TTL_SECS),
            threads: 0,
            context_length: 8192,
            use_gpu: true,
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
    /// Flag to signal shutdown
    shutting_down: AtomicBool,
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
            shutting_down: AtomicBool::new(false),
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

    /// Update idle TTL
    pub async fn set_idle_ttl(&self, ttl: Duration) {
        self.config.write().await.idle_ttl = ttl;
    }

    /// Start the server if not already running
    pub async fn ensure_started(&self) -> Result<()> {
        // Check if already running
        let status = self.status().await;
        if status == ServerStatus::Running {
            self.touch().await;
            return Ok(());
        }

        // Check if starting
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
            return Err(LlmError::ModelInitError("Server is shutting down".to_string()));
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
            
            match self.start_with_mode(&llama_server_path, &config, port, true).await {
                Ok(()) => {
                    info!("llama-server started successfully with GPU on port {}", port);
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
        info!(
            "Starting llama-server on port {} in CPU-only mode",
            port
        );
        
        match self.start_with_mode(&llama_server_path, &config, port, false).await {
            Ok(()) => {
                info!("llama-server started successfully with CPU on port {}", port);
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
            .arg(config.context_length.to_string());

        // Add GPU flag if enabled
        if use_gpu {
            cmd.arg("-ngl").arg("99"); // Offload all layers to GPU
        }

        // Add threads if specified
        if config.threads > 0 {
            cmd.arg("-t").arg(config.threads.to_string());
        }

        // Redirect stdout/stderr to null to avoid blocking
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        // Spawn the process
        let child = cmd.spawn().map_err(|e| {
            let err = format!("Failed to spawn llama-server: {}", e);
            LlmError::ModelInitError(err)
        })?;

        *self.child.lock().await = Some(child);

        // Use shorter timeout for GPU mode to allow faster fallback (45s vs 120s)
        let timeout_secs = if use_gpu { 45 } else { HEALTH_CHECK_TIMEOUT_SECS };

        // Wait for server to be ready
        match self.wait_for_health_with_timeout(port, timeout_secs).await {
            Ok(()) => {
                *self.status.write().await = ServerStatus::Running;
                *self.restart_count.write().await = 0;
                self.touch().await;
                Ok(())
            }
            Err(e) => {
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
            let err = format!(
                "Server crashed {} times, giving up",
                MAX_RESTART_ATTEMPTS
            );
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
                    return Err(LlmError::ModelInitError("Server stopped unexpectedly".to_string()))
                }
                ServerStatus::Starting => {
                    tokio::time::sleep(Duration::from_millis(HEALTH_CHECK_POLL_MS)).await;
                }
            }
        }

        Err(LlmError::Timeout(HEALTH_CHECK_TIMEOUT_SECS))
    }


    /// Wait for server health endpoint to respond with a custom timeout
    async fn wait_for_health_with_timeout(&self, port: u16, timeout_secs: u64) -> Result<()> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let health_url = format!("http://127.0.0.1:{}/health", port);

        info!("Waiting for llama-server health check at {} (timeout: {}s)", health_url, timeout_secs);
        
        let mut attempt = 0;

        while start.elapsed() < timeout {
            attempt += 1;
            match self.client.get(&health_url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        info!("llama-server health check passed after {:?} ({} attempts)", start.elapsed(), attempt);
                        return Ok(());
                    } else if status.as_u16() == 503 {
                        // 503 = model still loading, this is expected
                        if attempt % 10 == 1 {
                            info!("llama-server loading model... (attempt {}, elapsed: {:?})", attempt, start.elapsed());
                        }
                    } else {
                        warn!("Health check returned unexpected status {}, retrying...", status);
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
                        error!("llama-server process exited unexpectedly with status: {}", exit_status);
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

        error!("llama-server health check timed out after {}s ({} attempts)", timeout_secs, attempt);
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
                warn!(
                    "Port {} busy, using fallback port {}",
                    preferred_port, port
                );
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
}
