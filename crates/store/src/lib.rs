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
    AppSuppression, ChunkSource, Embedding, EmbeddingProvider, FrameContextRow,
    FrameEnrichmentInput, FrameMeta, InsightsSummary, Job, JobKind, JobStats, Mark, NewFrame,
    NewJob, NewSession, NewSessionArtifact, OcrResult, Result, SearchHit, SearchQuery,
    SegmenterFrame, Session, SessionArtifact, SessionArtifactKind, SessionContent, SessionFilter,
    SessionFrameSample, SessionReference, TextFilterContext, TimelineBucket, VisionAnalysis,
};

mod embeddings;
mod frames;
mod insights;
mod jobs;
mod marks;
mod records;
mod schema;
mod search;
mod sessions;
mod settings;
mod timeline;

pub use schema::{EMBEDDING_DIM, FILTER_VERSION, LATEST_SCHEMA_VERSION};
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

/// Opens a connection, applies the connection-scoped pragmas, and runs migrations.
fn open_connection(conn: Connection) -> Result<Connection> {
    // WAL is a no-op on `:memory:` (returns "memory"); foreign_keys and
    // recursive_triggers are per-connection and must be set every open so the
    // cascade + vec-cleanup triggers fire (`03 §4`).
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA recursive_triggers=ON;
         PRAGMA busy_timeout=5000;",
    )?;
    let mut conn = conn;
    bootstrap_and_migrate(&mut conn)?;
    Ok(conn)
}

/// Ensures `schema_version` exists, then applies every pending migration in its
/// own transaction, advancing the tracked version forward-only (`03 §4/§12`).
fn bootstrap_and_migrate(conn: &mut Connection) -> Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")?;
    let rows: i64 = conn.query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))?;
    if rows == 0 {
        conn.execute("INSERT INTO schema_version (version) VALUES (0)", [])?;
    }
    let mut current: i32 =
        conn.query_row("SELECT version FROM schema_version", [], |r| r.get(0))?;
    let max_version = schema::MIGRATIONS
        .iter()
        .map(|(version, _)| *version)
        .max()
        .unwrap_or(0);
    debug_assert_eq!(
        schema::LATEST_SCHEMA_VERSION,
        max_version,
        "LATEST_SCHEMA_VERSION is out of sync with MIGRATIONS"
    );
    if current > max_version {
        anyhow::bail!(
            "database has newer schema_version {current}; this build supports up to {max_version}"
        );
    }
    // A migration may rebuild a table that has inbound foreign keys (v6 widens the
    // `frames.capture_trigger` CHECK, which SQLite can only do by recreating `frames`).
    // Such a rebuild is only safe with foreign keys OFF — otherwise `DROP TABLE frames`
    // fires an implicit cascade DELETE that wipes every child row. `PRAGMA foreign_keys`
    // is a no-op inside a transaction, so toggle it here, around the whole loop, per
    // SQLite's documented schema-change recipe. Only when there is work to do (the common
    // already-migrated open keeps foreign keys ON untouched).
    let had_pending = current < max_version;
    if had_pending {
        conn.execute_batch("PRAGMA foreign_keys=OFF;")?;
    }
    for (version, sql) in schema::MIGRATIONS {
        if *version > current {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute("UPDATE schema_version SET version = ?1", [version])?;
            tx.commit()?;
            current = *version;
            tracing::info!(schema_version = current, "applied store migration");
        }
    }
    if had_pending {
        // Surface any integrity breakage loudly rather than silently shipping a corrupt
        // DB, then re-enable enforcement for the connection's lifetime.
        {
            let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
            let mut rows = stmt.query([])?;
            if rows.next()?.is_some() {
                anyhow::bail!("post-migration foreign_key_check found integrity violations");
            }
        }
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    }
    Ok(())
}

impl SqliteStore {
    /// Opens an in-memory store (tests / ephemeral use, `03 §10`).
    pub fn open_in_memory() -> Result<Self> {
        register_vec_extension();
        let conn = open_connection(Connection::open_in_memory()?)?;
        Ok(Self::from_conn(conn))
    }

    /// Opens (creating if needed) the store at `path` — the on-disk
    /// `screensearch.db` (`03 §4`).
    pub fn open_path(path: &Path) -> Result<Self> {
        register_vec_extension();
        let conn = open_connection(Connection::open(path)?)?;
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
}

#[cfg(test)]
mod migration_tests {
    use super::{bootstrap_and_migrate, register_vec_extension, schema, SqliteStore};
    use rusqlite::Connection;
    use std::sync::Arc;
    use traits::{Embedding, EmbeddingProvider, SearchQuery};

    /// Applies migrations `v1..=through` to a fresh connection exactly as an older build
    /// would (each in its own transaction, foreign keys ON), leaving the DB at
    /// `schema_version = through` so the production runner can be exercised across the
    /// next upgrade with realistic pre-existing data.
    fn open_at_version(through: i32) -> Connection {
        register_vec_extension();
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA recursive_triggers=ON;")
            .expect("pragmas");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL);")
            .expect("schema_version table");
        conn.execute("INSERT INTO schema_version (version) VALUES (0)", [])
            .expect("seed version row");
        for (version, sql) in schema::MIGRATIONS.iter().filter(|(v, _)| *v <= through) {
            let tx = conn.transaction().expect("begin");
            tx.execute_batch(sql).expect("apply migration");
            tx.execute("UPDATE schema_version SET version = ?1", [version])
                .expect("bump version");
            tx.commit().expect("commit");
        }
        conn
    }

    fn fk_violation_count(conn: &Connection) -> i64 {
        let mut stmt = conn
            .prepare("PRAGMA foreign_key_check")
            .expect("prepare check");
        let mut rows = stmt.query([]).expect("run check");
        let mut n = 0;
        while rows.next().expect("row").is_some() {
            n += 1;
        }
        n
    }

    /// v6 rebuilds the `frames` table to widen the `capture_trigger` CHECK (adding the
    /// `click` / `scroll_stop` tokens). A naive rebuild with foreign keys ON would fire an
    /// implicit cascade DELETE on every child row when the old `frames` is dropped — so
    /// this proves the runner disables foreign keys around the migration: child rows must
    /// survive, the new tokens must be accepted, a bogus token must still be rejected, FK
    /// integrity must hold, and ON DELETE CASCADE must still be wired afterward.
    #[test]
    fn migration_v6_widens_capture_trigger_check_without_dropping_children() {
        let mut conn = open_at_version(5);
        // A frame at v5 plus a child row (frame_text CASCADEs on frames).
        conn.execute(
            "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path, content_hash, capture_trigger)
             VALUES (1, 100, 0, 10, 10, 'p', 'h', 'timer')",
            [],
        )
        .expect("insert frame");
        conn.execute(
            "INSERT INTO frame_text (frame_id, raw_text, content_text, primary_source, filter_version, suppressed_count)
             VALUES (1, 'r', 'c', 'ocr', 0, 0)",
            [],
        )
        .expect("insert child frame_text");

        // Upgrade to LATEST via the real runner.
        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        let version: i32 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("read version");
        assert_eq!(version, schema::LATEST_SCHEMA_VERSION);
        assert_eq!(
            version, 11,
            "latest schema is v11 (0.4.0 PR3 added the sessions tables)"
        );

        let frame_rows: i64 = conn
            .query_row("SELECT count(*) FROM frames WHERE id = 1", [], |r| r.get(0))
            .expect("count frames");
        assert_eq!(frame_rows, 1, "the frame row must survive the rebuild");
        let child_rows: i64 = conn
            .query_row(
                "SELECT count(*) FROM frame_text WHERE frame_id = 1",
                [],
                |r| r.get(0),
            )
            .expect("count children");
        assert_eq!(
            child_rows, 1,
            "child rows must NOT be cascade-deleted by the frames rebuild"
        );

        // New tokens accepted.
        conn.execute(
            "UPDATE frames SET capture_trigger = 'click' WHERE id = 1",
            [],
        )
        .expect("'click' must satisfy the widened CHECK");
        conn.execute(
            "UPDATE frames SET capture_trigger = 'scroll_stop' WHERE id = 1",
            [],
        )
        .expect("'scroll_stop' must satisfy the widened CHECK");
        // A bogus token is still rejected loudly.
        assert!(
            conn.execute(
                "UPDATE frames SET capture_trigger = 'bogus' WHERE id = 1",
                []
            )
            .is_err(),
            "an unknown token must still violate the CHECK"
        );

        // FK integrity intact, and ON DELETE CASCADE still wired post-rebuild.
        assert_eq!(
            fk_violation_count(&conn),
            0,
            "no FK violations after rebuild"
        );
        conn.execute("DELETE FROM frames WHERE id = 1", [])
            .expect("delete frame");
        let child_after: i64 = conn
            .query_row(
                "SELECT count(*) FROM frame_text WHERE frame_id = 1",
                [],
                |r| r.get(0),
            )
            .expect("count children after delete");
        assert_eq!(
            child_after, 0,
            "ON DELETE CASCADE must still fire after rebuild"
        );
    }

    /// v7 adds `frames.image_purged` for degrade-to-text retention. An existing frame
    /// (carried up from v5, through the v6 rebuild) must read back as `0` ("image
    /// present"); the column must accept `1` and reject anything else via its CHECK.
    #[test]
    fn migration_v7_adds_image_purged_present_by_default() {
        let mut conn = open_at_version(6);
        conn.execute(
            "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path, content_hash, capture_trigger)
             VALUES (1, 100, 0, 10, 10, 'p', 'h', 'timer')",
            [],
        )
        .expect("insert v6 frame");

        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        let version: i32 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("read version");
        assert_eq!(version, schema::LATEST_SCHEMA_VERSION);

        // Pre-existing row defaults to "image present".
        let purged: i64 = conn
            .query_row("SELECT image_purged FROM frames WHERE id = 1", [], |r| {
                r.get(0)
            })
            .expect("read image_purged");
        assert_eq!(purged, 0, "existing frames keep their image (purged = 0)");

        // The degrade write is accepted; a bogus value is rejected by the CHECK.
        conn.execute("UPDATE frames SET image_purged = 1 WHERE id = 1", [])
            .expect("marking image purged must satisfy the CHECK");
        assert!(
            conn.execute("UPDATE frames SET image_purged = 2 WHERE id = 1", [])
                .is_err(),
            "image_purged outside {{0,1}} must violate the CHECK"
        );
    }

    /// v8 adds a **partial** index for the degrade-to-text retention sweep. The candidate
    /// query (`frames_with_image_older_than`) must use it instead of scanning past an
    /// ever-growing prefix of already-purged rows — so the planner must pick
    /// `idx_frames_image_retention`, not the plain `captured_at` index, for that predicate.
    #[test]
    fn migration_v8_indexes_image_retention_sweep() {
        let mut conn = open_at_version(7);
        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        // The partial index exists.
        let idx: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type = 'index' AND name = 'idx_frames_image_retention'",
                [],
                |r| r.get(0),
            )
            .expect("read sqlite_master");
        assert_eq!(idx, 1, "v8 creates idx_frames_image_retention");

        // The sweep query actually uses it (the whole point of the index).
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN
                 SELECT id, captured_at, image_path, image_purged, app_hint FROM frames
                 WHERE captured_at < 1 AND image_purged = 0
                 ORDER BY captured_at ASC LIMIT 1",
                [],
                |r| r.get(3),
            )
            .expect("explain query plan");
        assert!(
            plan.contains("idx_frames_image_retention"),
            "sweep must use the partial retention index, got plan: {plan}"
        );
    }

    /// v9 (0.3.0 PR4) removes the dark-launched image-embedding lane: it DROPs the
    /// `image_embeddings` + `image_embedding_vectors` tables (the `image_embeddings_ad`
    /// vec0-sync trigger goes with its table) and deletes `embed_image` jobs in **any**
    /// state. Seeded at v8 with a frame carrying BOTH embedding lanes plus a mixed jobs
    /// queue, this proves the drop is surgical: the text lane, the surviving job kinds,
    /// the frame, and FK integrity all remain; no image-lane schema object is left behind
    /// (`docs/0.3.0.md` PR4, `03 §4`/`§13b.3`, D5).
    #[test]
    fn migration_v9_drops_image_lane_and_embed_image_jobs() {
        let mut conn = open_at_version(8);
        conn.execute(
            "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path, content_hash, capture_trigger)
             VALUES (1, 100, 0, 10, 10, 'p', 'h', 'timer')",
            [],
        )
        .expect("insert frame");
        // A valid FLOAT[768] blob (all zeros) — vec0 only stores the bytes here; no KNN runs.
        let zeros = vec![0u8; schema::EMBEDDING_DIM * 4];
        // Text lane (must survive v9): metadata row + vec0 shadow row.
        conn.execute(
            "INSERT INTO embeddings (id, frame_id, chunk_index, chunk_text, source, model, dim, content_hash)
             VALUES (1, 1, 0, 't', 'ocr', 'gemma', 768, 'ch')",
            [],
        )
        .expect("insert text embedding");
        conn.execute(
            "INSERT INTO embedding_vectors (embedding_id, embedding) VALUES (1, ?1)",
            rusqlite::params![zeros],
        )
        .expect("insert text vector");
        // Image lane (must vanish): metadata row + vec0 shadow row.
        conn.execute(
            "INSERT INTO image_embeddings (id, frame_id, model, dim)
             VALUES (1, 1, 'nomic-embed-vision-v1.5', 768)",
            [],
        )
        .expect("insert image embedding");
        conn.execute(
            "INSERT INTO image_embedding_vectors (image_embedding_id, embedding) VALUES (1, ?1)",
            rusqlite::params![zeros],
        )
        .expect("insert image vector");
        // embed_image jobs across the whole state machine, plus one survivor of each other kind.
        for (id, kind, state) in [
            (1, "embed_image", "pending"),
            (2, "embed_image", "running"),
            (3, "embed_image", "done"),
            (4, "embed_image", "dead"),
            (5, "embed_text", "pending"),
            (6, "vision_tag", "pending"),
        ] {
            conn.execute(
                "INSERT INTO jobs (id, kind, frame_id, state) VALUES (?1, ?2, 1, ?3)",
                rusqlite::params![id, kind, state],
            )
            .expect("insert job");
        }

        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        let version: i32 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("read version");
        assert_eq!(version, schema::LATEST_SCHEMA_VERSION);
        assert_eq!(
            version, 11,
            "latest schema is v11 (0.4.0 PR3 added the sessions tables)"
        );

        // No image-lane object survives — tables, trigger, or vec0 shadow tables.
        let leftovers: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE name IN ('image_embeddings', 'image_embedding_vectors', 'image_embeddings_ad')
                    OR name LIKE 'image_embedding_vectors_%'",
                [],
                |r| r.get(0),
            )
            .expect("read sqlite_master");
        assert_eq!(
            leftovers, 0,
            "the image lane must leave no schema objects behind"
        );

        // embed_image jobs gone in every state; the other kinds intact.
        let gone: i64 = conn
            .query_row(
                "SELECT count(*) FROM jobs WHERE kind = 'embed_image'",
                [],
                |r| r.get(0),
            )
            .expect("count embed_image jobs");
        assert_eq!(gone, 0, "embed_image jobs must be deleted in any state");
        let survivors: Vec<String> = conn
            .prepare("SELECT kind FROM jobs ORDER BY kind")
            .expect("prepare survivors")
            .query_map([], |r| r.get(0))
            .expect("query survivors")
            .collect::<rusqlite::Result<_>>()
            .expect("collect survivors");
        assert_eq!(
            survivors,
            ["embed_text", "vision_tag"],
            "other job kinds must survive the image-lane drop"
        );

        // Frame + text lane untouched; integrity holds.
        let frames: i64 = conn
            .query_row("SELECT count(*) FROM frames", [], |r| r.get(0))
            .expect("count frames");
        assert_eq!(frames, 1, "the frame must survive");
        let text_meta: i64 = conn
            .query_row("SELECT count(*) FROM embeddings", [], |r| r.get(0))
            .expect("count text embeddings");
        let text_vecs: i64 = conn
            .query_row("SELECT count(*) FROM embedding_vectors", [], |r| r.get(0))
            .expect("count text vectors");
        assert_eq!(
            (text_meta, text_vecs),
            (1, 1),
            "the text embedding lane must survive v9"
        );
        assert_eq!(fk_violation_count(&conn), 0, "no FK violations after v9");
    }

    /// v10 (0.3.0 PR6) adds the `marks` table (mark-this-moment; `03 §4`/`§13b.5`, D15).
    /// Seeded at v9 with a frame, this proves the migration is additive and correctly
    /// wired: the `marks` table + `idx_marks_open` index exist, a mark inserts and reads
    /// back, `ON DELETE CASCADE` removes the mark when its frame is hard-deleted (the
    /// per-frame-child convention), and FK integrity holds throughout.
    #[test]
    fn migration_v10_adds_marks_with_cascade() {
        let mut conn = open_at_version(9);
        conn.execute(
            "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path, content_hash, capture_trigger)
             VALUES (1, 100, 0, 10, 10, 'p', 'h', 'manual')",
            [],
        )
        .expect("insert frame");

        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        let version: i32 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("read version");
        assert_eq!(version, schema::LATEST_SCHEMA_VERSION);
        assert_eq!(
            version, 11,
            "latest schema is v11 (0.4.0 PR3 added the sessions tables)"
        );

        // The marks table and its open-marks index exist.
        let objs: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE (type = 'table' AND name = 'marks')
                    OR (type = 'index' AND name = 'idx_marks_open')",
                [],
                |r| r.get(0),
            )
            .expect("read sqlite_master");
        assert_eq!(objs, 2, "v10 creates the marks table + idx_marks_open");

        // A mark inserts and reads back.
        conn.execute(
            "INSERT INTO marks (id, frame_id, created_at, note) VALUES (1, 1, 200, 'ship it')",
            [],
        )
        .expect("insert mark");
        let (fid, note): (i64, String) = conn
            .query_row("SELECT frame_id, note FROM marks WHERE id = 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("read mark");
        assert_eq!((fid, note.as_str()), (1, "ship it"));

        // ON DELETE CASCADE: deleting the frame removes its mark.
        assert_eq!(fk_violation_count(&conn), 0, "no FK violations after v10");
        conn.execute("DELETE FROM frames WHERE id = 1", [])
            .expect("delete frame");
        let marks_after: i64 = conn
            .query_row("SELECT count(*) FROM marks", [], |r| r.get(0))
            .expect("count marks after frame delete");
        assert_eq!(marks_after, 0, "a mark must CASCADE with its frame (D10)");
    }

    /// Acceptance (`03 §13b.3`): a fresh DB (V1..V11 in one bootstrap) and a v8 DB upgraded
    /// by the same runner must agree on the final schema — identical object set and
    /// identical DDL text. Both paths execute the same forward-only migration chain, so
    /// the removed image lane (created in V1, dropped in V9) leaves no trace in either, and
    /// the 0.4.0 PR3 sessions objects (added in V11) appear identically on both.
    #[test]
    fn fresh_and_migrated_schemas_agree_at_latest() {
        fn schema_objects(conn: &Connection) -> Vec<(String, String, String)> {
            conn.prepare(
                "SELECT type, name, COALESCE(sql, '') FROM sqlite_master
                 WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
            )
            .expect("prepare schema query")
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("query sqlite_master")
            .collect::<rusqlite::Result<_>>()
            .expect("collect schema objects")
        }

        register_vec_extension();
        let mut fresh = Connection::open_in_memory().expect("open fresh in-memory db");
        bootstrap_and_migrate(&mut fresh).expect("fresh bootstrap to latest");

        let mut migrated = open_at_version(8);
        bootstrap_and_migrate(&mut migrated).expect("migrate v8 db to latest");

        assert_eq!(
            schema_objects(&fresh),
            schema_objects(&migrated),
            "fresh-DB and migrated-DB schemas must agree at LATEST (03 §13b.3)"
        );
    }

    /// Little-endian `FLOAT[768]` blob with `1.0` at `hot` — the byte shape sqlite-vec
    /// stores. Distinct hot indices give cosine distance 1.0, identical ones ~0.
    fn embedding_blob(hot: usize) -> Vec<u8> {
        let mut v = vec![0.0_f32; schema::EMBEDDING_DIM];
        v[hot] = 1.0;
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    /// A one-hot [`Embedding`] (dim 768) — the query-side counterpart of
    /// [`embedding_blob`], used to drive the vector arm deterministically.
    fn one_hot(hot: usize) -> Embedding {
        let mut v = vec![0.0_f32; schema::EMBEDDING_DIM];
        v[hot] = 1.0;
        Embedding(v)
    }

    /// Real-shaped schema-10 data (`docs/0.4.0.md` §3 PR3): three frames carrying
    /// `app_hint`/`window_title`/`browser_url`/`capture_trigger` (one image-purged),
    /// `frame_text` (the FTS mirrors fill via the v3 triggers), a `text_spans` row each,
    /// two marks (open + resolved), two text embeddings with their vec0 vector blobs, and
    /// a mixed jobs queue. This is what the 10 → 11 migration must carry across untouched.
    fn seed_v10_fixture(conn: &Connection) {
        conn.execute_batch(
            "INSERT INTO frames (id, captured_at, monitor_index, width, height, image_path, content_hash, app_hint, window_title, browser_url, capture_trigger, image_purged) VALUES
               (1, 1000, 0, 1920, 1080, 'frames/1.jpg', 'h1', 'Code.exe',   'store.rs screensearch',    NULL,                        'timer',             0),
               (2, 2000, 0, 1920, 1080, 'frames/2.jpg', 'h2', 'chrome.exe', 'Inbox',                    'https://mail.example.com',  'foreground_change', 0),
               (3, 3000, 0, 1920, 1080, 'frames/3.jpg', 'h3', 'Code.exe',   'lib.rs screensearch',      NULL,                        'idle',              1);

             INSERT INTO frame_text (frame_id, raw_text, content_text, primary_source, filter_version, suppressed_count) VALUES
               (1, 'quarterly invoice draft ready for review [window chrome]', 'quarterly invoice draft ready for review', 'ocr', 2, 1),
               (2, 'inbox unread messages from the team [window chrome]',       'inbox unread messages from the team',       'ocr', 2, 0),
               (3, 'editing lib.rs migration tests [window chrome]',            'editing lib.rs migration tests',            'ocr', 2, 2);

             INSERT INTO text_spans (frame_id, span_index, text, normalized_text, source, role, x, y, w, h, is_searchable, line_index) VALUES
               (1, 0, 'quarterly invoice draft', 'quarterly invoice draft', 'ocr', 'content', 0.1, 0.1, 0.5, 0.1, 1, 0),
               (2, 0, 'inbox unread messages',   'inbox unread messages',   'ocr', 'content', 0.1, 0.1, 0.5, 0.1, 1, 0),
               (3, 0, 'editing lib.rs',          'editing lib.rs',          'ocr', 'content', 0.1, 0.1, 0.5, 0.1, 1, 0);

             INSERT INTO marks (id, frame_id, created_at, note, resolved_at) VALUES
               (1, 1, 1500, 'ship it', NULL),
               (2, 2, 2500, NULL,      2600);

             INSERT INTO embeddings (id, frame_id, chunk_index, chunk_text, source, model, dim, content_hash) VALUES
               (1, 1, 0, 'quarterly invoice draft ready for review', 'ocr', 'gemma', 768, 'ce1'),
               (2, 2, 0, 'inbox unread messages from the team',       'ocr', 'gemma', 768, 'ce2');

             INSERT INTO jobs (id, kind, frame_id, state) VALUES
               (1, 'embed_text', 1, 'pending'),
               (2, 'vision_tag', 2, 'done');",
        )
        .expect("seed v10 fixture");

        // vec0 vector rows need the blob bound as a parameter.
        conn.execute(
            "INSERT INTO embedding_vectors (embedding_id, embedding) VALUES (1, ?1)",
            rusqlite::params![embedding_blob(7)],
        )
        .expect("insert vector 1");
        conn.execute(
            "INSERT INTO embedding_vectors (embedding_id, embedding) VALUES (2, ?1)",
            rusqlite::params![embedding_blob(300)],
        )
        .expect("insert vector 2");
    }

    /// v11 (0.4.0 PR3) creates the sessions structure. Seeded at v10 with real-shaped data,
    /// this proves: the version bumps by exactly one to LATEST; all five new schema objects
    /// exist; the migration is STRUCTURE ONLY — no session rows, no artifact rows, and every
    /// pre-existing frame's `session_id IS NULL` (no backfill, D4); every pre-existing row
    /// survives (incl. FTS matching); FK integrity holds.
    #[test]
    fn migration_v11_adds_sessions_structure_only() {
        let mut conn = open_at_version(10);
        seed_v10_fixture(&conn);

        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        let version: i32 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .expect("read version");
        assert_eq!(version, schema::LATEST_SCHEMA_VERSION);
        assert_eq!(
            version, 11,
            "latest schema is v11 (0.4.0 PR3 added the sessions tables)"
        );

        // All six new objects (2 tables + 4 indexes) exist by exact name and type.
        let objs: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE (type = 'table' AND name IN ('sessions', 'session_artifacts'))
                    OR (type = 'index' AND name IN ('idx_sessions_time', 'idx_frames_session', 'idx_artifacts_session', 'idx_artifacts_frame'))",
                [],
                |r| r.get(0),
            )
            .expect("read sqlite_master");
        assert_eq!(objs, 6, "v11 creates 2 tables + 4 indexes");

        // Structure only — the migration creates zero rows and backfills nothing (D4).
        let sessions: i64 = conn
            .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
            .expect("count sessions");
        assert_eq!(
            sessions, 0,
            "the migration creates no session rows (no backfill)"
        );
        let artifacts: i64 = conn
            .query_row("SELECT count(*) FROM session_artifacts", [], |r| r.get(0))
            .expect("count artifacts");
        assert_eq!(artifacts, 0, "the migration creates no artifact rows");
        let assigned: i64 = conn
            .query_row(
                "SELECT count(*) FROM frames WHERE session_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .expect("count assigned frames");
        assert_eq!(
            assigned, 0,
            "every pre-existing frame has session_id NULL (no backfill)"
        );

        // Every pre-existing row survives the migration.
        for (table, expected) in [
            ("frames", 3),
            ("frame_text", 3),
            ("text_spans", 3),
            ("marks", 2),
            ("embeddings", 2),
            ("embedding_vectors", 2),
            ("jobs", 2),
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap_or_else(|e| panic!("count {table}: {e}"));
            assert_eq!(n, expected, "{table} rows must survive the migration");
        }

        // FTS still answers over the carried-across content.
        let fts: i64 = conn
            .query_row(
                "SELECT count(*) FROM frame_text_fts WHERE frame_text_fts MATCH 'invoice'",
                [],
                |r| r.get(0),
            )
            .expect("fts match");
        assert_eq!(fts, 1, "FTS index must still match pre-existing content");

        assert_eq!(fk_violation_count(&conn), 0, "no FK violations after v11");
    }

    /// The `sessions` CHECK constraints fail loudly on invalid writes (`03 §4`): `kind` is a
    /// closed set; `tool` is NULL unless `kind='ai'` (D7); `host` is a closed set (NULL passes
    /// by design — the un-guarded `IN (…)` is the deliberate nullable-CHECK idiom); `frozen`
    /// is `{0,1}`.
    #[test]
    fn migration_v11_sessions_check_constraints() {
        let mut conn = open_at_version(10);
        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        // Valid: an AI session with a tool + host, and a focus session with both NULL.
        conn.execute(
            "INSERT INTO sessions (id, started_at, kind, tool, host, context_key, confidence)
             VALUES (1, 1000, 'ai', 'claude-code', 'terminal', 'ai:claude-code', 0.9)",
            [],
        )
        .expect("valid ai session");
        conn.execute(
            "INSERT INTO sessions (id, started_at, kind, tool, host, context_key, confidence)
             VALUES (2, 2000, 'focus', NULL, NULL, 'focus', 0.5)",
            [],
        )
        .expect("valid focus session with NULL host (nullable-CHECK idiom)");

        // Rejections.
        assert!(
            conn.execute(
                "INSERT INTO sessions (id, started_at, kind, context_key, confidence)
                 VALUES (3, 3000, 'bogus', 'k', 0.1)",
                []
            )
            .is_err(),
            "an unknown kind must violate the CHECK"
        );
        assert!(
            conn.execute(
                "INSERT INTO sessions (id, started_at, kind, tool, context_key, confidence)
                 VALUES (4, 4000, 'focus', 'claude-code', 'k', 0.1)",
                []
            )
            .is_err(),
            "a non-NULL tool with kind != 'ai' must violate the CHECK (D7)"
        );
        assert!(
            conn.execute(
                "INSERT INTO sessions (id, started_at, kind, host, context_key, confidence)
                 VALUES (5, 5000, 'focus', 'bogus', 'k', 0.1)",
                []
            )
            .is_err(),
            "an unknown host must violate the CHECK"
        );
        assert!(
            conn.execute(
                "INSERT INTO sessions (id, started_at, kind, context_key, confidence, frozen)
                 VALUES (6, 6000, 'focus', 'k', 0.1, 2)",
                []
            )
            .is_err(),
            "frozen outside {{0,1}} must violate the CHECK"
        );

        // The freeze write is accepted.
        conn.execute("UPDATE sessions SET frozen = 1 WHERE id = 1", [])
            .expect("freezing a session must satisfy the CHECK");
    }

    /// The compound `session_artifacts.role` CHECK (`03 §4`): exchanges REQUIRE a
    /// `'user'`/`'agent'` role; reserved kinds (`transcript`/`note`) forbid one. The
    /// NULL-role-on-exchange rejection is the load-bearing case — without the `role IS NOT
    /// NULL` guard, `NULL IN (…)` is NULL and SQLite would admit a roleless exchange.
    #[test]
    fn migration_v11_artifact_role_kind_coupling() {
        let mut conn = open_at_version(10);
        bootstrap_and_migrate(&mut conn).expect("migrate to latest");
        conn.execute(
            "INSERT INTO sessions (id, started_at, kind, tool, host, context_key, confidence)
             VALUES (1, 1000, 'ai', 'claude-code', 'terminal', 'ai:claude-code', 0.9)",
            [],
        )
        .expect("parent session");

        // Accepted shapes.
        for (id, kind, role) in [
            (1, "exchange", Some("user")),
            (2, "exchange", Some("agent")),
            (3, "transcript", None),
            (4, "note", None),
        ] {
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (?1, 1, ?2, ?3, 'c')",
                rusqlite::params![id, kind, role],
            )
            .unwrap_or_else(|e| panic!("valid artifact ({kind}, {role:?}) rejected: {e}"));
        }

        // Rejected shapes.
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (10, 1, 'exchange', NULL, 'c')",
                []
            )
            .is_err(),
            "an exchange with a NULL role must violate the CHECK (the load-bearing guard case)"
        );
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (11, 1, 'exchange', 'bogus', 'c')",
                []
            )
            .is_err(),
            "an exchange with an unknown role must violate the CHECK"
        );
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (12, 1, 'transcript', 'user', 'c')",
                []
            )
            .is_err(),
            "a transcript with a role must violate the CHECK"
        );
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (13, 1, 'note', 'agent', 'c')",
                []
            )
            .is_err(),
            "a note with a role must violate the CHECK"
        );
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (14, 1, 'bogus', NULL, 'c')",
                []
            )
            .is_err(),
            "an unknown kind must violate the CHECK"
        );
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (15, 1, 'transcript', NULL, NULL)",
                []
            )
            .is_err(),
            "NULL content must violate NOT NULL"
        );
        assert!(
            conn.execute(
                "INSERT INTO session_artifacts (id, session_id, kind, role, content)
                 VALUES (16, 999, 'transcript', NULL, 'c')",
                []
            )
            .is_err(),
            "a dangling session_id must violate the FK (foreign_keys ON post-migration)"
        );
    }

    /// The three v11 FK behaviors (`03 §4`, D1): deleting a session SET-NULLs
    /// `frames.session_id` (a frame SURVIVES its session) and CASCADEs its artifacts;
    /// deleting a frame SET-NULLs `session_artifacts.frame_id` (the artifact survives as
    /// session evidence).
    #[test]
    fn migration_v11_fk_set_null_and_cascade() {
        let mut conn = open_at_version(10);
        seed_v10_fixture(&conn);
        bootstrap_and_migrate(&mut conn).expect("migrate to latest");

        conn.execute(
            "INSERT INTO sessions (id, started_at, kind, tool, host, context_key, confidence)
             VALUES (1, 1000, 'ai', 'claude-code', 'terminal', 'ai:claude-code', 0.9)",
            [],
        )
        .expect("session");
        conn.execute("UPDATE frames SET session_id = 1 WHERE id IN (1, 2)", [])
            .expect("link frames to session");
        // A frames.session_id pointing at a missing session is rejected (FK, foreign_keys ON).
        assert!(
            conn.execute("UPDATE frames SET session_id = 999 WHERE id = 3", [])
                .is_err(),
            "a dangling frames.session_id must violate the FK"
        );
        conn.execute(
            "INSERT INTO session_artifacts (id, session_id, kind, role, frame_id, content)
             VALUES (1, 1, 'exchange', 'user', 1, 'q'), (2, 1, 'exchange', 'agent', 2, 'a')",
            [],
        )
        .expect("artifacts");

        // The frame-delete SET-NULL lookup rides idx_artifacts_frame, not a full scan (the point
        // of the index): SQLite finds child rows via `WHERE frame_id = ?` when nulling the FK.
        let plan: String = conn
            .query_row(
                "EXPLAIN QUERY PLAN SELECT id FROM session_artifacts WHERE frame_id = 2",
                [],
                |r| r.get(3),
            )
            .expect("explain query plan");
        assert!(
            plan.contains("idx_artifacts_frame"),
            "frame-delete FK lookup must use idx_artifacts_frame, got plan: {plan}"
        );

        // Deleting a frame SET-NULLs the artifact's frame_id (artifact survives).
        conn.execute("DELETE FROM frames WHERE id = 2", [])
            .expect("delete frame 2");
        let (rows, null_fid): (i64, i64) = conn
            .query_row(
                "SELECT count(*), count(*) FILTER (WHERE frame_id IS NULL) FROM session_artifacts WHERE id = 2",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read artifact 2");
        assert_eq!(
            (rows, null_fid),
            (1, 1),
            "artifact survives frame delete with frame_id NULL"
        );

        // Deleting the session SET-NULLs frames.session_id and CASCADEs its artifacts.
        conn.execute("DELETE FROM sessions WHERE id = 1", [])
            .expect("delete session 1");
        let frame_alive: i64 = conn
            .query_row(
                "SELECT count(*) FROM frames WHERE id = 1 AND session_id IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("read frame 1");
        assert_eq!(
            frame_alive, 1,
            "frame survives session delete with session_id NULL (D1)"
        );
        let artifacts_left: i64 = conn
            .query_row("SELECT count(*) FROM session_artifacts", [], |r| r.get(0))
            .expect("count artifacts");
        assert_eq!(artifacts_left, 0, "artifacts CASCADE with their session");

        assert_eq!(
            fk_violation_count(&conn),
            0,
            "no FK violations after the deletes"
        );
    }

    /// Deterministic stand-in embedder: every query maps to the same one-hot vector, so the
    /// vec0 KNN arm of `hybrid_search` runs identically before and after the migration.
    struct FixedEmbedder(Embedding);

    #[async_trait::async_trait]
    impl EmbeddingProvider for FixedEmbedder {
        fn dim(&self) -> usize {
            schema::EMBEDDING_DIM
        }
        async fn embed_texts(&self, inputs: &[String]) -> traits::Result<Vec<Embedding>> {
            Ok(vec![self.0.clone(); inputs.len()])
        }
    }

    /// Every store read behind the frame-level features (`03 §13c.3`, D10): search (FTS +
    /// vector arms) also backs overlay/Ask; `ocr_texts` is Ask's context assembly; `list_marks`
    /// backs marks + overlay; `recent_frame_contexts` is the kernel `where_was_i` heuristic's
    /// only store input; `timeline_buckets` + `sample_frames_in_range` + `insights_summary`
    /// back reports.
    #[allow(clippy::type_complexity)]
    async fn surface_snapshot(
        store: &SqliteStore,
    ) -> (
        Vec<traits::SearchHit>,
        std::collections::HashMap<i64, String>,
        Vec<traits::Mark>,
        Vec<traits::FrameContextRow>,
        Vec<traits::TimelineBucket>,
        Vec<traits::FrameMeta>,
        traits::InsightsSummary,
    ) {
        let hits = store
            .hybrid_search(&SearchQuery {
                text: "invoice".to_string(),
                limit: 10,
                time_range: None,
                include_chrome: false,
            })
            .await
            .expect("hybrid_search");
        let ocr = store.ocr_texts(&[1, 2, 3]).await.expect("ocr_texts");
        let marks = store.list_marks().await.expect("list_marks");
        let contexts = store
            .recent_frame_contexts(10)
            .await
            .expect("recent_frame_contexts");
        let buckets = store
            .timeline_buckets(0, 10_000, 10)
            .await
            .expect("timeline_buckets");
        let samples = store
            .sample_frames_in_range(0, 10_000, 10)
            .await
            .expect("sample_frames_in_range");
        let insights = store
            .insights_summary(0, 10_000, 10)
            .await
            .expect("insights_summary");
        (hits, ocr, marks, contexts, buckets, samples, insights)
    }

    /// D10 acceptance (`docs/0.4.0.md` §3 PR3): every frame-level surface returns identical
    /// results on the same populated fixture before and after the 10 → 11 migration. The
    /// migration only ADDs (two tables, three indexes, one nullable `frames` column) and every
    /// `frames` read uses an explicit column list, so nothing observable may change.
    #[tokio::test]
    async fn migration_v11_preserves_frame_surfaces() {
        let conn = open_at_version(10);
        seed_v10_fixture(&conn);
        let store = SqliteStore::from_conn(conn);
        store.set_embedder(Arc::new(FixedEmbedder(one_hot(7))));

        let pre = surface_snapshot(&store).await;

        // Vacuity guards — an empty ≡ empty comparison would prove nothing.
        assert!(!pre.0.is_empty(), "fixture must produce search hits");
        assert_eq!(pre.1.len(), 3, "fixture must produce ocr texts");
        assert_eq!(pre.2.len(), 2, "fixture must produce marks");
        assert!(
            !pre.3.is_empty(),
            "fixture must produce where-was-i contexts"
        );
        assert!(!pre.4.is_empty(), "fixture must produce timeline buckets");
        assert!(!pre.5.is_empty(), "fixture must produce report samples");
        assert!(pre.6.total_frames > 0, "fixture must produce insights");

        // Migrate 10 -> 11 on the same connection the store holds.
        {
            let mut guard = store.conn.lock().expect("store mutex");
            bootstrap_and_migrate(&mut guard).expect("migrate 10 -> 11");
            let v: i32 = guard
                .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
                .expect("read version");
            assert_eq!(v, schema::LATEST_SCHEMA_VERSION);
        }

        let post = surface_snapshot(&store).await;
        assert_eq!(
            pre, post,
            "D10: every frame-level surface must be identical across the 10 -> 11 migration"
        );
    }
}
