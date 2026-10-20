//! Setup-visibility integration tests (`03 §7g`, approved 2026-10-20).
//!
//! Drives the *real* kernel's `SetupStatus` snapshot through `set_setup_step` and
//! `emit_model_download`, and verifies the acceptance contract: revision increments
//! by exactly one per change, independent operations never overwrite each other, a
//! `failed` step persists until *that* operation reports again, steps stay sorted by
//! `SetupOperation`, the state is applied before the event is broadcast, signed URL
//! query strings are redacted from error text, and `model_download` progress is
//! mirrored into a `vision_model`/`answer_model` step while the `model_download`
//! event itself stays unchanged.

use std::sync::Arc;

use async_trait::async_trait;

use kernel::{CaptureFactory, Kernel, KernelEvent};
use store::SqliteStore;
use traits::{
    CaptureSource, ModelDownloadPhase, ModelDownloadStatus, ModelLane, OcrProvider, OcrResult,
    Readiness, Result, SetupOperation, SetupPhase, SetupStatus, SetupStep,
};

/// Never produces a frame — the setup snapshot needs nothing from the capture loop.
struct NoCapture;
#[async_trait]
impl CaptureSource for NoCapture {
    fn monitors(&self) -> Vec<traits::MonitorInfo> {
        Vec::new()
    }
    async fn next_frame(&mut self) -> Result<Option<traits::CapturedFrame>> {
        Ok(None)
    }
}

/// A no-op OCR provider (the kernel requires one; never consulted here).
struct NoOcr;
#[async_trait]
impl OcrProvider for NoOcr {
    async fn recognize(&self, _f: &traits::CapturedFrame) -> Result<OcrResult> {
        Ok(OcrResult {
            text: String::new(),
            mean_confidence: -1.0,
            engine: "fake".to_string(),
            spans: Vec::new(),
        })
    }
}

/// Builds a real kernel with inert capture/OCR providers: the setup snapshot is a
/// pure kernel-owned state machine, so nothing else is exercised here.
fn test_kernel() -> Arc<Kernel> {
    let store: Arc<dyn traits::Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let factory: CaptureFactory =
        Arc::new(|_c, _rx| Ok(Box::new(NoCapture) as Box<dyn CaptureSource>));
    Arc::new(Kernel::new(
        store,
        Arc::new(NoOcr) as Arc<dyn OcrProvider>,
        factory,
        std::path::PathBuf::from("frames"),
        Readiness::default(),
    ))
}

fn step_(operation: SetupOperation, phase: SetupPhase) -> SetupStep {
    SetupStep {
        operation,
        phase,
        attempt: 1,
        downloaded_bytes: None,
        total_bytes: None,
        error: None,
    }
}

fn failed_engine_step(error: &str) -> SetupStep {
    SetupStep {
        operation: SetupOperation::Engine,
        phase: SetupPhase::Failed,
        attempt: 1,
        downloaded_bytes: None,
        total_bytes: None,
        error: Some(error.to_string()),
    }
}

/// `revision` increases by exactly one on every change, starting from the empty
/// snapshot (`revision == 0`, no steps) (`03 §7g`).
#[tokio::test]
async fn setup_revision_increments_by_one_per_change() {
    let kernel = test_kernel();
    let initial = kernel.setup_status();
    assert_eq!(initial.revision, 0, "fresh snapshot starts at revision 0");
    assert!(initial.steps.is_empty(), "fresh snapshot has no steps");

    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Checking));
    assert_eq!(kernel.setup_status().revision, 1);

    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Downloading));
    assert_eq!(kernel.setup_status().revision, 2);

    kernel.set_setup_step(step_(SetupOperation::Embeddings, SetupPhase::Preparing));
    assert_eq!(
        kernel.setup_status().revision,
        3,
        "a different operation's change also bumps the shared snapshot revision"
    );
}

/// An identical re-report is not a change: the revision stays put and no
/// `setup_changed` event is broadcast (`03 §7g` "increases by one on every change").
#[tokio::test]
async fn setup_identical_re_report_neither_bumps_revision_nor_broadcasts() {
    let kernel = test_kernel();
    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Downloading));
    assert_eq!(kernel.setup_status().revision, 1);

    let mut rx = kernel.subscribe();
    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Downloading));
    assert_eq!(
        kernel.setup_status().revision,
        1,
        "the same step reported twice must not move the revision"
    );
    loop {
        match rx.try_recv() {
            Ok(KernelEvent::SetupChanged(_)) => panic!("an unchanged step must not broadcast"),
            Ok(_) => continue,
            Err(_) => break,
        }
    }
}

/// Every independent operation keeps its own step: a change to one never overwrites
/// another (`03 §7g`).
#[tokio::test]
async fn setup_independent_operations_do_not_overwrite_each_other() {
    let kernel = test_kernel();
    kernel.set_setup_step(SetupStep {
        operation: SetupOperation::Engine,
        phase: SetupPhase::Downloading,
        attempt: 2,
        downloaded_bytes: Some(4_096),
        total_bytes: Some(16_000_000),
        error: None,
    });
    kernel.set_setup_step(step_(SetupOperation::Embeddings, SetupPhase::Preparing));

    let status = kernel.setup_status();
    assert_eq!(
        status.steps.len(),
        2,
        "both operations must present: {status:?}"
    );
    let engine = &status.steps[0];
    assert_eq!(engine.operation, SetupOperation::Engine);
    assert_eq!(engine.phase, SetupPhase::Downloading);
    assert_eq!(engine.attempt, 2, "per-operation attempt is preserved");
    assert_eq!(
        engine.downloaded_bytes,
        Some(4_096),
        "engine bytes preserved"
    );
    assert_eq!(engine.total_bytes, Some(16_000_000));
    let embeddings = &status.steps[1];
    assert_eq!(embeddings.operation, SetupOperation::Embeddings);
    assert_eq!(embeddings.phase, SetupPhase::Preparing);
    assert_eq!(
        embeddings.downloaded_bytes, None,
        "embeddings stays indeterminate"
    );
}

/// A `failed` step persists until that *same* operation reports a new phase — never
/// merely because another operation succeeded (`03 §7g`, failure persistence).
#[tokio::test]
async fn setup_failed_step_persists_after_another_operation_succeeds() {
    let kernel = test_kernel();
    kernel.set_setup_step(failed_engine_step("release query returned an error status"));
    kernel.set_setup_step(step_(SetupOperation::Embeddings, SetupPhase::Done));

    let status = kernel.setup_status();
    let engine = &status.steps[0];
    assert_eq!(
        engine.operation,
        SetupOperation::Engine,
        "the engine step is still present after embeddings reported"
    );
    assert_eq!(
        engine.phase,
        SetupPhase::Failed,
        "another operation's success must not clear this failure: {status:?}"
    );
    assert_eq!(
        engine.error.as_deref(),
        Some("release query returned an error status"),
        "the diagnostic copy stays visible"
    );

    // ...and only the same operation's next report clears/replaces it.
    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Checking));
    let engine = &kernel.setup_status().steps[0];
    assert_eq!(engine.phase, SetupPhase::Checking);
    assert_eq!(engine.error, None, "a non-failed phase carries no error");
}

/// A step for an already-reporting operation replaces its own entry (no duplicates),
/// and the steps stay sorted by `SetupOperation`'s `Ord` regardless of arrival order
/// (`03 §7g`).
#[tokio::test]
async fn setup_replaces_same_operation_and_keeps_steps_sorted() {
    let kernel = test_kernel();
    // Reported deliberately out of order (Answer → Vision → Engine → Embeddings).
    kernel.set_setup_step(step_(SetupOperation::AnswerModel, SetupPhase::Downloading));
    kernel.set_setup_step(step_(SetupOperation::VisionModel, SetupPhase::Downloading));
    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Checking));
    kernel.set_setup_step(step_(SetupOperation::Embeddings, SetupPhase::Preparing));
    // Second report for the first operation: replacement, not a new entry.
    kernel.set_setup_step(step_(SetupOperation::AnswerModel, SetupPhase::Done));

    let status = kernel.setup_status();
    let ops: Vec<SetupOperation> = status.steps.iter().map(|s| s.operation).collect();
    assert_eq!(
        ops,
        vec![
            SetupOperation::Engine,
            SetupOperation::Embeddings,
            SetupOperation::VisionModel,
            SetupOperation::AnswerModel,
        ],
        "steps stay in SetupOperation order: {status:?}"
    );
    assert_eq!(
        status.steps.len(),
        4,
        "replacement must not add a duplicate"
    );
    assert_eq!(status.steps[3].phase, SetupPhase::Done);
}

/// The snapshot is updated **before** the event is broadcast: a subscriber's
/// first `recv()` already carries the same state that `setup_status()` returns
/// (`03 §7g`).
#[tokio::test]
async fn setup_snapshot_is_updated_before_the_event_is_received() {
    let kernel = test_kernel();
    let mut rx = kernel.subscribe();

    kernel.set_setup_step(step_(SetupOperation::Engine, SetupPhase::Verifying));

    let event_snapshot = match rx.recv().await {
        Ok(KernelEvent::SetupChanged(status)) => status,
        other => panic!("expected SetupChanged, got {other:?}"),
    };
    assert_eq!(
        event_snapshot,
        kernel.setup_status(),
        "the event payload and the authoritative snapshot must match exactly"
    );
    assert_eq!(event_snapshot.revision, 1);
    assert_eq!(event_snapshot.steps[0].phase, SetupPhase::Verifying);
}

/// Error text has every URL query string removed — signed CDN parameters
/// (`?X-Amz-Signature=…`, tokens) never reach the UI — while the path and the
/// surrounding diagnostic text are kept (`03 §7g`).
#[tokio::test]
async fn setup_error_redacts_url_query_strings() {
    let kernel = test_kernel();
    kernel.set_setup_step(failed_engine_step(
        "download failed: https://github.com/x/y/releases/download/v1/llama-server.exe\
         ?X-Amz-Signature=abc123&signature=z&token=secret (status 403)",
    ));

    let error = kernel.setup_status().steps[0]
        .error
        .clone()
        .expect("failed step keeps its error");
    assert_eq!(
        error,
        "download failed: \
         https://github.com/x/y/releases/download/v1/llama-server.exe (status 403)",
        "path and surrounding text kept, query dropped entirely: {error:?}"
    );
    assert!(!error.contains("Signature"), "no signed parameter leaks in");
    assert!(!error.contains("token=secret"), "no auth token leaks in");

    // Multiple URLs in one diagnostic string are all redacted.
    kernel.set_setup_step(failed_engine_step(
        "https://a.dev/1/b.exe?sig=1 then fell back to https://b.dev/2/c.exe?token=q",
    ));
    let error = kernel.setup_status().steps[0].error.clone().unwrap();
    assert_eq!(
        error, "https://a.dev/1/b.exe then fell back to https://b.dev/2/c.exe",
        "every URL token redacted: {error:?}"
    );
}

/// `emit_model_download` mirrors the lane's download into the setup snapshot
/// (`vision_model`/`answer_model`, phase-mapped, attempt 1, received bytes into
/// `downloaded_bytes`, `total_bytes` as-is) while the `model_download` event itself
/// is unchanged (`03 §7g`).
#[tokio::test]
async fn setup_model_download_maps_lane_into_a_step() {
    let kernel = test_kernel();
    let mut rx = kernel.subscribe();

    kernel.emit_model_download(ModelDownloadStatus {
        lane: ModelLane::Vision,
        model: Some("mmproj-f16.gguf".to_string()),
        phase: ModelDownloadPhase::Downloading,
        downloaded_bytes: 1_048_576,
        total_bytes: Some(5_242_880),
        error: None,
    });

    // 1st event: the mirrored setup snapshot; 2nd: the unchanged model_download.
    let setup = match rx.recv().await {
        Ok(KernelEvent::SetupChanged(status)) => status,
        other => panic!("expected SetupChanged first, got {other:?}"),
    };
    assert_eq!(setup.steps.len(), 1);
    let step = &setup.steps[0];
    assert_eq!(step.operation, SetupOperation::VisionModel);
    assert_eq!(step.phase, SetupPhase::Downloading);
    assert_eq!(step.attempt, 1, "model lanes always report attempt 1");
    assert_eq!(
        step.downloaded_bytes,
        Some(1_048_576),
        "received bytes map into downloaded_bytes"
    );
    assert_eq!(
        step.total_bytes,
        Some(5_242_880),
        "total_bytes passes through"
    );
    assert_eq!(step.error, None);

    match rx.recv().await {
        Ok(KernelEvent::ModelDownload(status)) => {
            assert_eq!(status.lane, ModelLane::Vision);
            assert_eq!(status.phase, ModelDownloadPhase::Downloading);
            assert_eq!(status.downloaded_bytes, 1_048_576);
        }
        other => panic!("expected the unchanged ModelDownload event, got {other:?}"),
    }

    // The answer lane maps to `answer_model`; a download failure keeps the honest
    // error (any URL in it is redacted in the snapshot, not in the event).
    kernel.emit_model_download(ModelDownloadStatus {
        lane: ModelLane::Answer,
        model: Some("q4.gguf".to_string()),
        phase: ModelDownloadPhase::Failed,
        downloaded_bytes: 0,
        total_bytes: None,
        error: Some("https://cdn.gguf/mx?sig=x not found".to_string()),
    });
    let _ = rx.recv().await; // SetupChanged
    let _ = rx.recv().await; // ModelDownload
    let snapshot = kernel.setup_status();
    assert_eq!(
        snapshot.steps.len(),
        2,
        "both lane steps present: {snapshot:?}"
    );
    let answer = &snapshot.steps[1];
    assert_eq!(answer.operation, SetupOperation::AnswerModel);
    assert_eq!(answer.phase, SetupPhase::Failed);
    assert_eq!(answer.downloaded_bytes, Some(0));
    assert_eq!(answer.total_bytes, None);
    assert_eq!(
        answer.error.as_deref(),
        Some("https://cdn.gguf/mx not found"),
        "the event keeps the raw error; the snapshot redacts its URL query"
    );
}

/// The empty snapshot stays valid (`SetupStatus::default()`), which the shell
/// command falls back to when no kernel exists yet (`03 §7g`).
#[tokio::test]
async fn setup_empty_snapshot_has_revision_zero_and_no_steps() {
    let kernel = test_kernel();
    assert_eq!(kernel.setup_status(), SetupStatus::default());
}
