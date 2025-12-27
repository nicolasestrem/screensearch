use anyhow::{Context, Result};
use screensearch_db::{DatabaseManager, models::FrameAnalysisUpdate};
use screensearch_vision::{client::OllamaClient, VisionModel};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{error, info, debug};

/// Spawn the vision analysis worker
pub fn spawn_vision_worker(
    db: Arc<DatabaseManager>,
) {
    tokio::spawn(async move {
        info!("Vision worker started");

        let mut current_provider = String::new();
        let mut current_model = String::new();
        let mut current_endpoint = String::new();
        let mut client: Option<Arc<dyn VisionModel>> = None;

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

            // Check if config changed
            let config_changed = settings.vision_provider != current_provider ||
                                 settings.vision_model != current_model ||
                                 settings.vision_endpoint != current_endpoint ||
                                 client.is_none();

            if config_changed {
                info!("Vision settings changed, updating client: provider={}, model={}", settings.vision_provider, settings.vision_model);
                
                // create new client
                let new_client: Arc<dyn VisionModel> = if settings.vision_provider == "ollama" {
                    Arc::new(OllamaClient::new(
                        settings.vision_endpoint.clone(),
                        settings.vision_model.clone(),
                        settings.vision_api_key.clone(),
                        settings.vision_provider.clone(),
                    ))
                } else {
                     // TODO: Local model support
                     // For now default to Ollama if unknown or "local" unimplemented
                     if settings.vision_provider == "local" {
                        // error!("Local model not fully implemented, falling back to Ollama stub or error");
                        // For now we don't have LocalClient implemented fully, so we might want to warn
                        // But let's stick to Ollama for the prototype as verified.
                     }
                     
                     Arc::new(OllamaClient::new(
                        settings.vision_endpoint.clone(),
                        settings.vision_model.clone(),
                        settings.vision_api_key.clone(),
                        settings.vision_provider.clone(),
                    ))
                };

                client = Some(new_client);
                current_provider = settings.vision_provider;
                current_model = settings.vision_model;
                current_endpoint = settings.vision_endpoint;
            }

            if let Some(c) = &client {
                match process_next_item(&db, c).await {
                    Ok(did_work) => {
                        if !did_work {
                            sleep(Duration::from_secs(1)).await;
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

async fn process_next_item(
    db: &DatabaseManager,
    client: &Arc<dyn VisionModel>,
) -> Result<bool> {
    // 1. Claim task
    let task = db.claim_analysis_task("worker-1").await?;
    
    if let Some(task) = task {
        debug!("Processing analysis task id: {} for frame: {}", task.id, task.frame_id);
        
        // 2. Fetch frame data (image path)
        let frame = db.get_frame(task.frame_id).await?
            .context("Frame not found for analysis task")?;
            
        // 3. Load image
        let image = image::open(&frame.file_path)
            .context(format!("Failed to open image at {}", frame.file_path))?; // map_err?

        // 4. Analyze
        let context = format!(
            "App: {}, Window: {}", 
            frame.active_process.unwrap_or_default(), 
            frame.active_window.unwrap_or_default()
        );

        match client.analyze(&image, &context).await {
            Ok(analysis) => {
                // 5. Update success
                let update = FrameAnalysisUpdate {
                    description: Some(analysis.description),
                    visible_text_json: Some(serde_json::to_string(&analysis.visible_text)?),
                    activity_type: Some(analysis.activity_type),
                    app_hint: analysis.app_hint,
                    confidence: Some(analysis.confidence),
                    analysis_time_ms: Some(0), // Measure time?
                };
                
                db.complete_analysis_task(task.id, task.frame_id, update).await?;
                info!("Analysis completed for frame {}", task.frame_id);
            },
            Err(e) => {
                // 6. Update failure
                error!("Analysis failed for frame {}: {}", task.frame_id, e);
                db.fail_analysis_task(task.id, task.frame_id, e.to_string()).await?;
            }
        }
        
        Ok(true)
    } else {
        Ok(false)
    }
}
