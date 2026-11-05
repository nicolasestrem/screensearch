//! The `graph_extract` job body: one frame's heuristic entities plus its session link
//! (`03 §7h.2/.3`, 0.4.5 P5).
//!
//! One frame per job. The body reads the frame's stored context and filtered content
//! text, asks the `sessions` read-side helper for the bundled taxonomy's recognition
//! (input data only, never a model call), runs the pure `graph` rule table and persists
//! the hits through one [`Store::graph_apply_hits`] transaction.
//!
//! Linking is **ownership-only** (`03 §7e`, the 0.4.5 P5 review ruling): a frame's evidence
//! belongs to the session named by `frames.session_id`, so the store reports `Some(session)`
//! only for an owned frame whose owner row exists. An unowned frame (or one whose owner row
//! vanished) still contributes its entities but never a link row. The
//! `[started_at, ended_at)` interval rule remains a documented primitive of its own
//! (`Store::graph_session_at`), not a linking fallback.
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
    // Linking is ownership-only: the store resolves the frame's owner from `frames.session_id`
    // and reports `None` for an unowned frame, which then persists entities without a link
    // row. The interval rule is never a linking fallback (`03 §7e`).
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
    async fn extract_links_only_to_the_owning_session() {
        let store = test_store().await;
        // Ownership (`frames.session_id`), not the interval rule, decides linking: the
        // closed and open sessions below are only containers for the ownership fixtures.
        let closed_session = insert_session(store.as_ref(), 1_000, Some(2_000)).await;
        let open_session = insert_session(store.as_ref(), 5_000, None).await;
        let owned_in_closed = insert_frame(
            store.as_ref(),
            1_500,
            Some("https://github.com/acme/widgets/pull/42"),
            Some("Add the work graph"),
            "see #7 for the follow up",
        )
        .await;
        // Owned by the closed session even though its timestamp is outside that session's
        // interval: ownership wins and the interval rule is never a linking fallback.
        let owned_outside_interval = insert_frame(
            store.as_ref(),
            3_000,
            Some("https://github.com/acme/widgets/pull/42"),
            Some("Add the work graph"),
            "see #7 for the follow up",
        )
        .await;
        let unowned_between = insert_frame(
            store.as_ref(),
            2_500,
            Some("https://example.org/no-owner"),
            Some("Unowned between sessions"),
            "nothing to link",
        )
        .await;
        // Inside the open session's interval but unowned: entities only, never a link.
        let unowned_inside_open = insert_frame(
            store.as_ref(),
            6_000,
            Some("https://example.net/no-owner"),
            Some("Unowned inside the open interval"),
            "nothing to link",
        )
        .await;

        assert_eq!(
            store
                .assign_frames_session(
                    &[owned_in_closed, owned_outside_interval],
                    Some(closed_session)
                )
                .await
                .expect("assign the owned frames"),
            2,
            "the fixture assigns exactly the two owned frames"
        );

        for frame_id in [
            owned_in_closed,
            owned_outside_interval,
            unowned_between,
            unowned_inside_open,
        ] {
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
        // Only the owning session gets links: an unowned frame never links, even when a
        // session's interval contains its timestamp (the open session here).
        let linked_sessions: std::collections::BTreeSet<i64> =
            input.links.iter().map(|link| link.session_id).collect();
        assert_eq!(
            linked_sessions,
            std::collections::BTreeSet::from([closed_session]),
            "only the owning session links; the open session and unowned frames never do"
        );
        assert!(input
            .links
            .iter()
            .all(|link| link.session_id == closed_session));
        assert!(
            !input
                .links
                .iter()
                .any(|link| link.session_id == open_session),
            "an unowned frame inside the open session's interval never links"
        );
        // The outside-interval frame is linked through ownership, and the link window
        // records its capture time rather than the session's ended_at.
        let domain_link = input
            .links
            .iter()
            .find(|link| link.canonical_name == "github.com")
            .expect("the owned frames produced the shared domain link");
        assert_eq!(
            domain_link.last_seen_at, 3_000,
            "ownership links the outside-interval frame"
        );
        // All four frames contributed entities, including the two unlinked ones.
        let catalog = store
            .graph_entity_catalog(100)
            .await
            .expect("entity catalog");
        for expected in ["github.com", "example.org", "example.net"] {
            assert!(
                catalog
                    .iter()
                    .any(|entity| entity.canonical_name == expected),
                "the unlinked frames still contribute their entities: {expected}"
            );
        }
        // The link carries the rule's own confidence, not a smoothed value (03 §7h.2).
        assert!(
            (domain_link.confidence - graph::extraction::confidence::URL_DOMAIN_SCHEME).abs()
                < 1e-9,
            "link confidence is the URL domain rule confidence"
        );
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
