use crate::state::AppState;
use anyhow::{Context, Result};
use screensearch_db::{models::FrameAnalysisUpdate, DatabaseManager};
use screensearch_vision::{client::OllamaClient, VisionModel};
use std::sync::Arc;
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

/// How many recent un-analyzed frames the throttled auto-enqueuer pulls in per
/// idle cycle. Small so vision analysis trickles through history (newest first)
/// instead of flooding the GPU. On-demand requests via `POST /api/vision/analyze`
/// get a higher priority and jump the queue.
const AUTO_ENQUEUE_BATCH: i64 = 4;

/// Spawn the vision analysis worker.
///
/// When the local provider is selected, the worker drives the same auto-managed
/// llama-server used for AI reports — started with `--mmproj` so a single
/// gemma-4 model serves both text and vision (Option B). For external providers
/// (ollama / OpenAI-compatible) it talks to the configured endpoint.
pub fn spawn_vision_worker(state: Arc<AppState>) {
    tokio::spawn(async move {
        info!("Vision worker started");

        let db = Arc::clone(&state.db);

        let mut current_provider = String::new();
        let mut current_model = String::new();
        let mut current_endpoint = String::new();
        let mut client: Option<Arc<dyn VisionModel>> = None;
        // Throttle the "llama-server not downloaded" warning so it doesn't spam.
        let mut last_missing_warn: Option<Instant> = None;

        loop {
            // Fetch settings
            let settings = match db.get_settings().await {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to fetch settings for vision worker: {}", e);
                    sleep(Duration::from_secs(10)).await;
                    continue;
                }
            };

            if settings.vision_enabled == 0 {
                // Disabled, sleep and check later
                sleep(Duration::from_secs(5)).await;
                continue;
            }

            // Resolve the effective provider / model / endpoint for this cycle.
            // For the local provider we drive the unified llama-server.
            let (provider, model, endpoint, api_key) = if settings.vision_provider == "local" {
                // The vision-capable model is served by the local llama-server,
                // which must be downloaded and current.
                let bin_dir = screensearch_llm::get_bin_dir();
                if !screensearch_llm::llama_server_up_to_date(&bin_dir) {
                    let should_warn = last_missing_warn
                        .map(|t| t.elapsed() > Duration::from_secs(60))
                        .unwrap_or(true);
                    if should_warn {
                        warn!(
                            "Vision enabled (local) but llama-server is not downloaded/current. \
                             Download it from Settings → AI → Download Server."
                        );
                        last_missing_warn = Some(Instant::now());
                    }
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }

                // Get (rebuilding if needed) the unified server and ensure it is
                // running with the vision projector loaded.
                let server = match state.get_llama_server().await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Failed to initialize llama-server for vision: {}", e);
                        sleep(Duration::from_secs(10)).await;
                        continue;
                    }
                };

                let (model_path, mmproj_path) = server.current_models().await;
                if mmproj_path.is_none() {
                    let should_warn = last_missing_warn
                        .map(|t| t.elapsed() > Duration::from_secs(60))
                        .unwrap_or(true);
                    if should_warn {
                        warn!(
                            "Vision enabled (local) but no vision model + mmproj projector was \
                             found in .models/. Drop a gemma-4 model and its *mmproj*.gguf there."
                        );
                        last_missing_warn = Some(Instant::now());
                    }
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }

                if let Err(e) = server.ensure_started().await {
                    error!("Failed to start local llama-server for vision: {}", e);
                    sleep(Duration::from_secs(10)).await;
                    continue;
                }

                let endpoint = server.endpoint().await;
                let model = model_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "local".to_string());

                // provider "local" routes OllamaClient through its OpenAI-compatible
                // (image_url) path, which llama.cpp serves at /v1/chat/completions.
                ("local".to_string(), model, endpoint, None)
            } else {
                (
                    settings.vision_provider.clone(),
                    settings.vision_model.clone(),
                    settings.vision_endpoint.clone(),
                    settings.vision_api_key.clone(),
                )
            };

            // (Re)build the client when the effective config changed.
            let config_changed = provider != current_provider
                || model != current_model
                || endpoint != current_endpoint
                || client.is_none();

            if config_changed {
                info!(
                    "Vision client (re)configured: provider={}, model={}, endpoint={}",
                    provider, model, endpoint
                );
                client = Some(Arc::new(OllamaClient::new(
                    endpoint.clone(),
                    model.clone(),
                    api_key,
                    provider.clone(),
                )));
                current_provider = provider;
                current_model = model;
                current_endpoint = endpoint;
            }

            if let Some(c) = &client {
                match process_next_item(&db, c).await {
                    Ok(true) => {
                        // Did work; loop immediately to drain the queue.
                    }
                    Ok(false) => {
                        // Queue empty: throttled trickle of recent un-analyzed frames.
                        match auto_enqueue(&db).await {
                            Ok(n) if n > 0 => {
                                debug!("Auto-enqueued {} frame(s) for vision analysis", n);
                                // Small pause to keep GPU load bounded between batches.
                                sleep(Duration::from_millis(500)).await;
                            }
                            Ok(_) => {
                                // Nothing left to analyze.
                                sleep(Duration::from_secs(5)).await;
                            }
                            Err(e) => {
                                error!("Auto-enqueue failed: {}", e);
                                sleep(Duration::from_secs(5)).await;
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error in vision worker: {}", e);
                        sleep(Duration::from_secs(5)).await;
                    }
                }
            } else {
                sleep(Duration::from_secs(5)).await;
            }
        }
    });
}

/// Enqueue a throttled batch of recent un-analyzed frames. Returns how many
/// were newly queued.
async fn auto_enqueue(db: &DatabaseManager) -> Result<usize> {
    let ids = db.get_unanalyzed_frame_ids(AUTO_ENQUEUE_BATCH).await?;
    let mut enqueued = 0;
    for id in ids {
        // Priority 0 = background trickle (on-demand requests use higher priority).
        if db.enqueue_frame_for_analysis(id, 0).await? > 0 {
            enqueued += 1;
        }
    }
    Ok(enqueued)
}

async fn process_next_item(db: &DatabaseManager, client: &Arc<dyn VisionModel>) -> Result<bool> {
    // 1. Claim task
    let task = db.claim_analysis_task("worker-1").await?;

    if let Some(task) = task {
        debug!(
            "Processing analysis task id: {} for frame: {}",
            task.id, task.frame_id
        );

        // 2. Fetch frame data (image path)
        let frame = db
            .get_frame(task.frame_id)
            .await?
            .context("Frame not found for analysis task")?;

        // 3. Load image
        let image = image::open(&frame.file_path)
            .context(format!("Failed to open image at {}", frame.file_path))?;

        // 4. Analyze
        let context = format!(
            "App: {}, Window: {}",
            frame.active_process.unwrap_or_default(),
            frame.active_window.unwrap_or_default()
        );

        let started = Instant::now();
        match client.analyze(&image, &context).await {
            Ok(analysis) => {
                // 5. Update success
                let update = FrameAnalysisUpdate {
                    description: Some(analysis.description),
                    visible_text_json: Some(serde_json::to_string(&analysis.visible_text)?),
                    activity_type: Some(analysis.activity_type),
                    app_hint: analysis.app_hint,
                    confidence: Some(analysis.confidence),
                    analysis_time_ms: Some(started.elapsed().as_millis() as i64),
                };

                db.complete_analysis_task(task.id, task.frame_id, update)
                    .await?;
                info!(
                    "Analysis completed for frame {} in {} ms",
                    task.frame_id,
                    started.elapsed().as_millis()
                );
            }
            Err(e) => {
                // 6. Update failure
                error!("Analysis failed for frame {}: {}", task.frame_id, e);
                db.fail_analysis_task(task.id, task.frame_id, e.to_string())
                    .await?;
            }
        }

        Ok(true)
    } else {
        Ok(false)
    }
}
