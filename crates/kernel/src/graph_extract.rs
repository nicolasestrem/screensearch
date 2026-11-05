//! The `graph_extract` job body: one frame's heuristic entities plus its session link
//! (`03 §7h.2/.3`, 0.4.5 P5).
//!
//! One frame per job. The body reads the frame's stored context and filtered content
//! text, asks the `sessions` read-side helper for the bundled taxonomy's recognition
//! (input data only, never a model call), runs the pure `graph` rule table and persists
//! the hits through one [`Store::graph_apply_hits`] transaction. A frame with no
//! containing session still contributes its entities; it simply has no link row
//! (`03 §7h.3`).
//!
//! Deterministic and model-free (`03 §7h.2`): with no answer model attached, extraction
//! and linking still run, so the graph never depends on the LLM lane.

use std::sync::Arc;

use traits::Store;

use crate::worker_pool::Outcome;

/// Runs one claimed `graph_extract` job. A job without a `frame_id` can never do useful
/// work and is dead-lettered; a purged frame is a clean no-op, and an I/O failure is
/// retryable with the queue's normal backoff (`03 §5`).
pub(crate) async fn run(store: &Arc<dyn Store>, frame_id: Option<i64>) -> Outcome {
    let Some(frame_id) = frame_id else {
        return Outcome::DeadLetter("graph_extract job has no frame_id".to_string());
    };
    match extract_frame(store.as_ref(), frame_id).await {
        Ok(ExtractOutcome::Changed) => Outcome::Complete { changed_data: true },
        Ok(ExtractOutcome::Noop) => Outcome::Complete {
            changed_data: false,
        },
        Err(error) => Outcome::Retry(format!("graph extract frame {frame_id}: {error}")),
    }
}

/// What one frame's extraction wrote: `Changed` when hits were persisted, `Noop` when the
/// frame is gone or produced no hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExtractOutcome {
    Changed,
    Noop,
}

/// Extracts and persists one frame's hits. Shared by the job body and the resumable
/// backfill's completion driver ([`crate::graph_backfill::run_to_completion`]).
pub(crate) async fn extract_frame(
    store: &dyn Store,
    frame_id: i64,
) -> anyhow::Result<ExtractOutcome> {
    let Some(frame) = store.graph_frame_input(frame_id).await? else {
        // The frame was purged between enqueue and run: nothing to extract, not an error.
        return Ok(ExtractOutcome::Noop);
    };
    let recognition =
        sessions::recognize_tool(frame.app_hint.as_deref(), frame.window_title.as_deref());
    let hits = graph::extract(&graph::ExtractionInput {
        frame: &frame,
        recognition: recognition.as_ref(),
    });
    if hits.is_empty() {
        return Ok(ExtractOutcome::Noop);
    }
    // The store resolves the containing session on the frame input; no second lookup is
    // needed. A `None` session persists entities without a link row (`03 §7h.3`).
    let session_id = frame.session.as_ref().map(|session| session.session_id);
    store
        .graph_apply_hits(session_id, frame.frame_id, frame.captured_at, &hits)
        .await?;
    Ok(ExtractOutcome::Changed)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use store::SqliteStore;
    use traits::{NewFrame, NewSession, OcrResult, SessionKind, Store};

    use super::*;
    use crate::worker_pool::Outcome;

    async fn test_store() -> Arc<dyn Store> {
        Arc::new(SqliteStore::open_in_memory().expect("in-memory store"))
    }

    async fn insert_frame(
        store: &dyn Store,
        captured_at: i64,
        browser_url: Option<&str>,
        window_title: Option<&str>,
        text: &str,
    ) -> i64 {
        let frame_id = store
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: format!("frames/{captured_at}.webp"),
                content_hash: format!("hash-{captured_at}"),
                app_hint: Some("firefox".to_string()),
                window_title: window_title.map(str::to_string),
                browser_url: browser_url.map(str::to_string),
                capture_trigger: None,
            })
            .await
            .expect("insert frame");
        store
            .insert_ocr(
                frame_id,
                OcrResult {
                    text: text.to_string(),
                    mean_confidence: 0.9,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .expect("insert ocr");
        frame_id
    }

    async fn insert_session(store: &dyn Store, started_at: i64, ended_at: Option<i64>) -> i64 {
        store
            .insert_session(NewSession {
                started_at,
                ended_at,
                kind: SessionKind::Focus,
                tool: None,
                host: None,
                context_key: format!("ctx:{started_at}"),
                confidence: 0.9,
                frozen: false,
            })
            .await
            .expect("insert session")
    }

    #[tokio::test]
    async fn extract_links_only_when_a_session_contains_the_frame() {
        let store = test_store().await;
        // A closed session, then an open one; the middle frame falls between both.
        let first_session = insert_session(store.as_ref(), 1_000, Some(2_000)).await;
        let open_session = insert_session(store.as_ref(), 5_000, None).await;
        let inside_closed = insert_frame(
            store.as_ref(),
            1_500,
            Some("https://github.com/acme/widgets/pull/42"),
            Some("Add the work graph"),
            "see #7 for the follow up",
        )
        .await;
        let between_sessions = insert_frame(
            store.as_ref(),
            2_500,
            Some("https://example.org/no-session"),
            Some("Between sessions"),
            "nothing to link",
        )
        .await;
        let inside_open = insert_frame(
            store.as_ref(),
            6_000,
            Some("https://github.com/acme/widgets/pull/42"),
            Some("Add the work graph"),
            "see #7 for the follow up",
        )
        .await;

        for frame_id in [inside_closed, between_sessions, inside_open] {
            assert_eq!(
                extract_frame(store.as_ref(), frame_id)
                    .await
                    .expect("extract frame"),
                ExtractOutcome::Changed,
                "fixture frames carry extractable context"
            );
        }

        let input = store
            .graph_cluster_input(10_000)
            .await
            .expect("cluster input");
        // The linked sessions are exactly the two whose interval contains a frame.
        let linked_sessions: std::collections::BTreeSet<i64> =
            input.links.iter().map(|link| link.session_id).collect();
        assert_eq!(
            linked_sessions,
            std::collections::BTreeSet::from([first_session, open_session])
        );
        // All three frames contributed entities, including the unlinked one.
        let catalog = store
            .graph_entity_catalog(100)
            .await
            .expect("entity catalog");
        assert!(
            catalog
                .iter()
                .any(|entity| entity.canonical_name == "example.org"),
            "an unlinked frame still contributes its entities"
        );
        // The link carries the rule's own confidence, not a smoothed value (03 §7h.2).
        let domain_links: Vec<f64> = input
            .links
            .iter()
            .filter(|link| link.canonical_name == "github.com")
            .map(|link| link.confidence)
            .collect();
        assert_eq!(domain_links.len(), 2);
        for confidence in domain_links {
            assert!(
                (confidence - graph::extraction::confidence::URL_DOMAIN_SCHEME).abs() < 1e-9,
                "link confidence is the URL domain rule confidence"
            );
        }
    }

    #[tokio::test]
    async fn a_purged_frame_is_a_clean_noop() {
        let store = test_store().await;
        let outcome = run(&store, Some(424_242)).await;
        assert!(matches!(
            outcome,
            Outcome::Complete {
                changed_data: false
            }
        ));
        assert_eq!(
            store.graph_table_counts().await.expect("counts"),
            traits::GraphTableCounts::default()
        );
    }

    #[tokio::test]
    async fn a_job_without_a_frame_id_is_dead_lettered() {
        let store = test_store().await;
        assert!(matches!(run(&store, None).await, Outcome::DeadLetter(_)));
    }
}
