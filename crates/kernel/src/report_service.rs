//! Shared report execution for desktop recall and durable memory runs.
//! Adapters select model policy; this service owns evidence probing and coverage.

use std::future::Future;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use traits::{
    AnswerProvider, ReportConfig, ReportError, ReportMode, ReportOutput, ReportRange, Store,
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
    let output = crate::reports::generate_report(
        store,
        answer.as_ref(),
        request.range,
        request.start,
        request.end,
        request.prompt,
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
    let model = if output.mode == ReportMode::Empty {
        None
    } else {
        answer.answer_model_label().await
    };
    Ok(ReportExecution { output, model })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
