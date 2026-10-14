use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use kernel::sessions_intel::generate_session_title_summary;
use store::SqliteStore;
use tokio::sync::mpsc::Sender;
use traits::{
    AnswerDelta, AnswerOpts, AnswerProvider, NewFrame, NewSession, OcrResult, RetrievedChunk,
    SessionHost, SessionKind,
};

struct CountingAnswer {
    calls: AtomicUsize,
}

#[async_trait]
impl AnswerProvider for CountingAnswer {
    async fn answer(
        &self,
        _query: &str,
        _context: &[RetrievedChunk],
        _opts: AnswerOpts,
        _tx: Sender<AnswerDelta>,
    ) -> traits::Result<()> {
        anyhow::bail!("streaming answer is not used by session intelligence")
    }

    async fn summarize(
        &self,
        _system_prompt: &str,
        _instruction: &str,
        context: &[RetrievedChunk],
        _opts: AnswerOpts,
    ) -> traits::Result<(String, Vec<i64>)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        assert_eq!(context.len(), 2);
        Ok((
            "Title: Fix the scheduler\nSummary: Reconciled session rows safely and reliably."
                .to_string(),
            context.iter().map(|chunk| chunk.frame_id).collect(),
        ))
    }

    async fn answer_model_label(&self) -> Option<String> {
        Some("answer-model.gguf".to_string())
    }

    fn answer_context_budget(&self) -> Option<u32> {
        Some(8192)
    }
}

struct DegenerateAnswer {
    calls: AtomicUsize,
}

#[async_trait]
impl AnswerProvider for DegenerateAnswer {
    async fn answer(
        &self,
        _query: &str,
        _context: &[RetrievedChunk],
        _opts: AnswerOpts,
        _tx: Sender<AnswerDelta>,
    ) -> traits::Result<()> {
        anyhow::bail!("streaming answer is not used by session intelligence")
    }

    async fn summarize(
        &self,
        _system_prompt: &str,
        _instruction: &str,
        context: &[RetrievedChunk],
        _opts: AnswerOpts,
    ) -> traits::Result<(String, Vec<i64>)> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok((
            "Title: Speaker\nSummary: User".to_string(),
            context.iter().map(|chunk| chunk.frame_id).collect(),
        ))
    }

    async fn answer_model_label(&self) -> Option<String> {
        Some("degenerate-model.gguf".to_string())
    }
}

async fn insert_frame(store: &SqliteStore, at: i64, text: &str) -> i64 {
    let id = store
        .insert_frame(NewFrame {
            captured_at: at,
            monitor_index: 0,
            width: 1920,
            height: 1080,
            image_path: format!("frames/{at}.webp"),
            content_hash: format!("h{at}"),
            app_hint: Some("codex".to_string()),
            window_title: Some("Codex".to_string()),
            browser_url: None,
            capture_trigger: None,
        })
        .await
        .unwrap();
    store
        .insert_ocr(
            id,
            OcrResult {
                text: text.to_string(),
                mean_confidence: 0.9,
                engine: "test".to_string(),
                spans: Vec::new(),
            },
        )
        .await
        .unwrap();
    id
}

async fn seed_session(store: &SqliteStore) -> i64 {
    let f1 = insert_frame(store, 100, "first filtered content").await;
    let f2 = insert_frame(store, 200, "second filtered content").await;
    let session_id = store
        .insert_session(NewSession {
            started_at: 100,
            ended_at: Some(200),
            kind: SessionKind::Ai,
            tool: Some("codex".to_string()),
            host: Some(SessionHost::Desktop),
            context_key: "ai:codex".to_string(),
            confidence: 0.9,
            frozen: false,
        })
        .await
        .unwrap();
    store
        .assign_frames_session(&[f1, f2], Some(session_id))
        .await
        .unwrap();
    session_id
}

#[tokio::test]
async fn lazy_intelligence_calls_summarize_once_then_serves_the_cache() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let session_id = seed_session(&store).await;
    let answer = Arc::new(CountingAnswer {
        calls: AtomicUsize::new(0),
    });

    let first = generate_session_title_summary(store.clone(), answer.clone(), session_id)
        .await
        .unwrap();
    let second = generate_session_title_summary(store.clone(), answer.clone(), session_id)
        .await
        .unwrap();

    assert_eq!(first, second);
    assert_eq!(first.title.as_deref(), Some("Fix the scheduler"));
    assert_eq!(
        first.summary.as_deref(),
        Some("Reconciled session rows safely and reliably.")
    );
    assert_eq!(first.summary_model.as_deref(), Some("answer-model.gguf"));
    assert_eq!(answer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_cached_summary_is_regenerated() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let session_id = seed_session(&store).await;
    store
        .set_session_title_summary(session_id, "Speaker", "User", "bad-model.gguf")
        .await
        .unwrap();
    let answer = Arc::new(CountingAnswer {
        calls: AtomicUsize::new(0),
    });

    let regenerated = generate_session_title_summary(store, answer.clone(), session_id)
        .await
        .unwrap();

    assert_eq!(
        regenerated.summary.as_deref(),
        Some("Reconciled session rows safely and reliably.")
    );
    assert_eq!(
        regenerated.summary_model.as_deref(),
        Some("answer-model.gguf")
    );
    assert_eq!(answer.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn invalid_cached_summary_is_cleared_when_retry_fails() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    let session_id = seed_session(&store).await;
    store
        .set_session_title_summary(session_id, "Speaker", "User", "bad-model.gguf")
        .await
        .unwrap();
    let answer = Arc::new(DegenerateAnswer {
        calls: AtomicUsize::new(0),
    });

    let error = generate_session_title_summary(store.clone(), answer.clone(), session_id)
        .await
        .expect_err("two rejected summaries must surface a terminal error");
    assert!(error
        .to_string()
        .contains("summary was rejected twice; cache left empty"));
    assert_eq!(answer.calls.load(Ordering::SeqCst), 2);
    let persisted = store.get_session(session_id).await.unwrap().unwrap();
    assert!(persisted.title.is_none());
    assert!(persisted.summary.is_none());
    assert!(persisted.summary_model.is_none());
}
