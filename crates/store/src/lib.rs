//! `store` — the durable data spine: frames, OCR, vision, embeddings, hybrid
//! retrieval, the job queue, and settings, on SQLite (WAL) + sqlite-vec + FTS5
//! (`03 §4/§5`). *Everything writes here*, so it is built before any producer.
//!
//! ## Concurrency model
//! One [`rusqlite::Connection`] is kept for the store's lifetime behind a
//! [`std::sync::Mutex`]; every async [`traits::Store`] method runs its SQL inside
//! [`tokio::task::spawn_blocking`]. SQLite allows a single writer, so serializing
//! through one connection is correct and simple, and an in-memory DB persists for
//! the store's lifetime (clean `:memory:` tests, `03 §10`). The Mutex guard is
//! only ever held *inside* a blocking task — never across an `.await`.
//!
//! ## Retrieval & the embedder
//! [`SqliteStore::hybrid_search`] fuses an FTS5 arm with a sqlite-vec KNN arm via
//! RRF. The KNN arm needs the *query* embedded, but the store must not depend on
//! the embeddings impl — so it optionally holds an `Arc<dyn `[`EmbeddingProvider`]`>`
//! (a trait; modularity holds). With no embedder injected the search degrades to
//! FTS-only; P3 injects the real fastembed provider. (`07` engineering note.)

use std::path::Path;
use std::sync::{Arc, Mutex, Once, RwLock};

use async_trait::async_trait;
use rusqlite::Connection;
use traits::{
    AppSuppression, AskEntityMatch, AskSessionHit, AskThreadCandidate, AskTurnRecord, ChunkSource,
    Embedding, EmbeddingProvider, FrameContextRow, FrameEnrichmentInput, FrameMeta,
    InsightsSummary, Job, JobKind, JobStats, Mark, NewAskTurn, NewFrame, NewJob, NewSession,
    NewSessionArtifact, OcrResult, Result, SearchHit, SearchQuery, SegmenterFrame, Session,
    SessionArtifact, SessionArtifactKind, SessionContent, SessionEmbedInput, SessionFilter,
    SessionFrameSample, SessionReference, TextFilterContext, TimeRange, TimelineBucket,
    VisionAnalysis,
};

mod ask;
mod embeddings;
mod frames;
mod graph;
mod history;
mod history_pages;
mod insights;
mod jobs;
mod marks;
mod memory;
mod records;
mod report_pages;
mod schema;
mod search;
mod sessions;
mod settings;
mod timeline;

pub use schema::{
    create_baseline, BASELINE_SQL, EMBEDDING_DIM, FILTER_VERSION, LATEST_SCHEMA_VERSION,
};
/// Re-export of the contract this crate implements (`03 §3`).
pub use traits::Store;

/// The durable data spine over SQLite + sqlite-vec + FTS5.
///
/// Cheap to [`Clone`] (it is a pair of `Arc`s); clones share the same underlying
/// connection and embedder.
#[derive(Clone)]
pub struct SqliteStore {
    conn: Arc<Mutex<Connection>>,
    /// Runtime-settable (`03 §5`): the composition root attaches the embedder *after*
    /// the model finishes loading off the launch thread. Behind an `Arc<RwLock>` so
    /// every clone shares it — injecting on one handle lights up every clone's vector
    /// arm. `None` ⇒ search degrades to FTS-only.
    embedder: Arc<RwLock<Option<Arc<dyn EmbeddingProvider>>>>,
}

/// Registers the sqlite-vec (`vec0`) loadable extension exactly once for the
/// process. `sqlite3_auto_extension` applies to every connection opened *after*
/// the call, so this must run before any [`Connection::open`].
fn register_vec_extension() {
    static VEC_INIT: Once = Once::new();
    VEC_INIT.call_once(|| {
        // The auto-extension entry point has the C ABI sqlite expects; sqlite-vec
        // exposes it as `sqlite3_vec_init`. The transmute is the documented
        // registration pattern (annotated to satisfy `missing_transmute_annotations`).
        type ExtInit = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        // SAFETY: `sqlite3_vec_init` is a valid C function with the signature
        // `sqlite3_auto_extension` expects; we register it once via the `Once`.
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute::<*const (), ExtInit>(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}

/// Connection-scoped pragmas. WAL is a no-op on `:memory:` (it returns "memory");
/// `foreign_keys` and `recursive_triggers` are per-connection and must be set on every
/// open so the cascade and vec-cleanup triggers fire (`03 §4`).
const PRAGMAS: &str = "PRAGMA busy_timeout=5000;
         PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA recursive_triggers=ON;";

/// Prepares a brand-new database: pragmas, the v14 baseline, then the history metadata
/// rows the allocator needs.
fn open_fresh(conn: Connection) -> Result<Connection> {
    conn.execute_batch(PRAGMAS)?;
    schema::create_baseline(&conn)?;
    let mut conn = conn;
    history::setup_history_metadata(&mut conn)?;
    Ok(conn)
}

/// Prepares a connection to an existing database file. There is no migration path: the
/// file must already be at [`schema::LATEST_SCHEMA_VERSION`] (`03 §4`, 0.4.5 D2).
fn open_existing(conn: Connection) -> Result<Connection> {
    // Validate before touching the pragmas: `journal_mode=WAL` rewrites the file header,
    // and a refused database must be left byte for byte untouched.
    let version = require_latest_schema(&conn)?;
    require_baseline_tables(&conn, version)?;
    conn.execute_batch(PRAGMAS)?;
    let mut conn = conn;
    history::setup_history_metadata(&mut conn)?;
    Ok(conn)
}

/// Number of schema objects (tables, indexes, views, triggers) in the database. Zero means
/// an empty database; a non-SQLite file is reported as a refusal naming the rebuild tool,
/// never treated as empty.
fn schema_object_count(conn: &Connection) -> Result<i64> {
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))
        .map_err(|e| refuse_schema(format!("unknown ({e})")))
}

/// Requires every table of [`schema::BASELINE_SQL`] to be present. A version row alone
/// does not prove completeness: a truncated restore, or a file from a different product,
/// must be refused here rather than fail later with a misleading query error.
fn require_baseline_tables(conn: &Connection, version: i32) -> Result<()> {
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type = 'table'")?;
    let present: std::collections::HashSet<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<_>>()?;
    for table in schema::BASELINE_TABLE_NAMES.iter() {
        if !present.contains(table) {
            anyhow::bail!(
                "database schema_version is {version} but the required table `{table}` is \
                 missing; rebuild the database with the dev-only `ssv2c-rebuild` tool \
                 (`03 §4`, 0.4.5 D2)"
            );
        }
    }
    Ok(())
}

/// Requires an existing database to be at the baseline version, returning it.
///
/// Every other state (older, newer, missing `schema_version`, empty or non-SQLite
/// file) is refused with the rebuild instruction that names both the version found
/// and the dev-only tool (`03 §4`, 0.4.5 D2/D3).
fn require_latest_schema(conn: &Connection) -> Result<i32> {
    let found = conn.query_row("SELECT version FROM schema_version", [], |r| {
        r.get::<_, i32>(0)
    });
    match found {
        Ok(version) if version == schema::LATEST_SCHEMA_VERSION => Ok(version),
        Ok(version) => Err(refuse_schema(format!("{version}"))),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            Err(refuse_schema("unknown (no schema_version row)".to_string()))
        }
        Err(e) => Err(refuse_schema(format!("unknown ({e})"))),
    }
}

/// The single refusal message: it must always name the version found and the tool that
/// rebuilds the database, so a user never sees a bare "cannot open".
fn refuse_schema(found: String) -> anyhow::Error {
    anyhow::anyhow!(
        "database schema_version is {found}, but this build requires {}; \
         rebuild the database with the dev-only `ssv2c-rebuild` tool (`03 §4`, 0.4.5 D2)",
        schema::LATEST_SCHEMA_VERSION
    )
}

impl SqliteStore {
    /// Opens an in-memory store (tests / ephemeral use, `03 §10`).
    pub fn open_in_memory() -> Result<Self> {
        register_vec_extension();
        Ok(Self::from_conn(open_fresh(Connection::open_in_memory()?)?))
    }

    /// Opens (creating if needed) the store at `path`, the on-disk `screensearch.db`
    /// (`03 §4`, 0.4.5 D2).
    ///
    /// Decided by the database's contents, not by whether the file exists. An empty
    /// database (no file, a zero-byte file, or a first run that rolled back) is created
    /// from the v14 baseline with `schema_version = 14` in the same transaction. Anything
    /// with schema objects must be at `schema_version = 14`; every other state, including
    /// a file that is not SQLite, is refused untouched with a message naming the version
    /// found and the `ssv2c-rebuild` tool.
    pub fn open_path(path: &Path) -> Result<Self> {
        register_vec_extension();
        let conn = Connection::open(path)?;
        // Decide by what the database *contains*, never by whether a file exists:
        //
        // - A missing file and a zero-byte file both read as a database with no schema
        //   objects, and so does a file left behind by a first run that failed mid-way
        //   (the baseline is one transaction, so a failure rolls back to empty). All of
        //   these are safely initialised here. Deciding by `path.exists()` instead would
        //   refuse that last case forever (`ssv2c-rebuild` cannot repair it either), and
        //   would race a file appearing between the check and the open.
        // - Anything with at least one schema object is someone's database: it goes
        //   through the strict v14 check and is refused, untouched, if it is not v14.
        // - A file that is not SQLite at all fails the probe and is refused, never
        //   overwritten.
        //
        // The probe is a read on the schema table and runs before any pragma, because
        // `journal_mode=WAL` rewrites the header of a file we may be about to refuse.
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        let conn = if schema_object_count(&conn)? == 0 {
            open_fresh(conn)?
        } else {
            open_existing(conn)?
        };
        Ok(Self::from_conn(conn))
    }

    fn from_conn(conn: Connection) -> Self {
        Self {
            conn: Arc::new(Mutex::new(conn)),
            embedder: Arc::new(RwLock::new(None)),
        }
    }

    /// Builder form of [`Self::set_embedder`] — injects the query-embedding provider
    /// that lights up the vector arm of [`Self::hybrid_search`]. Without it, search
    /// is FTS-only (P1 default; P3 injects fastembed once it has loaded).
    #[must_use]
    pub fn with_embedder(self, embedder: Arc<dyn EmbeddingProvider>) -> Self {
        self.set_embedder(embedder);
        self
    }

    /// Injects (or replaces) the query-embedding provider at runtime (`03 §5`). Takes
    /// effect immediately for every clone sharing this store — the composition root
    /// calls this once the fastembed model has loaded off the launch thread.
    pub fn set_embedder(&self, embedder: Arc<dyn EmbeddingProvider>) {
        *self.embedder.write().expect("store embedder lock poisoned") = Some(embedder);
    }

    /// The DB's current (tracked) schema version (`03 §4`). Useful for the
    /// readiness/diagnostics panel.
    pub fn schema_version(&self) -> Result<i32> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let v = conn.query_row("SELECT version FROM schema_version", [], |r| r.get(0))?;
        Ok(v)
    }

    /// Runs `f` against the (exclusively locked) connection inside a blocking
    /// task, so async callers never block the runtime and the Mutex guard is
    /// never held across an `.await`. All data-access methods funnel through here.
    pub(crate) async fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().expect("store mutex poisoned");
            f(&guard)
        })
        .await
        .map_err(|e| anyhow::anyhow!("store blocking task failed: {e}"))?
    }
}

/// The [`traits::Store`] contract, forwarding to the inherent methods (which the
/// module files implement and the test-suite exercises directly). The composition
/// root depends on `dyn Store`, so this is the seam that keeps it impl-agnostic
/// (`03 §2`). Inherent methods win method resolution, so the qualified
/// `SqliteStore::…` calls are non-recursive.
#[async_trait]
impl Store for SqliteStore {
    async fn history_readiness(
        &self,
    ) -> std::result::Result<traits::HistoryMetadata, traits::HistoryReadError> {
        SqliteStore::history_readiness(self).await
    }

    async fn frame_page(
        &self,
        request: traits::FramePageRequest,
    ) -> std::result::Result<traits::StoredFramePage, traits::HistoryReadError> {
        SqliteStore::frame_page(self, request).await
    }

    async fn stored_coverage(
        &self,
        window: traits::EvidenceWindow,
    ) -> std::result::Result<traits::StoredCoverage, traits::HistoryReadError> {
        SqliteStore::stored_coverage(self, window).await
    }

    async fn report_page(
        &self,
        request: traits::ReportPageRequest,
    ) -> std::result::Result<traits::StoredReportPage, traits::HistoryReadError> {
        SqliteStore::report_page(self, request).await
    }

    async fn report_detail(
        &self,
        run_id: &str,
    ) -> std::result::Result<Option<traits::ReportDetail>, traits::HistoryReadError> {
        SqliteStore::report_detail(self, run_id).await
    }

    async fn list_report_family_deliveries(
        &self,
        run_id: &str,
    ) -> Result<Vec<traits::ReportDelivery>> {
        SqliteStore::list_report_family_deliveries(self, run_id).await
    }

    async fn create_report_deliveries(
        &self,
        deliveries: Vec<traits::ReportDelivery>,
    ) -> Result<Vec<traits::ReportDelivery>> {
        SqliteStore::create_report_deliveries(self, deliveries).await
    }
    async fn get_report_delivery(&self, id: &str) -> Result<Option<traits::ReportDelivery>> {
        SqliteStore::get_report_delivery(self, id).await
    }

    async fn report_has_usable_content(&self, start: i64, end: i64) -> Result<bool> {
        SqliteStore::report_has_usable_content(self, start, end).await
    }

    async fn list_pending_report_runs(&self) -> Result<Vec<traits::ReportRun>> {
        SqliteStore::list_pending_report_runs(self).await
    }

    async fn list_report_files(&self) -> Result<Vec<traits::ReportFileOwnership>> {
        SqliteStore::list_report_files(self).await
    }

    async fn ensure_report_source_id(&self, candidate: &str) -> Result<String> {
        SqliteStore::ensure_report_source_id(self, candidate).await
    }
    async fn reset_report_source_id(&self, new_id: &str) -> Result<()> {
        SqliteStore::reset_report_source_id(self, new_id).await
    }
    async fn create_report_run(&self, run: traits::ReportRun) -> Result<traits::ReportRun> {
        SqliteStore::create_report_run(self, run).await
    }
    async fn get_report_run(&self, id: &str) -> Result<Option<traits::ReportRun>> {
        SqliteStore::get_report_run(self, id).await
    }
    async fn list_report_runs(
        &self,
        limit: u32,
        kind: Option<traits::ReportRunKind>,
    ) -> Result<Vec<traits::ReportRun>> {
        SqliteStore::list_report_runs(self, limit, kind).await
    }
    async fn saved_daily_runs(&self, start: i64, end: i64) -> Result<Vec<traits::ReportRun>> {
        SqliteStore::saved_daily_runs(self, start, end).await
    }
    async fn update_report_run(&self, run: traits::ReportRun) -> Result<()> {
        SqliteStore::update_report_run(self, run).await
    }
    async fn claim_report_run(&self, id: &str, now: i64) -> Result<bool> {
        SqliteStore::claim_report_run(self, id, now).await
    }
    async fn recover_report_work(&self, now: i64) -> Result<()> {
        SqliteStore::recover_report_work(self, now).await
    }
    async fn create_report_delivery(
        &self,
        delivery: traits::ReportDelivery,
    ) -> Result<traits::ReportDelivery> {
        SqliteStore::create_report_delivery(self, delivery).await
    }
    async fn update_report_delivery(
        &self,
        delivery: traits::ReportDelivery,
        expected_state: traits::ReportDeliveryState,
    ) -> Result<bool> {
        SqliteStore::update_report_delivery(self, delivery, expected_state).await
    }
    async fn resolve_report_delivery(
        &self,
        previous: traits::ReportDelivery,
        replacement: Option<traits::ReportDelivery>,
    ) -> Result<bool> {
        SqliteStore::resolve_report_delivery(self, previous, replacement).await
    }
    async fn list_report_deliveries(&self, run_id: &str) -> Result<Vec<traits::ReportDelivery>> {
        SqliteStore::list_report_deliveries(self, run_id).await
    }
    async fn get_report_file(&self, path: &str) -> Result<Option<traits::ReportFileOwnership>> {
        SqliteStore::get_report_file(self, path).await
    }
    async fn save_report_file(&self, ownership: traits::ReportFileOwnership) -> Result<()> {
        SqliteStore::save_report_file(self, ownership).await
    }
    async fn report_evidence(&self, frame_ids: Vec<i64>) -> Result<traits::ReportEvidence> {
        SqliteStore::report_evidence(self, frame_ids).await
    }
    async fn prune_report_runs(&self, before: i64) -> Result<u64> {
        SqliteStore::prune_report_runs(self, before).await
    }
    async fn insert_frame(&self, f: NewFrame) -> Result<i64> {
        SqliteStore::insert_frame(self, f).await
    }
    async fn insert_ocr(&self, frame_id: i64, ocr: OcrResult) -> Result<()> {
        SqliteStore::insert_ocr(self, frame_id, ocr).await
    }
    async fn insert_ocr_filtered(
        &self,
        frame_id: i64,
        ocr: OcrResult,
        ctx: TextFilterContext,
    ) -> Result<()> {
        SqliteStore::insert_ocr_filtered(self, frame_id, ocr, ctx).await
    }
    async fn insert_vision(&self, frame_id: i64, v: VisionAnalysis) -> Result<()> {
        SqliteStore::insert_vision(self, frame_id, v).await
    }
    async fn upsert_text_embedding(
        &self,
        frame_id: i64,
        chunk_index: i32,
        chunk_text: &str,
        source: ChunkSource,
        emb: &Embedding,
        model: &str,
    ) -> Result<()> {
        SqliteStore::upsert_text_embedding(
            self,
            frame_id,
            chunk_index,
            chunk_text,
            source,
            emb,
            model,
        )
        .await
    }
    async fn hybrid_search(&self, q: &SearchQuery) -> Result<Vec<SearchHit>> {
        SqliteStore::hybrid_search(self, q).await
    }
    async fn hybrid_search_in_session(
        &self,
        q: &SearchQuery,
        session_id: i64,
    ) -> Result<Vec<SearchHit>> {
        SqliteStore::hybrid_search_in_session(self, q, session_id).await
    }
    async fn timeline_buckets(
        &self,
        start: i64,
        end: i64,
        bucket_count: u32,
    ) -> Result<Vec<TimelineBucket>> {
        SqliteStore::timeline_buckets(self, start, end, bucket_count).await
    }
    async fn insights_summary(
        &self,
        start: i64,
        end: i64,
        bucket_count: u32,
    ) -> Result<InsightsSummary> {
        SqliteStore::insights_summary(self, start, end, bucket_count).await
    }
    async fn insert_insight(&self, insight: traits::NewInsight) -> Result<i64> {
        SqliteStore::insert_insight(self, insight).await
    }
    async fn replace_insights(
        &self,
        kind: traits::InsightKind,
        window_start: i64,
        window_end: i64,
        batch: Vec<traits::NewInsight>,
    ) -> Result<Vec<i64>> {
        SqliteStore::replace_insights(self, kind, window_start, window_end, batch).await
    }
    async fn list_insights(&self, filter: traits::InsightFilter) -> Result<Vec<traits::Insight>> {
        SqliteStore::list_insights(self, filter).await
    }
    async fn unevidenced_insight_ids(&self) -> Result<Vec<i64>> {
        SqliteStore::unevidenced_insight_ids(self).await
    }
    async fn mark_insights_stale_where_evidence_missing(&self) -> Result<u64> {
        SqliteStore::mark_insights_stale_where_evidence_missing(self).await
    }
    async fn threads_in_range(
        &self,
        filter: traits::ThreadFilter,
    ) -> Result<Vec<traits::ThreadSummary>> {
        SqliteStore::threads_in_range(self, filter).await
    }
    async fn thread_detail(&self, thread_id: i64) -> Result<Option<traits::ThreadDetail>> {
        SqliteStore::thread_detail(self, thread_id).await
    }
    async fn entities_filtered(
        &self,
        filter: traits::EntityFilter,
    ) -> Result<Vec<traits::EntitySummary>> {
        SqliteStore::entities_filtered(self, filter).await
    }
    async fn insight_thread_activity(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Vec<traits::ThreadActivityInput>> {
        SqliteStore::insight_thread_activity(self, window_start, window_end).await
    }
    async fn insight_open_marks(&self, window_end: i64) -> Result<Vec<traits::OpenMarkInput>> {
        SqliteStore::insight_open_marks(self, window_end).await
    }
    async fn insight_entity_sessions(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Vec<traits::EntitySessionInput>> {
        SqliteStore::insight_entity_sessions(self, window_start, window_end).await
    }
    async fn insight_session_neighbours(
        &self,
        window_start: i64,
        window_end: i64,
        k: u32,
    ) -> Result<Vec<traits::SessionNeighbourInput>> {
        SqliteStore::insight_session_neighbours(self, window_start, window_end, k).await
    }
    async fn insight_thread_sessions(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Vec<traits::ThreadSessionInput>> {
        SqliteStore::insight_thread_sessions(self, window_start, window_end).await
    }
    async fn get_enrichment_input(&self, frame_id: i64) -> Result<Option<FrameEnrichmentInput>> {
        SqliteStore::frame_enrichment_input(self, frame_id).await
    }
    async fn untagged_frame_ids(&self, limit: u32, range: Option<(i64, i64)>) -> Result<Vec<i64>> {
        SqliteStore::untagged_frame_ids(self, limit, range).await
    }
    async fn ocr_texts(&self, frame_ids: &[i64]) -> Result<std::collections::HashMap<i64, String>> {
        SqliteStore::ocr_texts(self, frame_ids).await
    }
    async fn sample_frames_in_range(
        &self,
        start: i64,
        end: i64,
        limit: u32,
    ) -> Result<Vec<FrameMeta>> {
        SqliteStore::sample_frames_in_range(self, start, end, limit).await
    }
    async fn enqueue_job(&self, job: NewJob) -> Result<i64> {
        SqliteStore::enqueue_job(self, job).await
    }
    async fn claim_jobs(&self, kinds: &[JobKind], limit: u32, now: i64) -> Result<Vec<Job>> {
        SqliteStore::claim_jobs(self, kinds, limit, now).await
    }
    async fn complete_job(&self, id: i64) -> Result<()> {
        SqliteStore::complete_job(self, id).await
    }
    async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> Result<()> {
        SqliteStore::fail_job(self, id, err, retry_at).await
    }
    async fn job_stats(&self) -> Result<JobStats> {
        SqliteStore::job_stats(self).await
    }
    async fn pending_vision_job_count(&self) -> Result<u64> {
        SqliteStore::pending_vision_job_count(self).await
    }
    async fn cancel_pending_vision_jobs(&self) -> Result<u64> {
        SqliteStore::cancel_pending_vision_jobs(self).await
    }
    async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> Result<u64> {
        SqliteStore::reset_stale_running_jobs(self, older_than_ms).await
    }
    async fn text_filter_stats(&self, filter_version: i32) -> Result<Vec<AppSuppression>> {
        SqliteStore::text_filter_stats(self, filter_version).await
    }
    async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        SqliteStore::get_setting(self, key).await
    }
    async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        SqliteStore::set_setting(self, key, value).await
    }
    async fn set_settings_batch(&self, kvs: &[(String, String)]) -> Result<()> {
        SqliteStore::set_settings_batch(self, kvs).await
    }
    async fn delete_settings(&self, keys: &[&str]) -> Result<Vec<String>> {
        SqliteStore::delete_settings(self, keys).await
    }
    async fn insert_mark(
        &self,
        frame_id: i64,
        created_at: i64,
        note: Option<String>,
    ) -> Result<i64> {
        SqliteStore::insert_mark(self, frame_id, created_at, note).await
    }
    async fn list_marks(&self) -> Result<Vec<Mark>> {
        SqliteStore::list_marks(self).await
    }
    async fn resolve_mark(&self, mark_id: i64, resolved_at: i64) -> Result<()> {
        SqliteStore::resolve_mark(self, mark_id, resolved_at).await
    }
    async fn set_mark_note(&self, mark_id: i64, note: &str) -> Result<()> {
        SqliteStore::set_mark_note(self, mark_id, note).await
    }
    async fn recent_frame_contexts(&self, limit: u32) -> Result<Vec<FrameContextRow>> {
        SqliteStore::recent_frame_contexts(self, limit).await
    }
    async fn insert_session(&self, session: NewSession) -> Result<i64> {
        SqliteStore::insert_session(self, session).await
    }
    async fn update_unfrozen_session(&self, id: i64, session: NewSession) -> Result<bool> {
        SqliteStore::update_unfrozen_session(self, id, session).await
    }
    async fn delete_unfrozen_session(&self, id: i64) -> Result<bool> {
        SqliteStore::delete_unfrozen_session(self, id).await
    }
    async fn assign_frames_session(
        &self,
        frame_ids: &[i64],
        session_id: Option<i64>,
    ) -> Result<u64> {
        SqliteStore::assign_frames_session(self, frame_ids, session_id).await
    }
    async fn freeze_sessions(&self, older_than_ms: i64) -> Result<u64> {
        SqliteStore::freeze_sessions(self, older_than_ms).await
    }
    async fn unfrozen_sessions(&self) -> Result<Vec<Session>> {
        SqliteStore::unfrozen_sessions(self).await
    }
    async fn sessions_in_range(&self, filter: SessionFilter) -> Result<Vec<Session>> {
        SqliteStore::sessions_in_range(self, filter).await
    }
    async fn get_session(&self, id: i64) -> Result<Option<Session>> {
        SqliteStore::get_session(self, id).await
    }

    async fn session_frame_sample(
        &self,
        session_id: i64,
        limit: u32,
    ) -> Result<SessionFrameSample> {
        SqliteStore::session_frame_sample(self, session_id, limit).await
    }

    async fn session_reference_for_frame(&self, frame_id: i64) -> Result<Option<SessionReference>> {
        SqliteStore::session_reference_for_frame(self, frame_id).await
    }
    async fn session_has_usable_content(&self, session_id: i64) -> Result<bool> {
        SqliteStore::session_has_usable_content(self, session_id).await
    }
    async fn session_frames_meta(&self, session_id: i64) -> Result<Vec<SegmenterFrame>> {
        SqliteStore::session_frames_meta(self, session_id).await
    }
    async fn frames_meta_in_range(&self, from_ms: i64, to_ms: i64) -> Result<Vec<SegmenterFrame>> {
        SqliteStore::frames_meta_in_range(self, from_ms, to_ms).await
    }
    async fn content_texts_for_frames(&self, frame_ids: &[i64]) -> Result<Vec<SessionContent>> {
        SqliteStore::content_texts_for_frames(self, frame_ids).await
    }
    async fn insert_session_artifacts(
        &self,
        session_id: i64,
        artifacts: &[NewSessionArtifact],
    ) -> Result<Vec<i64>> {
        SqliteStore::insert_session_artifacts(self, session_id, artifacts).await
    }
    async fn list_session_artifacts(&self, session_id: i64) -> Result<Vec<SessionArtifact>> {
        SqliteStore::list_session_artifacts(self, session_id).await
    }
    async fn delete_session_artifacts_by_kind(
        &self,
        session_id: i64,
        kind: SessionArtifactKind,
    ) -> Result<u64> {
        SqliteStore::delete_session_artifacts_by_kind(self, session_id, kind).await
    }
    async fn set_session_title_summary(
        &self,
        id: i64,
        title: &str,
        summary: &str,
        model: &str,
    ) -> Result<bool> {
        SqliteStore::set_session_title_summary(self, id, title, summary, model).await
    }
    async fn clear_session_title_summary(&self, id: i64) -> Result<bool> {
        SqliteStore::clear_session_title_summary(self, id).await
    }
    fn set_embedder(&self, embedder: Arc<dyn EmbeddingProvider>) {
        SqliteStore::set_embedder(self, embedder);
    }

    async fn graph_frame_input(&self, frame_id: i64) -> Result<Option<traits::GraphFrameInput>> {
        SqliteStore::graph_frame_input(self, frame_id).await
    }
    async fn graph_session_at(&self, at_ms: i64) -> Result<Option<traits::GraphSessionInput>> {
        SqliteStore::graph_session_at(self, at_ms).await
    }
    async fn graph_frame_ids_after(&self, after_id: Option<i64>, limit: u32) -> Result<Vec<i64>> {
        SqliteStore::graph_frame_ids_after(self, after_id, limit).await
    }
    async fn graph_apply_hits(
        &self,
        session_id: Option<i64>,
        frame_id: i64,
        captured_at: i64,
        hits: &[traits::GraphHit],
    ) -> Result<()> {
        SqliteStore::graph_apply_hits(self, session_id, frame_id, captured_at, hits).await
    }
    async fn graph_replace_session_links(
        &self,
        session_id: i64,
        links: &[traits::GraphSessionLinkAggregate],
    ) -> Result<bool> {
        SqliteStore::graph_replace_session_links(self, session_id, links).await
    }
    async fn graph_cluster_input(&self, now_ms: i64) -> Result<traits::GraphClusterInput> {
        SqliteStore::graph_cluster_input(self, now_ms).await
    }
    async fn graph_stored_threads(&self) -> Result<Vec<traits::StoredThread>> {
        SqliteStore::graph_stored_threads(self).await
    }
    async fn graph_thread_overrides(&self) -> Result<Vec<traits::ThreadOverrideRow>> {
        SqliteStore::graph_thread_overrides(self).await
    }
    async fn graph_insert_thread_override(
        &self,
        draft: &traits::ThreadOverrideDraft,
        now_ms: i64,
    ) -> Result<i64> {
        SqliteStore::graph_insert_thread_override(self, draft, now_ms).await
    }
    async fn graph_apply_thread_plan(&self, plan: &traits::ThreadPlan) -> Result<()> {
        SqliteStore::graph_apply_thread_plan(self, plan).await
    }
    async fn graph_table_counts(&self) -> Result<traits::GraphTableCounts> {
        SqliteStore::graph_table_counts(self).await
    }
    async fn graph_backfill_status(&self) -> Result<traits::GraphBackfillStatus> {
        SqliteStore::graph_backfill_status(self).await
    }
    async fn graph_sessions_with_summaries(
        &self,
        limit: u32,
    ) -> Result<Vec<traits::SessionSummaryInput>> {
        SqliteStore::graph_sessions_with_summaries(self, limit).await
    }
    async fn graph_entity_catalog(&self, limit: u32) -> Result<Vec<traits::EntityCatalogRow>> {
        SqliteStore::graph_entity_catalog(self, limit).await
    }
    async fn graph_apply_normalization(
        &self,
        proposals: &[traits::NormalizationProposal],
    ) -> Result<traits::NormalizationOutcome> {
        SqliteStore::graph_apply_normalization(self, proposals).await
    }

    async fn ask_resolve_entities(&self, question: &str) -> Result<Vec<AskEntityMatch>> {
        SqliteStore::ask_resolve_entities(self, question).await
    }
    async fn ask_candidate_threads(
        &self,
        window: Option<TimeRange>,
        entity_sessions: &[i64],
    ) -> Result<Vec<AskThreadCandidate>> {
        SqliteStore::ask_candidate_threads(self, window, entity_sessions).await
    }
    async fn ask_candidate_sessions(
        &self,
        threads: &[AskThreadCandidate],
        window: Option<TimeRange>,
        entity_sessions: &[i64],
    ) -> Result<Vec<i64>> {
        SqliteStore::ask_candidate_sessions(self, threads, window, entity_sessions).await
    }
    async fn ask_search_sessions(
        &self,
        question: &str,
        candidates: &[i64],
        limit: u32,
    ) -> Result<Vec<AskSessionHit>> {
        SqliteStore::ask_search_sessions(self, question, candidates, limit).await
    }
    async fn ask_search_frames(
        &self,
        query: &SearchQuery,
        sessions: &[i64],
    ) -> Result<Vec<SearchHit>> {
        SqliteStore::ask_search_frames(self, query, sessions).await
    }
    async fn ask_frame_sessions(
        &self,
        frame_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>> {
        SqliteStore::ask_frame_sessions(self, frame_ids).await
    }
    async fn ask_session_labels(
        &self,
        session_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, String>> {
        SqliteStore::ask_session_labels(self, session_ids).await
    }
    async fn ask_thread_exists(&self, thread_id: i64) -> Result<bool> {
        SqliteStore::ask_thread_exists(self, thread_id).await
    }
    async fn create_ask_thread(&self) -> Result<i64> {
        SqliteStore::create_ask_thread(self).await
    }
    async fn delete_ask_thread(&self, thread_id: i64) -> Result<bool> {
        SqliteStore::delete_ask_thread(self, thread_id).await
    }
    async fn last_ask_turn(&self, thread_id: i64) -> Result<Option<AskTurnRecord>> {
        SqliteStore::last_ask_turn(self, thread_id).await
    }
    async fn append_ask_turn(&self, turn: NewAskTurn) -> Result<AskTurnRecord> {
        SqliteStore::append_ask_turn(self, turn).await
    }
    async fn finish_ask_turn(&self, turn_id: i64, answer: &str) -> Result<bool> {
        SqliteStore::finish_ask_turn(self, turn_id, answer).await
    }
    async fn list_ask_turns(&self, thread_id: i64) -> Result<Vec<AskTurnRecord>> {
        SqliteStore::list_ask_turns(self, thread_id).await
    }
    async fn session_embed_candidates(&self, limit: u32) -> Result<Vec<SessionEmbedInput>> {
        SqliteStore::session_embed_candidates(self, limit).await
    }
    async fn session_embed_pending(&self) -> Result<u64> {
        SqliteStore::session_embed_pending(self).await
    }
    async fn upsert_session_vector(
        &self,
        session_id: i64,
        text: &str,
        emb: &Embedding,
    ) -> Result<bool> {
        SqliteStore::upsert_session_vector(self, session_id, text, emb).await
    }
    async fn pending_jobs_of_kind(&self, kind: JobKind) -> Result<u64> {
        SqliteStore::pending_jobs_of_kind(self, kind).await
    }
}
