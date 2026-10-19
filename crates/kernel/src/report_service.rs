//! Shared report execution for desktop recall and durable memory runs.
//! Adapters select model policy; this service owns evidence probing and coverage.

use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use traits::{
    AnswerProvider, ReportConfig, ReportError, ReportMode, ReportOutcome, ReportOutput,
    ReportRange, Store,
};

pub struct ReportExecution {
    pub output: ReportOutput,
    pub model: Option<String>,
}

pub struct ReportExecutionRequest<'a> {
    pub range: ReportRange,
    pub start: i64,
    pub end: i64,
    pub prompt: Option<&'a str>,
    /// Saved IANA zone for durable memory reports (`03 §8c`); the coverage grid then
    /// follows local midnights. `None` keeps the legacy exact-UTC grid.
    pub timezone: Option<&'a str>,
    pub config: ReportConfig,
}

fn error(code: &str, message: &str) -> ReportError {
    ReportError {
        code: code.into(),
        message: message.into(),
    }
}

/// Resolves the provider only after successful evidence inspection. A failed
/// database read never becomes an empty report. The provider resolver owns its
/// installed-only versus interactive model policy.
pub async fn execute<F, Fut>(
    store: &dyn Store,
    request: ReportExecutionRequest<'_>,
    resolve: F,
    progress: Option<&crate::reports::ReportProgress<'_>>,
    cancel: &AtomicBool,
) -> Result<ReportExecution, ReportError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Arc<dyn AnswerProvider>, ReportError>>,
{
    if request.end <= request.start {
        return Err(error(
            "INVALID_CONFIG",
            "Report window must have a positive duration.",
        ));
    }
    let has_content = store
        .report_has_usable_content(request.start, request.end)
        .await
        .map_err(|_| {
            error(
                "STORE_ERROR",
                "Could not inspect the report evidence window.",
            )
        })?;
    if !has_content {
        return Ok(ReportExecution {
            output: ReportOutput {
                markdown: "No captured content in this time range.".into(),
                cited_frame_ids: Vec::new(),
                frame_ids: Vec::new(),
                mode: ReportMode::Empty,
                periods_total: 0,
                periods_covered: 0,
                frames_sampled: 0,
                frames_summarized: 0,
                passes: 0,
                truncated: false,
            },
            model: None,
        });
    }
    let answer = resolve().await?;
    let coverage_requested = request.range == ReportRange::MemoryDaily
        || request
            .prompt
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .is_none();
    let output = crate::reports::generate_report(
        store,
        answer.as_ref(),
        request.range,
        request.start,
        request.end,
        request.prompt,
        request.timezone,
        request.config,
        progress,
        cancel,
    )
    .await
    .map_err(|_| {
        error(
            "GENERATION_FAILED",
            "Report generation failed or was cancelled; retry after checking the local model.",
        )
    })?;
    // The probe already found usable evidence. An empty generation result cannot
    // truthfully claim NO_DATA, and blank model prose must not become a successful note.
    if (coverage_requested && output.mode == ReportMode::Empty) || output.markdown.trim().is_empty()
    {
        return Err(error(
            "GENERATION_FAILED",
            "Report generation produced no usable text despite captured evidence; check the local model or retry.",
        ));
    }
    let model = if output.mode == ReportMode::Empty {
        None
    } else {
        answer.answer_model_label().await
    };
    Ok(ReportExecution { output, model })
}

/// Inputs for one weekly synthesis execution (`03 §8e`): the schedule's explicit UTC
/// `[start, end)` ISO-week window plus the saved IANA zone used for local calendar dates.
pub struct WeeklyExecutionRequest<'a> {
    pub start: i64,
    pub end: i64,
    pub timezone: &'a str,
    pub config: ReportConfig,
}

/// Result of [`execute_weekly`]: the same output shape the daily path returns, plus the
/// weekly provenance the note writer and run outcome need (`03 §8e`).
pub struct WeeklyExecution {
    pub output: ReportOutput,
    pub model: Option<String>,
    /// Exactly the saved daily runs consumed, in store order (oldest first). Base runs and
    /// linked revisions are both consumed, as returned — no revision precedence is defined.
    pub daily_run_ids: Vec<String>,
    /// Local `YYYY-MM-DD` dates in `[start, end)` with no saved daily report or only
    /// partial/truncated coverage — the PARTIAL signal the caller maps to the run outcome.
    pub missing_local_dates: Vec<String>,
}

/// Executes one weekly synthesis through the same shared service and model path as
/// [`execute`]: the week's saved daily reports are inspected *before* a model is
/// resolved, so a store failure is never an empty report and a genuinely no-data week
/// acquires no model. Only per-day dailies saved in the run's own zone are consumed
/// (`03 §8e` inputs); a week represented only by multi-day/range runs or other-zone
/// dailies is consumed-empty and refused the same way a week with no saved dailies is.
/// A week with no saved dailies and no usable evidence returns [`ReportMode::Empty`] with
/// `model: None`; a week that has captured frames but no saved dailies is refused (see
/// below) rather than mislabelled or synthesized without daily-run provenance. A week
/// whose consumable saved dailies are *all* `NO_DATA` — every day measured and empty — is
/// treated the same way (empty when the window holds no usable evidence, refused when it
/// does) with the consumed runs recorded as provenance; a partially-empty week is
/// untouched, since a `NO_DATA` day is a measured day and still counts as covered. Once
/// measurable saved dailies are found, the same positive-probe rules as [`execute`] apply: a blank
/// or empty generation is `GENERATION_FAILED`, a non-blank synthesis with an uncited
/// promotable item or day claim is `GENERATION_FAILED` (untraceable prose is never
/// persisted as OK), and a resolver failure propagates unchanged.
pub async fn execute_weekly<F, Fut>(
    store: &dyn Store,
    request: WeeklyExecutionRequest<'_>,
    resolve: F,
    progress: Option<&crate::reports::ReportProgress<'_>>,
    cancel: &AtomicBool,
) -> Result<WeeklyExecution, ReportError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Arc<dyn AnswerProvider>, ReportError>>,
{
    if request.end <= request.start {
        return Err(error(
            "INVALID_CONFIG",
            "Report window must have a positive duration.",
        ));
    }
    // Weekly inputs are exactly the saved daily reports in the window (`03 §8e`) — never
    // a fresh scan of raw frames. A failed read is a store failure, not an empty week.
    let dailies = store
        .saved_daily_runs(request.start, request.end)
        .await
        .map_err(|_| {
            error(
                "STORE_ERROR",
                "Could not inspect the saved daily reports for this week.",
            )
        })?;
    // Only per-day dailies saved in this run's own zone are consumed (`03 §8e` inputs): a
    // manual multi-day/date-range run and a daily saved under another zone are not "the
    // saved daily reports in the window" and never credit a date, so a week of nothing
    // but those is consumed-empty and refused below instead of being saved as complete.
    let consumed = crate::reports::weekly_consumed_dailies(
        &dailies,
        request.start,
        request.end,
        request.timezone,
    )
    .map_err(|_| error("INVALID_CONFIG", "Invalid weekly report time zone."))?;
    // Local calendar dates resolve before the model: an invalid zone is a configuration
    // error, never a reason to load one.
    let missing_local_dates = crate::reports::weekly_missing_local_dates(
        &consumed,
        request.start,
        request.end,
        request.timezone,
    )
    .map_err(|_| error("INVALID_CONFIG", "Invalid weekly report time zone."))?;
    // `03 §8e`: NO_DATA is a measured day — a saved daily whose own window was inspected
    // and held nothing — so a week whose consumable dailies are *all* NO_DATA recorded no
    // activity at all and is an empty week, exactly like a week with no saved dailies.
    // It must never reach the model: a synthesis could only invent days that no saved
    // daily measured, and saving it as a covered week would be a false OK. A week with
    // any measured day among its dailies is untouched — there the NO_DATA day still
    // counts as covered coverage, so partially-empty weeks keep their exact behavior.
    let all_dailies_no_data = consumed
        .iter()
        .all(|run| run.outcome == Some(ReportOutcome::NoData));
    if consumed.is_empty() || all_dailies_no_data {
        // `03 §8e` permits NO_DATA only when the week has neither saved dailies nor
        // usable evidence.
        let has_evidence = store
            .report_has_usable_content(request.start, request.end)
            .await
            .map_err(|_| {
                error(
                    "STORE_ERROR",
                    "Could not inspect the report evidence window.",
                )
            })?;
        if has_evidence {
            // The contract defines neither an output nor an outcome for captured frames
            // without saved dailies, and a weekly report that describes days without
            // citing their daily runs is a defect (`03 §8e`). Stop explicitly instead of
            // guessing: no NO_DATA claim, no uncited raw-frame narrative.
            return Err(error(
                "GENERATION_FAILED",
                "No saved daily reports cover this week; the weekly synthesis is built from saved daily reports and none were consumable for its local days, so none was generated.",
            ));
        }
        return Ok(WeeklyExecution {
            output: ReportOutput {
                markdown: "No captured content in this time range.".into(),
                cited_frame_ids: Vec::new(),
                frame_ids: Vec::new(),
                mode: ReportMode::Empty,
                periods_total: 0,
                periods_covered: 0,
                frames_sampled: 0,
                frames_summarized: 0,
                passes: 0,
                truncated: false,
            },
            model: None,
            // Provenance of what was inspected (`03 §8e`): the consumed dailies are
            // recorded even though nothing was synthesized — store order, exactly as the
            // generating path records them — and empty when the week had none.
            daily_run_ids: consumed.iter().map(|run| run.run_id.clone()).collect(),
            missing_local_dates,
        });
    }
    let answer = resolve().await?;
    let output = crate::reports::generate_weekly_synthesis(
        store,
        answer.as_ref(),
        &consumed,
        request.start,
        request.end,
        request.timezone,
        request.config,
        progress,
        cancel,
    )
    .await
    .map_err(|_| {
        error(
            "GENERATION_FAILED",
            "Weekly synthesis failed or was cancelled; retry after checking the local model.",
        )
    })?;
    // Saved dailies were found: an empty generation cannot truthfully claim NO_DATA and
    // blank prose must not become a successful note.
    if output.mode == ReportMode::Empty || output.markdown.trim().is_empty() {
        return Err(error(
            "GENERATION_FAILED",
            "Weekly synthesis produced no usable text despite saved daily reports; check the local model or retry.",
        ));
    }
    // `03 §8e`: a weekly report citing no daily run for a day it describes is a defect,
    // and every promotable item must be traceable. After the positive saved-dailies
    // probe, an uncited item or day claim cannot be trusted — never persisted as a
    // successful synthesis (`03 §8c`).
    if !crate::reports::weekly_cites_consumed_daily(&output.markdown, &consumed) {
        return Err(error(
            "GENERATION_FAILED",
            "Weekly synthesis did not cite a consumed daily run for every item or day claim; prose that cannot be traced to a saved daily report is not saved.",
        ));
    }
    let model = answer.answer_model_label().await;
    Ok(WeeklyExecution {
        output,
        model,
        daily_run_ids: consumed.into_iter().map(|run| run.run_id).collect(),
        missing_local_dates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReasoningOnlyAnswer;

    #[async_trait::async_trait]
    impl AnswerProvider for ReasoningOnlyAnswer {
        async fn answer(
            &self,
            _: &str,
            context: &[traits::RetrievedChunk],
            _: traits::AnswerOpts,
            tx: tokio::sync::mpsc::Sender<traits::AnswerDelta>,
        ) -> anyhow::Result<()> {
            tx.send(traits::AnswerDelta::Thinking {
                text: "Reasoning without final report text".into(),
            })
            .await?;
            tx.send(traits::AnswerDelta::Token { text: " \n".into() })
                .await?;
            for chunk in context {
                tx.send(traits::AnswerDelta::Citation {
                    frame_id: chunk.frame_id,
                })
                .await?;
            }
            tx.send(traits::AnswerDelta::Done).await?;
            Ok(())
        }
    }

    #[tokio::test]
    async fn blank_model_output_is_failure_for_single_pass_and_map_reduce() {
        let store = store::SqliteStore::open_in_memory().unwrap();
        const DAY: i64 = 86_400_000;
        for captured_at in [1, DAY + 1] {
            let frame = store
                .insert_frame(traits::NewFrame {
                    captured_at,
                    monitor_index: 0,
                    width: 1,
                    height: 1,
                    image_path: "unused.jpg".into(),
                    content_hash: captured_at.to_string(),
                    app_hint: None,
                    window_title: None,
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .unwrap();
            store
                .insert_ocr(
                    frame,
                    traits::OcrResult {
                        text: "Captured content must not become a successful empty report".into(),
                        mean_confidence: -1.0,
                        engine: "test".into(),
                        spans: Vec::new(),
                    },
                )
                .await
                .unwrap();
        }
        for (range, prompt, end) in [
            (ReportRange::MemoryDaily, None, DAY),
            (ReportRange::MemoryDaily, None, 2 * DAY),
            (ReportRange::Custom, Some(" \n"), 2 * DAY),
        ] {
            let result = execute(
                &store,
                ReportExecutionRequest {
                    range,
                    start: 0,
                    end,
                    prompt,
                    timezone: None,
                    config: ReportConfig {
                        daily_top_k: 20,
                        weekly_top_k: 100,
                        map_reduce_min_frames: 0,
                        reply_budget: 2048,
                    },
                },
                || async { Ok(Arc::new(ReasoningOnlyAnswer) as Arc<dyn AnswerProvider>) },
                None,
                &AtomicBool::new(false),
            )
            .await;
            assert!(matches!(result, Err(ReportError { code, .. }) if code == "GENERATION_FAILED"));
        }
        let no_match = execute(
            &store,
            ReportExecutionRequest {
                range: ReportRange::Custom,
                start: 0,
                end: 2 * DAY,
                prompt: Some("uniquelyunmatchedquery"),
                timezone: None,
                config: ReportConfig {
                    daily_top_k: 20,
                    weekly_top_k: 100,
                    map_reduce_min_frames: 0,
                    reply_budget: 2048,
                },
            },
            || async { Ok(Arc::new(ReasoningOnlyAnswer) as Arc<dyn AnswerProvider>) },
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(no_match.output.mode, ReportMode::Empty);
        assert!(no_match.model.is_none());
    }

    #[tokio::test]
    async fn empty_window_needs_no_model() {
        let store = store::SqliteStore::open_in_memory().unwrap();
        let result = execute(
            &store,
            ReportExecutionRequest {
                range: ReportRange::Daily,
                start: 10,
                end: 20,
                prompt: None,
                timezone: None,
                config: ReportConfig {
                    daily_top_k: 20,
                    weekly_top_k: 100,
                    map_reduce_min_frames: 30,
                    reply_budget: 2048,
                },
            },
            || async { panic!("empty evidence must not resolve or download a model") },
            None,
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert_eq!(result.output.mode, ReportMode::Empty);
        assert_eq!(result.output.frames_summarized, 0);
        assert!(result.output.frame_ids.is_empty());
        assert!(result.model.is_none());
    }
    #[tokio::test]
    async fn report_model_load_failure_is_visible() {
        let store = store::SqliteStore::open_in_memory().unwrap();
        let frame = store
            .insert_frame(traits::NewFrame {
                captured_at: 15,
                monitor_index: 0,
                width: 1,
                height: 1,
                image_path: "unused.jpg".into(),
                content_hash: "unit-evidence".into(),
                app_hint: None,
                window_title: None,
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .unwrap();
        store
            .insert_ocr(
                frame,
                traits::OcrResult {
                    text: "Evidence requiring a report model".into(),
                    mean_confidence: -1.0,
                    engine: "test".into(),
                    spans: Vec::new(),
                },
            )
            .await
            .unwrap();
        let result = execute(
            &store,
            ReportExecutionRequest {
                range: ReportRange::MemoryDaily,
                start: 10,
                end: 20,
                prompt: None,
                timezone: None,
                config: ReportConfig {
                    daily_top_k: 20,
                    weekly_top_k: 100,
                    map_reduce_min_frames: 30,
                    reply_budget: 2048,
                },
            },
            || async {
                Err(error(
                    "MODEL_UNAVAILABLE",
                    "The selected model is not installed.",
                ))
            },
            None,
            &AtomicBool::new(false),
        )
        .await;
        assert!(matches!(result,Err(ReportError{code,..}) if code=="MODEL_UNAVAILABLE"));
    }

    /// A `Store` whose evidence probe is forced **positive** while its sampling surface
    /// stays empty — the reviewer's false-NO_DATA scenario: the window holds usable
    /// content, but the coverage sampler finds none of it. The real in-memory store
    /// cannot express that split, so this decorator pins the probe and lets the honest-empty
    /// sampling defaults answer (no sampling trait methods are added anywhere).
    struct ProbeMissesSamplerStore(store::SqliteStore);

    #[async_trait::async_trait]
    impl Store for ProbeMissesSamplerStore {
        async fn report_has_usable_content(&self, _start: i64, _end: i64) -> anyhow::Result<bool> {
            Ok(true)
        }
        async fn timeline_buckets(
            &self,
            _start: i64,
            _end: i64,
            _bucket_count: u32,
        ) -> anyhow::Result<Vec<traits::TimelineBucket>> {
            Ok(Vec::new())
        }
        async fn sample_frames_in_range(
            &self,
            _start: i64,
            _end: i64,
            _limit: u32,
        ) -> anyhow::Result<Vec<traits::FrameMeta>> {
            Ok(Vec::new())
        }
        // Required `Store` methods: delegate to a real (empty) in-memory store.
        async fn insert_frame(&self, f: traits::NewFrame) -> traits::Result<i64> {
            self.0.insert_frame(f).await
        }
        async fn insert_ocr(&self, frame_id: i64, ocr: traits::OcrResult) -> traits::Result<()> {
            self.0.insert_ocr(frame_id, ocr).await
        }
        async fn insert_vision(
            &self,
            frame_id: i64,
            v: traits::VisionAnalysis,
        ) -> traits::Result<()> {
            self.0.insert_vision(frame_id, v).await
        }
        async fn upsert_text_embedding(
            &self,
            frame_id: i64,
            chunk_index: i32,
            chunk_text: &str,
            source: traits::ChunkSource,
            emb: &traits::Embedding,
            model: &str,
        ) -> traits::Result<()> {
            self.0
                .upsert_text_embedding(frame_id, chunk_index, chunk_text, source, emb, model)
                .await
        }
        async fn hybrid_search(
            &self,
            q: &traits::SearchQuery,
        ) -> traits::Result<Vec<traits::SearchHit>> {
            self.0.hybrid_search(q).await
        }
        async fn get_enrichment_input(
            &self,
            frame_id: i64,
        ) -> traits::Result<Option<traits::FrameEnrichmentInput>> {
            self.0.get_enrichment_input(frame_id).await
        }
        async fn enqueue_job(&self, job: traits::NewJob) -> traits::Result<i64> {
            self.0.enqueue_job(job).await
        }
        async fn claim_jobs(
            &self,
            kinds: &[traits::JobKind],
            limit: u32,
            now: i64,
        ) -> traits::Result<Vec<traits::Job>> {
            self.0.claim_jobs(kinds, limit, now).await
        }
        async fn complete_job(&self, id: i64) -> traits::Result<()> {
            self.0.complete_job(id).await
        }
        async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> traits::Result<()> {
            self.0.fail_job(id, err, retry_at).await
        }
        async fn job_stats(&self) -> traits::Result<traits::JobStats> {
            self.0.job_stats().await
        }
        async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> traits::Result<u64> {
            self.0.reset_stale_running_jobs(older_than_ms).await
        }
        async fn get_setting(&self, key: &str) -> traits::Result<Option<String>> {
            self.0.get_setting(key).await
        }
        async fn set_setting(&self, key: &str, value: &str) -> traits::Result<()> {
            self.0.set_setting(key, value).await
        }
        async fn delete_settings(&self, keys: &[&str]) -> traits::Result<Vec<String>> {
            self.0.delete_settings(keys).await
        }
    }

    /// Regression (reviewer r4285794717): a *positive* usable-content probe followed by a
    /// coverage grid that samples nothing of it must surface GENERATION_FAILED — never a
    /// successful NO_DATA/Empty result, which would silently discard captured evidence.
    #[tokio::test]
    async fn positive_probe_with_missing_sampled_content_is_generation_failed_not_no_data() {
        let store = ProbeMissesSamplerStore(store::SqliteStore::open_in_memory().unwrap());
        let result = execute(
            &store,
            ReportExecutionRequest {
                range: ReportRange::MemoryDaily,
                start: 0,
                end: 86_400_000,
                prompt: None,
                timezone: Some("Etc/UTC"),
                config: ReportConfig {
                    daily_top_k: 20,
                    weekly_top_k: 100,
                    map_reduce_min_frames: 0,
                    reply_budget: 2048,
                },
            },
            || async { Ok(Arc::new(ReasoningOnlyAnswer) as Arc<dyn AnswerProvider>) },
            None,
            &AtomicBool::new(false),
        )
        .await;
        let Err(ReportError { code, message }) = result else {
            panic!(
                "positive probe + sampler miss must fail generation, not report an empty window"
            );
        };
        assert_eq!(code, "GENERATION_FAILED");
        assert!(
            message.contains("despite captured evidence"),
            "the failure must name the captured evidence, got: {message}"
        );
    }
}
