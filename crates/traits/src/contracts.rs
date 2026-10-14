//! The normative module contracts (`03 §3`).
//!
//! Names/shapes may be refined in impl, but these boundaries are fixed: `kernel`
//! and module crates depend on these traits, never on each other's concrete impls
//! (`03 §2`). All methods are fallible async; `Result<T>` is [`crate::Result`].

use std::sync::Arc;

use async_trait::async_trait;
use image::RgbaImage;
use tokio::sync::mpsc::Sender;

use crate::domain::{
    AnswerOpts, CapturedFrame, ChunkSource, Embedding, FrameContextRow, FrameEnrichmentInput,
    NewFrame, OcrResult, RetrievedChunk, TextFilterContext, VisionAnalysis,
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
                    AnswerDelta::Thinking { .. } | AnswerDelta::Done => {}
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
    // frames + ocr + vision
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
