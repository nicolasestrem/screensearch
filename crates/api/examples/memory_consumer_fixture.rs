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
    AnswerProvider, CaptureCoverage, CaptureInterruption, CaptureInterruptionReason,
    CaptureObservations, EvidenceWindow, ExportFrameRow, FrameContinuation, FrameDetail,
    FrameFilters, FramePageRequest, HistoryReadError, NewFrame, OcrResult, Readiness,
    ReportDeliveryState, ReportRunPhase, ResumeContext, RetentionScope, RetrievedChunk,
    SavedReportRetention, Settings, Store, StoredFramePage,
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
    seeded_frames: AtomicU64,
    seeded_texts: AtomicU64,
    tail_frames: AtomicU64,
    catch_up_frames: AtomicU64,
    enriched_texts: AtomicU64,
    timestamp_reuses: AtomicU64,
    restores: AtomicU64,
    history_rotations: AtomicU64,
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

    // 7. Ready: actual seeded store metadata, never substituted by HTTP responses.
    emit(&json!({
        "event": "ready",
        "api_url": api_url,
        "from": window.from,
        "to": window.to,
        "history_id": history_id,
        "expected_frame_ids": expected,
        "textless_frame_id": seeded.textless_frame_id,
    }));

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
