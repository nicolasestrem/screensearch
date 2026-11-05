//! The normative module contracts (`03 §3`).
//!
//! Names/shapes may be refined in impl, but these boundaries are fixed: `kernel`
//! and module crates depend on these traits, never on each other's concrete impls
//! (`03 §2`). All methods are fallible async; `Result<T>` is [`crate::Result`].

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use image::RgbaImage;
use tokio::sync::mpsc::Sender;

use crate::domain::{
    AnswerOpts, AskEntityMatch, AskSessionHit, AskThreadCandidate, CapturedFrame, ChunkSource,
    Embedding, FrameContextRow, FrameEnrichmentInput, NewAskTurn, NewFrame, OcrResult,
    RetrievedChunk, SessionEmbedInput, TextFilterContext, VisionAnalysis,
};
use crate::ipc::{
    AnswerDelta, AppSuppression, FrameMeta, InsightsSummary, Mark, PressureSample, SearchHit,
    SearchQuery, SessionReference, TimelineBucket,
};
use crate::jobs::{Job, JobKind, JobStats, NewJob};
use crate::sessions::{
    ExtractedExchange, NewSession, NewSessionArtifact, SegmentationParams, SegmenterFrame, Session,
    SessionArtifact, SessionArtifactKind, SessionContent, SessionDraft, SessionFilter,
    SessionFrameSample,
};
use crate::{MonitorInfo, Result};

/// Screen capture source (WGC impl in `capture`, `03 §3`).
#[async_trait]
pub trait CaptureSource: Send + Sync {
    fn monitors(&self) -> Vec<MonitorInfo>;
    /// Yields the next *changed* frame (diff-gated) or `None` on shutdown.
    async fn next_frame(&mut self) -> Result<Option<CapturedFrame>>;
}

/// Text recognition over a captured frame (WinRT `Media.Ocr` impl in `ocr`).
#[async_trait]
pub trait OcrProvider: Send + Sync {
    async fn recognize(&self, frame: &CapturedFrame) -> Result<OcrResult>;

    /// The capture loop has stopped. Providers may release per-session OS resources here.
    /// The UIA-text composite (`src-tauri`) uses it to disconnect its UI Automation client so
    /// Chromium/Electron apps leave accessibility mode instead of staying degraded after the
    /// user stops capture. Default no-op — the plain OCR provider holds nothing to release.
    fn on_capture_stopped(&self) {}
}

/// Dense embeddings (fastembed impl in `embeddings`, `03 §3`).
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Vector dimensionality (768).
    fn dim(&self) -> usize;
    /// NOTE: the quantized text model cannot batch — the impl embeds one input at a time.
    async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>>;
    /// Identifier of the active text model, recorded in `embeddings.model` for
    /// provenance (`03 §4`). Defaults to `"unknown"` for providers that don't track it.
    fn text_model_name(&self) -> &str {
        "unknown"
    }
}

/// Deferred vision tagging via the inference sidecar (`inference`).
#[async_trait]
pub trait VisionProvider: Send + Sync {
    async fn analyze(&self, image: &RgbaImage) -> Result<VisionAnalysis>;
}

/// Grounded, streaming RAG answers via the inference sidecar (`inference`).
#[async_trait]
pub trait AnswerProvider: Send + Sync {
    /// Streams answer deltas over `tx`; returns when complete.
    async fn answer(
        &self,
        query: &str,
        context: &[RetrievedChunk],
        opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> Result<()>;

    /// One non-streaming summarization pass for recall reports (`03 §8b`): grounds
    /// on the `content_text` chunks and returns the collected summary text plus the
    /// frame ids the model actually read (its citations). The kernel report
    /// orchestrator uses it for both the map step (per-period) and the reduce step.
    ///
    /// The default reuses [`Self::answer`] (passing `instruction` as the query) and
    /// collects its deltas — draining concurrently so the bounded delta channel can't
    /// deadlock — so a provider that only implements [`Self::answer`] still compiles and
    /// runs. **The default does NOT forward `system_prompt`**: it falls back to
    /// `answer`'s own grounding prompt, so a provider whose report quality depends on the
    /// map/reduce/final prompts must override this. `AnswerSidecar` does, honoring
    /// `system_prompt` directly.
    async fn summarize(
        &self,
        system_prompt: &str,
        instruction: &str,
        context: &[RetrievedChunk],
        opts: AnswerOpts,
    ) -> Result<(String, Vec<i64>)> {
        let query = if instruction.is_empty() {
            system_prompt
        } else {
            instruction
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel::<AnswerDelta>(64);
        let drain = async {
            let mut text = String::new();
            let mut cited: Vec<i64> = Vec::new();
            let mut err: Option<String> = None;
            while let Some(delta) = rx.recv().await {
                match delta {
                    AnswerDelta::Token { text: t } => text.push_str(&t),
                    AnswerDelta::Citation { frame_id } => {
                        if !cited.contains(&frame_id) {
                            cited.push(frame_id);
                        }
                    }
                    AnswerDelta::Error { message } => err = Some(message),
                    AnswerDelta::Thinking { .. }
                    | AnswerDelta::Done { .. }
                    | AnswerDelta::Citations { .. } => {}
                }
            }
            (text, cited, err)
        };
        let (answered, (text, cited, err)) =
            tokio::join!(self.answer(query, context, opts, tx), drain);
        answered?;
        if let Some(message) = err {
            return Err(anyhow::anyhow!(message));
        }
        Ok((text, cited))
    }

    /// A cheap, human-facing label for the active answer model (the report footer's
    /// provenance — e.g. the resolved GGUF filename). Resolves without downloading;
    /// `None` if no model is available. Default `None` for providers that don't track
    /// a model.
    async fn answer_model_label(&self) -> Option<String> {
        None
    }

    /// The effective answer-lane context window (`n_ctx`, in tokens) this provider runs
    /// with, if known. The report planner budgets its map/reduce passes against this so a
    /// user-lowered `sidecar.ctx_size` can't make it pack summaries the provider then
    /// silently truncates. `None` = unknown → the caller falls back to its own default.
    /// (The provider's own per-pass prompt bound remains the hard safety net regardless.)
    fn answer_context_budget(&self) -> Option<u32> {
        None
    }
}

/// Pure session segmentation + best-effort exchange extraction (`03 §7e`). The concrete
/// implementation lives in the `sessions` module crate; the kernel sees only this provider
/// seam, preserving the workspace dependency rule (`03 §2`).
pub trait SessionSegmenter: Send + Sync {
    /// Segments ordered or unordered frame metadata into deterministic session drafts.
    fn segment(&self, frames: &[SegmenterFrame], params: &SegmentationParams) -> Vec<SessionDraft>;

    /// Extracts only explicitly marked user/agent exchanges from filtered `content_text`.
    /// No recognized marker means no artifact; roles are never inferred.
    fn extract_exchanges(
        &self,
        tool_id: &str,
        contents: &[SessionContent],
    ) -> Vec<ExtractedExchange>;
}

/// The durable data spine: frames, OCR, embeddings, retrieval, job queue, settings
/// (SQLite + sqlite-vec + FTS5 impl in `store`, `03 §3/§4/§5`).
#[async_trait]
pub trait Store: Send + Sync {
    /// Read-only metadata validation; never initializes or repairs history.
    async fn history_readiness(&self) -> Result<crate::HistoryMetadata, crate::HistoryReadError> {
        Err(crate::HistoryReadError::Unavailable(
            crate::HistoryUnavailableReason::UnsupportedStore,
        ))
    }
    /// Bounded tuple-keyset enumeration with same-transaction full hydration.
    async fn frame_page(
        &self,
        _request: crate::FramePageRequest,
    ) -> Result<crate::StoredFramePage, crate::HistoryReadError> {
        Err(crate::HistoryReadError::Unavailable(
            crate::HistoryUnavailableReason::UnsupportedStore,
        ))
    }
    /// SQL facts only; controller intent/observations and policies are composed above Store.
    async fn stored_coverage(
        &self,
        _window: crate::EvidenceWindow,
    ) -> Result<crate::StoredCoverage, crate::HistoryReadError> {
        Err(crate::HistoryReadError::Unavailable(
            crate::HistoryUnavailableReason::UnsupportedStore,
        ))
    }
    /// Bounded overlap/status keyset enumeration over saved reports (`03 §7f.5/.6`).
    /// One read transaction: the frozen maximum ordering tuple and the candidate
    /// summaries share a snapshot; filters and window apply in SQL before the limit.
    /// Never generates, retries, delivers, writes or overlays current sources.
    async fn report_page(
        &self,
        _request: crate::ReportPageRequest,
    ) -> Result<crate::StoredReportPage, crate::HistoryReadError> {
        Err(crate::HistoryReadError::Unavailable(
            crate::HistoryUnavailableReason::UnsupportedStore,
        ))
    }
    /// Public detail projection of one stored run, or `None` (→404). The stored
    /// saved result is exposed whenever it exists, independent of phase; a
    /// malformed stored payload is a storage error, never null.
    async fn report_detail(
        &self,
        _run_id: &str,
    ) -> Result<Option<crate::ReportDetail>, crate::HistoryReadError> {
        Err(crate::HistoryReadError::Unavailable(
            crate::HistoryUnavailableReason::UnsupportedStore,
        ))
    }
    /// All revision-family receipts; missing ancestors fail closed.
    async fn list_report_family_deliveries(
        &self,
        _run_id: &str,
    ) -> Result<Vec<crate::ReportDelivery>> {
        anyhow::bail!("durable report storage is unsupported")
    }

    /// Atomically persist every chunk intent before dispatch.
    async fn create_report_deliveries(
        &self,
        _deliveries: Vec<crate::ReportDelivery>,
    ) -> Result<Vec<crate::ReportDelivery>> {
        anyhow::bail!("durable report storage is unsupported")
    }
    async fn get_report_delivery(&self, _id: &str) -> Result<Option<crate::ReportDelivery>> {
        anyhow::bail!("durable report storage is unsupported")
    }

    /// Fallible content-only evidence probe; no model needed for empty windows.
    async fn report_has_usable_content(&self, _start: i64, _end: i64) -> Result<bool> {
        anyhow::bail!("durable report storage is unsupported")
    }

    /// Recovery work is unbounded by the paginated presentation history.
    async fn list_pending_report_runs(&self) -> Result<Vec<crate::ReportRun>> {
        anyhow::bail!("durable report storage is unsupported")
    }

    /// Generated-file ownership survives report retention.
    async fn list_report_files(&self) -> Result<Vec<crate::ReportFileOwnership>> {
        anyhow::bail!("durable report storage is unsupported")
    }

    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn ensure_report_source_id(&self, _candidate: &str) -> Result<String> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn reset_report_source_id(&self, _new_id: &str) -> Result<()> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn create_report_run(&self, _run: crate::ReportRun) -> Result<crate::ReportRun> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn get_report_run(&self, _id: &str) -> Result<Option<crate::ReportRun>> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly. `kind` filters
    /// the listing on the persisted discriminator (`03 §8e`); `None` lists every kind.
    async fn list_report_runs(
        &self,
        _limit: u32,
        _kind: Option<crate::ReportRunKind>,
    ) -> Result<Vec<crate::ReportRun>> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Finished saved daily runs whose `[window_start, window_end)` lies inside `[start, end)`,
    /// oldest first by `window_start` (`03 §8e` weekly inputs).
    /// A queued/running/blocked/failed run, a finished run without a stored
    /// result and a saved weekly synthesis are not saved dailies; a range
    /// containing none is an empty list, never an error (`03 §8c`; unsupported
    /// stores fail explicitly). This is the storage-level set: manual range runs and
    /// dailies saved under another zone are included, and the weekly engine narrows it
    /// to dailies that own their local date (`kernel::reports::weekly_consumed_dailies`).
    async fn saved_daily_runs(&self, _start: i64, _end: i64) -> Result<Vec<crate::ReportRun>> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn update_report_run(&self, _run: crate::ReportRun) -> Result<()> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn claim_report_run(&self, _id: &str, _now: i64) -> Result<bool> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn recover_report_work(&self, _now: i64) -> Result<()> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn create_report_delivery(
        &self,
        _delivery: crate::ReportDelivery,
    ) -> Result<crate::ReportDelivery> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn update_report_delivery(
        &self,
        _delivery: crate::ReportDelivery,
        _expected_state: crate::ReportDeliveryState,
    ) -> Result<bool> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn resolve_report_delivery(
        &self,
        _previous: crate::ReportDelivery,
        _replacement: Option<crate::ReportDelivery>,
    ) -> Result<bool> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn list_report_deliveries(&self, _run_id: &str) -> Result<Vec<crate::ReportDelivery>> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn get_report_file(&self, _path: &str) -> Result<Option<crate::ReportFileOwnership>> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn save_report_file(&self, _ownership: crate::ReportFileOwnership) -> Result<()> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn report_evidence(&self, _frame_ids: Vec<i64>) -> Result<crate::ReportEvidence> {
        anyhow::bail!("durable report storage is unsupported")
    }
    /// Durable reports (`03 §8c`); unsupported stores fail explicitly.
    async fn prune_report_runs(&self, _before: i64) -> Result<u64> {
        anyhow::bail!("durable report storage is unsupported")
    }
    // frames + ocr + vision
    /// Production inserts atomically allocate above persisted high-water and current MAX(id).
    /// No deletion/settings reset may reduce that counter; unavailable metadata fails closed.
    async fn insert_frame(&self, f: NewFrame) -> Result<i64>;
    async fn insert_ocr(&self, frame_id: i64, ocr: OcrResult) -> Result<()>;
    /// Inserts a frame's OCR **and** applies PR3's attention filter in one
    /// transaction (`03 §3b`): classifies spans into roles, writes the filtered
    /// `content_text` (so the content FTS index is written once, never a transient
    /// unfiltered window), and bumps the chrome catalog. The capture loop calls this
    /// instead of [`Self::insert_ocr`] so embeddings (which read `content_text`) and
    /// default search operate on filtered text from the first capture. The default is
    /// the passthrough [`Self::insert_ocr`] (filter unavailable), so fakes and stores
    /// without the classifier still work.
    async fn insert_ocr_filtered(
        &self,
        frame_id: i64,
        ocr: OcrResult,
        _ctx: TextFilterContext,
    ) -> Result<()> {
        self.insert_ocr(frame_id, ocr).await
    }
    async fn insert_vision(&self, frame_id: i64, v: VisionAnalysis) -> Result<()>;

    // embeddings
    async fn upsert_text_embedding(
        &self,
        frame_id: i64,
        chunk_index: i32,
        chunk_text: &str,
        source: ChunkSource,
        emb: &Embedding,
        model: &str,
    ) -> Result<()>;

    // retrieval
    async fn hybrid_search(&self, q: &SearchQuery) -> Result<Vec<SearchHit>>;
    /// Session-scoped hybrid search backing `POST /v1/ask?session_id` (0.4.0 PR6, D12):
    /// retrieval restricted to frames owned by `session_id`, so a scoped answer cites
    /// **only** in-session frames. Concurrent sessions can overlap in wall-clock time
    /// (`06` #27/#28), so this filters by exclusive `frames.session_id` ownership, not by
    /// a time window. Default errors loudly — a store without session ownership must never
    /// silently ground a scoped ask on nothing (an `Ok(vec![])` default would do exactly
    /// that).
    async fn hybrid_search_in_session(
        &self,
        _q: &SearchQuery,
        _session_id: i64,
    ) -> Result<Vec<SearchHit>> {
        Err(anyhow::anyhow!(
            "session-scoped search not supported by this store"
        ))
    }
    /// The minimal inputs the embedding worker needs for a frame (image path + OCR
    /// text), or `None` if the frame no longer exists (`03 §5`).
    async fn get_enrichment_input(&self, frame_id: i64) -> Result<Option<FrameEnrichmentInput>>;

    /// Frame ids that have **no** vision analysis yet **and** no in-flight
    /// (`pending`/`running`) `vision_tag` job (oldest first, capped at `limit`),
    /// optionally restricted to a `[start, end)` capture window. The source for the
    /// timer/idle vision batch and the `enqueue_vision` range target (`03 §5`); the
    /// in-flight exclusion keeps a trigger from re-enqueuing already-queued frames.
    /// Default returns empty for stores that don't track vision.
    async fn untagged_frame_ids(
        &self,
        _limit: u32,
        _range: Option<(i64, i64)>,
    ) -> Result<Vec<i64>> {
        Ok(Vec::new())
    }

    /// Bulk-fetch the OCR text for many frames in **one** round-trip (avoids an N+1
    /// when hydrating grounding context for `ask`). Returns `frame_id → text` only for
    /// frames that have non-empty OCR text. Default returns empty.
    async fn ocr_texts(
        &self,
        _frame_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, String>> {
        Ok(std::collections::HashMap::new())
    }

    /// Up to `limit` frames sampled **evenly across** `[start, end)` by
    /// `captured_at` ascending (chronological), for temporal-coverage report
    /// sampling (`03 §8b`). Unlike a newest-first capped list, this spreads the
    /// sample across the whole window so a report covers the period, not just its
    /// tail. Returns all rows when the window holds `<= limit` frames; an
    /// empty/invalid window (`end <= start`) or `limit == 0` yields no rows. Default
    /// returns empty for stores without frame browsing.
    async fn sample_frames_in_range(
        &self,
        _start: i64,
        _end: i64,
        _limit: u32,
    ) -> Result<Vec<FrameMeta>> {
        Ok(Vec::new())
    }

    /// Frame-count density buckets across the half-open window `[start, end)`, split
    /// into at most `bucket_count` fixed-width buckets and returned **sparse** (only
    /// occupied buckets, ascending). Backs the `get_timeline` command (`03 §7`).
    /// Default returns empty for stores without a timeline.
    async fn timeline_buckets(
        &self,
        _start: i64,
        _end: i64,
        _bucket_count: u32,
    ) -> Result<Vec<TimelineBucket>> {
        Ok(Vec::new())
    }

    /// Truthful activity aggregates over `[start, end)` for the Insights screen
    /// (`get_insights`, P5). Default returns the honest-empty summary.
    async fn insights_summary(
        &self,
        _start: i64,
        _end: i64,
        _bucket_count: u32,
    ) -> Result<InsightsSummary> {
        Ok(InsightsSummary::default())
    }

    // job queue (see `03 §5`)
    async fn enqueue_job(&self, job: NewJob) -> Result<i64>;
    async fn claim_jobs(&self, kinds: &[JobKind], limit: u32, now: i64) -> Result<Vec<Job>>;
    async fn complete_job(&self, id: i64) -> Result<()>;
    async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> Result<()>;
    async fn job_stats(&self) -> Result<JobStats>;
    /// Count of `vision_tag` jobs currently `pending` or `running`. The idle backfill
    /// uses it as a low-watermark — it refills the queue only when in-flight vision work
    /// has drained below a threshold, rather than piling batches on top of each other.
    /// Default returns 0 for stores that don't track jobs.
    async fn pending_vision_job_count(&self) -> Result<u64> {
        Ok(0)
    }
    /// Cancels all `pending` `vision_tag` jobs, returning how many were removed
    /// (the tray/quick-menu "Stop vision tagging" action, 0.3.2 PR3; `03 §7d`). Only
    /// `pending` rows are dropped — a `running` job is left to finish (no lease to
    /// revoke), so vision work already in flight completes and is not lost. Default
    /// returns 0 for stores that don't track jobs.
    async fn cancel_pending_vision_jobs(&self) -> Result<u64> {
        Ok(0)
    }
    /// Requeues jobs stuck in `running` whose `updated_at` is older than
    /// `older_than_ms` before now (a worker died mid-job; there is no lease). Resets
    /// them to `pending` so they can be reclaimed; returns how many were requeued
    /// (`03 §6` "restart + requeue"). Passing `0` requeues *all* `running` jobs — the
    /// startup sweep, when by definition no worker is live.
    async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> Result<u64>;

    /// Per-app text-filter suppression metric over frames classified by
    /// `filter_version` (`03 §3b`): the guardrail that makes silent over-suppression
    /// observable. Default returns empty for stores without the classifier.
    async fn text_filter_stats(&self, _filter_version: i32) -> Result<Vec<AppSuppression>> {
        Ok(Vec::new())
    }

    // settings
    async fn get_setting(&self, key: &str) -> Result<Option<String>>;
    async fn set_setting(&self, key: &str, value: &str) -> Result<()>;

    /// Atomically upserts many `(key, value)` settings in a single transaction —
    /// **all land or none do**. Used by `kernel::settings::save_settings` so a crash
    /// or error mid-save cannot leave the `settings` table in a mixed state (some keys
    /// new, the rest stale), which `load_settings`' per-key default fallback would
    /// silently hide. The default applies them one-by-one via [`Self::set_setting`]
    /// (non-atomic) for stores without transaction support; `SqliteStore` overrides it
    /// with a real `BEGIN … COMMIT`.
    async fn set_settings_batch(&self, kvs: &[(String, String)]) -> Result<()> {
        for (key, value) in kvs {
            self.set_setting(key, value).await?;
        }
        Ok(())
    }

    /// Deletes the given settings keys and returns those that actually existed (were
    /// removed). Used by `kernel::settings::drop_retired_settings` to purge keys that a
    /// version retired (e.g. the 0.3.0 PR2 event triggers), so a config persisted with a
    /// removed key still loads and the stale row does not linger. Absent keys are no-ops.
    async fn delete_settings(&self, keys: &[&str]) -> Result<Vec<String>>;

    // 0.3.0 marks + flow recall (`03 §4`/`§7b`, PR6)

    /// Inserts a mark on `frame_id` (mark-this-moment; `03 §4`/`§7b`). Returns the new
    /// `mark_id`. `created_at` is passed by the caller (unix epoch ms, the kernel clock,
    /// like `claim_jobs`). The default errors — an id can't be honestly faked — so a
    /// store that opts out of marks fails loudly rather than returning a bogus 0.
    async fn insert_mark(
        &self,
        _frame_id: i64,
        _created_at: i64,
        _note: Option<String>,
    ) -> Result<i64> {
        Err(anyhow::anyhow!("marks not supported by this store"))
    }

    /// All marks, **unresolved first then newest-first within each group** (`03 §7`;
    /// the Intentions strip renders the unresolved head). Carries joined frame fields
    /// for display (no N+1). Default returns empty for stores without marks.
    async fn list_marks(&self) -> Result<Vec<Mark>> {
        Ok(Vec::new())
    }

    /// Marks a mark resolved at `resolved_at` (resolve = done, dismiss = resolve-with-
    /// no-action; same operation — `03 §7b`). Idempotent: re-resolving an already-
    /// resolved mark is a no-op (`Ok`); an unknown `mark_id` errors. Default is a no-op.
    async fn resolve_mark(&self, _mark_id: i64, _resolved_at: i64) -> Result<()> {
        Ok(())
    }

    /// Attaches/overwrites the optional one-line note on an existing mark (`set_mark_note`,
    /// `03 §7`). The mark row is inserted at hotkey-press time; the note (if any) arrives
    /// seconds later from the confirmation toast. An unknown `mark_id` errors. Default no-op.
    async fn set_mark_note(&self, _mark_id: i64, _note: &str) -> Result<()> {
        Ok(())
    }

    /// The most recent `limit` frames' context rows (newest-first by `captured_at`,
    /// ties broken by `id` descending), for the where-was-i heuristic (`03 §7b`, D9).
    /// The pure `kernel::resume` function does all filtering/exclusion, so this is a
    /// plain capped window. Default returns empty for stores without frame browsing.
    async fn recent_frame_contexts(&self, _limit: u32) -> Result<Vec<FrameContextRow>> {
        Ok(Vec::new())
    }

    // 0.4.0 sessions (`03 §4`/`§7e`, PR4). Defaults keep existing fakes/source-compatible;
    // mutating operations that need an honest id fail loudly when unsupported.

    async fn insert_session(&self, _session: NewSession) -> Result<i64> {
        Err(anyhow::anyhow!("sessions not supported by this store"))
    }

    /// Updates classification + boundaries only while `frozen = 0`. Returns whether a row changed.
    async fn update_unfrozen_session(&self, _id: i64, _session: NewSession) -> Result<bool> {
        Ok(false)
    }

    /// Deletes only while `frozen = 0`. Frames survive via `ON DELETE SET NULL`.
    async fn delete_unfrozen_session(&self, _id: i64) -> Result<bool> {
        Ok(false)
    }

    /// Bulk assignment. Implementations must never rewrite a frame owned by a frozen session.
    async fn assign_frames_session(
        &self,
        _frame_ids: &[i64],
        _session_id: Option<i64>,
    ) -> Result<u64> {
        Ok(0)
    }

    /// Freezes closed sessions whose end is older than `older_than_ms`.
    async fn freeze_sessions(&self, _older_than_ms: i64) -> Result<u64> {
        Ok(0)
    }

    async fn unfrozen_sessions(&self) -> Result<Vec<Session>> {
        Ok(Vec::new())
    }

    async fn sessions_in_range(&self, _filter: SessionFilter) -> Result<Vec<Session>> {
        Ok(Vec::new())
    }

    async fn get_session(&self, _id: i64) -> Result<Option<Session>> {
        Ok(None)
    }

    async fn session_frame_sample(
        &self,
        _session_id: i64,
        _limit: u32,
    ) -> Result<SessionFrameSample> {
        Ok(SessionFrameSample {
            total_count: 0,
            frames: Vec::new(),
        })
    }

    async fn session_reference_for_frame(
        &self,
        _frame_id: i64,
    ) -> Result<Option<SessionReference>> {
        Ok(None)
    }

    async fn session_has_usable_content(&self, _session_id: i64) -> Result<bool> {
        Ok(false)
    }

    async fn session_frames_meta(&self, _session_id: i64) -> Result<Vec<SegmenterFrame>> {
        Ok(Vec::new())
    }

    async fn frames_meta_in_range(
        &self,
        _from_ms: i64,
        _to_ms: i64,
    ) -> Result<Vec<SegmenterFrame>> {
        Ok(Vec::new())
    }

    async fn content_texts_for_frames(&self, _frame_ids: &[i64]) -> Result<Vec<SessionContent>> {
        Ok(Vec::new())
    }

    async fn insert_session_artifacts(
        &self,
        _session_id: i64,
        artifacts: &[NewSessionArtifact],
    ) -> Result<Vec<i64>> {
        if artifacts.is_empty() {
            Ok(Vec::new())
        } else {
            Err(anyhow::anyhow!(
                "session artifacts not supported by this store"
            ))
        }
    }

    async fn list_session_artifacts(&self, _session_id: i64) -> Result<Vec<SessionArtifact>> {
        Ok(Vec::new())
    }

    async fn delete_session_artifacts_by_kind(
        &self,
        _session_id: i64,
        _kind: SessionArtifactKind,
    ) -> Result<u64> {
        Ok(0)
    }

    async fn set_session_title_summary(
        &self,
        _id: i64,
        _title: &str,
        _summary: &str,
        _model: &str,
    ) -> Result<bool> {
        Ok(false)
    }

    async fn clear_session_title_summary(&self, _id: i64) -> Result<bool> {
        Ok(false)
    }

    /// Injects (or replaces) the query-embedding provider that lights up the vector
    /// arm of [`Self::hybrid_search`]. Set once the model has finished loading off
    /// the launch thread (`03 §5`). Default is a no-op for stores that never embed.
    fn set_embedder(&self, _embedder: Arc<dyn EmbeddingProvider>) {}

    // 0.4.5 work graph (`03 §7h`, P5). Every method carries a default body so no existing
    // implementation or test fake breaks; `SqliteStore` overrides them in
    // `crates/store/src/graph.rs`. The defaults are the honest "this store does not
    // implement the work graph" behaviour, never fabricated data.

    /// Everything heuristic extraction may read for one frame (`03 §7h.2`), including the
    /// filtered content text and the session the frame belongs to. Ownership is authoritative
    /// (`03 §7e`): `session` is `Some` only when `frames.session_id` resolves to an existing
    /// session row. An unowned frame contributes its entities but no link row; the `03 §7h.3`
    /// interval rule is deliberately not used for linking because it cannot be reconciled with a
    /// per-frame ledger (the `03 §7e`/`03 §7h.3` conflict resolution, `06` #41, revised by the
    /// #157 review fix).
    async fn graph_frame_input(&self, _frame_id: i64) -> Result<Option<crate::GraphFrameInput>> {
        Ok(None)
    }

    /// The session whose `[started_at, ended_at)` contains `at_ms`; an open session is
    /// open-ended (`03 §7h.3`). This is the interval primitive, retained on the store contract
    /// for the recorded rule and its tests; **linking does not use it** (see
    /// [`Self::graph_frame_input`]). Default returns no session for stores without sessions.
    async fn graph_session_at(&self, _at_ms: i64) -> Result<Option<crate::GraphSessionInput>> {
        Ok(None)
    }

    /// Frame ids greater than `after_id` (exclusive; `None` starts from the beginning),
    /// oldest first, bounded. The backfill enqueues these as real `graph_extract` jobs; the
    /// cursor itself is the queue's `MAX(jobs.frame_id)` (`03 §7h.6`). Default returns empty.
    async fn graph_frame_ids_after(&self, _after_id: Option<i64>, _limit: u32) -> Result<Vec<i64>> {
        Ok(Vec::new())
    }

    /// Upserts one frame's hits and its session links in one transaction (`03 §7h.2/.3`):
    /// entities dedup on `UNIQUE(kind, normalized_key)`, links on the
    /// `(session_id, entity_id)` primary key, `hit_count` accumulates one per processed
    /// frame, and link confidence is the maximum of the contributing hits. A frame with no
    /// containing session still contributes its entities, with no link row.
    ///
    /// `hit_count` is exact for a normal single extraction pass. The schema has no per-frame
    /// contribution ledger (0.4.5 D2 keeps it frozen), so a job that commits and then loses
    /// its completion acknowledgement can increment a link twice when the queue replays it;
    /// rows never duplicate. Recorded in `07` and not worked around silently.
    async fn graph_apply_hits(
        &self,
        _session_id: Option<i64>,
        _frame_id: i64,
        _captured_at: i64,
        _hits: &[crate::GraphHit],
    ) -> Result<()> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// Replaces one session's entity links with an exactly recomputed set in one transaction
    /// (`03 §7h.3`, P5 review fix). The kernel rebuilds the links after a session ownership change
    /// (frames assigned or moved, session deleted and re-created) so a link that cascaded away or
    /// missed the first extraction is restored, and a stale link from a moved frame is removed.
    /// Existing entity rows are widened, never rewritten or deleted; only this session's
    /// `session_entities` rows are replaced. Returns `true` only when the link set actually
    /// changed, so an unchanged rebuild cannot trigger a needless re-cluster. Default fails as
    /// unsupported.
    async fn graph_replace_session_links(
        &self,
        _session_id: i64,
        _links: &[crate::GraphSessionLinkAggregate],
    ) -> Result<bool> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// The complete input to one clustering run (`03 §7h.4`): sessions and every
    /// session-entity link with the entity display data the title rule needs.
    async fn graph_cluster_input(&self, _now_ms: i64) -> Result<crate::GraphClusterInput> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// Persisted threads with their current members, read before planning (`03 §7h.4`).
    async fn graph_stored_threads(&self) -> Result<Vec<crate::StoredThread>> {
        Ok(Vec::new())
    }

    /// `thread_overrides` rows in `(created_at, id)` order (`03 §7h.5`).
    async fn graph_thread_overrides(&self) -> Result<Vec<crate::ThreadOverrideRow>> {
        Ok(Vec::new())
    }

    /// Inserts one typed override and enqueues `graph_cluster` in the same transaction, so
    /// an override always wins on the next clustering pass (`03 §7h.5`). Nothing in P5's
    /// production code calls this; P9's UI and the P5 tests do.
    async fn graph_insert_thread_override(
        &self,
        _draft: &crate::ThreadOverrideDraft,
        _now_ms: i64,
    ) -> Result<i64> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// Persists one planned thread model in a single transaction (`03 §7h.4/.5`): upserts
    /// threads by `cluster_key`, syncs the `thread_sessions` rows, applies merge-override
    /// rewrites, and deletes the threads that no longer exist.
    async fn graph_apply_thread_plan(&self, _plan: &crate::ThreadPlan) -> Result<()> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// The five table counts logged when the backfill completes (`03 §7h.6`). Default fails
    /// explicitly rather than returning zeros: a zero count would read as a completed empty
    /// graph, which is not what "this store has no work graph" means.
    async fn graph_table_counts(&self) -> Result<crate::GraphTableCounts> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// The queue-derived backfill checkpoint and in-flight counts (`03 §7h.6`). Default
    /// fails explicitly for the same reason as [`Self::graph_table_counts`].
    async fn graph_backfill_status(&self) -> Result<crate::GraphBackfillStatus> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    /// Sessions with a cached title or summary, most recent first, bounded (`03 §7h.2`
    /// normalization input). Default returns empty for stores without sessions.
    async fn graph_sessions_with_summaries(
        &self,
        _limit: u32,
    ) -> Result<Vec<crate::SessionSummaryInput>> {
        Ok(Vec::new())
    }

    /// The existing entity catalog plus aliases the normalization pass proposes against
    /// (`03 §7h.2`). Default returns empty.
    async fn graph_entity_catalog(&self, _limit: u32) -> Result<Vec<crate::EntityCatalogRow>> {
        Ok(Vec::new())
    }

    /// Applies validated normalization proposals in one transaction (`03 §7h.2`): new
    /// aliases are written with `source='llm'`, new rows are `entities.source='llm'`, and a
    /// heuristic entity's `normalized_key` is never rewritten or deleted.
    async fn graph_apply_normalization(
        &self,
        _proposals: &[crate::NormalizationProposal],
    ) -> Result<crate::NormalizationOutcome> {
        anyhow::bail!("work-graph storage is unsupported")
    }

    // ---- Ask v2 (`03 §7i`) ---------------------------------------------------------------
    //
    // Every method below is additive and default-bodied: an existing implementation or test
    // fake keeps compiling and behaving exactly as before, and a store without these
    // capabilities degrades to Ask v2's documented non-blocking fallback (`03 §7i.1`) rather
    // than failing. No existing method's signature or semantics change (`03 §7c`, D12).

    /// Entity mentions in `question` resolved against `entities.canonical_name` and
    /// `entity_aliases.alias` (`03 §7i.1`, `§7h.2`). A mention with no matching graph row is
    /// simply absent — the pipeline falls back to plain text matching, so a database whose
    /// graph tables are empty still answers. Default returns nothing (no graph).
    async fn ask_resolve_entities(&self, _question: &str) -> Result<Vec<AskEntityMatch>> {
        Ok(Vec::new())
    }

    /// Stage 1 (`03 §7i.2`): threads whose `[first_seen_at, last_seen_at]` overlaps
    /// `window`, plus threads containing a session linked to any of `entity_ids` (resolved
    /// **entity** ids, not session ids — the store joins them through `session_entities`).
    /// `window: None` means the whole history, so every thread is a candidate. Default
    /// returns nothing.
    async fn ask_candidate_threads(
        &self,
        _window: Option<crate::TimeRange>,
        _entity_ids: &[i64],
    ) -> Result<Vec<AskThreadCandidate>> {
        Ok(Vec::new())
    }

    /// The session candidate set fed to stage 2: the member sessions of `threads` that
    /// overlap `window`, narrowed to the sessions linked to `entity_ids` when an entity
    /// resolved. With no thread candidates the window's sessions are the set (the documented
    /// fallback). Ascending session ids. Default returns nothing.
    async fn ask_candidate_sessions(
        &self,
        _threads: &[AskThreadCandidate],
        _window: Option<crate::TimeRange>,
        _entity_ids: &[i64],
    ) -> Result<Vec<i64>> {
        Ok(Vec::new())
    }

    /// Stage 2 (`03 §7i.2`): hybrid retrieval over `candidates` — FTS on session
    /// `title`/`summary` combined with `session_vectors` cosine similarity — returning at
    /// most `limit` hits, best first. Default returns nothing.
    async fn ask_search_sessions(
        &self,
        _question: &str,
        _candidates: &[i64],
        _limit: u32,
    ) -> Result<Vec<AskSessionHit>> {
        Ok(Vec::new())
    }

    /// Stage 3 (`03 §7i.2`): the existing hybrid frame search restricted to frames whose
    /// `frames.session_id` is in `sessions`. An empty `sessions` slice means no restriction
    /// (the fallback path searches the window directly). This is a *new* method: the
    /// existing [`Self::hybrid_search`] keeps its exact semantics. Default returns nothing.
    async fn ask_search_frames(
        &self,
        _query: &SearchQuery,
        _sessions: &[i64],
    ) -> Result<Vec<SearchHit>> {
        Ok(Vec::new())
    }

    /// `frames.session_id` for each of `frame_ids` that belongs to a session. Frames outside
    /// any session are absent from the map. Default returns nothing.
    async fn ask_frame_sessions(&self, _frame_ids: &[i64]) -> Result<HashMap<i64, i64>> {
        Ok(HashMap::new())
    }

    /// A short display label per session (`title`, else tool/kind, else the start date) for
    /// the prompt's session context (`03 §7i.2` stage 4). Default returns nothing.
    async fn ask_session_labels(&self, _session_ids: &[i64]) -> Result<HashMap<i64, String>> {
        Ok(HashMap::new())
    }

    /// Whether an `ask_threads` row exists. Default fails explicitly: a store without
    /// conversations must not silently create a turn for a thread it cannot verify.
    async fn ask_thread_exists(&self, _thread_id: i64) -> Result<bool> {
        anyhow::bail!("ask conversations are unsupported")
    }

    /// Creates an `ask_threads` row with a NULL title (naming threads is P9's job, `§7i.4`)
    /// and returns its id. Default fails explicitly.
    async fn create_ask_thread(&self) -> Result<i64> {
        anyhow::bail!("ask conversations are unsupported")
    }

    /// Deletes a conversation; `ON DELETE CASCADE` removes its turns and touches no frame,
    /// session, graph row or insight (`§7i.4`). Default returns `false`.
    async fn delete_ask_thread(&self, _thread_id: i64) -> Result<bool> {
        Ok(false)
    }

    /// The highest-ordinal turn of a thread, for follow-up window/entity reuse (`§7i.4`).
    /// Default returns `None`.
    async fn last_ask_turn(&self, _thread_id: i64) -> Result<Option<crate::AskTurnRecord>> {
        Ok(None)
    }

    /// Appends a turn at the next `ordinal` (`UNIQUE(thread_id, ordinal)`) with a NULL
    /// answer, returning the persisted row. Default fails explicitly.
    async fn append_ask_turn(&self, _turn: NewAskTurn) -> Result<crate::AskTurnRecord> {
        anyhow::bail!("ask conversations are unsupported")
    }

    /// Stores the final answer of a streamed turn (`§7i.4`: NULL while streaming and on
    /// failure). Default returns `false`.
    async fn finish_ask_turn(&self, _turn_id: i64, _answer: &str) -> Result<bool> {
        Ok(false)
    }

    /// Every turn of a thread in `ordinal` order. Default returns nothing.
    async fn list_ask_turns(&self, _thread_id: i64) -> Result<Vec<crate::AskTurnRecord>> {
        Ok(Vec::new())
    }

    /// Sessions whose concatenated non-NULL `title`/`summary` still needs a
    /// `session_vectors` row, most recently updated first, bounded by `limit`. A session
    /// with both NULL is never returned (it is never embedded as an empty string). Default
    /// returns nothing.
    async fn session_embed_candidates(&self, _limit: u32) -> Result<Vec<SessionEmbedInput>> {
        Ok(Vec::new())
    }

    /// How many sessions still need a `session_vectors` row — the backfill's drain
    /// condition. Default returns `0`.
    async fn session_embed_pending(&self) -> Result<u64> {
        Ok(0)
    }

    /// Replaces the `session_vectors` row for `session_id` with `emb` (vec0 has no
    /// `ON CONFLICT`, so this deletes then inserts in one transaction) and returns whether
    /// a row was written. `text` is the exact concatenation that was embedded: the write is
    /// skipped when the session's current title/summary no longer produce it, and when the
    /// session no longer exists or has both NULL — so a concurrent summary edit can never
    /// leave a stale vector. Replay is idempotent. Default returns `false`.
    async fn upsert_session_vector(
        &self,
        _session_id: i64,
        _text: &str,
        _emb: &Embedding,
    ) -> Result<bool> {
        Ok(false)
    }

    /// Pending jobs of one kind — lets the `session_embed` producer avoid stacking a second
    /// run on an idle tick. Default returns `0`.
    async fn pending_jobs_of_kind(&self, _kind: JobKind) -> Result<u64> {
        Ok(0)
    }
}

/// Lets the kernel's idle vision backfill tell the inference sidecar to keep the model
/// loaded while it is actively draining the untagged-frame backlog, so the sidecar is not
/// idle-TTL-evicted between batches (`03 §5/§6`). The kernel sets it `true` when a backlog
/// remains and the user is idle, and `false` once the backlog is empty or the user
/// resumes — after which normal idle eviction frees the VRAM. Implemented by the inference
/// supervisor; the kernel holds it as `dyn BackfillControl` so it keeps depending only on
/// traits, never on `inference` (`03 §2`).
pub trait BackfillControl: Send + Sync {
    /// `true` suppresses idle-TTL eviction (keep warm); `false` resumes it.
    fn set_backfill_active(&self, active: bool);
}

/// Reads instantaneous system CPU/GPU pressure for the smart enrichment throttle
/// (`03 §5/§8`). Windows-native impl in `sysmon` (`GetSystemTimes` + PDH GPU-Engine
/// counters). The kernel forbids `unsafe`, so the composition root injects this as
/// `dyn PressureProbe` — the same seam as [`BackfillControl`] and the capture
/// `IdleSource`, keeping the kernel dependent only on `traits` (`03 §2`).
///
/// **Infallible by contract:** a failed or absent reading degrades to a truthful sample
/// (best-effort CPU, `gpu_pct = None`, `gpu_monitored = false`), never an error —
/// mirroring the sidecar flag probe's "degrade, don't fail" stance (`inference::flags`).
/// So the throttle loop can sample on a hot path without error handling.
pub trait PressureProbe: Send + Sync {
    /// Samples CPU (always) and GPU (when monitored) at this instant.
    fn sample(&self) -> PressureSample;
    /// Whether GPU monitoring is live — lets the UI show the honest "GPU not monitored"
    /// state without taking a full sample. Cheap.
    fn gpu_monitored(&self) -> bool;
}
