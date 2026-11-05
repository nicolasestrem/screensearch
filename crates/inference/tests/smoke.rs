//! Real end-to-end sidecar smoke (`03 §6/§10/§13.5`, DoD #5). These are **`#[ignore]`d**
//! because they download multi-GB GGUF models and run a real `llama-server` on a GPU —
//! the gated manual verification, not part of the always-green suite (the lifecycle and
//! provider logic are covered deterministically by the other tests).
//!
//! Run manually on a machine with a Vulkan GPU:
//! ```text
//! cargo test -p inference --test smoke -- --ignored --nocapture
//! ```
//! Set `SSV2C_SMOKE_DIR` to reuse a download cache across runs (default: a temp dir).

#![cfg(windows)]

use std::path::PathBuf;
use std::time::Duration;

use inference::download;
use inference::models::{ModelLane, ModelTier};
use inference::{AnswerSidecar, ModelDownloader, ModelSupervisor, SupervisorConfig, VisionSidecar};
use tokio::sync::mpsc;
use traits::{
    AnswerDelta, AnswerOpts, FlashAttnSetting, KvCacheType, RetrievedChunk, SidecarParams,
};

/// Default-tuned launch params for the smoke runs (auto context, q8_0 KV, auto flash) —
/// exercises the real `build_args` tuning path against the live binary.
fn smoke_params() -> SidecarParams {
    SidecarParams {
        ngl: 99,
        device: None,
        ctx_size: 0,
        kv_cache_type: KvCacheType::Q8_0,
        flash_attn: FlashAttnSetting::Auto,
    }
}

fn smoke_dir() -> PathBuf {
    std::env::var("SSV2C_SMOKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("ssv2c-smoke"))
}

fn supervisor_for(
    binary: PathBuf,
    sidecar_dir: &std::path::Path,
) -> std::sync::Arc<ModelSupervisor> {
    let mut reap_binaries = download::installed_binary_candidates(sidecar_dir);
    reap_binaries.push(binary.clone());
    let caps = inference::probe_caps(&binary);
    ModelSupervisor::new(SupervisorConfig {
        binary,
        reap_binaries,
        pidfile: sidecar_dir.join("llama-server.pid"),
        sidecar_log: Some(sidecar_dir.join("llama-server.log")),
        idle_ttl: Duration::from_secs(60),
        // First-run model load + GPU warmup can be slow; allow plenty of headroom.
        health_timeout: Duration::from_secs(600),
        caps,
        recycle_ceiling_bytes: 0,
    })
    .expect("build supervisor")
}

/// Downloads the default answer model and streams a real, grounded answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads a multi-GB model and runs a real llama-server on a GPU"]
async fn real_answer_streams_tokens() {
    let dir = smoke_dir();
    let sidecar_dir = dir.join("sidecar");
    let models_root = dir.join("models");
    std::fs::create_dir_all(&sidecar_dir).unwrap();

    let binary = download::ensure_binary(&sidecar_dir)
        .await
        .expect("ensure llama-server binary");
    download::ensure_model(&models_root, ModelLane::Answer, ModelTier::Default)
        .await
        .expect("ensure answer model");

    let supervisor = supervisor_for(binary, &sidecar_dir);
    let downloader = ModelDownloader::new(models_root.clone());
    let answer = AnswerSidecar::new(
        supervisor.clone(),
        downloader,
        models_root,
        ModelTier::Default,
        smoke_params(),
    );

    let context = vec![RetrievedChunk {
        frame_id: 42,
        text: "The deploy finished at 14:32 and all CI checks passed.".to_string(),
        score: 1.0,
        captured_at: 0,
        session: None,
    }];
    let (tx, mut rx) = mpsc::channel::<AnswerDelta>(256);
    let task = tokio::spawn(async move {
        use traits::AnswerProvider;
        answer
            .answer(
                "When did the deploy finish?",
                &context,
                AnswerOpts {
                    thinking: true,
                    max_tokens: 256,
                },
                tx,
            )
            .await
    });

    let mut tokens = String::new();
    let mut citations = Vec::new();
    let mut done = false;
    while let Some(delta) = rx.recv().await {
        match delta {
            AnswerDelta::Token { text } => tokens.push_str(&text),
            AnswerDelta::Citation { frame_id } => citations.push(frame_id),
            AnswerDelta::Done { .. } => done = true,
            AnswerDelta::Error { message } => panic!("answer errored: {message}"),
            // Ask v2's aggregated citation event belongs to the kernel pipeline; the
            // provider itself still emits per-frame `Citation` deltas.
            AnswerDelta::Citations { .. } => {}
            AnswerDelta::Thinking { .. } => {}
        }
    }
    task.await.unwrap().unwrap();

    println!("ANSWER: {tokens}\nCITATIONS: {citations:?}");
    assert!(done, "stream must end with Done");
    assert!(!tokens.trim().is_empty(), "must produce answer tokens");
    assert_eq!(citations, vec![42], "must cite the grounding frame");

    supervisor.shutdown().await;
}

/// Downloads the default vision model (+ mmproj) and tags a generated image.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads a multi-GB model + projector and runs a real llama-server on a GPU"]
async fn real_vision_tags_an_image() {
    let dir = smoke_dir();
    let sidecar_dir = dir.join("sidecar");
    let models_root = dir.join("models");
    std::fs::create_dir_all(&sidecar_dir).unwrap();

    let binary = download::ensure_binary(&sidecar_dir)
        .await
        .expect("ensure llama-server binary");
    download::ensure_model(&models_root, ModelLane::Vision, ModelTier::Default)
        .await
        .expect("ensure vision model");

    let supervisor = supervisor_for(binary, &sidecar_dir);
    let downloader = ModelDownloader::new(models_root.clone());
    let vision = VisionSidecar::new(
        supervisor.clone(),
        downloader,
        models_root,
        ModelTier::Default,
        smoke_params(),
    );

    // A simple two-tone image — enough for the model to produce a description.
    let image = image::RgbaImage::from_fn(320, 160, |x, _| {
        if x < 160 {
            image::Rgba([20, 30, 200, 255])
        } else {
            image::Rgba([220, 220, 220, 255])
        }
    });

    use traits::VisionProvider;
    let analysis = vision.analyze(&image).await.expect("vision analyze");
    println!(
        "VISION: {} | activity={:?} | conf={}",
        analysis.description, analysis.activity_type, analysis.confidence
    );
    assert!(
        !analysis.description.trim().is_empty(),
        "must produce a description"
    );

    // Honest output (`07` #20): confidence is either the unknown sentinel (-1.0) or a
    // real score in (0.0, 1.0] — never a fabricated 0.0 echoed from the prompt.
    let conf = analysis.confidence;
    assert!(
        conf == -1.0 || (conf > 0.0 && conf <= 1.0),
        "confidence must be the unknown sentinel or a real (0,1] score, got {conf}"
    );
    // activity_type is either absent or one of the closed label set we asked for — no
    // free-form / "unknown" labels are stored.
    const ALLOWED: &[&str] = &[
        "coding", "browsing", "email", "reading", "chat", "terminal", "design", "video",
    ];
    if let Some(a) = analysis.activity_type.as_deref() {
        assert!(
            ALLOWED.contains(&a),
            "activity_type {a:?} is off the allowed set"
        );
    }

    supervisor.shutdown().await;
}
