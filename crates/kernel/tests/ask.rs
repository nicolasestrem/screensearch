//! Ask v2 pipeline tests (`03 §7i`, 0.4.5 P6): the planned window, the mandated
//! non-blocking stage fallbacks, citation provenance, the `Citations`/`Done` terminal pair,
//! cancellation, and the `session_embed` backfill.
//!
//! The retrieval side is a test store: every pre-Ask method delegates to a real in-memory
//! `SqliteStore`, so frame search, OCR hydration and the job queue are the real
//! implementations, while the additive Ask v2 surface answers from a small in-memory
//! fixture. That is how the stage contract is pinned here: `crates/store`'s Ask
//! implementation is a separate slice, and this file exercises the pipeline that drives it
//! (stage order, fallbacks, citation derivation, persistence) rather than its SQL.
//!
//! Every expectation is derived from the fixture the test builds: frame ids come from the
//! inserts, windows from the persisted turn, session ids from the fixture assignment, and
//! embedded text from the session title/summary the test wrote.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chrono::{Days, Local, LocalResult, NaiveDate, TimeZone};
use image::RgbaImage;
use tokio::sync::mpsc::Sender;
use tokio::sync::Notify;
use tokio::sync::Semaphore;

use kernel::{CaptureFactory, Kernel};
use store::SqliteStore;
use traits::{
    AnswerDelta, AnswerOpts, AnswerProvider, AskEntityMatch, AskSessionHit, AskThreadCandidate,
    AskThreadScope, AskTurnRecord, AskWindowOverride, CapturedFrame, Embedding, EmbeddingProvider,
    Job, JobKind, NewAskTurn, NewFrame, NewJob, NewSession, OcrProvider, OcrResult, Readiness,
    RetrievedChunk, SearchQuery, SessionEmbedInput, SessionFilter, SessionKind, StageCounts, Store,
    TimeRange, VisionAnalysis, VisionProvider,
};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after the epoch")
        .as_millis() as i64
}

/// Unix ms of a local wall-clock instant, resolved the way the planner resolves one: an
/// ambiguous local time takes the earlier instant and a nonexistent one advances by
/// minutes (a DST gap).
fn local_ms(date: NaiveDate, hour: u32) -> i64 {
    let mut wall = date.and_hms_opt(hour, 0, 0).expect("valid wall clock");
    for _ in 0..=2880 {
        match chrono::Local.from_local_datetime(&wall) {
            LocalResult::Single(at) => return at.timestamp_millis(),
            LocalResult::Ambiguous(first, second) => return first.min(second).timestamp_millis(),
            LocalResult::None => {
                wall += chrono::Duration::minutes(1);
            }
        }
    }
    panic!("local time {wall} cannot be resolved");
}

/// The local calendar day `days_ago` days back — the anchor the tests phrase their
/// questions with.
fn date_days_ago(days_ago: u64) -> NaiveDate {
    Local::now()
        .date_naive()
        .checked_sub_days(Days::new(days_ago))
        .expect("calendar day exists")
}

// ---------------------------------------------------------------------------------------
// The store: real pre-Ask behaviour, fixture-driven Ask v2 behaviour.
// ---------------------------------------------------------------------------------------

/// A retrieval-fixture session (the real session table is seeded separately, where a test
/// needs rows there).
struct FixtureSession {
    id: i64,
    started_at: i64,
    ended_at: i64,
    label: String,
}

/// A retrieval-fixture frame: its session ownership and capture time.
#[derive(Clone)]
struct FixtureFrame {
    id: i64,
    session: Option<i64>,
    captured_at: i64,
}

/// The Ask v2 retrieval/conversation fixture.
struct AskStore {
    inner: SqliteStore,
    /// Alias/canonical text → entity id; resolution is a case-insensitive substring match.
    entities: Vec<(String, i64)>,
    /// Entity id → the sessions linked to it (`session_entities`).
    entity_sessions: HashMap<i64, Vec<i64>>,
    sessions: Vec<FixtureSession>,
    frames: Mutex<Vec<FixtureFrame>>,
    /// Whether stage 2 has winners (a summarized, vectorized history). `false` is the
    /// documented "no summaries, no vectors" fallback.
    session_winners: bool,
    threads: Mutex<HashMap<i64, Vec<AskTurnRecord>>>,
    /// The work-graph threads this fixture holds (`threads.id` → member session ids). Empty is
    /// the documented empty-graph case; the real store's window/entity narrowing of stage 1 is
    /// covered by its own tests, so this fixture returns every seeded thread as a candidate.
    work_threads: Mutex<HashMap<i64, Vec<i64>>>,
    next_thread: AtomicI64,
    next_turn: AtomicI64,
    /// The `session_vectors` rows this fixture holds, keyed by session.
    vectors: Mutex<HashMap<i64, Embedding>>,
    /// Counts real vector writes (a skipped write does not count), so a replayed batch can
    /// be shown to write nothing.
    upsert_calls: AtomicUsize,
}

impl AskStore {
    fn new() -> Self {
        Self {
            inner: SqliteStore::open_in_memory().expect("open in-memory store"),
            entities: Vec::new(),
            entity_sessions: HashMap::new(),
            sessions: Vec::new(),
            frames: Mutex::new(Vec::new()),
            session_winners: false,
            threads: Mutex::new(HashMap::new()),
            work_threads: Mutex::new(HashMap::new()),
            next_thread: AtomicI64::new(1),
            next_turn: AtomicI64::new(1),
            vectors: Mutex::new(HashMap::new()),
            upsert_calls: AtomicUsize::new(0),
        }
    }

    /// Seeds one real frame (+ OCR text, so the real hybrid search can find it) and records
    /// its fixture metadata.
    async fn seed_frame(&self, captured_at: i64, session: Option<i64>, text: &str) -> i64 {
        let frame_id = self
            .inner
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 0,
                width: 4,
                height: 4,
                image_path: format!("frames/{captured_at}.webp"),
                content_hash: format!("seed-{captured_at}-{text}"),
                app_hint: Some("Fixture".to_string()),
                window_title: None,
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .expect("seed frame");
        self.inner
            .insert_ocr(
                frame_id,
                OcrResult {
                    text: text.to_string(),
                    mean_confidence: 0.9,
                    engine: "fixture".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .expect("seed ocr");
        self.frames.lock().expect("frames lock").push(FixtureFrame {
            id: frame_id,
            session,
            captured_at,
        });
        frame_id
    }

    /// Seeds one real session row. `title`/`summary` of `None` stay NULL, which is what the
    /// `session_embed` candidate rule must skip; `Some` writes the real columns.
    async fn seed_session(
        &self,
        started_at: i64,
        title: Option<&str>,
        summary: Option<&str>,
    ) -> i64 {
        let session_id = self
            .inner
            .insert_session(NewSession {
                started_at,
                ended_at: Some(started_at + 60_000),
                kind: SessionKind::Focus,
                // `sessions` only allows a tool on an `ai` session.
                tool: None,
                host: None,
                context_key: format!("seed-{started_at}"),
                confidence: 1.0,
                frozen: false,
            })
            .await
            .expect("seed session");
        if title.is_some() || summary.is_some() {
            self.inner
                .set_session_title_summary(
                    session_id,
                    title.unwrap_or(""),
                    summary.unwrap_or(""),
                    "fixture",
                )
                .await
                .expect("seed session title/summary");
        }
        session_id
    }

    fn add_session(&mut self, id: i64, started_at: i64, ended_at: i64, label: &str) {
        self.sessions.push(FixtureSession {
            id,
            started_at,
            ended_at,
            label: label.to_string(),
        });
    }

    /// Seeds one work-graph thread with its member sessions — stage 1's candidates.
    fn add_work_thread(&self, thread_id: i64, sessions: &[i64]) {
        self.work_threads
            .lock()
            .expect("work threads lock")
            .insert(thread_id, sessions.to_vec());
    }

    fn session(&self, id: i64) -> Option<&FixtureSession> {
        self.sessions.iter().find(|session| session.id == id)
    }

    fn frame(&self, id: i64) -> Option<FixtureFrame> {
        self.frames
            .lock()
            .expect("frames lock")
            .iter()
            .find(|frame| frame.id == id)
            .cloned()
    }

    fn turn(&self, thread_id: i64) -> Vec<AskTurnRecord> {
        self.threads
            .lock()
            .expect("threads lock")
            .get(&thread_id)
            .cloned()
            .unwrap_or_default()
    }

    fn vectors(&self) -> HashMap<i64, Embedding> {
        self.vectors.lock().expect("vectors lock").clone()
    }

    fn upsert_calls(&self) -> usize {
        self.upsert_calls.load(Ordering::SeqCst)
    }
}

/// Half-open overlap of a fixture session with the planned window (`None` = all history).
fn overlaps(session: &FixtureSession, window: Option<TimeRange>) -> bool {
    match window {
        None => true,
        Some(window) => session.started_at < window.end && window.start < session.ended_at,
    }
}

/// The text one session embeds: its non-NULL, non-blank title and summary joined by `\n`.
/// `None` when neither carries text — such a session is never embedded as an empty string.
fn embed_text(title: Option<&str>, summary: Option<&str>) -> Option<String> {
    let parts: Vec<&str> = [title, summary]
        .into_iter()
        .flatten()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join("\n"))
}

#[async_trait]
impl Store for AskStore {
    // --- pre-Ask surface: the real in-memory implementation ---------------------------

    async fn insert_frame(&self, frame: NewFrame) -> traits::Result<i64> {
        self.inner.insert_frame(frame).await
    }
    async fn insert_ocr(&self, frame_id: i64, ocr: OcrResult) -> traits::Result<()> {
        self.inner.insert_ocr(frame_id, ocr).await
    }
    async fn insert_vision(&self, frame_id: i64, vision: VisionAnalysis) -> traits::Result<()> {
        self.inner.insert_vision(frame_id, vision).await
    }
    async fn upsert_text_embedding(
        &self,
        frame_id: i64,
        chunk_index: i32,
        chunk_text: &str,
        source: traits::ChunkSource,
        emb: &Embedding,
        model: &str,
    ) -> traits::Result<()> {
        self.inner
            .upsert_text_embedding(frame_id, chunk_index, chunk_text, source, emb, model)
            .await
    }
    async fn hybrid_search(&self, query: &SearchQuery) -> traits::Result<Vec<traits::SearchHit>> {
        self.inner.hybrid_search(query).await
    }
    async fn hybrid_search_in_session(
        &self,
        query: &SearchQuery,
        session_id: i64,
    ) -> traits::Result<Vec<traits::SearchHit>> {
        self.inner.hybrid_search_in_session(query, session_id).await
    }
    async fn ocr_texts(&self, frame_ids: &[i64]) -> traits::Result<HashMap<i64, String>> {
        self.inner.ocr_texts(frame_ids).await
    }
    async fn sessions_in_range(
        &self,
        filter: SessionFilter,
    ) -> traits::Result<Vec<traits::Session>> {
        self.inner.sessions_in_range(filter).await
    }
    async fn get_session(&self, id: i64) -> traits::Result<Option<traits::Session>> {
        self.inner.get_session(id).await
    }
    async fn get_enrichment_input(
        &self,
        frame_id: i64,
    ) -> traits::Result<Option<traits::FrameEnrichmentInput>> {
        self.inner.get_enrichment_input(frame_id).await
    }
    async fn enqueue_job(&self, job: NewJob) -> traits::Result<i64> {
        self.inner.enqueue_job(job).await
    }
    async fn claim_jobs(
        &self,
        kinds: &[JobKind],
        limit: u32,
        now: i64,
    ) -> traits::Result<Vec<Job>> {
        self.inner.claim_jobs(kinds, limit, now).await
    }
    async fn complete_job(&self, id: i64) -> traits::Result<()> {
        self.inner.complete_job(id).await
    }
    async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> traits::Result<()> {
        self.inner.fail_job(id, err, retry_at).await
    }
    async fn job_stats(&self) -> traits::Result<traits::JobStats> {
        self.inner.job_stats().await
    }
    async fn pending_jobs_of_kind(&self, kind: JobKind) -> traits::Result<u64> {
        self.inner.pending_jobs_of_kind(kind).await
    }
    async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> traits::Result<u64> {
        self.inner.reset_stale_running_jobs(older_than_ms).await
    }
    async fn get_setting(&self, key: &str) -> traits::Result<Option<String>> {
        self.inner.get_setting(key).await
    }
    async fn set_setting(&self, key: &str, value: &str) -> traits::Result<()> {
        self.inner.set_setting(key, value).await
    }
    async fn delete_settings(&self, keys: &[&str]) -> traits::Result<Vec<String>> {
        self.inner.delete_settings(keys).await
    }

    // --- Ask v2 planning (`03 §7i.1`) --------------------------------------------------

    async fn ask_resolve_entities(&self, question: &str) -> traits::Result<Vec<AskEntityMatch>> {
        let question = question.to_lowercase();
        Ok(self
            .entities
            .iter()
            .filter(|(text, _)| question.contains(&text.to_lowercase()))
            .map(|(text, id)| AskEntityMatch {
                entity_id: *id,
                matched: text.clone(),
            })
            .collect())
    }

    // --- Ask v2 stages (`03 §7i.2`) ----------------------------------------------------

    async fn ask_candidate_threads(
        &self,
        _window: Option<TimeRange>,
        _entity_ids: &[i64],
    ) -> traits::Result<Vec<AskThreadCandidate>> {
        // Every seeded thread with its members, ascending by thread id like the store's own
        // read. An empty map (the default) is the empty-graph case the fallback is for.
        let threads = self.work_threads.lock().expect("work threads lock");
        let mut candidates: Vec<AskThreadCandidate> = threads
            .iter()
            .map(|(thread_id, session_ids)| {
                let mut session_ids = session_ids.clone();
                session_ids.sort_unstable();
                AskThreadCandidate {
                    thread_id: *thread_id,
                    session_ids,
                }
            })
            .collect();
        candidates.sort_by_key(|candidate| candidate.thread_id);
        Ok(candidates)
    }

    async fn thread_detail(&self, thread_id: i64) -> traits::Result<Option<traits::ThreadDetail>> {
        // The scoped-Ask read (`§7i.2` stage 1). Only the member sessions matter to the kernel;
        // the thread summary and the entity list are filled from the same fixture so a caller
        // that reads them gets fixture values, never invented ones.
        let members = self
            .work_threads
            .lock()
            .expect("work threads lock")
            .get(&thread_id)
            .cloned();
        let Some(mut session_ids) = members else {
            return Ok(None);
        };
        session_ids.sort_unstable();
        let mut sessions = Vec::with_capacity(session_ids.len());
        for session_id in &session_ids {
            let session = self
                .session(*session_id)
                .expect("a work thread's member session is in the fixture");
            sessions.push(traits::ThreadSessionRef {
                session_id: *session_id,
                started_at: session.started_at,
                ended_at: Some(session.ended_at),
                kind: SessionKind::Focus.as_str().to_string(),
                title: Some(session.label.clone()),
                confidence: 1.0,
            });
        }
        let first_seen_at = sessions
            .iter()
            .map(|session| session.started_at)
            .min()
            .unwrap_or(0);
        let last_seen_at = sessions
            .iter()
            .map(|session| session.ended_at.unwrap_or(0))
            .max()
            .unwrap_or(0);
        Ok(Some(traits::ThreadDetail {
            thread: traits::ThreadSummary {
                id: thread_id,
                title: format!("thread-{thread_id}"),
                first_seen_at,
                last_seen_at,
                session_count: u32::try_from(sessions.len()).expect("fixture thread size"),
                active_ms: sessions
                    .iter()
                    .map(|session| session.ended_at.unwrap_or(0) - session.started_at)
                    .sum(),
                confidence: 1.0,
            },
            sessions,
            entities: Vec::new(),
        }))
    }

    async fn ask_candidate_sessions(
        &self,
        threads: &[AskThreadCandidate],
        window: Option<TimeRange>,
        entity_ids: &[i64],
    ) -> traits::Result<Vec<i64>> {
        let mut ids: Vec<i64> = if threads.is_empty() {
            // Documented fallback: with no thread candidates the window's sessions are the
            // candidate set.
            self.sessions
                .iter()
                .filter(|session| overlaps(session, window))
                .map(|session| session.id)
                .collect()
        } else {
            let mut ids: Vec<i64> = threads
                .iter()
                .flat_map(|thread| thread.session_ids.iter().copied())
                .collect();
            ids.sort_unstable();
            ids.dedup();
            ids
        };
        if !entity_ids.is_empty() {
            let linked: HashSet<i64> = entity_ids
                .iter()
                .flat_map(|entity| {
                    self.entity_sessions
                        .get(entity)
                        .cloned()
                        .unwrap_or_default()
                })
                .collect();
            ids.retain(|id| linked.contains(id));
        }
        Ok(ids)
    }

    async fn ask_search_sessions(
        &self,
        _question: &str,
        candidates: &[i64],
        limit: u32,
    ) -> traits::Result<Vec<AskSessionHit>> {
        if !self.session_winners {
            // No summaries and no session vectors: no session wins (the fallback path).
            return Ok(Vec::new());
        }
        Ok(candidates
            .iter()
            .take(limit as usize)
            .enumerate()
            .map(|(rank, id)| AskSessionHit {
                session_id: *id,
                score: 1.0 / (rank + 1) as f64,
            })
            .collect())
    }

    async fn ask_search_frames(
        &self,
        query: &SearchQuery,
        sessions: &[i64],
    ) -> traits::Result<Vec<traits::SearchHit>> {
        // The store's stage 3 is the existing hybrid search over the window, restricted to
        // frames owned by the winning sessions when there are any.
        let hits = self.inner.hybrid_search(query).await?;
        if sessions.is_empty() {
            return Ok(hits);
        }
        let allowed: HashSet<i64> = self
            .frames
            .lock()
            .expect("frames lock")
            .iter()
            .filter(|frame| frame.session.is_some_and(|id| sessions.contains(&id)))
            .map(|frame| frame.id)
            .collect();
        Ok(hits
            .into_iter()
            .filter(|hit| allowed.contains(&hit.frame_id))
            .collect())
    }

    async fn ask_frame_sessions(&self, frame_ids: &[i64]) -> traits::Result<HashMap<i64, i64>> {
        Ok(self
            .frames
            .lock()
            .expect("frames lock")
            .iter()
            .filter(|frame| frame_ids.contains(&frame.id))
            .filter_map(|frame| frame.session.map(|session| (frame.id, session)))
            .collect())
    }

    async fn ask_session_labels(
        &self,
        session_ids: &[i64],
    ) -> traits::Result<HashMap<i64, String>> {
        Ok(session_ids
            .iter()
            .filter_map(|id| {
                self.session(*id)
                    .map(|session| (*id, session.label.clone()))
            })
            .collect())
    }

    // --- Ask v2 conversations (`03 §7i.4`) --------------------------------------------

    async fn ask_thread_exists(&self, thread_id: i64) -> traits::Result<bool> {
        Ok(self
            .threads
            .lock()
            .expect("threads lock")
            .contains_key(&thread_id))
    }

    async fn create_ask_thread(&self) -> traits::Result<i64> {
        let id = self.next_thread.fetch_add(1, Ordering::SeqCst);
        self.threads
            .lock()
            .expect("threads lock")
            .insert(id, Vec::new());
        Ok(id)
    }

    async fn delete_ask_thread(&self, thread_id: i64) -> traits::Result<bool> {
        Ok(self
            .threads
            .lock()
            .expect("threads lock")
            .remove(&thread_id)
            .is_some())
    }

    async fn last_ask_turn(&self, thread_id: i64) -> traits::Result<Option<AskTurnRecord>> {
        Ok(self
            .threads
            .lock()
            .expect("threads lock")
            .get(&thread_id)
            .and_then(|turns| turns.last().cloned()))
    }

    async fn append_ask_turn(&self, turn: NewAskTurn) -> traits::Result<AskTurnRecord> {
        let mut threads = self.threads.lock().expect("threads lock");
        let turns = threads.entry(turn.thread_id).or_default();
        let ordinal = turns.len() as i64 + 1;
        let record = AskTurnRecord {
            id: self.next_turn.fetch_add(1, Ordering::SeqCst),
            thread_id: turn.thread_id,
            ordinal,
            question: turn.question,
            // NULL while streaming and on failure (`§7i.4`).
            answer: None,
            window: turn.window,
            resolved_entities: turn.resolved_entities,
            scope_thread_id: turn.scope_thread_id,
            citations: None,
            created_at: now_ms(),
        };
        turns.push(record.clone());
        Ok(record)
    }

    async fn finish_ask_turn(
        &self,
        turn_id: i64,
        answer: &str,
        citations: &traits::AskCitations,
    ) -> traits::Result<bool> {
        let mut threads = self.threads.lock().expect("threads lock");
        for turns in threads.values_mut() {
            if let Some(turn) = turns.iter_mut().find(|turn| turn.id == turn_id) {
                turn.answer = Some(answer.to_string());
                turn.citations = Some(citations.clone());
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn list_ask_turns(&self, thread_id: i64) -> traits::Result<Vec<AskTurnRecord>> {
        Ok(self.turn(thread_id))
    }

    // --- Ask v2 session vectors (`03 §7i.2`, D11) -------------------------------------

    async fn session_embed_candidates(&self, limit: u32) -> traits::Result<Vec<SessionEmbedInput>> {
        let sessions = self
            .inner
            .sessions_in_range(SessionFilter {
                from_ms: None,
                to_ms: None,
                kind: None,
                tool: None,
                limit: None,
                now_ms: now_ms(),
            })
            .await?;
        let vectors = self.vectors.lock().expect("vectors lock");
        Ok(sessions
            .into_iter()
            .filter(|session| !vectors.contains_key(&session.id))
            .filter_map(|session| {
                embed_text(session.title.as_deref(), session.summary.as_deref()).map(|text| {
                    SessionEmbedInput {
                        session_id: session.id,
                        text,
                    }
                })
            })
            .take(limit as usize)
            .collect())
    }

    async fn session_embed_pending(&self) -> traits::Result<u64> {
        Ok(self.session_embed_candidates(u32::MAX).await?.len() as u64)
    }

    async fn upsert_session_vector(
        &self,
        session_id: i64,
        text: &str,
        emb: &Embedding,
    ) -> traits::Result<bool> {
        // The store contract: skip when the session no longer exists or its current
        // title/summary no longer produce the text that was embedded.
        let Some(session) = self.inner.get_session(session_id).await? else {
            return Ok(false);
        };
        if embed_text(session.title.as_deref(), session.summary.as_deref()).as_deref() != Some(text)
        {
            return Ok(false);
        }
        self.upsert_calls.fetch_add(1, Ordering::SeqCst);
        self.vectors
            .lock()
            .expect("vectors lock")
            .insert(session_id, emb.clone());
        Ok(true)
    }
}

// ---------------------------------------------------------------------------------------
// Provider fakes.
// ---------------------------------------------------------------------------------------

/// Cites every frame it was given, exactly like the real provider (one citation per chunk
/// placed in the prompt), then terminates its own stream with a bare `done`.
struct CitingAnswer {
    budget: Option<u32>,
    answer: String,
}

#[async_trait]
impl AnswerProvider for CitingAnswer {
    async fn answer(
        &self,
        _query: &str,
        context: &[RetrievedChunk],
        _opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> traits::Result<()> {
        let _ = tx
            .send(AnswerDelta::Thinking {
                text: "considering".to_string(),
            })
            .await;
        let _ = tx
            .send(AnswerDelta::Token {
                text: self.answer.clone(),
            })
            .await;
        for chunk in context {
            let _ = tx
                .send(AnswerDelta::Citation {
                    frame_id: chunk.frame_id,
                })
                .await;
        }
        let _ = tx.send(AnswerDelta::done()).await;
        Ok(())
    }

    fn answer_context_budget(&self) -> Option<u32> {
        self.budget
    }
}

/// Emits one token, then waits for its channel to close — the observable end of a
/// cancelled turn (the pipeline drops the provider's sender when the consumer goes away).
struct HoldAnswer {
    closed: Arc<Notify>,
}

#[async_trait]
impl AnswerProvider for HoldAnswer {
    async fn answer(
        &self,
        _query: &str,
        _context: &[RetrievedChunk],
        _opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> traits::Result<()> {
        let _ = tx
            .send(AnswerDelta::Token {
                text: "partial".to_string(),
            })
            .await;
        tx.closed().await;
        self.closed.notify_one();
        Ok(())
    }
}

/// A provider that never observes its channel closing — the preparation phase (a model
/// download or a supervisor acquire). Aborting the task is the only thing that can stop it, so
/// the guard's drop is the proof the abort reached it.
struct StuckAnswer {
    stopped: Arc<Semaphore>,
}

struct StopGuard(Arc<Semaphore>);

impl Drop for StopGuard {
    fn drop(&mut self) {
        // A permit survives the drop, so the test cannot miss the signal the way a
        // `notify_waiters` call would if it landed before the waiter registered.
        self.0.add_permits(1);
    }
}

#[async_trait]
impl AnswerProvider for StuckAnswer {
    async fn answer(
        &self,
        _query: &str,
        _context: &[RetrievedChunk],
        _opts: AnswerOpts,
        _tx: Sender<AnswerDelta>,
    ) -> traits::Result<()> {
        let _guard = StopGuard(self.stopped.clone());
        std::future::pending::<()>().await;
        Ok(())
    }
}

/// A provider that cites a frame it was never given, next to the ones it was: the stream and
/// the terminal `Citations` list must name the same evidence.
struct OverCitingAnswer {
    phantom_frame: i64,
}

#[async_trait]
impl AnswerProvider for OverCitingAnswer {
    async fn answer(
        &self,
        _query: &str,
        context: &[RetrievedChunk],
        _opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> traits::Result<()> {
        for chunk in context {
            let _ = tx
                .send(AnswerDelta::Citation {
                    frame_id: chunk.frame_id,
                })
                .await;
        }
        let _ = tx
            .send(AnswerDelta::Citation {
                frame_id: self.phantom_frame,
            })
            .await;
        let _ = tx
            .send(AnswerDelta::Token {
                text: "grounded".to_string(),
            })
            .await;
        let _ = tx.send(AnswerDelta::done()).await;
        Ok(())
    }
}

struct NoopOcr;

#[async_trait]
impl OcrProvider for NoopOcr {
    async fn recognize(&self, _frame: &CapturedFrame) -> traits::Result<OcrResult> {
        anyhow::bail!("OCR is not part of the Ask pipeline")
    }
}

struct NoopVision;

#[async_trait]
impl VisionProvider for NoopVision {
    async fn analyze(&self, _image: &RgbaImage) -> traits::Result<VisionAnalysis> {
        anyhow::bail!("vision is not part of the Ask pipeline")
    }
}

/// Deterministic embedding per input text; counts calls so a replay can be shown to
/// re-embed nothing.
struct FixtureEmbedder {
    calls: AtomicUsize,
    inputs: Mutex<Vec<Vec<String>>>,
}

impl FixtureEmbedder {
    const DIM: usize = 768;

    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            inputs: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    fn inputs(&self) -> Vec<Vec<String>> {
        self.inputs.lock().expect("embed inputs lock").clone()
    }

    /// The vector this embedder produces for `text` — shared with the assertions.
    fn vector_for(text: &str) -> Vec<f32> {
        (0..Self::DIM)
            .map(|index| ((text.len() + index) % 11) as f32 / 11.0)
            .collect()
    }
}

#[async_trait]
impl EmbeddingProvider for FixtureEmbedder {
    fn dim(&self) -> usize {
        Self::DIM
    }
    async fn embed_texts(&self, inputs: &[String]) -> traits::Result<Vec<Embedding>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inputs
            .lock()
            .expect("embed inputs lock")
            .push(inputs.to_vec());
        Ok(inputs
            .iter()
            .map(|text| Embedding(Self::vector_for(text)))
            .collect())
    }
    fn text_model_name(&self) -> &str {
        "fixture-embed"
    }
}

// ---------------------------------------------------------------------------------------
// Harness helpers.
// ---------------------------------------------------------------------------------------

/// A kernel with the fake answer provider attached, and nothing else running: no embedder,
/// no capture source.
async fn kernel_with_answer(
    store: Arc<dyn Store>,
    answer: Arc<dyn AnswerProvider>,
) -> (Kernel, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp dir");
    let ocr: Arc<dyn OcrProvider> = Arc::new(NoopOcr);
    let factory: CaptureFactory =
        Arc::new(|_config, _rx| panic!("capture is not part of the Ask pipeline"));
    let kernel = Kernel::new(
        store,
        ocr,
        factory,
        temp.path().join("frames"),
        Readiness::default(),
    );
    kernel
        .attach_inference(Arc::new(NoopVision), answer, None, None)
        .await;
    (kernel, temp)
}

fn request(question: &str, thread_id: Option<i64>) -> traits::AskTurnRequest {
    traits::AskTurnRequest {
        query: question.to_string(),
        thread_id,
        session_id: None,
        top_k: 20,
        window_override: None,
        entity_override: None,
        thread_scope: traits::AskThreadScope::Inherit,
        opts: AnswerOpts {
            thinking: true,
            max_tokens: 512,
        },
    }
}

/// The same request with the P9 scope overrides set (`§7i.4`): the three fields the UI's scope
/// chips send, so a test names only what it changes.
fn scoped_request(
    question: &str,
    thread_id: Option<i64>,
    window_override: Option<AskWindowOverride>,
    entity_override: Option<Vec<i64>>,
    thread_scope: AskThreadScope,
) -> traits::AskTurnRequest {
    traits::AskTurnRequest {
        window_override,
        entity_override,
        thread_scope,
        ..request(question, thread_id)
    }
}

/// Drains a turn to its end, returning its ids and every delta in order.
async fn drain(mut stream: traits::AskTurnStream) -> (i64, i64, Vec<AnswerDelta>) {
    let thread_id = stream.thread_id;
    let turn_id = stream.turn_id;
    let mut deltas = Vec::new();
    while let Some(delta) = stream.deltas.recv().await {
        deltas.push(delta);
    }
    (thread_id, turn_id, deltas)
}

/// The single `citations` event of a turn: `(sessions, frames, stage counts, tokens)`.
struct CitationEvent {
    sessions: Vec<i64>,
    frames: Vec<i64>,
    stage_counts: StageCounts,
    tokens_assembled: u32,
}

fn citation_events(deltas: &[AnswerDelta]) -> Vec<CitationEvent> {
    deltas
        .iter()
        .filter_map(|delta| match delta {
            AnswerDelta::Citations {
                sessions,
                frames,
                stage_counts,
                tokens_assembled,
                ..
            } => Some(CitationEvent {
                sessions: sessions.clone(),
                frames: frames.clone(),
                stage_counts: *stage_counts,
                tokens_assembled: *tokens_assembled,
            }),
            _ => None,
        })
        .collect()
}

/// Enqueues and claims one `session_embed` job from the real queue.
async fn claim_session_embed(store: &AskStore) -> Job {
    store
        .inner
        .enqueue_job(NewJob {
            kind: JobKind::SessionEmbed,
            frame_id: None,
            priority: 0,
            max_attempts: 3,
            not_before: 0,
        })
        .await
        .expect("enqueue a session_embed job");
    let mut jobs = store
        .inner
        .claim_jobs(&[JobKind::SessionEmbed], 1, now_ms())
        .await
        .expect("claim the session_embed job");
    jobs.pop().expect("the queued job is claimable")
}

// ---------------------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------------------

/// The mandated non-blocking fallback (`03 §7i.1`): no graph threads, no session vector, so
/// stage 3 is the existing hybrid frame search over the planned window.
#[tokio::test]
async fn ask_empty_graph_answers_from_the_window_frame_search() {
    let date = date_days_ago(1);
    let day_start = local_ms(date, 0);
    let day_end = local_ms(date.checked_add_days(Days::new(1)).expect("next day"), 0);
    let mut fixture = AskStore::new();
    // Two sessions inside the window, one outside it; every frame carries the same query
    // text, so only the window (and the session assignment) can exclude any of them.
    fixture.add_session(5, day_start, day_end, "Ingest work");
    fixture.add_session(6, day_start, day_end, "Release notes");
    fixture.add_session(7, day_end, day_end + 3_600_000, "Today");
    let in_first = fixture
        .seed_frame(
            local_ms(date, 9),
            Some(5),
            &format!("zebra deployment on {date} in the morning"),
        )
        .await;
    let in_second = fixture
        .seed_frame(
            local_ms(date, 14),
            Some(6),
            &format!("zebra deployment on {date} in the afternoon"),
        )
        .await;
    let outside = fixture
        .seed_frame(
            day_end + 3_600_000,
            Some(7),
            &format!("zebra deployment on {date} reviewed again today"),
        )
        .await;

    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "The zebra deployment happened twice.".to_string(),
        }),
    )
    .await;

    let (thread_id, turn_id, deltas) = drain(
        kernel
            .ask_turn(request(&format!("zebra deployment on {date}"), None))
            .await
            .expect("the turn starts"),
    )
    .await;

    let turns = store.turn(thread_id);
    let turn = turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("the turn was persisted");
    let window = turn.window.expect("the date phrase resolves a window");
    assert_eq!(window.start, day_start, "the window is that local day");
    assert_eq!(window.end, day_end);
    assert_eq!(turn.ordinal, 1, "a new conversation starts at ordinal 1");
    assert_eq!(
        turn.answer.as_deref(),
        Some("The zebra deployment happened twice."),
        "the completed answer is persisted on the turn"
    );

    let events = citation_events(&deltas);
    assert_eq!(events.len(), 1, "exactly one citations event");
    let event = &events[0];
    let mut cited = event.frames.clone();
    cited.sort_unstable();
    let mut expected = vec![in_first, in_second];
    expected.sort_unstable();
    assert_eq!(
        cited, expected,
        "both in-window frames are cited; the out-of-window frame is not"
    );
    assert!(
        !event.frames.contains(&outside),
        "a frame outside the planned window is never cited"
    );
    assert_eq!(
        event.stage_counts.threads, 0,
        "the empty graph yields no thread candidate"
    );
    assert_eq!(
        event.stage_counts.sessions, 0,
        "no session vector: no session winner"
    );
    assert_eq!(
        event.stage_counts.frames, 2,
        "stage 3 returned exactly the in-window frames"
    );
    assert!(
        event.tokens_assembled > 0,
        "the assembled context is non-empty"
    );
    assert_eq!(
        event.sessions,
        vec![5, 6],
        "the cited sessions are the frames' sessions, distinct and ascending"
    );
    for frame_id in &event.frames {
        let frame = store.frame(*frame_id).expect("a cited frame exists");
        if let Some(session) = frame.session {
            assert!(
                event.sessions.contains(&session),
                "every cited frame's session is cited"
            );
        }
        assert!(
            window.start <= frame.captured_at && frame.captured_at < window.end,
            "every cited frame lies in the turn window"
        );
    }
    for session_id in &event.sessions {
        let session = store.session(*session_id).expect("a cited session exists");
        assert!(
            overlaps(session, Some(window)),
            "every cited session overlaps the turn window"
        );
    }
}

/// The stream's terminal pair: every existing delta keeps its meaning, exactly one
/// `citations` precedes exactly one `done`, and both carry the same numbers.
#[tokio::test]
async fn ask_citations_precede_done_exactly_once_with_the_same_numbers() {
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    fixture.add_session(5, local_ms(date, 0), local_ms(date, 23), "Ingest work");
    fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date} notes"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It is in the notes.".to_string(),
        }),
    )
    .await;

    let (_thread_id, _turn_id, deltas) = drain(
        kernel
            .ask_turn(request(&format!("zebra deployment on {date}"), None))
            .await
            .expect("the turn starts"),
    )
    .await;

    let citing: Vec<usize> = deltas
        .iter()
        .enumerate()
        .filter(|(_, delta)| matches!(delta, AnswerDelta::Citations { .. }))
        .map(|(index, _)| index)
        .collect();
    let finishing: Vec<usize> = deltas
        .iter()
        .enumerate()
        .filter(|(_, delta)| matches!(delta, AnswerDelta::Done { .. }))
        .map(|(index, _)| index)
        .collect();
    assert_eq!(citing.len(), 1, "exactly one citations event");
    assert_eq!(finishing.len(), 1, "exactly one done event");
    assert_eq!(finishing[0], deltas.len() - 1, "done is the terminal event");
    assert!(citing[0] < finishing[0], "citations precede done");
    // The existing events are forwarded unchanged, before the new terminal pair.
    let forwarded = &deltas[..citing[0]];
    assert!(forwarded
        .iter()
        .any(|delta| matches!(delta, AnswerDelta::Thinking { .. })));
    assert!(forwarded
        .iter()
        .any(|delta| matches!(delta, AnswerDelta::Token { .. })));
    assert!(forwarded
        .iter()
        .any(|delta| matches!(delta, AnswerDelta::Citation { .. })));
    // The provider's own bare `done` is replaced, never duplicated.
    let bare_done = deltas
        .iter()
        .filter(|delta| {
            matches!(
                delta,
                AnswerDelta::Done {
                    stage_counts: None,
                    tokens_assembled: None
                }
            )
        })
        .count();
    assert_eq!(bare_done, 0, "the provider's own done is swallowed");

    let (stage_counts, tokens) = match &deltas[finishing[0]] {
        AnswerDelta::Done {
            stage_counts,
            tokens_assembled,
        } => (
            stage_counts.expect("done carries the turn's stage counts"),
            tokens_assembled.expect("done carries the assembled token total"),
        ),
        other => panic!("expected done, got {other:?}"),
    };
    let event = &citation_events(&deltas)[0];
    assert_eq!(
        event.stage_counts, stage_counts,
        "the same stage counts ride both terminal events"
    );
    assert_eq!(
        event.tokens_assembled, tokens,
        "the same token total rides both terminal events"
    );
    assert!(event.stage_counts.frames >= 1);
}

/// `§7i.4`: a follow-up reuses the previous turn's window and entity scope, and a relative
/// phrase moves the inherited window instead of replacing it.
#[tokio::test]
async fn ask_follow_up_reuses_the_previous_window_and_entities() {
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    // One entity, linked to a session; the questions mention it by its canonical text.
    fixture.entities.push(("zebra".to_string(), 42));
    fixture.entity_sessions.insert(42, vec![5]);
    // The first turn's frame carries the follow-up's words too, so only the window — never
    // the text match — can exclude it from the shifted turn.
    let first = fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date} and the day before the release"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It landed.".to_string(),
        }),
    )
    .await;

    let (thread_id, first_turn_id, first_deltas) = drain(
        kernel
            .ask_turn(request(&format!("zebra deployment on {date}"), None))
            .await
            .expect("the first turn starts"),
    )
    .await;
    let turns = store.turn(thread_id);
    let first_turn = turns
        .iter()
        .find(|turn| turn.id == first_turn_id)
        .expect("the first turn was persisted");
    let first_window = first_turn.window.expect("the date phrase resolves");
    assert_eq!(
        first_turn.resolved_entities,
        vec![42],
        "the mention resolved against the graph"
    );
    assert_eq!(
        citation_events(&first_deltas)[0].frames,
        vec![first],
        "the first turn retrieves its day's frame"
    );

    // A frame in the day before the first window, seeded now that the window is known.
    let before = store
        .seed_frame(
            first_window.start - 12 * 3_600_000,
            Some(6),
            "zebra deployment and the day before it landed",
        )
        .await;

    // "and the day before?" moves the inherited window one day back and keeps the entity
    // scope.
    let (_thread, shifted_turn_id, shifted_deltas) = drain(
        kernel
            .ask_turn(request(
                "zebra deployment and the day before?",
                Some(thread_id),
            ))
            .await
            .expect("the follow-up starts"),
    )
    .await;
    let turns = store.turn(thread_id);
    let shifted = turns
        .iter()
        .find(|turn| turn.id == shifted_turn_id)
        .expect("the follow-up was persisted");
    let shifted_window = shifted.window.expect("the shift resolves");
    assert_eq!(
        shifted_window.end, first_window.start,
        "`the day before` ends where the inherited window started"
    );
    assert!(
        shifted_window.start < first_window.start && shifted_window.start < shifted_window.end,
        "the shifted window is the calendar day before the inherited one"
    );
    assert_eq!(
        shifted.ordinal,
        first_turn.ordinal + 1,
        "ordinal increments"
    );
    assert_eq!(
        shifted.resolved_entities, first_turn.resolved_entities,
        "the follow-up inherits the entity scope"
    );
    let shifted_event = &citation_events(&shifted_deltas)[0];
    assert_eq!(
        shifted_event.frames,
        vec![before],
        "the shifted window retrieves the previous day's frame only"
    );
    assert!(
        shifted_event.sessions.contains(&6),
        "the cited frame's session is cited"
    );

    // No time phrase at all: the window is inherited unchanged.
    let (_thread, reused_turn_id, reused_deltas) = drain(
        kernel
            .ask_turn(request("zebra deployment", Some(thread_id)))
            .await
            .expect("the third turn starts"),
    )
    .await;
    let turns = store.turn(thread_id);
    let reused = turns
        .iter()
        .find(|turn| turn.id == reused_turn_id)
        .expect("the third turn was persisted");
    assert_eq!(
        reused.window,
        Some(shifted_window),
        "a follow-up without a time phrase reuses the previous window verbatim"
    );
    assert_eq!(
        reused.resolved_entities,
        vec![42],
        "the entity scope is still inherited"
    );
    assert_eq!(
        citation_events(&reused_deltas)[0].frames,
        vec![before],
        "the reused window retrieves the same frame"
    );
}

/// `§7i.4`: a cancelled turn persists no answer.
#[tokio::test]
async fn ask_cancelled_turn_persists_no_answer() {
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    fixture.add_session(5, local_ms(date, 0), local_ms(date, 23), "Ingest work");
    fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let closed = Arc::new(Notify::new());
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(HoldAnswer {
            closed: closed.clone(),
        }),
    )
    .await;

    let mut stream = kernel
        .ask_turn(request(&format!("zebra deployment on {date}"), None))
        .await
        .expect("the turn starts");
    let thread_id = stream.thread_id;
    let turn_id = stream.turn_id;
    let first = stream.deltas.recv().await.expect("the first delta arrives");
    assert!(matches!(first, AnswerDelta::Token { .. }));
    // The consumer goes away mid-answer (a dropped SSE stream / a superseded request).
    drop(stream);

    tokio::time::timeout(Duration::from_secs(5), closed.notified())
        .await
        .expect("the pipeline releases the provider when the consumer leaves");

    let turns = store.turn(thread_id);
    let turn = turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("the cancelled turn row still exists");
    assert_eq!(turn.answer, None, "a cancelled turn persists no answer");
}

#[tokio::test]
async fn ask_aborts_a_provider_that_never_observes_closure() {
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    fixture.add_session(5, local_ms(date, 0), local_ms(date, 23), "Ingest work");
    fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();

    let stopped = Arc::new(Semaphore::new(0));
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(StuckAnswer {
            stopped: stopped.clone(),
        }),
    )
    .await;

    let stream = kernel
        .ask_turn(request(&format!("zebra deployment on {date}"), None))
        .await
        .expect("the turn starts");

    // The consumer leaves while the provider is still in its non-observing phase.
    drop(stream);

    let _stopped = tokio::time::timeout(Duration::from_secs(5), stopped.acquire())
        .await
        .expect("a cancelled turn aborts the provider task even when it ignores the closed channel")
        .expect("the stop signal's semaphore is open");
}

#[tokio::test]
async fn ask_entity_scope_survives_a_session_ranking_gap() {
    // Two sessions inside the window, both matching the question text. Only session 5 is
    // linked to the resolved entity. Stage 2 cannot rank anything here (no summaries, no
    // vectors), which used to make stage 3 search the whole window and cite the unrelated
    // session — discarding the entity constraint `§7i.1` establishes.
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    fixture.add_session(
        5,
        local_ms(date, 0),
        local_ms(date, 23),
        "Screensearch work",
    );
    fixture.add_session(
        7,
        local_ms(date, 0),
        local_ms(date, 23),
        "Unrelated reading",
    );
    let scoped = fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("screensearch deployment on {date}"),
        )
        .await;
    let decoy = fixture
        .seed_frame(
            local_ms(date, 11),
            Some(7),
            &format!("screensearch deployment on {date}"),
        )
        .await;
    // The entity the question names, linked only to session 5.
    fixture.entities.push(("screensearch".to_string(), 42));
    fixture.entity_sessions.insert(42, vec![5]);
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();

    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: None,
            answer: "grounded".to_string(),
        }),
    )
    .await;

    let mut stream = kernel
        .ask_turn(request(&format!("screensearch deployment on {date}"), None))
        .await
        .expect("the turn starts");

    let mut cited: Vec<i64> = Vec::new();
    while let Some(delta) = stream.deltas.recv().await {
        match delta {
            AnswerDelta::Citation { frame_id } => cited.push(frame_id),
            AnswerDelta::Done { .. } => break,
            _ => {}
        }
    }

    assert!(
        cited.contains(&scoped),
        "the entity-linked session's evidence is still retrieved"
    );
    assert!(
        !cited.contains(&decoy),
        "an unrelated session must not be cited when the mention constrained retrieval"
    );
}

#[tokio::test]
async fn ask_unusable_entity_match_falls_back_to_plain_text() {
    // The mention resolves to an entity, but none of its linked sessions survives the window.
    // `§7i.1` answers a mention with no usable match by plain-text matching, so the window's
    // evidence is still retrieved — a dead answer would be worse than the baseline.
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    fixture.add_session(
        5,
        local_ms(date, 0),
        local_ms(date, 23),
        "Unrelated reading",
    );
    let frame = fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("screensearch deployment on {date}"),
        )
        .await;
    fixture.entities.push(("screensearch".to_string(), 42));
    // Linked session 9 does not exist in the window (or at all).
    fixture.entity_sessions.insert(42, vec![9]);
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();

    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: None,
            answer: "grounded".to_string(),
        }),
    )
    .await;

    let mut stream = kernel
        .ask_turn(request(&format!("screensearch deployment on {date}"), None))
        .await
        .expect("the turn starts");

    let mut cited: Vec<i64> = Vec::new();
    while let Some(delta) = stream.deltas.recv().await {
        match delta {
            AnswerDelta::Citation { frame_id } => cited.push(frame_id),
            AnswerDelta::Done { .. } => break,
            _ => {}
        }
    }

    assert!(
        cited.contains(&frame),
        "an entity whose sessions are outside the window falls back to plain-text matching"
    );
}

#[tokio::test]
async fn ask_citations_name_exactly_the_frames_it_streamed() {
    let date = date_days_ago(1);
    let mut fixture = AskStore::new();
    fixture.add_session(5, local_ms(date, 0), local_ms(date, 23), "Ingest work");
    let frame_id = fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    // Far above any id the fixture hands out, so it can never be in the assembled context.
    let phantom_frame = frame_id + 10_000;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();

    let (kernel, _temp) =
        kernel_with_answer(dyn_store, Arc::new(OverCitingAnswer { phantom_frame })).await;

    let mut stream = kernel
        .ask_turn(request(&format!("zebra deployment on {date}"), None))
        .await
        .expect("the turn starts");

    let mut streamed: Vec<i64> = Vec::new();
    let mut terminal: Option<Vec<i64>> = None;
    while let Some(delta) = stream.deltas.recv().await {
        match delta {
            AnswerDelta::Citation { frame_id } => streamed.push(frame_id),
            AnswerDelta::Citations { frames, .. } => terminal = Some(frames),
            AnswerDelta::Done { .. } => break,
            _ => {}
        }
    }

    let terminal = terminal.expect("the terminal citations event arrives");
    assert!(
        !streamed.contains(&phantom_frame),
        "a citation the terminal event will not carry must not be streamed"
    );
    assert!(
        !terminal.contains(&phantom_frame),
        "the terminal event only names frames that were assembled"
    );
    assert_eq!(
        streamed, terminal,
        "the streamed citations and the terminal pair name the same evidence"
    );
}

/// `session_embed` (`03 §7i.2`, D11): one bounded batch embeds the sessions that carry
/// text, skips a session whose title and summary are both NULL, and replays without
/// embedding or writing anything twice.
#[tokio::test]
async fn ask_session_embed_batch_is_idempotent_and_skips_empty_sessions() {
    let title = "Scheduler fix";
    let summary = "Reconciled the worker pool and the queue.";
    let fixture = AskStore::new();
    let with_both = fixture
        .seed_session(now_ms() - 90_000, Some(title), Some(summary))
        .await;
    // Neither a title nor a summary: both columns stay NULL, so this session must never be
    // embedded as an empty string.
    let both_null = fixture.seed_session(now_ms() - 60_000, None, None).await;
    let title_only = fixture
        .seed_session(now_ms() - 30_000, Some("Standup notes"), None)
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let embedder = Arc::new(FixtureEmbedder::new());
    let provider: Arc<dyn EmbeddingProvider> = embedder.clone();
    let temp = tempfile::tempdir().expect("temp dir");

    // A real queued job, claimed from the real queue, then run through the pool's dispatch
    // seam — the same path production takes.
    let first_job = claim_session_embed(&store).await;
    kernel::process_job(&dyn_store, &provider, None, temp.path(), first_job)
        .await
        .expect("the batch completes");

    // The store owns candidate order (most recently updated first), so the batch is
    // compared as a set of texts: exactly the sessions that carry text, never the empty one.
    let expected_texts = vec![format!("{title}\n{summary}"), "Standup notes".to_string()];
    let inputs = embedder.inputs();
    assert_eq!(embedder.calls(), 1, "one embed call for one batch");
    let mut embedded = inputs[0].clone();
    embedded.sort();
    let mut expected_sorted = expected_texts.clone();
    expected_sorted.sort();
    assert_eq!(
        embedded, expected_sorted,
        "only sessions with text are embedded, never an empty string"
    );
    let vectors = store.vectors();
    assert_eq!(
        vectors.len(),
        2,
        "the title+summary-NULL session gets no `session_vectors` row"
    );
    assert!(!vectors.contains_key(&both_null));
    assert_eq!(
        vectors.get(&with_both).map(|emb| emb.0.clone()),
        Some(FixtureEmbedder::vector_for(&expected_texts[0]))
    );
    assert_eq!(
        vectors.get(&title_only).map(|emb| emb.0.clone()),
        Some(FixtureEmbedder::vector_for(&expected_texts[1]))
    );
    assert_eq!(store.upsert_calls(), 2);
    assert_eq!(
        store.session_embed_pending().await.expect("pending count"),
        0,
        "the backlog drained"
    );

    // Replay: a second job finds nothing pending and writes (and re-embeds) nothing.
    let second_job = claim_session_embed(&store).await;
    kernel::process_job(&dyn_store, &provider, None, temp.path(), second_job)
        .await
        .expect("the replayed batch completes");
    assert_eq!(embedder.calls(), 1, "nothing is re-embedded on replay");
    assert_eq!(embedder.inputs().len(), 1, "no second embed call");
    assert_eq!(store.upsert_calls(), 2, "no vector is rewritten on replay");
    assert_eq!(store.vectors().len(), 2, "the vectors are unchanged");
}

/// The legacy frame-level seam (`03 §7c`) is unchanged: the same hybrid retrieval, hydrated
/// with full OCR text, with no session context attached to any chunk.
#[tokio::test]
async fn ask_context_returns_frame_level_chunks_without_session_context() {
    let date = date_days_ago(1);
    let fixture = AskStore::new();
    let frame_id = fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "unused".to_string(),
        }),
    )
    .await;

    let context = kernel
        .ask_context(&format!("zebra deployment on {date}"), 10, None)
        .await
        .expect("the frame-level path answers");
    assert_eq!(context.len(), 1, "one matching frame");
    assert_eq!(context[0].frame_id, frame_id);
    assert!(
        context[0]
            .text
            .contains(&format!("zebra deployment on {date}")),
        "the chunk carries the frame's full OCR text"
    );
    assert_eq!(
        context[0].session, None,
        "the legacy path never attaches session context"
    );
}

/// `§7i.4`: a turn naming a conversation that does not exist is rejected, not created.
#[tokio::test]
async fn ask_unknown_thread_is_not_found() {
    let fixture = AskStore::new();
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "unused".to_string(),
        }),
    )
    .await;

    let missing = 9_999;
    match kernel
        .ask_turn(request("anything at all", Some(missing)))
        .await
    {
        Err(traits::AskTurnError::ThreadNotFound(id)) => assert_eq!(id, missing),
        Err(other) => panic!("expected thread-not-found, got {other:?}"),
        Ok(_) => panic!("a turn must not start on an unknown conversation"),
    }
    assert!(store.turn(missing).is_empty(), "no turn was appended");
}

/// Without an attached answer model the turn never starts (the route maps this to 503).
#[tokio::test]
async fn ask_without_an_answer_model_is_rejected_before_persisting() {
    let fixture = AskStore::new();
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let temp = tempfile::tempdir().expect("temp dir");
    let ocr: Arc<dyn OcrProvider> = Arc::new(NoopOcr);
    let factory: CaptureFactory =
        Arc::new(|_config, _rx| panic!("capture is not part of the Ask pipeline"));
    let kernel = Kernel::new(
        dyn_store,
        ocr,
        factory,
        temp.path().join("frames"),
        Readiness::default(),
    );

    match kernel.ask_turn(request("anything at all", None)).await {
        Err(traits::AskTurnError::NoAnswerModel) => {}
        Err(other) => panic!("expected no-answer-model, got {other:?}"),
        Ok(_) => panic!("a turn must not start without a provider"),
    }
    assert!(
        store.threads.lock().expect("threads lock").is_empty(),
        "no conversation was created for a turn that cannot run"
    );
}

/// Stage 2's winners restrict stage 3 (`03 §7i.2`), and the entity scope narrows the
/// candidate sessions before that: a matching frame outside the entity's sessions is never
/// cited, even though it lies inside the window and matches the text.
#[tokio::test]
async fn ask_entity_scope_narrows_the_winning_sessions() {
    let date = date_days_ago(1);
    let day_start = local_ms(date, 0);
    let day_end = local_ms(date.checked_add_days(Days::new(1)).expect("next day"), 0);
    let mut fixture = AskStore::new();
    fixture.session_winners = true;
    fixture.entities.push(("zebra".to_string(), 42));
    fixture.entity_sessions.insert(42, vec![5]);
    fixture.add_session(5, day_start, day_end, "Ingest work");
    fixture.add_session(6, day_start, day_end, "Other work");
    let linked = fixture
        .seed_frame(
            local_ms(date, 10),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let unlinked = fixture
        .seed_frame(
            local_ms(date, 15),
            Some(6),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It landed.".to_string(),
        }),
    )
    .await;

    let (_thread, _turn, deltas) = drain(
        kernel
            .ask_turn(request(&format!("zebra deployment on {date}"), None))
            .await
            .expect("the turn starts"),
    )
    .await;
    let event = &citation_events(&deltas)[0];
    assert_eq!(
        event.stage_counts.sessions, 1,
        "the entity scope narrowed the candidate sessions to the linked one"
    );
    assert_eq!(
        event.frames,
        vec![linked],
        "stage 3 is restricted to the winning session's frames"
    );
    assert!(
        !event.frames.contains(&unlinked),
        "a matching frame in a session the entity does not link is never cited"
    );
    assert_eq!(event.sessions, vec![5]);
}

/// The same fallback against the **real** `SqliteStore` (no fixture): the pipeline runs on
/// the landed store implementation end to end — planning, the three retrieval stages,
/// assembly, the citation pair and the persisted turn.
#[tokio::test]
async fn ask_real_store_cites_the_window_session_and_persists_the_turn() {
    let date = date_days_ago(1);
    let store = Arc::new(SqliteStore::open_in_memory().expect("open in-memory store"));
    let session = store
        .insert_session(NewSession {
            started_at: local_ms(date, 0),
            ended_at: Some(local_ms(
                date.checked_add_days(Days::new(1)).expect("next day"),
                0,
            )),
            kind: SessionKind::Focus,
            tool: None,
            host: None,
            context_key: "real-store-ask".to_string(),
            confidence: 1.0,
            frozen: false,
        })
        .await
        .expect("insert session");
    let frame = store
        .insert_frame(NewFrame {
            captured_at: local_ms(date, 10),
            monitor_index: 0,
            width: 4,
            height: 4,
            image_path: "frames/real.webp".to_string(),
            content_hash: "real-store-ask".to_string(),
            app_hint: Some("Fixture".to_string()),
            window_title: None,
            browser_url: None,
            capture_trigger: None,
        })
        .await
        .expect("insert frame");
    store
        .insert_ocr(
            frame,
            OcrResult {
                text: format!("zebra deployment on {date}"),
                mean_confidence: 0.9,
                engine: "fixture".to_string(),
                spans: Vec::new(),
            },
        )
        .await
        .expect("insert ocr");
    store
        .assign_frames_session(&[frame], Some(session))
        .await
        .expect("assign the frame to its session");

    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It landed.".to_string(),
        }),
    )
    .await;

    let (thread_id, turn_id, deltas) = drain(
        kernel
            .ask_turn(request(&format!("zebra deployment on {date}"), None))
            .await
            .expect("the turn starts"),
    )
    .await;
    let events = citation_events(&deltas);
    assert_eq!(events.len(), 1, "exactly one citations event");
    assert_eq!(events[0].frames, vec![frame]);
    assert_eq!(events[0].sessions, vec![session]);
    assert!(events[0].tokens_assembled > 0);

    let turns = store
        .list_ask_turns(thread_id)
        .await
        .expect("read the thread's turns");
    let turn = turns
        .iter()
        .find(|turn| turn.id == turn_id)
        .expect("the real store persisted the turn");
    let window = turn.window.expect("the date phrase resolves a window");
    assert!(
        window.start <= local_ms(date, 10) && local_ms(date, 10) < window.end,
        "the cited frame lies inside the persisted window"
    );
    assert_eq!(turn.answer.as_deref(), Some("It landed."));
}

// ---------------------------------------------------------------------------------------
// P9 scope overrides (`§7i.4`): window, entities, work-graph thread.
// ---------------------------------------------------------------------------------------

/// `Range` is exactly the window the request named (no planning) and `All` clears it, while a
/// request with no override still plans the question's date — so the override is provably what
/// decided the window, not a coincidence of the phrase.
#[tokio::test]
async fn ask_window_override_beats_the_planner() {
    let date = date_days_ago(1);
    let day_start = local_ms(date, 0);
    let day_end = local_ms(date.checked_add_days(Days::new(1)).expect("next day"), 0);
    let mut fixture = AskStore::new();
    fixture.add_session(5, day_start, day_end, "Ingest work");
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It landed.".to_string(),
        }),
    )
    .await;
    let question = format!("zebra deployment on {date}");

    // No override: the planner parses the date into that local day.
    let (thread_id, planned_id, _) = drain(
        kernel
            .ask_turn(request(&question, None))
            .await
            .expect("the planned turn starts"),
    )
    .await;
    let planned = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == planned_id)
        .expect("the planned turn was persisted");
    assert_eq!(
        planned.window,
        Some(TimeRange {
            start: day_start,
            end: day_end
        }),
        "without an override the question's date is the window"
    );

    // `Range` replaces it outright — the question's own date phrase is ignored.
    let ranged = TimeRange {
        start: day_start - 7 * 86_400_000,
        end: day_start - 6 * 86_400_000,
    };
    let (_, ranged_id, _) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                Some(AskWindowOverride::Range(ranged)),
                None,
                AskThreadScope::Inherit,
            ))
            .await
            .expect("the overridden turn starts"),
    )
    .await;
    let overridden = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == ranged_id)
        .expect("the overridden turn was persisted");
    assert_eq!(
        overridden.window,
        Some(ranged),
        "the explicit range is the window, not the parsed date"
    );

    // `All` clears it to the whole history (both columns NULL), again ignoring the date phrase.
    let (_, all_id, _) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                Some(AskWindowOverride::All),
                None,
                AskThreadScope::Inherit,
            ))
            .await
            .expect("the all-history turn starts"),
    )
    .await;
    let all = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == all_id)
        .expect("the all-history turn was persisted");
    assert_eq!(all.window, None, "`All` leaves both window columns NULL");
}

/// `Some([])` clears the entity scope — a mention of a known entity in the question cannot
/// re-add one — `Some(ids)` is exactly those ids, and no override still inherits the previous
/// turn's scope (`§7i.4`).
#[tokio::test]
async fn ask_entity_override_replaces_resolution_and_inheritance() {
    let date = date_days_ago(1);
    let day_start = local_ms(date, 0);
    let day_end = local_ms(date.checked_add_days(Days::new(1)).expect("next day"), 0);
    let mut fixture = AskStore::new();
    // One entity linked to one session; every question below names it in its canonical text.
    fixture.entities.push(("zebra".to_string(), 42));
    fixture.entity_sessions.insert(42, vec![5]);
    fixture.add_session(5, day_start, day_end, "Ingest work");
    fixture
        .seed_frame(
            local_ms(date, 9),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It landed.".to_string(),
        }),
    )
    .await;
    let question = format!("zebra deployment on {date}");

    let (thread_id, first_id, _) = drain(
        kernel
            .ask_turn(request(&question, None))
            .await
            .expect("the first turn starts"),
    )
    .await;
    let first = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == first_id)
        .expect("the first turn was persisted");
    assert_eq!(
        first.resolved_entities,
        vec![42],
        "the mention resolves against the graph"
    );

    // `Some([])`: the same question still mentions "zebra", and the planner must not resolve it.
    let (_, cleared_id, _) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                None,
                Some(Vec::new()),
                AskThreadScope::Inherit,
            ))
            .await
            .expect("the cleared turn starts"),
    )
    .await;
    let cleared = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == cleared_id)
        .expect("the cleared turn was persisted");
    assert!(
        cleared.resolved_entities.is_empty(),
        "`Some([])` clears the scope instead of resolving or inheriting, got {:?}",
        cleared.resolved_entities
    );

    // `Some([id])`: exactly that scope, whatever the question mentions.
    let explicit_entity = 77;
    let (_, explicit_id, _) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                None,
                Some(vec![explicit_entity]),
                AskThreadScope::Inherit,
            ))
            .await
            .expect("the explicitly scoped turn starts"),
    )
    .await;
    let explicit = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == explicit_id)
        .expect("the explicitly scoped turn was persisted");
    assert_eq!(explicit.resolved_entities, vec![explicit_entity]);

    // No override on a follow-up that mentions no entity: with nothing to resolve, the previous
    // turn's explicit scope is inherited.
    let (_, inherited_id, _) = drain(
        kernel
            .ask_turn(request("and then?", Some(thread_id)))
            .await
            .expect("the follow-up starts"),
    )
    .await;
    let inherited = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == inherited_id)
        .expect("the follow-up was persisted");
    assert_eq!(
        inherited.resolved_entities,
        vec![explicit_entity],
        "a follow-up with no override inherits the previous turn's scope"
    );
}

/// `Thread(id)` makes stage 1 exactly that work-graph thread, `None` clears the scope, and
/// `Inherit` keeps the previous turn's — with the scope persisted on every turn (`§7i.2`).
///
/// Stage 2 ranks the fixture's candidate sessions, so the scoped turn's citations show which
/// sessions stage 1 actually admitted: both threads' sessions unscoped, one thread's when scoped.
#[tokio::test]
async fn ask_thread_scope_narrows_stage_one_and_follow_ups_inherit_it() {
    let date = date_days_ago(1);
    let day_start = local_ms(date, 0);
    let day_end = local_ms(date.checked_add_days(Days::new(1)).expect("next day"), 0);
    let mut fixture = AskStore::new();
    // Stage 2 must rank the candidates, otherwise no session wins and stage 3 searches the
    // whole window — which would hide stage 1's narrowing.
    fixture.session_winners = true;
    fixture.add_session(5, day_start, day_end, "Thread one work");
    fixture.add_session(6, day_start, day_end, "Thread two work");
    let in_first = fixture
        .seed_frame(
            local_ms(date, 9),
            Some(5),
            &format!("zebra deployment on {date}"),
        )
        .await;
    let in_second = fixture
        .seed_frame(
            local_ms(date, 15),
            Some(6),
            &format!("zebra deployment on {date}"),
        )
        .await;
    fixture.add_work_thread(1, &[5]);
    fixture.add_work_thread(2, &[6]);
    let store = Arc::new(fixture);
    let dyn_store: Arc<dyn Store> = store.clone();
    let (kernel, _temp) = kernel_with_answer(
        dyn_store,
        Arc::new(CitingAnswer {
            budget: Some(8_192),
            answer: "It landed.".to_string(),
        }),
    )
    .await;
    let question = format!("zebra deployment on {date}");

    // Unscoped: both threads are stage-1 candidates and both threads' sessions are retrieved.
    let (thread_id, unscoped_id, unscoped_deltas) = drain(
        kernel
            .ask_turn(request(&question, None))
            .await
            .expect("the unscoped turn starts"),
    )
    .await;
    let unscoped = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == unscoped_id)
        .expect("the unscoped turn was persisted");
    assert_eq!(unscoped.scope_thread_id, None);
    let unscoped_event = &citation_events(&unscoped_deltas)[0];
    assert_eq!(
        unscoped_event.stage_counts.threads, 2,
        "both work threads are candidates"
    );
    assert_eq!(unscoped_event.sessions, vec![5, 6]);
    assert_eq!(unscoped_event.frames, vec![in_first, in_second]);
    assert_eq!(
        unscoped.citations,
        Some(traits::AskCitations {
            sessions: unscoped_event.sessions.clone(),
            frames: unscoped_event.frames.clone(),
        }),
        "the persisted citations are the pair the turn emitted"
    );

    // Scoped to thread 1: stage 1 is exactly that thread, so only its session and frame survive.
    let (_, scoped_id, scoped_deltas) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                None,
                None,
                AskThreadScope::Thread(1),
            ))
            .await
            .expect("the scoped turn starts"),
    )
    .await;
    let scoped = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == scoped_id)
        .expect("the scoped turn was persisted");
    assert_eq!(scoped.scope_thread_id, Some(1), "the scope is persisted");
    let scoped_event = &citation_events(&scoped_deltas)[0];
    assert_eq!(scoped_event.stage_counts.threads, 1);
    assert_eq!(
        scoped_event.sessions,
        vec![5],
        "only the scoped thread's session is retrieved"
    );
    assert_eq!(scoped_event.frames, vec![in_first]);

    // `Inherit` keeps it.
    let (_, inherited_id, inherited_deltas) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                None,
                None,
                AskThreadScope::Inherit,
            ))
            .await
            .expect("the inheriting follow-up starts"),
    )
    .await;
    let inherited = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == inherited_id)
        .expect("the inheriting follow-up was persisted");
    assert_eq!(
        inherited.scope_thread_id,
        Some(1),
        "`Inherit` reuses the previous turn's scope"
    );
    assert_eq!(citation_events(&inherited_deltas)[0].sessions, vec![5]);

    // `None` clears it, and stage 1 widens again.
    let (_, cleared_id, cleared_deltas) = drain(
        kernel
            .ask_turn(scoped_request(
                &question,
                Some(thread_id),
                None,
                None,
                AskThreadScope::None,
            ))
            .await
            .expect("the cleared turn starts"),
    )
    .await;
    let cleared = store
        .turn(thread_id)
        .into_iter()
        .find(|turn| turn.id == cleared_id)
        .expect("the cleared turn was persisted");
    assert_eq!(cleared.scope_thread_id, None, "`None` clears the scope");
    let cleared_event = &citation_events(&cleared_deltas)[0];
    assert_eq!(cleared_event.stage_counts.threads, 2);
    assert_eq!(cleared_event.sessions, vec![5, 6]);
}
