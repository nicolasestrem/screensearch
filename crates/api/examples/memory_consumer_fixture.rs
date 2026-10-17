//! Real API + SqliteStore fixture runner for the Python reference consumer.
//!
//! Test-process-only control seam required by `docs/MEMORY_CONSUMER.md` and driven by
//! `examples/test_memory_consumer.py` (`RealApiFixtureTests`). This example is **not**
//! part of the app: it never starts capture, never touches the production history, never
//! downloads models and never answers with canned page data. It composes the actual
//! `ApiServer` router over an isolated real `SqliteStore` plus real `kernel` capture
//! observations (capture disabled, schedules off, answer providers unavailable), seeds
//! >1000 frames through the production writers, and speaks the stdin control protocol:
//!
//! * stdout, one line per event: `MEMORY_FIXTURE {json}` — `event: "ready"` once, then
//!   `event: "ack"` per control command (with read/mutation/provider counters), and
//!   `event: "error"` on fatal failure.
//! * stdin, newline JSON commands: `enrich_textless`, `rewind`, `catch_up_high_water`,
//!   `reuse_timestamp`, `stop`.
//!
//! Environment (set by the harness):
//! * `SCREENSEARCH_CONSUMER_FIXTURE_DIR` — fresh private directory for the disposable
//!   fixture DB, its `VACUUM INTO` backup and the fixture log. A non-fresh directory is
//!   refused so a real user database can never be opened by accident.
//! * `SCREENSEARCH_API_TOKEN` — random bearer, installed as the **read** credential of
//!   the running server and never printed, logged or echoed in any event.
//!
//! The `reuse_timestamp` control is a deliberately labeled **unsupported** identity
//! injection (same history, same frame id, different `captured_at`) performed with
//! direct SQL on the disposable fixture database only — the same fixture-only direct-SQL
//! class `crates/store/tests/history_allocator.rs` documents. It is not a production
//! writer and never a continuity guarantee. `rewind` restores the isolated earlier
//! SQLite backup (preserving the history identity and every original frame timestamp)
//! and rebinds the live store behind the same API origin, so coverage observes the
//! decreased allocator high-water.
//!
//! ## Opt-in saved-report seeding (`03 §7f`)
//!
//! Report seeding is **off by default** so every existing frame-only consumer fixture
//! keeps byte-identical `ready`/`ack` behavior. It turns on with the explicit opt-in
//! environment variable `SCREENSEARCH_CONSUMER_FIXTURE_SEED_REPORTS=1` (also `true`,
//! `yes`, `on`), or on demand with the stdin control `seed_reports` (idempotent). Both
//! paths seed the isolated disposable store through the **existing production Store
//! writers only** (`create_report_run`, `update_report_run`, `create_report_delivery`,
//! `save_report_file`, `ensure_report_source_id`); no report schema is invented, no row
//! is generated, and the fixture still starts no scheduler, no `MemoryRuntime` worker,
//! no capture, no model and no Telegram delivery.
//!
//! The seeded set is a fixed, known UTC window with: an ordering tie group plus a
//! `revision_of` link; queued and running runs that already carry an immutable saved
//! output; a finished `NO_DATA` run that keeps its factual saved output; blocked and
//! failed runs with a literal null result; one run whose private error code/message,
//! `config_snapshot`, delivery destination/path and file path are nonprivate **canaries**
//! that the public projection must never serve; a saved result whose complete
//! `frame_ids` provenance union exceeds 200 and references historical frames that no
//! longer exist; and two runs whose windows do not overlap the query window. When the
//! opt-in is on the `ready` event gains **only** safe report ids, counts and booleans —
//! never Markdown, a stored message, a path, a credential or the bearer.
//!
//! The fixture is a pure `ApiServer` + `Kernel` composition: it overrides
//! [`ApiHost::report_page`]/[`ApiHost::report_detail`] with the real Store read methods,
//! so report reads travel the genuine router → host → `SqliteStore` path once the Store
//! read contract is published. The `report_state` control snapshots the stored
//! report/delivery/file state through the existing Store reads so a driver can observe
//! real before/after state; `report_canaries` returns the exact nonprivate canary strings
//! so the driver can assert the public projection excludes them. This fixture never
//! launches the real application: queued/running delivery rows are inert here and MUST
//! NOT be reused for a real-app smoke run (see `p3-report-producer-fixture.md`).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use api::{ApiHost, ApiServer, FrameImage, SharedToken};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use store::SqliteStore;
use traits::{
    allowlisted_error_code, AnswerProvider, CaptureCoverage, CaptureInterruption,
    CaptureInterruptionReason, CaptureObservations, EvidenceWindow, ExportFrameRow,
    FrameContinuation, FrameDetail, FrameFilters, FramePageRequest, HistoryReadError, NewFrame,
    OcrResult, Readiness, ReportDelivery, ReportDeliveryState, ReportDestination, ReportDetail,
    ReportError, ReportFileOwnership, ReportOutcome, ReportPageRequest, ReportRun, ReportRunKind,
    ReportRunPhase, ResumeContext, RetentionScope, RetrievedChunk, SavedReport,
    SavedReportRetention, Settings, Store, StoredFramePage, StoredReportPage,
};

/// Selected window of the nonprivate fixture history: 2026-01-01T00:00:00Z for 10
/// minutes. Reported to the harness in the `ready` event; never a real capture window.
const WINDOW_FROM: i64 = 1_767_225_600_000;
const WINDOW_TO: i64 = WINDOW_FROM + 600_000;
/// >1000 frames in the window, with deliberate timestamp ties and out-of-order captures.
const WINDOW_FRAMES: i64 = 1035;
/// Insertion index (0-based) of the one designated textless frame (no `frame_text` row).
const TEXTLESS_SEED_INDEX: i64 = 2;
/// Insertion index of the one available-but-empty text frame.
const EMPTY_TEXT_SEED_INDEX: i64 = 1;
/// Tail frames allocated outside the window after the backup snapshot: the allocator
/// high-water the `rewind` control later rolls back.
const TAIL_FRAMES: i64 = 60;
/// `captured_at = from + STEP + ((n * 37) % BUCKETS) * STEP` — many equal timestamps
/// (`n` and `n + 101` collide) inserted out of chronological order on purpose.
const TIMESTAMP_BUCKETS: i64 = 101;
const TIMESTAMP_STEP_MS: i64 = 1_000;
/// Timestamp shift applied by the labeled unsupported `reuse_timestamp` injection.
const REUSE_DELTA_MS: i64 = 50_000;
/// Late-enrichment text target (>4096 chars, asserted by the consumer test).
const ENRICH_TEXT_TARGET_LEN: usize = 6_000;
/// The API's maximum page size; used to enumerate the expected window membership from
/// the actual store page adapter (never a hand-maintained list).
const PAGE_ENUM_LIMIT: u32 = 1_000;

/// Opt-in switch for saved-report seeding. Absent/false keeps the fixture exactly as it
/// was (frame-only): no report rows, an unchanged `ready` event.
const SEED_REPORTS_ENV: &str = "SCREENSEARCH_CONSUMER_FIXTURE_SEED_REPORTS";

/// Fixed known UTC report window (the same nonprivate 10-minute window as the frames),
/// reported to the driver so it can build the private manifest independently.
const REPORT_WINDOW_FROM: i64 = WINDOW_FROM;
const REPORT_WINDOW_TO: i64 = WINDOW_TO;
/// One nonoverlapping window entirely before the query window, one entirely after.
const REPORT_BEFORE_FROM: i64 = WINDOW_FROM - 3_600_000;
const REPORT_BEFORE_TO: i64 = WINDOW_FROM - 1_800_000;
const REPORT_AFTER_FROM: i64 = WINDOW_TO + 1_800_000;
const REPORT_AFTER_TO: i64 = WINDOW_TO + 3_600_000;

/// The three tie-group runs share this exact `created_at`; ordering must fall back to
/// descending `run_id`, so the listing order inside the tie is fully deterministic.
const REPORT_TIE_CREATED_AT: i64 = 1_767_300_000_000;
/// Distinct `created_at` for the non-tie runs, all strictly below the tie group.
const REPORT_BASE_CREATED_AT: i64 = REPORT_TIE_CREATED_AT - 1_000_000;
/// The full `frame_ids` provenance union for the "beyond 200" run (all historical
/// sources that no longer exist in the fixture store).
const REPORT_PROVENANCE_IDS: i64 = 250;
const REPORT_PROVENANCE_BASE: i64 = 900_000;
/// The 200-item citation display cap (`03 §8c`), applied to `cited_frame_ids` only.
const REPORT_CITED_CAP: usize = 200;

/// Canonical opaque UUID run ids (v4-shaped, deterministic, nonprivate).
const REPORT_RUN_FULL_PROVENANCE: &str = "11111111-1111-4111-8111-111111111101";
const REPORT_RUN_QUEUED_SAVED: &str = "11111111-1111-4111-8111-111111111102";
const REPORT_RUN_RUNNING_SAVED: &str = "11111111-1111-4111-8111-111111111103";
const REPORT_RUN_FINISHED_NODATA: &str = "11111111-1111-4111-8111-111111111104";
const REPORT_RUN_BLOCKED_NOOUTPUT: &str = "11111111-1111-4111-8111-111111111105";
const REPORT_RUN_FAILED_NOOUTPUT: &str = "11111111-1111-4111-8111-111111111106";
const REPORT_RUN_UNKNOWN_ERROR: &str = "11111111-1111-4111-8111-111111111107";
const REPORT_RUN_BEFORE_WINDOW: &str = "11111111-1111-4111-8111-111111111108";
const REPORT_RUN_AFTER_WINDOW: &str = "11111111-1111-4111-8111-111111111109";
const REPORT_RUN_REVISION_CHILD: &str = "11111111-1111-4111-8111-111111111110";
const REPORT_RUN_TIE_A: &str = "11111111-1111-4111-8111-111111111201";
const REPORT_RUN_TIE_B: &str = "11111111-1111-4111-8111-111111111202";
const REPORT_RUN_TIE_C: &str = "11111111-1111-4111-8111-111111111203";
/// Canonical opaque UUID delivery ids for the private-field canaries.
const REPORT_DELIVERY_VAULT_CANARY: &str = "22222222-2222-4222-8222-222222222201";
const REPORT_DELIVERY_TELEGRAM_CANARY: &str = "22222222-2222-4222-8222-222222222202";
const REPORT_DELIVERY_VAULT_OK: &str = "22222222-2222-4222-8222-222222222203";
/// The fixture profile identity used as `source_id` (never the app's real identity).
const REPORT_SOURCE_ID: &str = "fixture-report-source";

/// Nonprivate canaries embedded in private stored fields. The public report projection
/// must never serve any of them; `report_canaries` returns the exact strings so a driver
/// can assert their absence from every response body.
const CANARY_ERROR_CODE: &str = "PRIVATE_INTERNAL_CODE_CANARY";
const CANARY_ERROR_MESSAGE: &str = "canary-private-error-message-do-not-serve";
const CANARY_CONFIG_SNAPSHOT: &str = "canary-private-config-snapshot-do-not-serve";
const CANARY_DESTINATION: &str = "canary-private-destination-do-not-serve";
const CANARY_VAULT_PATH: &str = "/private/canary/report-output-do-not-serve.md";
const CANARY_RECOVERY_PATH: &str = "/private/canary/report-recovery-do-not-serve.md";
const CANARY_TELEGRAM_MESSAGE: &str = "canary-private-telegram-error-do-not-serve";

/// One `MEMORY_FIXTURE {json}` stdout event, flushed immediately (the harness reads
/// the pipe line-buffered and blocks on `ready`/`ack`).
fn emit(value: &Value) {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "MEMORY_FIXTURE {value}");
    let _ = stdout.flush();
}

/// Fixture progress log. Stderr is merged into the harness pipe, so every line here is
/// deliberately NOT prefixed `MEMORY_FIXTURE ` (the harness ignores it) and never
/// contains the bearer or any private path outside the fixture directory.
#[derive(Clone)]
struct FixtureLogger {
    path: PathBuf,
}

impl FixtureLogger {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn log(&self, message: impl AsRef<str>) {
        let message = message.as_ref();
        eprintln!("memory-consumer-fixture: {message}");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{message}");
        }
    }
}

/// Read/mutation/provider counters, reported in every ack and in the final log line.
#[derive(Default)]
struct Counters {
    frame_pages: AtomicU64,
    coverages: AtomicU64,
    frame_details: AtomicU64,
    frame_images: AtomicU64,
    export_pages: AtomicU64,
    ask_contexts: AtomicU64,
    report_pages: AtomicU64,
    report_details: AtomicU64,
    seeded_frames: AtomicU64,
    seeded_texts: AtomicU64,
    tail_frames: AtomicU64,
    catch_up_frames: AtomicU64,
    enriched_texts: AtomicU64,
    timestamp_reuses: AtomicU64,
    restores: AtomicU64,
    history_rotations: AtomicU64,
    seeded_report_runs: AtomicU64,
    seeded_report_deliveries: AtomicU64,
    seeded_report_files: AtomicU64,
    ocr_provider_calls: AtomicU64,
    answer_provider_requests: AtomicU64,
}

impl Counters {
    fn json(&self) -> Value {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        json!({
            "reads": {
                "frame_pages": load(&self.frame_pages),
                "coverages": load(&self.coverages),
                "frame_details": load(&self.frame_details),
                "frame_images": load(&self.frame_images),
                "export_pages": load(&self.export_pages),
                "ask_contexts": load(&self.ask_contexts),
                "report_pages": load(&self.report_pages),
                "report_details": load(&self.report_details),
            },
            "mutations": {
                "seeded_frames": load(&self.seeded_frames),
                "seeded_texts": load(&self.seeded_texts),
                "tail_frames": load(&self.tail_frames),
                "catch_up_frames": load(&self.catch_up_frames),
                "enriched_texts": load(&self.enriched_texts),
                "timestamp_reuses": load(&self.timestamp_reuses),
                "restores": load(&self.restores),
                "history_rotations": load(&self.history_rotations),
                "seeded_report_runs": load(&self.seeded_report_runs),
                "seeded_report_deliveries": load(&self.seeded_report_deliveries),
                "seeded_report_files": load(&self.seeded_report_files),
            },
            "providers": {
                "ocr_calls": load(&self.ocr_provider_calls),
                "answer_provider_requests": load(&self.answer_provider_requests),
            },
        })
    }
}

/// OCR must never be invoked: the fixture answers reads only. Any call is a loud
/// fixture bug, counted so a stray invocation is visible in the counters.
struct UnavailableOcr {
    counters: Arc<Counters>,
}

#[async_trait]
impl traits::OcrProvider for UnavailableOcr {
    async fn recognize(&self, _frame: &traits::CapturedFrame) -> anyhow::Result<OcrResult> {
        self.counters
            .ocr_provider_calls
            .fetch_add(1, Ordering::Relaxed);
        anyhow::bail!("OCR provider is unavailable in the consumer fixture")
    }
}

/// Capture never starts; the factory exists only so the kernel can observe controller
/// intent as disabled (never a capture session).
struct IdleCapture;

#[async_trait]
impl traits::CaptureSource for IdleCapture {
    fn monitors(&self) -> Vec<traits::MonitorInfo> {
        Vec::new()
    }

    async fn next_frame(&mut self) -> anyhow::Result<Option<traits::CapturedFrame>> {
        std::future::pending().await
    }
}

/// Builds a kernel around a store with capture disabled, schedules off and no attached
/// inference/answer providers — the fixture's real controller-observation source.
fn build_kernel(
    store: Arc<SqliteStore>,
    fixture_dir: &Path,
    counters: Arc<Counters>,
) -> Arc<kernel::Kernel> {
    Arc::new(kernel::Kernel::new(
        store,
        Arc::new(UnavailableOcr { counters }),
        Arc::new(|_, _| Ok(Box::new(IdleCapture))),
        fixture_dir.join("frames-never-written"),
        Readiness::default(),
    ))
}

/// The swappable live store + kernel behind the API origin. `rewind` replaces the whole
/// inner value (the old store's connection closes with the last Arc), while the server
/// keeps listening on the same address.
struct HostInner {
    store: Arc<SqliteStore>,
    kernel: Arc<kernel::Kernel>,
    db_path: PathBuf,
}

/// Everything the host needs that does not change across a restore.
struct HostConfig {
    fixture_dir: PathBuf,
    backup_path: PathBuf,
    window: EvidenceWindow,
    history_id: String,
    textless_frame_id: i64,
    textless_captured_at: i64,
    seeded_high_water: i64,
    served_high_water: i64,
}

struct FixtureHost {
    started_at_ms: i64,
    inner: RwLock<Arc<HostInner>>,
    config: HostConfig,
    counters: Arc<Counters>,
    /// When > 0, the real backup `rewind` runs immediately before that many further
    /// `/v1/frames` reads (test-only control; 0 disarmed). Never changes HTTP shape.
    armed_rewind_pages: AtomicI64,
}

impl FixtureHost {
    fn new(config: HostConfig, store: Arc<SqliteStore>, counters: Arc<Counters>) -> Self {
        let kernel = build_kernel(store.clone(), &config.fixture_dir, counters.clone());
        let db_path = config.fixture_dir.join("fixture.db");
        Self {
            started_at_ms: now_ms(),
            inner: RwLock::new(Arc::new(HostInner {
                store,
                kernel,
                db_path,
            })),
            config,
            counters,
            armed_rewind_pages: AtomicI64::new(0),
        }
    }

    fn inner(&self) -> Arc<HostInner> {
        self.inner
            .read()
            .expect("fixture inner lock poisoned")
            .clone()
    }

    /// `enrich_textless`: store nonprivate full raw/content text through the production
    /// text seam (`insert_ocr`), then read it back through the same detail path the API
    /// serves. The Python assertion compares the consumer against a fresh HTTP read.
    async fn enrich_textless(&self, frame_id: i64) -> anyhow::Result<Value> {
        let inner = self.inner();
        anyhow::ensure!(frame_id > 0, "frame_id must be positive");
        anyhow::ensure!(
            inner.store.get_frame(frame_id).await?.is_some(),
            "frame {frame_id} does not exist"
        );
        let text = enriched_text(frame_id);
        anyhow::ensure!(
            text.len() > 4096,
            "enrichment text must exceed 4096 characters"
        );
        inner
            .store
            .insert_ocr(
                frame_id,
                OcrResult {
                    text,
                    mean_confidence: 0.97,
                    engine: "winrt".to_string(),
                    spans: Vec::new(),
                },
            )
            .await?;
        self.counters.enriched_texts.fetch_add(1, Ordering::Relaxed);
        let detail = inner
            .store
            .get_frame(frame_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("frame {frame_id} disappeared after enrichment"))?;
        let raw_len = detail.raw_text.as_deref().map(str::len).unwrap_or(0);
        anyhow::ensure!(
            raw_len > 4096,
            "enriched text is not visible through the detail read"
        );
        Ok(json!({ "frame_id": frame_id, "raw_text_len": raw_len }))
    }

    /// `rewind`: restore the isolated earlier SQLite backup into the live slot. The
    /// history identity, the allocator (seeded high-water) and every original frame
    /// timestamp come from the backup; the server keeps its address. Coverage on the
    /// restored store must show the decreased allocator high-water.
    async fn rewind(&self) -> anyhow::Result<Value> {
        let restore_index = self.counters.restores.fetch_add(1, Ordering::Relaxed) + 1;
        anyhow::ensure!(
            self.config.backup_path.exists(),
            "fixture backup is missing"
        );
        let restored_path = self
            .config
            .fixture_dir
            .join(format!("fixture-restored-{restore_index}.db"));
        anyhow::ensure!(
            !restored_path.exists(),
            "restore target already exists; the fixture directory is not fresh"
        );
        std::fs::copy(&self.config.backup_path, &restored_path)?;
        // Production open on the restored copy: migrations are a no-op, history
        // metadata comes from the backup, WAL/foreign keys are configured as usual.
        let store = Arc::new(SqliteStore::open_path(&restored_path)?);
        let metadata = store
            .history_readiness()
            .await
            .map_err(|error| anyhow::anyhow!("restored store readiness failed: {error}"))?;
        anyhow::ensure!(
            metadata.history_id == self.config.history_id,
            "restored history identity changed"
        );
        anyhow::ensure!(
            metadata.frame_id_high_water == self.config.seeded_high_water,
            "restored allocator is not the seeded high-water"
        );
        // Original frame timestamps must survive the restore.
        let spot = store
            .get_frame(self.config.textless_frame_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("designated frame missing after restore"))?;
        anyhow::ensure!(
            spot.captured_at == self.config.textless_captured_at,
            "restored frame timestamp changed"
        );
        let kernel = build_kernel(
            store.clone(),
            &self.config.fixture_dir,
            self.counters.clone(),
        );
        let restored_inner = Arc::new(HostInner {
            store,
            kernel,
            db_path: restored_path.clone(),
        });
        {
            // Request-synchronous control: the harness waits for this ack before any
            // further HTTP request, so the brief slot replacement cannot race a read.
            let mut slot = self.inner.write().expect("fixture inner lock poisoned");
            *slot = restored_inner;
        }
        // The replaced inner Arc (old store + kernel) is dropped here: the old
        // connection closes and its WAL is checkpointed. The old file is left as-is.
        let coverage = self
            .inner()
            .store
            .stored_coverage(self.config.window)
            .await
            .map_err(|error| anyhow::anyhow!("restored coverage read failed: {error}"))?;
        let high_water = coverage.history.frame_id_high_water;
        anyhow::ensure!(
            high_water < self.config.served_high_water,
            "restore did not lower the allocator high-water"
        );
        Ok(json!({
            "frame_id_high_water": high_water,
            "seeded_high_water": self.config.seeded_high_water,
            "restored_db": restored_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        }))
    }

    /// `catch_up_high_water`: allocate additional real frames outside the selected
    /// window (production `insert_frame` + allocator) until the allocator is at least
    /// `minimum`.
    async fn catch_up_high_water(&self, minimum: i64) -> anyhow::Result<Value> {
        anyhow::ensure!(minimum >= 0, "minimum must be non-negative");
        let mut inserted: u64 = 0;
        loop {
            let inner = self.inner();
            let metadata = inner
                .store
                .history_readiness()
                .await
                .map_err(|error| anyhow::anyhow!("catch-up readiness failed: {error}"))?;
            if metadata.frame_id_high_water >= minimum {
                return Ok(json!({
                    "frame_id_high_water": metadata.frame_id_high_water,
                    "inserted": inserted,
                }));
            }
            let index = self.counters.catch_up_frames.load(Ordering::Relaxed) as i64;
            let captured_at = self.config.window.to + TIMESTAMP_STEP_MS * (index + 1);
            inner
                .store
                .insert_frame(NewFrame {
                    captured_at,
                    monitor_index: 2,
                    width: 1920,
                    height: 1080,
                    image_path: format!("fixture/catch-up/{index}.webp"),
                    content_hash: format!("fixture-catch-up-{index}"),
                    app_hint: Some("fixture-app".to_string()),
                    window_title: Some("nonprivate fixture catch-up".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await?;
            self.counters
                .catch_up_frames
                .fetch_add(1, Ordering::Relaxed);
            inserted += 1;
            anyhow::ensure!(inserted < 100_000, "catch-up allocation exceeded bound");
        }
    }

    /// `reuse_timestamp`: deliberately inject unsupported same-history/same-ID timestamp
    /// reuse on the disposable fixture database (direct SQL — never a production
    /// writer), still within the selected window, and verify it through the store read
    /// the API serves.
    async fn reuse_timestamp(&self, frame_id: i64) -> anyhow::Result<Value> {
        let inner = self.inner();
        let before = inner
            .store
            .get_frame(frame_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("frame {frame_id} does not exist"))?;
        let mut new_at = before.captured_at + REUSE_DELTA_MS;
        if new_at >= self.config.window.to {
            new_at = before.captured_at - REUSE_DELTA_MS;
        }
        anyhow::ensure!(
            new_at >= self.config.window.from && new_at < self.config.window.to,
            "reuse timestamp would leave the selected window"
        );
        anyhow::ensure!(
            new_at != before.captured_at,
            "reuse timestamp is identical to the original"
        );
        {
            // Fixture-only corruption, clearly labeled: the isolated disposable DB is
            // the only target; production history is never opened by this example.
            let connection = rusqlite::Connection::open(&inner.db_path)?;
            connection.busy_timeout(Duration::from_secs(5))?;
            let changed = connection.execute(
                "UPDATE frames SET captured_at = ?1 WHERE id = ?2",
                rusqlite::params![new_at, frame_id],
            )?;
            anyhow::ensure!(changed == 1, "timestamp reuse updated no frame");
        }
        let after = inner
            .store
            .get_frame(frame_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("frame {frame_id} disappeared"))?;
        anyhow::ensure!(
            after.captured_at == new_at,
            "timestamp reuse is not visible through the store read"
        );
        self.counters
            .timestamp_reuses
            .fetch_add(1, Ordering::Relaxed);
        Ok(json!({ "frame_id": frame_id, "captured_at": new_at }))
    }

    /// `arm_rewind_before_page_read`: arm the real backup restore to run immediately
    /// before the Nth subsequent `/v1/frames` read (1 = the very next one). A cursor
    /// retained from a pre-rewind traversal then carries a snapshot maximum above the
    /// restored allocator, so the real API answers 400 and the consumer must recover.
    fn arm_rewind_before_page_read(&self, count: i64) -> anyhow::Result<Value> {
        anyhow::ensure!(count >= 1, "count must be a positive number of page reads");
        self.armed_rewind_pages.store(count, Ordering::Relaxed);
        Ok(json!({ "armed_before_page_reads": count }))
    }

    /// Consume one armed page read; `true` when the restore must run before this page.
    fn take_armed_rewind(&self) -> bool {
        loop {
            let armed = self.armed_rewind_pages.load(Ordering::Relaxed);
            if armed <= 0 {
                return false;
            }
            if self
                .armed_rewind_pages
                .compare_exchange(armed, armed - 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                return armed == 1;
            }
        }
    }

    /// `rotate_history`: swap the live store to a **genuinely new** history identity in a
    /// fresh DB slot (production `open_path` setup). The disposable current DB is
    /// snapshotted first (`VACUUM INTO`) and never mutated in place; the new namespace is
    /// seeded through the production writers like the primary window, then raised with real
    /// tail frames to the previously served high-water so the consumer does not mistake the
    /// new identity for a rewound allocator on its next coverage read. A cursor retained
    /// over the previous history then hits the real 409.
    async fn rotate_history(&self) -> anyhow::Result<Value> {
        let index = self
            .counters
            .history_rotations
            .fetch_add(1, Ordering::Relaxed)
            + 1;
        {
            let current = self.inner();
            let prerotate = self
                .config
                .fixture_dir
                .join(format!("fixture-prerotate-{index}.db"));
            anyhow::ensure!(
                !prerotate.exists(),
                "pre-rotation snapshot already exists; the fixture directory is not fresh"
            );
            sqlite_snapshot(&current.db_path, &prerotate)?;
        }
        let new_path = self
            .config
            .fixture_dir
            .join(format!("fixture-newhistory-{index}.db"));
        anyhow::ensure!(
            !new_path.exists(),
            "new-history slot already exists; the fixture directory is not fresh"
        );
        let store = Arc::new(SqliteStore::open_path(&new_path)?);
        let metadata = store
            .history_readiness()
            .await
            .map_err(|error| anyhow::anyhow!("rotated store readiness failed: {error}"))?;
        anyhow::ensure!(
            metadata.history_id != self.config.history_id,
            "rotated history identity did not change"
        );
        anyhow::ensure!(
            metadata.frame_id_high_water == 0,
            "rotated history is not a fresh identity"
        );
        let new_history_id = metadata.history_id.clone();
        // Real production seeding: the same shape as the primary window, so the new
        // namespace is genuinely populated with a designated textless frame.
        seed_window(&store, self.config.window, &self.counters).await?;
        insert_tail(&store, self.config.window, &self.counters).await?;
        let high_water = store
            .history_readiness()
            .await
            .map_err(|error| anyhow::anyhow!("rotated readiness failed: {error}"))?
            .frame_id_high_water;
        anyhow::ensure!(
            high_water >= self.config.served_high_water,
            "rotated allocator is below the previously served high-water"
        );
        let kernel = build_kernel(
            store.clone(),
            &self.config.fixture_dir,
            self.counters.clone(),
        );
        {
            // Request-synchronous control: the harness waits for this ack before any
            // further HTTP request, so the slot replacement cannot race a read.
            let mut slot = self.inner.write().expect("fixture inner lock poisoned");
            *slot = Arc::new(HostInner {
                store,
                kernel,
                db_path: new_path.clone(),
            });
        }
        Ok(json!({
            "history_id": new_history_id,
            "frame_id_high_water": high_water,
            "db": new_path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default(),
        }))
    }

    /// `seed_reports`: idempotently seed the saved-report fixture set through the existing
    /// production Store writers. Safe to call repeatedly; the summary is rebuilt from the
    /// fixed seed inputs, never from an API response.
    async fn seed_reports(&self) -> anyhow::Result<Value> {
        let inner = self.inner();
        let seeded = seed_reports(inner.store.as_ref(), &self.counters).await?;
        Ok(seeded.json())
    }

    /// `report_state`: snapshot the stored report/delivery/file state through the existing
    /// Store reads so a driver can compare real before/after state. Carries only ids,
    /// phases, outcomes, booleans, counts and the live counters — never Markdown, a stored
    /// message, a path, a destination snapshot or a credential.
    async fn report_state(&self) -> anyhow::Result<Value> {
        let inner = self.inner();
        let store = inner.store.as_ref();
        let runs = store.list_report_runs(PAGE_ENUM_LIMIT, None).await?;
        let pending = store.list_pending_report_runs().await?;
        let files = store.list_report_files().await?;
        let mut run_views = Vec::with_capacity(runs.len());
        let mut delivery_views = Vec::new();
        for run in &runs {
            run_views.push(json!({
                "run_id": run.run_id.clone(),
                "phase": run.phase,
                "outcome": run.outcome,
                "revision_of": run.revision_of.clone(),
                "window_start": run.window_start,
                "window_end": run.window_end,
                "created_at": run.created_at,
                "updated_at": run.updated_at,
                "result_present": run.result.is_some(),
                "error_present": run.error.is_some(),
                "error_code_allowlisted": allowlisted_error_code(run.error.as_ref()),
            }));
            for delivery in store.list_report_deliveries(&run.run_id).await? {
                delivery_views.push(json!({
                    "id": delivery.id.clone(),
                    "run_id": delivery.run_id.clone(),
                    "destination": delivery.destination,
                    "state": delivery.state,
                    "attempt": delivery.attempt,
                    "next_attempt_present": delivery.next_attempt_at.is_some(),
                    "path_present": delivery.path.is_some(),
                    "recovery_path_present": delivery.recovery_path.is_some(),
                    "message_id_present": delivery.message_id.is_some(),
                    "error_present": delivery.error.is_some(),
                }));
            }
        }
        Ok(json!({
            "run_count": run_views.len(),
            "runs": run_views,
            "delivery_count": delivery_views.len(),
            "deliveries": delivery_views,
            "file_count": files.len(),
            "pending_run_ids": pending
                .iter()
                .map(|run| run.run_id.clone())
                .collect::<Vec<_>>(),
            "counters": self.counters.json(),
        }))
    }

    /// `report_canaries`: the exact nonprivate canary strings stored in private fields. A
    /// driver asserts none of them appears in any report response body.
    fn report_canaries(&self) -> Value {
        json!({
            "error_code": CANARY_ERROR_CODE,
            "error_message": CANARY_ERROR_MESSAGE,
            "config_snapshot": CANARY_CONFIG_SNAPSHOT,
            "destination_snapshot": CANARY_DESTINATION,
            "vault_path": CANARY_VAULT_PATH,
            "recovery_path": CANARY_RECOVERY_PATH,
            "telegram_error_message": CANARY_TELEGRAM_MESSAGE,
        })
    }
}

#[async_trait]
impl ApiHost for FixtureHost {
    fn version(&self) -> String {
        format!("memory-consumer-fixture-{}", env!("CARGO_PKG_VERSION"))
    }

    fn started_at_ms(&self) -> i64 {
        self.started_at_ms
    }

    async fn is_capturing(&self) -> bool {
        let inner = self.inner();
        inner.kernel.is_capturing().await
    }

    fn store(&self) -> Arc<dyn Store> {
        let inner = self.inner();
        let store: Arc<dyn Store> = inner.store.clone();
        store
    }

    async fn settings(&self) -> Settings {
        let inner = self.inner();
        kernel::settings::load_settings(inner.store.as_ref()).await
    }

    async fn frame_detail(&self, frame_id: i64) -> anyhow::Result<Option<FrameDetail>> {
        self.counters.frame_details.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner();
        inner.store.get_frame(frame_id).await
    }

    async fn frame_page(
        &self,
        request: FramePageRequest,
    ) -> Result<StoredFramePage, HistoryReadError> {
        self.counters.frame_pages.fetch_add(1, Ordering::Relaxed);
        // Armed mid-drain rewind (test-only): run the real backup restore immediately
        // before serving this page, so a cursor frozen at the pre-rewind snapshot maximum
        // hits the real 400 (`snapshot_max_frame_id > high-water`).
        if self.take_armed_rewind() {
            self.rewind().await.map_err(HistoryReadError::Storage)?;
        }
        let inner = self.inner();
        inner.store.frame_page(request).await
    }

    async fn capture_coverage(
        &self,
        window: EvidenceWindow,
    ) -> Result<CaptureCoverage, HistoryReadError> {
        self.counters.coverages.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner();
        compose_capture_coverage(&inner.store, &inner.kernel, window).await
    }

    async fn frame_image(&self, _frame_id: i64) -> anyhow::Result<Option<FrameImage>> {
        self.counters.frame_images.fetch_add(1, Ordering::Relaxed);
        // The fixture never writes image files; no frame has a served image.
        Ok(None)
    }

    async fn ask_context(
        &self,
        _query: &str,
        _top_k: u32,
        _session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        self.counters.ask_contexts.fetch_add(1, Ordering::Relaxed);
        anyhow::bail!("answer generation is unavailable in the consumer fixture")
    }

    async fn answer_provider(&self) -> Option<Arc<dyn AnswerProvider>> {
        self.counters
            .answer_provider_requests
            .fetch_add(1, Ordering::Relaxed);
        // No answer model is loaded: `/v1/ask` returns 503, exactly like the app
        // before the sidecar is attached.
        None
    }

    async fn add_mark(
        &self,
        _frame_id: Option<i64>,
        _capture_now: bool,
        _note: Option<String>,
    ) -> anyhow::Result<i64> {
        anyhow::bail!("capture is off; the consumer fixture does not create marks")
    }

    async fn resolve_mark(&self, _mark_id: i64) -> anyhow::Result<()> {
        anyhow::bail!("the consumer fixture does not resolve marks")
    }

    async fn where_was_i(&self) -> anyhow::Result<Option<ResumeContext>> {
        let inner = self.inner();
        let settings = kernel::settings::load_settings(inner.store.as_ref()).await;
        kernel::resume::where_was_i(inner.store.as_ref(), &settings).await
    }

    async fn export_frames_page(
        &self,
        after_id: i64,
        from: Option<i64>,
        to: Option<i64>,
        limit: u32,
    ) -> anyhow::Result<Vec<ExportFrameRow>> {
        self.counters.export_pages.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner();
        inner
            .store
            .export_frames_page(after_id, from, to, limit)
            .await
    }

    async fn report_page(
        &self,
        request: ReportPageRequest,
    ) -> Result<StoredReportPage, HistoryReadError> {
        self.counters.report_pages.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner();
        // The genuine Store read path: router → host → SqliteStore. Until the Store read
        // contract is published the trait default returns the safe `UnsupportedStore`
        // (→503); once it lands this serves real stored pages with no fixture adapter.
        inner.store.report_page(request).await
    }

    async fn report_detail(&self, run_id: &str) -> Result<Option<ReportDetail>, HistoryReadError> {
        self.counters.report_details.fetch_add(1, Ordering::Relaxed);
        let inner = self.inner();
        inner.store.report_detail(run_id).await
    }
}

/// The same composition the production root uses (`src-tauri/src/local_api.rs`):
/// Store facts + actual kernel controller observations + persisted retention policy.
async fn compose_capture_coverage(
    store: &SqliteStore,
    kernel: &kernel::Kernel,
    window: EvidenceWindow,
) -> Result<CaptureCoverage, HistoryReadError> {
    let store: &dyn Store = store;
    let facts = store.stored_coverage(window).await?;
    let (capture_enabled, observations) =
        kernel.capture_observations(window.from, window.to).await?;
    let settings = kernel::settings::load_settings(store).await;
    Ok(CaptureCoverage {
        window: facts.window,
        observed_at: facts.observed_at,
        capture_enabled,
        frame_id_high_water: facts.history.frame_id_high_water,
        oldest_frame: facts.oldest_frame,
        newest_frame: facts.newest_frame,
        newest_captured_at: facts.newest_captured_at,
        frames_stored: facts.frames_stored,
        frames_text_available: facts.frames_text_available,
        image_retention_days: settings.storage_retention_days,
        retention_scope: RetentionScope::ImagesOnly,
        saved_report_retention: saved_report_retention_policy(),
        capture_observations: map_capture_observations(observations),
    })
}

/// The existing saved-report run retention policy (`03 §7f.5/.6`), as composed by the
/// production root.
fn saved_report_retention_policy() -> SavedReportRetention {
    SavedReportRetention {
        terminal_days: 90,
        exempt_phases: vec![
            ReportRunPhase::Queued,
            ReportRunPhase::Running,
            ReportRunPhase::Blocked,
        ],
        exempt_delivery_states: vec![
            ReportDeliveryState::Pending,
            ReportDeliveryState::Sending,
            ReportDeliveryState::UnknownDelivery,
            ReportDeliveryState::Conflict,
        ],
        failed_delivery_with_pending_retry_exempt: true,
    }
}

/// Kernel observation fields map one-to-one onto the wire DTO; the reason match is
/// exhaustive on purpose so a new kernel reason cannot silently fold into an existing
/// wire variant.
fn map_capture_observations(observations: kernel::CaptureObservations) -> CaptureObservations {
    CaptureObservations {
        observation_started_at: observations.observation_started_at,
        unavailable_before: observations.unavailable_before,
        truncated: observations.truncated,
        intervals: observations
            .intervals
            .into_iter()
            .map(|interval| CaptureInterruption {
                start: interval.start,
                end: interval.end,
                reason: match interval.reason {
                    kernel::CaptureInterruptionReason::CaptureStopped => {
                        CaptureInterruptionReason::CaptureStopped
                    }
                    kernel::CaptureInterruptionReason::WorkerError => {
                        CaptureInterruptionReason::WorkerError
                    }
                },
            })
            .collect(),
    }
}

/// One newline-JSON stdin control command.
#[derive(Deserialize)]
struct ControlCommand {
    command: String,
    #[serde(default)]
    frame_id: Option<i64>,
    #[serde(default)]
    minimum: Option<i64>,
    #[serde(default)]
    count: Option<i64>,
}

struct SeededWindow {
    textless_frame_id: i64,
    textless_captured_at: i64,
    #[allow(dead_code)]
    empty_text_frame_id: i64,
}

/// `captured_at = from + STEP + ((n * 37) % BUCKETS) * STEP` — many equal timestamps
/// (`n` and `n + 101` collide) and a non-monotonic insertion order.
fn seeded_captured_at(window: EvidenceWindow, n: i64) -> i64 {
    window.from + TIMESTAMP_STEP_MS + ((n * 37) % TIMESTAMP_BUCKETS) * TIMESTAMP_STEP_MS
}

fn seeded_text(n: i64) -> String {
    format!(
        "nonprivate fixture frame {n}: full raw evidence text for chronological traversal, \
         deliberate timestamp ties and out-of-order captures."
    )
}

fn enriched_text(frame_id: i64) -> String {
    let mut text = format!("nonprivate fixture enriched text for frame {frame_id}. ");
    while text.len() < ENRICH_TEXT_TARGET_LEN {
        text.push_str("late text reread filler for the reference consumer; ");
    }
    text
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// Seeds >1000 frames in the selected window through the production writers
/// (`Store::insert_frame` + `Store::insert_ocr`, real allocator), with one designated
/// textless frame and one available-but-empty frame.
async fn seed_window(
    store: &SqliteStore,
    window: EvidenceWindow,
    counters: &Counters,
) -> anyhow::Result<SeededWindow> {
    let mut textless: Option<(i64, i64)> = None;
    let mut empty_text: Option<i64> = None;
    for n in 0..WINDOW_FRAMES {
        let captured_at = seeded_captured_at(window, n);
        let id = store
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 2,
                width: 1920,
                height: 1080,
                image_path: format!("fixture/frames/{n}.webp"),
                content_hash: format!("fixture-window-{n}"),
                app_hint: Some("fixture-app".to_string()),
                window_title: Some("nonprivate fixture".to_string()),
                browser_url: Some("https://example.invalid/nonprivate".to_string()),
                capture_trigger: None,
            })
            .await?;
        anyhow::ensure!(
            id == n + 1,
            "fixture directory is not fresh (unexpected allocated frame id {id})"
        );
        counters.seeded_frames.fetch_add(1, Ordering::Relaxed);
        if n == TEXTLESS_SEED_INDEX {
            // Designated textless frame: deliberately no `frame_text` row.
            textless = Some((id, captured_at));
            continue;
        }
        let text = if n == EMPTY_TEXT_SEED_INDEX {
            String::new()
        } else {
            seeded_text(n)
        };
        store
            .insert_ocr(
                id,
                OcrResult {
                    text,
                    mean_confidence: 0.95,
                    engine: "winrt".to_string(),
                    spans: Vec::new(),
                },
            )
            .await?;
        counters.seeded_texts.fetch_add(1, Ordering::Relaxed);
        if n == EMPTY_TEXT_SEED_INDEX {
            empty_text = Some(id);
        }
    }
    let (textless_frame_id, textless_captured_at) =
        textless.ok_or_else(|| anyhow::anyhow!("designated textless seed missing"))?;
    let empty_text_frame_id =
        empty_text.ok_or_else(|| anyhow::anyhow!("designated empty-text seed missing"))?;
    Ok(SeededWindow {
        textless_frame_id,
        textless_captured_at,
        empty_text_frame_id,
    })
}

/// Tail frames allocated OUTSIDE the selected window after the backup snapshot; they
/// raise the allocator high-water that `rewind` later rolls back.
async fn insert_tail(
    store: &SqliteStore,
    window: EvidenceWindow,
    counters: &Counters,
) -> anyhow::Result<()> {
    for k in 0..TAIL_FRAMES {
        let captured_at = window.to + TIMESTAMP_STEP_MS * (k + 1);
        store
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 2,
                width: 1920,
                height: 1080,
                image_path: format!("fixture/tail/{k}.webp"),
                content_hash: format!("fixture-tail-{k}"),
                app_hint: Some("fixture-app".to_string()),
                window_title: Some("nonprivate fixture tail".to_string()),
                browser_url: None,
                capture_trigger: None,
            })
            .await?;
        counters.tail_frames.fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

/// Real SQLite backup of the isolated fixture DB (`VACUUM INTO`): consistent even in
/// WAL mode, preserving the history identity and allocator exactly as seeded.
fn sqlite_snapshot(db_path: &Path, backup_path: &Path) -> anyhow::Result<()> {
    if backup_path.exists() {
        std::fs::remove_file(backup_path)?;
    }
    let connection = rusqlite::Connection::open(db_path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let target = backup_path.to_string_lossy().to_string();
    connection.execute("VACUUM INTO ?1", rusqlite::params![target])?;
    Ok(())
}

/// Expected window membership from the actual store page adapter the API serves —
/// never a hand-maintained list and never canned page data.
async fn enumerate_window(store: &SqliteStore, window: EvidenceWindow) -> anyhow::Result<Vec<i64>> {
    let mut ids = Vec::new();
    let mut continuation: Option<FrameContinuation> = None;
    loop {
        let page = store
            .frame_page(FramePageRequest {
                window,
                filters: FrameFilters::default(),
                limit: PAGE_ENUM_LIMIT,
                continuation: continuation.take(),
            })
            .await
            .map_err(|error| anyhow::anyhow!("window enumeration failed: {error}"))?;
        ids.extend(page.items.iter().map(|item| item.frame_id));
        match page.next_position {
            Some(after) => {
                continuation = Some(FrameContinuation {
                    snapshot: page.snapshot.clone(),
                    after,
                });
            }
            None => break,
        }
    }
    Ok(ids)
}

/// Deterministic nonprivate saved Markdown, one distinct body per role so every detail
/// digest differs. Never logged, emitted or served by the fixture itself.
fn report_markdown(role: &str) -> String {
    format!("nonprivate fixture saved report for {role}; no real user content.\n")
}

/// A base `ReportRun` with the fixed nonprivate identity of this fixture. Callers set
/// only the fields that vary; every write still travels the production Store writer.
fn base_report_run(
    source_id: &str,
    run_id: &str,
    window: (i64, i64),
    created_at: i64,
) -> ReportRun {
    ReportRun {
        requested_range: None,
        run_id: run_id.to_string(),
        source_id: source_id.to_string(),
        kind: ReportRunKind::Daily,
        schedule_id: None,
        revision_of: None,
        window_start: window.0,
        window_end: window.1,
        timezone: "UTC".to_string(),
        local_date: "2026-01-01".to_string(),
        phase: ReportRunPhase::Finished,
        outcome: Some(ReportOutcome::Ok),
        error: None,
        result: None,
        config_snapshot: "{}".to_string(),
        created_at,
        updated_at: created_at,
    }
}

/// A base `ReportDelivery` with the fixed nonprivate identity of this fixture.
fn delivery_fixture(
    id: &str,
    run_id: &str,
    destination: ReportDestination,
    state: ReportDeliveryState,
    created_at: i64,
) -> ReportDelivery {
    ReportDelivery {
        id: id.to_string(),
        run_id: run_id.to_string(),
        destination,
        state,
        revision_of: None,
        destination_snapshot: "{}".to_string(),
        path: None,
        hash: None,
        recovery_path: None,
        chunk_index: None,
        chunk_count: None,
        message_id: None,
        error: None,
        next_attempt_at: None,
        attempt: 0,
        created_at,
        updated_at: created_at,
    }
}

/// The seeded saved-report set: the exact run/delivery ids and counts a driver needs to
/// build its private manifest independently. No Markdown, message, path or config value
/// is carried here.
struct SeededReports {
    source_id: String,
    window: EvidenceWindow,
    run_ids: Vec<String>,
    delivery_ids: Vec<String>,
    file_count: usize,
}

impl SeededReports {
    fn json(&self) -> Value {
        json!({
            "source_id": self.source_id,
            "window": { "from": self.window.from, "to": self.window.to },
            "run_ids": self.run_ids,
            "run_count": self.run_ids.len(),
            "delivery_ids": self.delivery_ids,
            "delivery_count": self.delivery_ids.len(),
            "file_count": self.file_count,
            "flags": {
                "ordered_ties": true,
                "revision_link": true,
                "queued_saved_output": true,
                "running_saved_output": true,
                "finished_no_data": true,
                "blocked_no_output": true,
                "failed_no_output": true,
                "unknown_error_code": true,
                "nonoverlapping_windows": true,
                "provenance_over_200": true,
                "missing_source_references": true,
                "private_field_canaries": true,
            },
        })
    }
}

/// Seed the saved-report fixture set through the **existing production Store writers
/// only**. Idempotent: a repeat call re-runs the same fixed inputs and the writers
/// return the existing rows. Never generates, schedules, delivers, captures or calls a
/// model — it only persists the predetermined nonprivate rows.
async fn seed_reports(store: &SqliteStore, counters: &Counters) -> anyhow::Result<SeededReports> {
    let source_id = store.ensure_report_source_id(REPORT_SOURCE_ID).await?;
    let window = EvidenceWindow {
        from: REPORT_WINDOW_FROM,
        to: REPORT_WINDOW_TO,
    };

    // The complete `frame_ids` provenance union exceeds 200 and every id is a historical
    // source that no longer exists in this disposable store.
    let provenance: Vec<i64> = (0..REPORT_PROVENANCE_IDS)
        .map(|n| REPORT_PROVENANCE_BASE + n)
        .collect();
    let cited: Vec<i64> = provenance.iter().copied().take(REPORT_CITED_CAP).collect();

    // 1. Finished OK, full provenance >200 with missing historical sources, overlapping
    //    window, and a private `config_snapshot` canary the projection must never serve.
    let mut full = base_report_run(
        &source_id,
        REPORT_RUN_FULL_PROVENANCE,
        (REPORT_WINDOW_FROM, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT,
    );
    full.result = Some(SavedReport {
        markdown: report_markdown("full-provenance"),
        model: Some("fixture-local-model".to_string()),
        frame_ids: provenance,
        session_ids: vec![700_001, 700_002],
        cited_frame_ids: cited,
        frames_sampled: REPORT_PROVENANCE_IDS as u32,
        frames_summarized: REPORT_PROVENANCE_IDS as u32,
        periods_total: 3,
        periods_covered: 3,
        passes: 2,
        truncated: false,
        daily_run_ids: Vec::new(),
    });
    full.config_snapshot = format!(
        "{{\"canary\":\"{CANARY_CONFIG_SNAPSHOT}\",\"output_folder\":\"{CANARY_VAULT_PATH}\"}}"
    );
    full.updated_at = full.created_at + 500;
    store.create_report_run(full).await?;

    // 2. Queued run that ALREADY carries an immutable saved output.
    let mut queued = base_report_run(
        &source_id,
        REPORT_RUN_QUEUED_SAVED,
        (REPORT_WINDOW_FROM + 10_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 1_000,
    );
    queued.phase = ReportRunPhase::Queued;
    queued.outcome = None;
    queued.result = Some(SavedReport {
        markdown: report_markdown("queued-saved"),
        model: Some("fixture-local-model".to_string()),
        frame_ids: vec![1, 2, 3],
        session_ids: vec![],
        cited_frame_ids: vec![1, 2, 3],
        frames_sampled: 3,
        frames_summarized: 3,
        periods_total: 1,
        periods_covered: 1,
        passes: 1,
        truncated: false,
        daily_run_ids: Vec::new(),
    });
    store.create_report_run(queued).await?;

    // 3. Running run whose saved output is persisted through the real update writer:
    //    create with no result, then write the immutable result while still running.
    let mut running = base_report_run(
        &source_id,
        REPORT_RUN_RUNNING_SAVED,
        (REPORT_WINDOW_FROM + 20_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 2_000,
    );
    running.phase = ReportRunPhase::Running;
    running.outcome = None;
    store.create_report_run(running.clone()).await?;
    running.result = Some(SavedReport {
        markdown: report_markdown("running-saved"),
        model: Some("fixture-local-model".to_string()),
        frame_ids: vec![10, 11],
        session_ids: vec![700_003],
        cited_frame_ids: vec![10, 11],
        frames_sampled: 2,
        frames_summarized: 2,
        periods_total: 1,
        periods_covered: 1,
        passes: 1,
        truncated: true,
        daily_run_ids: Vec::new(),
    });
    running.updated_at = running.created_at + 250;
    store.update_report_run(running).await?;

    // 4. Finished NO_DATA that keeps its factual saved output.
    let mut nodata = base_report_run(
        &source_id,
        REPORT_RUN_FINISHED_NODATA,
        (REPORT_WINDOW_FROM + 30_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 3_000,
    );
    nodata.outcome = Some(ReportOutcome::NoData);
    nodata.result = Some(SavedReport {
        markdown: report_markdown("finished-no-data"),
        model: None,
        frame_ids: vec![],
        session_ids: vec![],
        cited_frame_ids: vec![],
        frames_sampled: 0,
        frames_summarized: 0,
        periods_total: 1,
        periods_covered: 0,
        passes: 0,
        truncated: false,
        daily_run_ids: Vec::new(),
    });
    nodata.updated_at = nodata.created_at + 100;
    store.create_report_run(nodata).await?;

    // 5. Blocked, no output: literal null result, allowlisted error code.
    let mut blocked = base_report_run(
        &source_id,
        REPORT_RUN_BLOCKED_NOOUTPUT,
        (REPORT_WINDOW_FROM + 40_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 4_000,
    );
    blocked.phase = ReportRunPhase::Blocked;
    blocked.outcome = Some(ReportOutcome::Blocked);
    blocked.error = Some(ReportError {
        code: "SOURCE_UNAVAILABLE".to_string(),
        message: "fixture source unavailable".to_string(),
    });
    store.create_report_run(blocked).await?;

    // 6. Failed, no output: literal null result, allowlisted error code.
    let mut failed = base_report_run(
        &source_id,
        REPORT_RUN_FAILED_NOOUTPUT,
        (REPORT_WINDOW_FROM + 50_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 5_000,
    );
    failed.outcome = Some(ReportOutcome::Failed);
    failed.error = Some(ReportError {
        code: "GENERATION_FAILED".to_string(),
        message: "fixture generation failed".to_string(),
    });
    store.create_report_run(failed).await?;

    // 7. Finished with an UNKNOWN private error code/message: the projection must serve a
    //    null error_code and never the stored message.
    let mut unknown = base_report_run(
        &source_id,
        REPORT_RUN_UNKNOWN_ERROR,
        (REPORT_WINDOW_FROM + 60_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 6_000,
    );
    unknown.outcome = Some(ReportOutcome::Failed);
    unknown.error = Some(ReportError {
        code: CANARY_ERROR_CODE.to_string(),
        message: CANARY_ERROR_MESSAGE.to_string(),
    });
    unknown.config_snapshot = format!("{{\"canary\":\"{CANARY_CONFIG_SNAPSHOT}\"}}");
    store.create_report_run(unknown).await?;

    // 8. Nonoverlapping window entirely before the query window.
    let mut before = base_report_run(
        &source_id,
        REPORT_RUN_BEFORE_WINDOW,
        (REPORT_BEFORE_FROM, REPORT_BEFORE_TO),
        REPORT_BASE_CREATED_AT + 7_000,
    );
    before.local_date = "2025-12-31".to_string();
    store.create_report_run(before).await?;

    // 9. Nonoverlapping window entirely after the query window.
    let after = base_report_run(
        &source_id,
        REPORT_RUN_AFTER_WINDOW,
        (REPORT_AFTER_FROM, REPORT_AFTER_TO),
        REPORT_BASE_CREATED_AT + 8_000,
    );
    store.create_report_run(after).await?;

    // 10. Revision child of the failed run: same window, `revision_of` link, finished OK.
    let mut revision = base_report_run(
        &source_id,
        REPORT_RUN_REVISION_CHILD,
        (REPORT_WINDOW_FROM + 50_000, REPORT_WINDOW_TO),
        REPORT_BASE_CREATED_AT + 9_000,
    );
    revision.revision_of = Some(REPORT_RUN_FAILED_NOOUTPUT.to_string());
    revision.result = Some(SavedReport {
        markdown: report_markdown("revision-child"),
        model: Some("fixture-local-model".to_string()),
        frame_ids: vec![20, 21, 22, 23],
        session_ids: vec![],
        cited_frame_ids: vec![20, 21],
        frames_sampled: 4,
        frames_summarized: 4,
        periods_total: 2,
        periods_covered: 2,
        passes: 1,
        truncated: false,
        daily_run_ids: Vec::new(),
    });
    store.create_report_run(revision).await?;

    // 11. Tie group: three runs with the SAME `created_at`; ordering must fall back to
    //     descending `run_id`, so the listing order inside the tie is deterministic.
    for (run_id, offset) in [
        (REPORT_RUN_TIE_A, 70_000_i64),
        (REPORT_RUN_TIE_B, 80_000),
        (REPORT_RUN_TIE_C, 90_000),
    ] {
        let mut tie = base_report_run(
            &source_id,
            run_id,
            (REPORT_WINDOW_FROM + offset, REPORT_WINDOW_TO),
            REPORT_TIE_CREATED_AT,
        );
        tie.result = Some(SavedReport {
            markdown: report_markdown(run_id),
            model: Some("fixture-local-model".to_string()),
            frame_ids: vec![offset],
            session_ids: vec![],
            cited_frame_ids: vec![offset],
            frames_sampled: 1,
            frames_summarized: 1,
            periods_total: 1,
            periods_covered: 1,
            passes: 1,
            truncated: false,
            daily_run_ids: Vec::new(),
        });
        tie.updated_at = tie.created_at + 42;
        store.create_report_run(tie).await?;
    }

    // Private delivery rows for the failed run: destination/path/recovery/message
    // canaries the public projection must never serve.
    let mut vault_canary = delivery_fixture(
        REPORT_DELIVERY_VAULT_CANARY,
        REPORT_RUN_FAILED_NOOUTPUT,
        ReportDestination::Vault,
        ReportDeliveryState::Pending,
        REPORT_BASE_CREATED_AT + 5_100,
    );
    vault_canary.destination_snapshot = format!("{{\"canary\":\"{CANARY_DESTINATION}\"}}");
    vault_canary.path = Some(CANARY_VAULT_PATH.to_string());
    vault_canary.recovery_path = Some(CANARY_RECOVERY_PATH.to_string());
    vault_canary.hash = Some("canary-private-hash-do-not-serve".to_string());
    store.create_report_delivery(vault_canary).await?;

    let mut telegram_canary = delivery_fixture(
        REPORT_DELIVERY_TELEGRAM_CANARY,
        REPORT_RUN_FAILED_NOOUTPUT,
        ReportDestination::Telegram,
        ReportDeliveryState::Failed,
        REPORT_BASE_CREATED_AT + 5_200,
    );
    telegram_canary.destination_snapshot = format!("{{\"canary\":\"{CANARY_DESTINATION}\"}}");
    telegram_canary.error = Some(ReportError {
        code: "DELIVERY_FAILED".to_string(),
        message: CANARY_TELEGRAM_MESSAGE.to_string(),
    });
    telegram_canary.message_id = Some(4242);
    telegram_canary.attempt = 2;
    store.create_report_delivery(telegram_canary).await?;

    // A succeeded vault delivery for the full-provenance run (a non-pending receipt).
    let mut vault_ok = delivery_fixture(
        REPORT_DELIVERY_VAULT_OK,
        REPORT_RUN_FULL_PROVENANCE,
        ReportDestination::Vault,
        ReportDeliveryState::Succeeded,
        REPORT_BASE_CREATED_AT + 600,
    );
    vault_ok.destination_snapshot = format!("{{\"canary\":\"{CANARY_DESTINATION}\"}}");
    vault_ok.path = Some(CANARY_VAULT_PATH.to_string());
    vault_ok.hash = Some("canary-private-hash-do-not-serve".to_string());
    store.create_report_delivery(vault_ok).await?;

    // One owned generated-file row (private path) that the API never serves.
    store
        .save_report_file(ReportFileOwnership {
            path: CANARY_VAULT_PATH.to_string(),
            source_id: source_id.clone(),
            run_id: REPORT_RUN_FULL_PROVENANCE.to_string(),
            hash: "canary-private-hash-do-not-serve".to_string(),
            updated_at: REPORT_BASE_CREATED_AT + 600,
        })
        .await?;

    let run_ids: Vec<String> = [
        REPORT_RUN_FULL_PROVENANCE,
        REPORT_RUN_QUEUED_SAVED,
        REPORT_RUN_RUNNING_SAVED,
        REPORT_RUN_FINISHED_NODATA,
        REPORT_RUN_BLOCKED_NOOUTPUT,
        REPORT_RUN_FAILED_NOOUTPUT,
        REPORT_RUN_UNKNOWN_ERROR,
        REPORT_RUN_BEFORE_WINDOW,
        REPORT_RUN_AFTER_WINDOW,
        REPORT_RUN_REVISION_CHILD,
        REPORT_RUN_TIE_A,
        REPORT_RUN_TIE_B,
        REPORT_RUN_TIE_C,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    let delivery_ids: Vec<String> = [
        REPORT_DELIVERY_VAULT_CANARY,
        REPORT_DELIVERY_TELEGRAM_CANARY,
        REPORT_DELIVERY_VAULT_OK,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    // Honest counters: the size of the seeded set. `.store` (not `.add`) keeps a repeat
    // seed idempotent instead of double-counting.
    counters
        .seeded_report_runs
        .store(run_ids.len() as u64, Ordering::Relaxed);
    counters
        .seeded_report_deliveries
        .store(delivery_ids.len() as u64, Ordering::Relaxed);
    counters.seeded_report_files.store(1, Ordering::Relaxed);

    Ok(SeededReports {
        source_id,
        window,
        run_ids,
        delivery_ids,
        file_count: 1,
    })
}

/// Stdin reader thread → async channel. `tokio`'s `io-std` feature is not part of this
/// crate's dependency set, so the blocking std reader runs on a plain thread.
fn spawn_stdin_reader() -> tokio::sync::mpsc::UnboundedReceiver<String> {
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    std::thread::Builder::new()
        .name("fixture-stdin".to_string())
        .spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        if sender.send(line).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        })
        .expect("spawn fixture stdin reader");
    receiver
}

#[tokio::main]
async fn main() {
    match run().await {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            emit(&json!({ "event": "error", "message": format!("{error:#}") }));
            eprintln!("memory-consumer-fixture: fatal: {error:#}");
            std::process::exit(1);
        }
    }
}

async fn run() -> anyhow::Result<i32> {
    let fixture_dir = std::env::var("SCREENSEARCH_CONSUMER_FIXTURE_DIR")
        .map_err(|_| anyhow::anyhow!("SCREENSEARCH_CONSUMER_FIXTURE_DIR is not set"))?;
    let fixture_dir = PathBuf::from(fixture_dir);
    std::fs::create_dir_all(&fixture_dir)?;
    let log = FixtureLogger::new(fixture_dir.join("fixture.log"));

    // The harness-supplied bearer becomes the READ credential of the running server; it
    // is never printed, logged or embedded in any event. A separate random legacy token
    // exists only to satisfy `ApiServer::start` and is never shared with the harness.
    let read_token = std::env::var("SCREENSEARCH_API_TOKEN")
        .map_err(|_| anyhow::anyhow!("SCREENSEARCH_API_TOKEN is not set"))?;
    anyhow::ensure!(
        !read_token.is_empty() && read_token.bytes().all(|byte| (33..=126).contains(&byte)),
        "SCREENSEARCH_API_TOKEN is not a valid bearer credential"
    );

    let db_path = fixture_dir.join("fixture.db");
    let backup_path = fixture_dir.join("fixture-backup.db");
    anyhow::ensure!(
        !db_path.exists(),
        "SCREENSEARCH_CONSUMER_FIXTURE_DIR is not fresh (fixture.db already exists)"
    );
    let window = EvidenceWindow {
        from: WINDOW_FROM,
        to: WINDOW_TO,
    };
    let counters = Arc::new(Counters::default());

    // 1. Isolated real store; `open_path` runs the production migrations and history
    //    setup (identity + allocator) exactly like the app.
    let store = Arc::new(SqliteStore::open_path(&db_path)?);
    let metadata = store
        .history_readiness()
        .await
        .map_err(|error| anyhow::anyhow!("history readiness failed: {error}"))?;
    let history_id = metadata.history_id.clone();
    log.log(format!(
        "store opened (history {history_id}, allocator {})",
        metadata.frame_id_high_water
    ));

    // 2. Seed the >1000-frame window through the production writers.
    let seeded = seed_window(&store, window, &counters).await?;
    let seeded_high_water = store
        .history_readiness()
        .await
        .map_err(|error| anyhow::anyhow!("seeded readiness failed: {error}"))?
        .frame_id_high_water;
    log.log(format!(
        "seeded window frames (allocator {seeded_high_water}, textless {}, empty-text {})",
        seeded.textless_frame_id, seeded.empty_text_frame_id
    ));

    // 3. Real SQLite backup of the isolated DB BEFORE the tail, so the `rewind` control
    //    can restore it without discarding any seeded frame.
    sqlite_snapshot(&db_path, &backup_path)?;

    // 4. Tail outside the window: raises the allocator the restore will roll back.
    insert_tail(&store, window, &counters).await?;
    let served_high_water = store
        .history_readiness()
        .await
        .map_err(|error| anyhow::anyhow!("tail readiness failed: {error}"))?
        .frame_id_high_water;
    log.log(format!(
        "tail inserted (allocator {served_high_water}, backup taken at {seeded_high_water})"
    ));

    // 5. Expected membership straight from the store page adapter (never canned).
    let expected = enumerate_window(&store, window).await?;
    anyhow::ensure!(
        expected.len() > 1000,
        "seeded window must contain more than 1000 frames"
    );
    anyhow::ensure!(
        expected.contains(&seeded.textless_frame_id),
        "designated textless frame is not part of the window"
    );

    // 5b. Opt-in saved-report seeding. Off by default so the frame-only ready/ack
    //     protocol is unchanged. The `seed_reports` control can also turn it on later.
    let seed_reports_opt_in = std::env::var(SEED_REPORTS_ENV)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false);
    let seeded_reports = if seed_reports_opt_in {
        let reports = seed_reports(&store, &counters).await?;
        log.log(format!(
            "seeded saved reports ({} runs, {} deliveries, window {}..{})",
            reports.run_ids.len(),
            reports.delivery_ids.len(),
            reports.window.from,
            reports.window.to
        ));
        Some(reports)
    } else {
        None
    };

    // 6. Host over the real store + kernel, actual router on an ephemeral loopback port.
    let host = Arc::new(FixtureHost::new(
        HostConfig {
            fixture_dir: fixture_dir.clone(),
            backup_path,
            window,
            history_id: history_id.clone(),
            textless_frame_id: seeded.textless_frame_id,
            textless_captured_at: seeded.textless_captured_at,
            seeded_high_water,
            served_high_water,
        },
        store.clone(),
        counters.clone(),
    ));
    // The host's inner value is now the only remaining store handle, so `rewind` can
    // close the old connection by replacing it.
    drop(store);
    let legacy_token: SharedToken = Arc::new(RwLock::new(api::generate_token()));
    let server = ApiServer::start(host.clone(), legacy_token, 0).await?;
    server.set_read_token(Some(read_token));
    let api_url = format!("http://127.0.0.1:{}", server.local_addr().port());
    let mut server = Some(server);
    log.log(format!("listening at {api_url}"));

    // 7. Ready: actual seeded store metadata, never substituted by HTTP responses. When
    //    report seeding is opted in, add ONLY safe report ids/counts/flags — never
    //    Markdown, a stored message, a path, a config value or the bearer.
    let mut ready = json!({
        "event": "ready",
        "api_url": api_url,
        "from": window.from,
        "to": window.to,
        "history_id": history_id,
        "expected_frame_ids": expected,
        "textless_frame_id": seeded.textless_frame_id,
    });
    if let Some(reports) = &seeded_reports {
        let summary = reports.json();
        ready["reports_seeded"] = json!(true);
        ready["report_source_id"] = summary["source_id"].clone();
        ready["report_window"] = summary["window"].clone();
        ready["report_run_ids"] = summary["run_ids"].clone();
        ready["report_run_count"] = summary["run_count"].clone();
        ready["report_delivery_ids"] = summary["delivery_ids"].clone();
        ready["report_delivery_count"] = summary["delivery_count"].clone();
        ready["report_file_count"] = summary["file_count"].clone();
        ready["report_flags"] = summary["flags"].clone();
    }
    emit(&ready);

    // 8. Control loop: one real isolated store operation per command, then an ack.
    let mut controls = spawn_stdin_reader();
    while let Some(line) = controls.recv().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let control: ControlCommand = match serde_json::from_str(line) {
            Ok(control) => control,
            Err(_) => {
                emit(&json!({ "event": "error", "message": "malformed control command" }));
                return Ok(1);
            }
        };
        log.log(format!("control: {}", control.command));
        let outcome: anyhow::Result<Value> = match control.command.as_str() {
            "enrich_textless" => match control.frame_id {
                Some(frame_id) => host.enrich_textless(frame_id).await,
                None => Err(anyhow::anyhow!("enrich_textless requires frame_id")),
            },
            "rewind" => host.rewind().await,
            "arm_rewind_before_page_read" => match control.count {
                Some(count) => host.arm_rewind_before_page_read(count),
                None => Err(anyhow::anyhow!(
                    "arm_rewind_before_page_read requires count"
                )),
            },
            "rotate_history" => host.rotate_history().await,
            "catch_up_high_water" => match control.minimum {
                Some(minimum) => host.catch_up_high_water(minimum).await,
                None => Err(anyhow::anyhow!("catch_up_high_water requires minimum")),
            },
            "reuse_timestamp" => match control.frame_id {
                Some(frame_id) => host.reuse_timestamp(frame_id).await,
                None => Err(anyhow::anyhow!("reuse_timestamp requires frame_id")),
            },
            "seed_reports" => host.seed_reports().await,
            "report_state" => host.report_state().await,
            "report_canaries" => Ok(host.report_canaries()),
            "stop" => {
                emit(&json!({
                    "event": "ack",
                    "command": "stop",
                    "counters": counters.json(),
                }));
                if let Some(server) = server.take() {
                    server.stop().await;
                }
                log.log(format!("stopped; final counters {}", counters.json()));
                drop(host);
                return Ok(0);
            }
            other => {
                emit(&json!({
                    "event": "error",
                    "message": format!("unknown control command {other}"),
                }));
                return Ok(1);
            }
        };
        match outcome {
            Ok(result) => emit(&json!({
                "event": "ack",
                "command": control.command,
                "result": result,
                "counters": counters.json(),
            })),
            Err(error) => {
                emit(&json!({ "event": "error", "message": format!("{error:#}") }));
                return Ok(1);
            }
        }
    }
    // Stdin closed: the harness is done with this fixture.
    if let Some(server) = server.take() {
        server.stop().await;
    }
    log.log(format!("stdin closed; final counters {}", counters.json()));
    Ok(0)
}
