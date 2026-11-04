//! Integration tests for the `store` crate — the data spine (`03 §4/§5/§10`).
//!
//! All tests run against an in-memory SQLite DB (`03 §10`). Because the store
//! keeps a single connection for its lifetime, `:memory:` persists across calls.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use store::{SqliteStore, EMBEDDING_DIM, FILTER_VERSION};
use traits::{
    ChunkSource, Embedding, EmbeddingProvider, FrameMeta, JobKind, JobState, NewFrame, NewJob,
    NewSession, OcrResult, SearchQuery, SessionHost, SessionKind, TextFilterContext, TextRole,
    TextSource, TextSpan, TimeRange, TimelineBucket, VisionAnalysis,
};

/// One unsuppressed OCR span at the given normalized bbox — the shape PR2's OCR
/// engine emits (role `unknown`, searchable, no suppression).
fn span(text: &str, x: f32, y: f32, w: f32, h: f32) -> TextSpan {
    TextSpan {
        normalized_text: traits::normalize_text(text),
        text: text.to_string(),
        source: TextSource::Ocr,
        role: TextRole::Unknown,
        x,
        y,
        w,
        h,
        line_index: 0,
        is_searchable: true,
        suppress_reason: None,
    }
}

/// A job of the given kind with sensible defaults (immediately runnable).
fn job(kind: JobKind, priority: i64, max_attempts: i64, not_before: i64) -> NewJob {
    NewJob {
        kind,
        frame_id: None,
        priority,
        max_attempts,
        not_before,
    }
}

/// A deterministic stand-in for fastembed: it maps each query string to a
/// pre-registered vector, so the vector arm of `hybrid_search` is fully exercised
/// without the real model (the P3 wiring just swaps this for the fastembed impl).
struct FakeEmbedder {
    by_text: HashMap<String, Embedding>,
}

#[async_trait::async_trait]
impl EmbeddingProvider for FakeEmbedder {
    fn dim(&self) -> usize {
        EMBEDDING_DIM
    }
    async fn embed_texts(&self, inputs: &[String]) -> traits::Result<Vec<Embedding>> {
        inputs
            .iter()
            .map(|s| {
                self.by_text
                    .get(s)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("fake embedder has no vector for {s:?}"))
            })
            .collect()
    }
}

/// A unit basis vector of the embedding dimension with `1.0` at `hot`. Cosine
/// distance between two distinct one-hot vectors is 1.0; between identical ones
/// it is ~0 — enough to make KNN ordering deterministic in tests.
fn one_hot(hot: usize) -> Embedding {
    let mut v = vec![0.0_f32; EMBEDDING_DIM];
    v[hot] = 1.0;
    Embedding(v)
}

/// A throwaway frame with the given content marker; `image_path`/`content_hash`
/// derive from it so fixtures are distinct.
fn frame_at(captured_at: i64) -> NewFrame {
    NewFrame {
        captured_at,
        monitor_index: 0,
        width: 1920,
        height: 1080,
        image_path: format!("frames/{captured_at}.jpg"),
        content_hash: format!("hash-{captured_at}"),
        app_hint: Some("Firefox".to_string()),
        window_title: Some("Inbox".to_string()),
        browser_url: Some("https://mail.example.com".to_string()),
        capture_trigger: None,
    }
}

/// Opening a fresh store creates the v14 baseline and lands at the latest schema
/// version (`03 §4`, 0.4.5 D2).
#[tokio::test]
async fn open_in_memory_creates_the_baseline_schema() {
    let store = SqliteStore::open_in_memory().expect("open in-memory store");
    assert_eq!(
        store.schema_version().expect("read schema_version"),
        store::LATEST_SCHEMA_VERSION,
    );
}

#[tokio::test]
async fn settings_round_trip_and_overwrite() {
    let store = SqliteStore::open_in_memory().unwrap();

    assert_eq!(
        store.get_setting("capture.interval_ms").await.unwrap(),
        None
    );

    store
        .set_setting("capture.interval_ms", "3000")
        .await
        .unwrap();
    assert_eq!(
        store.get_setting("capture.interval_ms").await.unwrap(),
        Some("3000".to_string())
    );

    // set is an upsert — second write overwrites
    store
        .set_setting("capture.interval_ms", "5000")
        .await
        .unwrap();
    assert_eq!(
        store.get_setting("capture.interval_ms").await.unwrap(),
        Some("5000".to_string())
    );
}

#[tokio::test]
async fn insert_frame_then_get_frame_returns_context() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(1_000)).await.unwrap();

    let detail = store.get_frame(id).await.unwrap().expect("frame exists");
    assert_eq!(detail.frame_id, id);
    assert_eq!(detail.captured_at, 1_000);
    assert_eq!(detail.width, 1920);
    assert_eq!(detail.app_hint.as_deref(), Some("Firefox"));
    assert_eq!(detail.window_title.as_deref(), Some("Inbox"));
    // no OCR / vision / tags yet
    assert_eq!(detail.raw_text, None);
    assert_eq!(detail.content_text, None);
    assert!(detail.vision.is_none());
    assert!(detail.tags.is_empty());

    // a missing id is None, not an error
    assert!(store.get_frame(999).await.unwrap().is_none());
}

#[tokio::test]
async fn insert_ocr_then_get_frame_has_text() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(2_000)).await.unwrap();
    store
        .insert_ocr(
            id,
            OcrResult {
                text: "quarterly invoice total due".to_string(),
                mean_confidence: 0.94,
                engine: "winrt".to_string(),
                spans: vec![span("quarterly", 0.0, 0.0, 0.2, 0.05)],
            },
        )
        .await
        .unwrap();

    let detail = store.get_frame(id).await.unwrap().unwrap();
    // PR2 interim: content_text is a passthrough copy of raw_text (`07` #51).
    assert_eq!(
        detail.raw_text.as_deref(),
        Some("quarterly invoice total due")
    );
    assert_eq!(
        detail.content_text.as_deref(),
        Some("quarterly invoice total due")
    );
    assert_eq!(detail.text_source, TextSource::Ocr);
    assert_eq!(detail.suppressed_text_count, 0);
}

#[tokio::test]
async fn insert_ocr_persists_spans_with_pr2_defaults() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(2_500)).await.unwrap();
    store
        .insert_ocr(
            id,
            OcrResult {
                text: "Hello World".to_string(),
                mean_confidence: -1.0,
                engine: "winrt".to_string(),
                spans: vec![
                    span("Hello", 0.10, 0.20, 0.15, 0.05),
                    span("World", 0.30, 0.20, 0.15, 0.05),
                ],
            },
        )
        .await
        .unwrap();

    let spans = store.frame_spans(id).await.unwrap();
    assert_eq!(spans.len(), 2);
    // Order preserved by span_index; PR2 emits role=unknown, searchable, no suppress.
    assert_eq!(spans[0].text, "Hello");
    assert_eq!(spans[0].normalized_text, "hello");
    assert_eq!(spans[1].text, "World");
    for s in &spans {
        assert_eq!(s.source, TextSource::Ocr);
        assert_eq!(s.role, TextRole::Unknown);
        assert!(s.is_searchable);
        assert!(s.suppress_reason.is_none());
        assert!((0.0..=1.0).contains(&s.x) && s.x + s.w <= 1.0 + 1e-6);
    }

    // Re-OCR replaces spans wholesale (idempotent).
    store
        .insert_ocr(
            id,
            OcrResult {
                text: "Replaced".to_string(),
                mean_confidence: -1.0,
                engine: "winrt".to_string(),
                spans: vec![span("Replaced", 0.0, 0.0, 0.4, 0.06)],
            },
        )
        .await
        .unwrap();
    let spans = store.frame_spans(id).await.unwrap();
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].text, "Replaced");
}

#[tokio::test]
async fn insert_vision_then_get_frame_has_analysis() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(3_000)).await.unwrap();
    store
        .insert_vision(
            id,
            VisionAnalysis {
                description: "a code editor showing Rust".to_string(),
                activity_type: Some("coding".to_string()),
                app_hint: Some("VS Code".to_string()),
                confidence: 0.81,
                model: "qwen3-vl-4b".to_string(),
            },
        )
        .await
        .unwrap();

    let detail = store.get_frame(id).await.unwrap().unwrap();
    let vision = detail.vision.expect("vision present");
    assert_eq!(vision.description, "a code editor showing Rust");
    assert_eq!(vision.activity_type.as_deref(), Some("coding"));
    // 03 §4: frames.activity_type is "filled by vision" — insert_vision mirrors the
    // classification onto the frame for fast timeline filtering.
    assert_eq!(detail.activity_type.as_deref(), Some("coding"));
}

#[tokio::test]
async fn text_embedding_knn_orders_by_cosine_distance() {
    let store = SqliteStore::open_in_memory().unwrap();
    let a = store.insert_frame(frame_at(10)).await.unwrap();
    let b = store.insert_frame(frame_at(20)).await.unwrap();
    store
        .upsert_text_embedding(a, 0, "alpha", ChunkSource::Ocr, &one_hot(0), "gemma")
        .await
        .unwrap();
    store
        .upsert_text_embedding(b, 0, "beta", ChunkSource::Ocr, &one_hot(1), "gemma")
        .await
        .unwrap();

    // querying near A's vector ranks A before B
    assert_eq!(
        store.nearest_text_frames(&one_hot(0), 5).await.unwrap(),
        vec![a, b]
    );
    // querying near B's vector ranks B before A
    assert_eq!(
        store.nearest_text_frames(&one_hot(1), 5).await.unwrap(),
        vec![b, a]
    );
}

#[tokio::test]
async fn upsert_text_embedding_replaces_vector_in_place() {
    let store = SqliteStore::open_in_memory().unwrap();
    let a = store.insert_frame(frame_at(10)).await.unwrap();
    let b = store.insert_frame(frame_at(20)).await.unwrap();
    store
        .upsert_text_embedding(a, 0, "v1", ChunkSource::Ocr, &one_hot(0), "gemma")
        .await
        .unwrap();
    store
        .upsert_text_embedding(b, 0, "fixed", ChunkSource::Ocr, &one_hot(0), "gemma")
        .await
        .unwrap();

    // replace A's chunk-0 vector; same (frame, chunk) key → no duplicate row
    store
        .upsert_text_embedding(a, 0, "v2", ChunkSource::Ocr, &one_hot(5), "gemma")
        .await
        .unwrap();
    assert_eq!(store.text_embedding_count().await.unwrap(), 2);

    // A now sits at one_hot(5); near one_hot(0) B comes first, A trails
    assert_eq!(
        store.nearest_text_frames(&one_hot(0), 5).await.unwrap(),
        vec![b, a]
    );
    assert_eq!(
        store.nearest_text_frames(&one_hot(5), 5).await.unwrap(),
        vec![a, b]
    );
}

#[tokio::test]
async fn wrong_dimension_embedding_is_rejected() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f = store.insert_frame(frame_at(10)).await.unwrap();
    let bad = Embedding(vec![0.0_f32; 10]); // not 768
    assert!(store
        .upsert_text_embedding(f, 0, "x", ChunkSource::Ocr, &bad, "gemma")
        .await
        .is_err());
}

#[tokio::test]
async fn delete_frame_cascades_and_purges_vectors() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f = store.insert_frame(frame_at(10)).await.unwrap();
    store
        .insert_ocr(
            f,
            OcrResult {
                text: "doomed".to_string(),
                mean_confidence: 0.9,
                engine: "winrt".to_string(),
                spans: vec![span("doomed", 0.0, 0.0, 0.3, 0.05)],
            },
        )
        .await
        .unwrap();
    store
        .upsert_text_embedding(f, 0, "doomed", ChunkSource::Ocr, &one_hot(0), "gemma")
        .await
        .unwrap();

    store.delete_frame(f).await.unwrap();

    assert!(store.get_frame(f).await.unwrap().is_none());
    // the AFTER DELETE trigger must have purged the vec0 shadow row
    assert_eq!(store.text_embedding_count().await.unwrap(), 0);
    assert!(store
        .nearest_text_frames(&one_hot(0), 5)
        .await
        .unwrap()
        .is_empty());
    // frame_text + text_spans cascade with the frame, and the content FTS mirror is
    // purged by its delete trigger (a search for the deleted term finds nothing).
    assert!(store.frame_spans(f).await.unwrap().is_empty());
    assert!(store.ocr_texts(&[f]).await.unwrap().is_empty());
    assert!(store
        .hybrid_search(&query("doomed", 10))
        .await
        .unwrap()
        .is_empty());
}

/// Seeds a frame with OCR text and (optionally) a text embedding; returns its id.
async fn seed(store: &SqliteStore, at: i64, text: &str, emb: Option<&Embedding>) -> i64 {
    let id = store.insert_frame(frame_at(at)).await.unwrap();
    store
        .insert_ocr(
            id,
            OcrResult {
                text: text.to_string(),
                mean_confidence: 0.9,
                engine: "winrt".to_string(),
                spans: vec![span(text, 0.0, 0.0, 0.5, 0.05)],
            },
        )
        .await
        .unwrap();
    if let Some(e) = emb {
        store
            .upsert_text_embedding(id, 0, text, ChunkSource::Ocr, e, "gemma")
            .await
            .unwrap();
    }
    id
}

fn query(text: &str, limit: u32) -> SearchQuery {
    SearchQuery {
        text: text.to_string(),
        limit,
        time_range: None,
        include_chrome: false,
    }
}

#[tokio::test]
async fn hybrid_search_fts_only_without_embedder() {
    let store = SqliteStore::open_in_memory().unwrap();
    let a = seed(&store, 100, "quarterly invoice total", None).await;
    let _b = seed(&store, 200, "vacation photos beach", None).await;

    let hits = store.hybrid_search(&query("invoice", 10)).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].frame_id, a);
    assert!(
        hits[0].snippet.contains("invoice"),
        "snippet should include the matched term: {:?}",
        hits[0].snippet
    );
}

#[tokio::test]
async fn hybrid_search_empty_query_returns_nothing() {
    let store = SqliteStore::open_in_memory().unwrap();
    seed(&store, 100, "anything at all", None).await;
    assert!(store
        .hybrid_search(&query("   ", 10))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn hybrid_search_fuses_fts_and_vector_arms_via_rrf() {
    // A matches both arms (FTS term + nearest vector) → must rank first.
    let store_a_vec = one_hot(0);
    let store = SqliteStore::open_in_memory().unwrap();
    let a = seed(&store, 100, "alpha apple", Some(&one_hot(0))).await;
    let b = seed(&store, 200, "beta banana", Some(&one_hot(1))).await;
    let c = seed(&store, 300, "alpha cherry", Some(&one_hot(2))).await;

    // query "alpha": FTS matches A and C; the fake embeds "alpha" -> A's vector.
    let mut by_text = HashMap::new();
    by_text.insert("alpha".to_string(), store_a_vec);
    let store = store.with_embedder(Arc::new(FakeEmbedder { by_text }));

    let hits = store.hybrid_search(&query("alpha", 10)).await.unwrap();
    let ids: Vec<i64> = hits.iter().map(|h| h.frame_id).collect();

    assert_eq!(hits[0].frame_id, a, "A is in both arms → top; got {ids:?}");
    assert!(ids.contains(&c), "C matches FTS → present; got {ids:?}");
    assert!(
        ids.contains(&b),
        "B is reachable via the vector arm; got {ids:?}"
    );
    // scores are descending
    assert!(hits.windows(2).all(|w| w[0].score >= w[1].score));
}

#[tokio::test]
async fn hybrid_search_honors_time_range() {
    let store = SqliteStore::open_in_memory().unwrap();
    let _old = seed(&store, 1_000, "status report", None).await;
    let recent = seed(&store, 5_000, "status report", None).await;

    let q = SearchQuery {
        text: "report".to_string(),
        limit: 10,
        time_range: Some(TimeRange {
            start: 4_000,
            end: 6_000,
        }),
        include_chrome: false,
    };
    let hits = store.hybrid_search(&q).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].frame_id, recent);
}

/// A per-word span on `line` at the given normalized bbox (role `unknown`, the shape OCR
/// emits pre-classification).
fn line_span(text: &str, line: u32, x: f32, y: f32, w: f32, h: f32) -> TextSpan {
    TextSpan {
        line_index: line,
        ..span(text, x, y, w, h)
    }
}

/// Degrade-to-text DB shrink (`07` #73a): merging a frame's per-word spans to per-line spans
/// must reclaim rows **without** breaking search (FTS `content_text` + the vector arm) or the
/// `FrameReconstruction` (which needs each line's text at its box).
#[tokio::test]
async fn merge_frame_spans_to_lines_shrinks_rows_but_keeps_search_and_reconstruction() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(1_000)).await.unwrap();
    // Four words across two lines — the per-word shape OCR emits.
    store
        .insert_ocr(
            id,
            OcrResult {
                text: "quarterly invoice total due".to_string(),
                mean_confidence: 0.9,
                engine: "winrt".to_string(),
                spans: vec![
                    line_span("quarterly", 0, 0.0, 0.0, 0.12, 0.05),
                    line_span("invoice", 0, 0.14, 0.0, 0.10, 0.05),
                    line_span("total", 1, 0.0, 0.10, 0.08, 0.05),
                    line_span("due", 1, 0.10, 0.10, 0.06, 0.05),
                ],
            },
        )
        .await
        .unwrap();
    store
        .upsert_text_embedding(
            id,
            0,
            "quarterly invoice",
            ChunkSource::Ocr,
            &one_hot(0),
            "gemma",
        )
        .await
        .unwrap();

    // Pre-merge: four per-word spans on disk.
    assert_eq!(store.frame_spans(id).await.unwrap().len(), 4);

    store.merge_frame_spans_to_lines(id).await.unwrap();

    // Post-merge: one span per line, still carrying non-empty text + a box (reconstruction).
    let merged = store.frame_spans(id).await.unwrap();
    assert_eq!(merged.len(), 2, "four words on two lines → two line spans");
    assert_eq!(merged[0].text, "quarterly invoice");
    assert!(
        merged
            .iter()
            .all(|s| !s.text.is_empty() && s.w > 0.0 && s.h > 0.0),
        "each line keeps text + geometry for the reconstruction: {merged:?}"
    );

    // content_text + FTS untouched → content search still matches (FTS-only, no embedder).
    let hits = store.hybrid_search(&query("invoice", 10)).await.unwrap();
    assert!(
        hits.iter().any(|h| h.frame_id == id),
        "content search still finds the frame after the span merge"
    );
    // Text embedding untouched → the vector KNN still returns it.
    let vids = store.nearest_text_frames(&one_hot(0), 5).await.unwrap();
    assert!(vids.contains(&id), "text embedding survives the span merge");
}

/// Merging a frame that has no spans is a safe no-op (never errors).
#[tokio::test]
async fn merge_frame_spans_to_lines_is_noop_without_spans() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(2_000)).await.unwrap();
    store.merge_frame_spans_to_lines(id).await.unwrap();
    assert!(store.frame_spans(id).await.unwrap().is_empty());
}

/// `degrade_frame_to_text` collapses the per-word spans **and** sets `image_purged` in one
/// transaction (`07` #73a, Codex review). The atomicity is what keeps a mid-sweep failure from
/// stranding per-word rows on a frame the sweep will never revisit: the flag is set iff the merge
/// committed. Idempotent — a second call is a no-op merge plus an unchanged purge flag.
#[tokio::test]
async fn degrade_frame_to_text_merges_spans_and_purges_atomically() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(1_000)).await.unwrap();
    store
        .insert_ocr(
            id,
            OcrResult {
                text: "quarterly invoice total due".to_string(),
                mean_confidence: 0.9,
                engine: "winrt".to_string(),
                spans: vec![
                    line_span("quarterly", 0, 0.0, 0.0, 0.12, 0.05),
                    line_span("invoice", 0, 0.14, 0.0, 0.10, 0.05),
                    line_span("total", 1, 0.0, 0.10, 0.08, 0.05),
                    line_span("due", 1, 0.10, 0.10, 0.06, 0.05),
                ],
            },
        )
        .await
        .unwrap();
    // Pre: four per-word spans, image still present.
    assert_eq!(store.frame_spans(id).await.unwrap().len(), 4);
    assert!(!store.get_frame(id).await.unwrap().unwrap().image_purged);

    store.degrade_frame_to_text(id).await.unwrap();

    // Post: spans merged to per-line AND the frame is marked purged (both, in one commit).
    assert_eq!(
        store.frame_spans(id).await.unwrap().len(),
        2,
        "four words on two lines → two line spans"
    );
    assert!(
        store.get_frame(id).await.unwrap().unwrap().image_purged,
        "degrade marks the image purged"
    );
    // No longer an image-retention candidate — the sweep won't re-list it.
    let cands = store.frames_with_image_older_than(2_000, 10).await.unwrap();
    assert!(cands.iter().all(|f| f.frame_id != id));

    // Idempotent: re-degrading an already-degraded frame keeps one span/line and stays purged.
    store.degrade_frame_to_text(id).await.unwrap();
    assert_eq!(store.frame_spans(id).await.unwrap().len(), 2);
    assert!(store.get_frame(id).await.unwrap().unwrap().image_purged);
}

/// A frame with no spans still degrades (the purge flag is set even when there is nothing to
/// merge) — the sweep must be able to retire a text-empty frame's image too.
#[tokio::test]
async fn degrade_frame_to_text_purges_even_without_spans() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame_at(2_000)).await.unwrap();
    store.degrade_frame_to_text(id).await.unwrap();
    assert!(store.frame_spans(id).await.unwrap().is_empty());
    assert!(store.get_frame(id).await.unwrap().unwrap().image_purged);
}

/// The one-time span-merge backfill (`07` #73a) lists **purged** frames only, id-ascending,
/// cursor-batched — so it can drain the pre-existing purged backlog without a full scan.
#[tokio::test]
async fn purged_frame_ids_lists_only_purged_after_cursor() {
    let store = SqliteStore::open_in_memory().unwrap();
    let a = store.insert_frame(frame_at(1_000)).await.unwrap();
    let _b = store.insert_frame(frame_at(2_000)).await.unwrap();
    let c = store.insert_frame(frame_at(3_000)).await.unwrap();
    store.purge_frame_image(a).await.unwrap();
    store.purge_frame_image(c).await.unwrap();

    // From cursor 0: both purged ids ascending; the unpurged `_b` is excluded.
    assert_eq!(store.purged_frame_ids(0, 10).await.unwrap(), vec![a, c]);
    // Cursor past `a`: only `c` remains.
    assert_eq!(store.purged_frame_ids(a, 10).await.unwrap(), vec![c]);
    // Limit is respected (drives the batch cursor).
    assert_eq!(store.purged_frame_ids(0, 1).await.unwrap(), vec![a]);
    // Zero limit → empty (never a full scan).
    assert!(store.purged_frame_ids(0, 0).await.unwrap().is_empty());
}

/// Unit vector at angle `theta` (radians) in the (dim 0, dim 1) plane:
/// `[cos θ, sin θ, 0, …]`. Cosine distance to the query `vec_at_angle(0.0)` grows
/// monotonically with θ over `[0, π/2]`, so a series of increasing angles yields a
/// deterministic KNN ordering with *graded* distances (unlike `one_hot`, whose only
/// distances are ~0 and 1.0 — too coarse to rank a pool of near-neighbours).
fn vec_at_angle(theta: f32) -> Embedding {
    let mut v = vec![0.0_f32; EMBEDDING_DIM];
    v[0] = theta.cos();
    v[1] = theta.sin();
    Embedding(v)
}

/// Regression for `07` #8: the vector arm must not miss an in-range match just because
/// it is buried beyond the top-`pool` nearest vectors. We bury the *only* in-window
/// vector-only match behind 55 nearer OUT-of-window vectors — more than the `pool = 50`
/// floor — so a fixed-`k = pool` KNN + time post-filter fetches only out-of-window rows,
/// drops them all, and returns nothing. Adaptive pool escalation must still surface it.
#[tokio::test]
async fn vector_arm_finds_in_range_match_buried_beyond_pool() {
    let store = SqliteStore::open_in_memory().unwrap();
    let query_vec = vec_at_angle(0.0);

    // 55 out-of-window frames, each nearer to the query than the in-window one (rank 1..55).
    // None contain the query term, so the FTS arm can't surface anything — the vector arm is
    // the only path, isolating the recall behaviour under test.
    for i in 1..=55 {
        seed(
            &store,
            1_000 + i,
            "filler noise unrelated",
            Some(&vec_at_angle(0.001 * i as f32)),
        )
        .await;
    }
    // One in-window frame, farther than all 55 (rank 56 — beyond the k = 50 fetch), also
    // vector-only (no query term in its text).
    let in_range = seed(
        &store,
        55_000,
        "filler noise unrelated",
        Some(&vec_at_angle(0.060)),
    )
    .await;

    let mut by_text = HashMap::new();
    by_text.insert("target".to_string(), query_vec);
    let store = store.with_embedder(Arc::new(FakeEmbedder { by_text }));

    let q = SearchQuery {
        text: "target".to_string(),
        limit: 1, // pool = max(1*5, 50) = 50; the in-window match sits at rank 56
        time_range: Some(TimeRange {
            start: 50_000,
            end: 60_000,
        }),
        include_chrome: false,
    };
    let hits = store.hybrid_search(&q).await.unwrap();
    let ids: Vec<i64> = hits.iter().map(|h| h.frame_id).collect();
    assert_eq!(
        ids,
        vec![in_range],
        "the in-window vector match must be found even when buried beyond the pool; got {ids:?}"
    );
}

/// `07` #8 review (P2): the escalation target is capped at the count of distinct embedded
/// frames in the window, so a **sparse** window (fewer than `pool`) stops early instead of
/// scanning to the k ceiling. This guards the correctness half of that change — the count cap
/// must not drop real in-window matches (a mis-count of 0 would skip the KNN and return
/// nothing). Every in-window frame, scattered across the KNN distance order, must come back.
#[tokio::test]
async fn sparse_time_window_returns_every_in_window_match() {
    let store = SqliteStore::open_in_memory().unwrap();
    let query_vec = vec_at_angle(0.0);

    // 20 near out-of-window frames (ranks 1..20), then 3 in-window frames scattered farther
    // out by distance (ranks ~21, ~26, ~31) — all vector-only (no query term in the text).
    for i in 1..=20 {
        seed(
            &store,
            1_000 + i,
            "filler noise unrelated",
            Some(&vec_at_angle(0.002 * i as f32)),
        )
        .await;
    }
    let w1 = seed(
        &store,
        50_000,
        "filler noise unrelated",
        Some(&vec_at_angle(0.042)),
    )
    .await;
    let w2 = seed(
        &store,
        51_000,
        "filler noise unrelated",
        Some(&vec_at_angle(0.052)),
    )
    .await;
    let w3 = seed(
        &store,
        52_000,
        "filler noise unrelated",
        Some(&vec_at_angle(0.062)),
    )
    .await;

    let mut by_text = HashMap::new();
    by_text.insert("target".to_string(), query_vec);
    let store = store.with_embedder(Arc::new(FakeEmbedder { by_text }));

    let q = SearchQuery {
        text: "target".to_string(),
        limit: 10, // pool = 50; the window holds only 3 embedded frames → target = 3
        time_range: Some(TimeRange {
            start: 50_000,
            end: 60_000,
        }),
        include_chrome: false,
    };
    let hits = store.hybrid_search(&q).await.unwrap();
    let mut ids: Vec<i64> = hits.iter().map(|h| h.frame_id).collect();
    ids.sort();
    let mut want = vec![w1, w2, w3];
    want.sort();
    assert_eq!(
        ids, want,
        "every in-window match must be present under the count-capped target; got {ids:?}"
    );
}

/// `07` #8 review (P2): a bounded window with **more** embedded frames than `pool` must behave
/// exactly like the pre-review code — the count cap collapses `target` to `pool`, so escalation
/// still returns the `pool` *nearest* in-window frames (never truncates to some arbitrary
/// subset). We over-fill the window and assert the nearest match ranks first and the count is
/// honored.
#[tokio::test]
async fn dense_time_window_returns_the_pool_nearest_in_window_matches() {
    let store = SqliteStore::open_in_memory().unwrap();
    let query_vec = vec_at_angle(0.0);

    // 60 in-window frames at increasing distance (angles 0.001..0.060) — more than the
    // pool floor of 50, so the count cap makes target = pool and only the 50 nearest return.
    let mut in_window = Vec::new();
    for i in 1..=60 {
        let id = seed(
            &store,
            50_000 + i,
            "filler noise unrelated",
            Some(&vec_at_angle(0.001 * i as f32)),
        )
        .await;
        in_window.push(id);
    }

    let mut by_text = HashMap::new();
    by_text.insert("target".to_string(), query_vec);
    let store = store.with_embedder(Arc::new(FakeEmbedder { by_text }));

    let q = SearchQuery {
        text: "target".to_string(),
        limit: 1, // pool = 50; window has 60 → target caps at 50, nearest-first
        time_range: Some(TimeRange {
            start: 50_000,
            end: 60_000,
        }),
        include_chrome: false,
    };
    let hits = store.hybrid_search(&q).await.unwrap();
    // limit=1 → one hit, and it must be the *nearest* in-window frame (angle 0.001, rank 1).
    assert_eq!(
        hits.first().map(|h| h.frame_id),
        Some(in_window[0]),
        "the nearest in-window match must rank first even when the window over-fills the pool"
    );
}

/// The count cap must not swallow an empty window into a stale result: a bounded window with
/// no embedded frames returns nothing (and, by design, skips the KNN altogether).
#[tokio::test]
async fn empty_time_window_returns_nothing_via_vector_arm() {
    let store = SqliteStore::open_in_memory().unwrap();
    // One embedded frame, well outside the queried window.
    seed(
        &store,
        1_000,
        "filler noise unrelated",
        Some(&vec_at_angle(0.01)),
    )
    .await;

    let mut by_text = HashMap::new();
    by_text.insert("target".to_string(), vec_at_angle(0.0));
    let store = store.with_embedder(Arc::new(FakeEmbedder { by_text }));

    let q = SearchQuery {
        text: "target".to_string(),
        limit: 10,
        time_range: Some(TimeRange {
            start: 50_000,
            end: 60_000,
        }),
        include_chrome: false,
    };
    assert!(
        store.hybrid_search(&q).await.unwrap().is_empty(),
        "a window with no embedded frames must yield nothing"
    );
}

#[tokio::test]
async fn hybrid_search_respects_limit() {
    let store = SqliteStore::open_in_memory().unwrap();
    for i in 0..5 {
        seed(&store, 100 + i, "common keyword here", None).await;
    }
    let hits = store.hybrid_search(&query("keyword", 3)).await.unwrap();
    assert_eq!(hits.len(), 3);

    let one = store.hybrid_search(&query("keyword", 0)).await.unwrap();
    assert_eq!(one.len(), 1);
}

#[tokio::test]
async fn hybrid_search_clamps_excessive_limit() {
    let store = SqliteStore::open_in_memory().unwrap();
    for i in 0..150 {
        seed(&store, 10_000 + i, "common keyword here", None).await;
    }

    // An absurd limit is clamped to the backend ceiling (2000, raised so prompted reports
    // can honor `reports.weekly_top_k`) without panicking; with 150 matches seeded — below
    // the ceiling — all 150 come back rather than being capped at the old UI max of 100.
    // (The clamp itself is unit-tested in `search.rs::normalized_limit_clamps_*`.)
    let hits = store
        .hybrid_search(&query("keyword", u32::MAX))
        .await
        .unwrap();
    assert_eq!(hits.len(), 150);
}

#[tokio::test]
async fn claim_returns_highest_priority_first_and_marks_running() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, 10, 3, 0))
        .await
        .unwrap();

    let claimed = store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 2);
    assert_eq!(claimed[0].priority, 10); // higher priority first
    assert_eq!(claimed[1].priority, 0);
    assert!(claimed.iter().all(|j| j.state == JobState::Running));

    // a second claim finds nothing pending
    assert!(store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap()
        .is_empty());

    let stats = store.job_stats().await.unwrap();
    assert_eq!(stats.running, 2);
    assert_eq!(stats.pending, 0);
}

#[tokio::test]
async fn claim_honors_not_before_schedule() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 200))
        .await
        .unwrap();

    assert!(store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .claim_jobs(&[JobKind::EmbedText], 10, 300)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn claim_filters_by_kind() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    store
        .enqueue_job(job(JobKind::VisionTag, 0, 3, 0))
        .await
        .unwrap();

    let claimed = store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].kind, JobKind::EmbedText);
    // no enabled kinds → claims nothing
    assert!(store.claim_jobs(&[], 10, 100).await.unwrap().is_empty());
}

#[tokio::test]
async fn complete_job_moves_to_done() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();

    store.complete_job(id).await.unwrap();
    let stats = store.job_stats().await.unwrap();
    assert_eq!(stats.done, 1);
    assert_eq!(stats.running, 0);
}

#[tokio::test]
async fn complete_job_requires_running_state() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();

    let pending_err = store.complete_job(id).await.unwrap_err().to_string();
    assert!(
        pending_err.contains("not running"),
        "pending job completion should be rejected, got {pending_err:?}"
    );
    assert_eq!(store.job_stats().await.unwrap().pending, 1);

    store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();
    store.complete_job(id).await.unwrap();
    assert_eq!(store.job_stats().await.unwrap().done, 1);

    let done_err = store.complete_job(id).await.unwrap_err().to_string();
    assert!(
        done_err.contains("not running"),
        "completed job should not be completed again, got {done_err:?}"
    );
}

#[tokio::test]
async fn job_stats_splits_out_vision_pending_and_running() {
    // The tray "Start/Stop vision tagging" label reads `vision_pending`/`vision_running`
    // (subsets of the aggregate counts). Mix kinds + states and assert the split (0.3.2 PR3).
    let store = SqliteStore::open_in_memory().unwrap();
    for _ in 0..3 {
        store
            .enqueue_job(job(JobKind::VisionTag, 0, 3, 0))
            .await
            .unwrap();
    }
    store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    // Claim one vision job → it moves to running (index-backed by kind/state).
    let claimed = store
        .claim_jobs(&[JobKind::VisionTag], 1, 100)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    let stats = store.job_stats().await.unwrap();
    // Aggregate: 2 vision + 1 embed pending, 1 vision running.
    assert_eq!(stats.pending, 3);
    assert_eq!(stats.running, 1);
    // Per-kind vision split.
    assert_eq!(stats.vision_pending, 2);
    assert_eq!(stats.vision_running, 1);
}

#[tokio::test]
async fn cancel_pending_vision_jobs_removes_only_pending_vision() {
    // "Stop vision tagging" drops pending vision jobs; a running vision job and other kinds
    // are untouched (no lease to revoke — the in-flight job finishes) (0.3.2 PR3).
    let store = SqliteStore::open_in_memory().unwrap();
    for _ in 0..3 {
        store
            .enqueue_job(job(JobKind::VisionTag, 0, 3, 0))
            .await
            .unwrap();
    }
    store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    // One vision job goes running before the cancel.
    let claimed = store
        .claim_jobs(&[JobKind::VisionTag], 1, 100)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    let cancelled = store.cancel_pending_vision_jobs().await.unwrap();
    assert_eq!(cancelled, 2, "both pending vision jobs are cancelled");

    let stats = store.job_stats().await.unwrap();
    assert_eq!(stats.vision_pending, 0, "no pending vision jobs remain");
    assert_eq!(
        stats.vision_running, 1,
        "the running vision job is left to finish"
    );
    assert_eq!(stats.pending, 1, "the pending embed_text job is untouched");

    // Idempotent: a second cancel with nothing pending removes zero.
    assert_eq!(store.cancel_pending_vision_jobs().await.unwrap(), 0);
}

#[tokio::test]
async fn fail_retries_with_backoff_then_dead_letters_at_max_attempts() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store
        .enqueue_job(job(JobKind::EmbedText, 0, 2, 0))
        .await
        .unwrap();

    // attempt 1 fails with a retry scheduled at t=500
    store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();
    store.fail_job(id, "boom", Some(500)).await.unwrap();
    assert_eq!(store.job_stats().await.unwrap().pending, 1);
    // not yet runnable (backoff), runnable at/after 500
    assert!(store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap()
        .is_empty());
    let again = store
        .claim_jobs(&[JobKind::EmbedText], 10, 500)
        .await
        .unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].attempts, 1);
    assert_eq!(again[0].last_error.as_deref(), Some("boom"));

    // attempt 2 fails → max_attempts reached → dead-letter (never silently dropped)
    store.fail_job(id, "boom again", Some(900)).await.unwrap();
    let stats = store.job_stats().await.unwrap();
    assert_eq!(stats.dead, 1);
    assert_eq!(stats.pending, 0);
    assert!(store
        .claim_jobs(&[JobKind::EmbedText], 10, 100_000)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn fail_without_retry_at_dead_letters_immediately() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store
        .enqueue_job(job(JobKind::EmbedText, 0, 5, 0))
        .await
        .unwrap();
    store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();

    // retry_at = None → terminal even though attempts remain
    store.fail_job(id, "fatal", None).await.unwrap();
    assert_eq!(store.job_stats().await.unwrap().dead, 1);
}

#[tokio::test]
async fn fail_job_requires_running_state() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store
        .enqueue_job(job(JobKind::EmbedText, 0, 2, 0))
        .await
        .unwrap();

    let pending_err = store
        .fail_job(id, "not claimed", Some(500))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        pending_err.contains("not running"),
        "pending job failure should be rejected, got {pending_err:?}"
    );
    let stats = store.job_stats().await.unwrap();
    assert_eq!(stats.pending, 1);
    assert_eq!(stats.dead, 0);

    store
        .claim_jobs(&[JobKind::EmbedText], 10, 100)
        .await
        .unwrap();
    store.fail_job(id, "retryable", Some(500)).await.unwrap();
    assert_eq!(store.job_stats().await.unwrap().pending, 1);

    let retry_pending_err = store
        .fail_job(id, "still not claimed", Some(700))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        retry_pending_err.contains("not running"),
        "retry-pending job failure should be rejected, got {retry_pending_err:?}"
    );

    store
        .claim_jobs(&[JobKind::EmbedText], 10, 500)
        .await
        .unwrap();
    store.fail_job(id, "fatal", None).await.unwrap();
    assert_eq!(store.job_stats().await.unwrap().dead, 1);

    let dead_err = store
        .fail_job(id, "already dead", None)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        dead_err.contains("not running"),
        "dead job failure should be rejected, got {dead_err:?}"
    );
}

#[tokio::test]
async fn completing_or_failing_an_unknown_job_is_an_error() {
    // a write that changes no rows is a programming error, not a silent no-op
    let store = SqliteStore::open_in_memory().unwrap();
    assert!(store.complete_job(999).await.is_err());
    assert!(store.fail_job(999, "boom", Some(10)).await.is_err());
}

// Proves the *production* concurrency model: many async callers share one
// `Mutex<Connection>`, so claims serialize through it and the atomic
// `UPDATE … RETURNING` hands each job to exactly one caller — none lost, none
// double-claimed. It does NOT exercise multi-connection WAL contention (the store
// is single-connection by design; see `05` "Still risky" and `07`).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_claims_never_double_claim() {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    const N: usize = 40;
    for _ in 0..N {
        store
            .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
            .await
            .unwrap();
    }

    let mut handles = Vec::new();
    for _ in 0..4 {
        let s = store.clone();
        handles.push(tokio::spawn(async move {
            let mut got = Vec::new();
            loop {
                let batch = s.claim_jobs(&[JobKind::EmbedText], 3, 1_000).await.unwrap();
                if batch.is_empty() {
                    break;
                }
                got.extend(batch.into_iter().map(|j| j.id));
            }
            got
        }));
    }

    let mut all = Vec::new();
    for h in handles {
        all.extend(h.await.unwrap());
    }
    let unique: HashSet<i64> = all.iter().copied().collect();
    assert_eq!(all.len(), N, "every job claimed exactly once (no loss)");
    assert_eq!(unique.len(), N, "no job claimed twice (no double-claim)");
}

#[tokio::test]
async fn works_through_the_store_trait_object() {
    use traits::Store;

    // the composition root will hold the store as `Arc<dyn Store>`; prove the
    // trait impl forwards to the inherent methods across the boundary.
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().unwrap());
    let id = store.insert_frame(frame_at(42)).await.unwrap();
    store
        .insert_ocr(
            id,
            OcrResult {
                text: "trait object path".to_string(),
                mean_confidence: 0.9,
                engine: "winrt".to_string(),
                spans: vec![span("trait", 0.0, 0.0, 0.2, 0.05)],
            },
        )
        .await
        .unwrap();

    let hits = store.hybrid_search(&query("trait", 10)).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].frame_id, id);

    let jid = store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    assert!(jid > 0);
    assert_eq!(store.job_stats().await.unwrap().pending, 1);

    store.set_setting("k", "v").await.unwrap();
    assert_eq!(store.get_setting("k").await.unwrap(), Some("v".to_string()));
}

// --- enrichment-input read + stale-job recovery (P3, `03 §5/§6`) ---------------

/// The embedding worker's lightweight read returns the JPEG path and, once OCR has
/// run, the text — and `None` for a frame that no longer exists.
#[tokio::test]
async fn frame_enrichment_input_reads_path_and_optional_text() {
    let store = SqliteStore::open_in_memory().unwrap();
    let fid = store.insert_frame(frame_at(1_000)).await.unwrap();

    // before OCR: image path present, text absent
    let pre = store
        .frame_enrichment_input(fid)
        .await
        .unwrap()
        .expect("frame exists");
    assert_eq!(pre.image_path, "frames/1000.jpg");
    assert_eq!(pre.ocr_text, None);

    // after OCR: text present
    store
        .insert_ocr(
            fid,
            OcrResult {
                text: "hello world".to_string(),
                mean_confidence: -1.0,
                engine: "test".to_string(),
                spans: vec![span("hello", 0.0, 0.0, 0.2, 0.05)],
            },
        )
        .await
        .unwrap();
    let post = store.frame_enrichment_input(fid).await.unwrap().unwrap();
    assert_eq!(post.ocr_text.as_deref(), Some("hello world"));

    // missing frame → None
    assert!(store.frame_enrichment_input(9_999).await.unwrap().is_none());
}

/// The startup sweep (`older_than == 0`) requeues a `running` job a dead worker
/// left behind, *without* consuming an attempt — it is reclaimable again (`03 §6`).
#[tokio::test]
async fn reset_stale_running_jobs_requeues_running() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    // claim → running, stamped at a past time
    let claimed = store
        .claim_jobs(&[JobKind::EmbedText], 10, 1_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    assert_eq!(store.reset_stale_running_jobs(0).await.unwrap(), 1);

    let again = store
        .claim_jobs(&[JobKind::EmbedText], 10, 2_000)
        .await
        .unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].id, id);
    assert_eq!(
        again[0].attempts, 0,
        "a crash sweep must not consume an attempt"
    );
}

/// A job claimed `now` is within the visibility window — a 5-minute periodic sweep
/// must not requeue it out from under a live worker.
#[tokio::test]
async fn reset_stale_running_jobs_spares_fresh_running() {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .enqueue_job(job(JobKind::EmbedText, 0, 3, 0))
        .await
        .unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    store
        .claim_jobs(&[JobKind::EmbedText], 10, now)
        .await
        .unwrap();

    assert_eq!(store.reset_stale_running_jobs(5 * 60_000).await.unwrap(), 0);
}

/// `untagged_frame_ids` returns frames with no vision row, oldest first, honoring the
/// limit and the optional time window (the timer/idle batch + `enqueue_vision` range
/// source, `03 §5`).
#[tokio::test]
async fn untagged_frame_ids_excludes_tagged_and_honors_range() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f1 = store.insert_frame(frame_at(100)).await.unwrap();
    let f2 = store.insert_frame(frame_at(200)).await.unwrap();
    let f3 = store.insert_frame(frame_at(300)).await.unwrap();

    // Tag the middle frame; it must drop out of the untagged set.
    store
        .insert_vision(
            f2,
            VisionAnalysis {
                description: "tagged".to_string(),
                activity_type: None,
                app_hint: None,
                confidence: 0.5,
                model: "m".to_string(),
            },
        )
        .await
        .unwrap();

    // All untagged, oldest first.
    assert_eq!(
        store.untagged_frame_ids(10, None).await.unwrap(),
        vec![f1, f3]
    );
    // Limit caps the result.
    assert_eq!(store.untagged_frame_ids(1, None).await.unwrap(), vec![f1]);
    // Range [150, 350) excludes f1 (too early) and f2 (tagged) → only f3.
    assert_eq!(
        store
            .untagged_frame_ids(10, Some((150, 350)))
            .await
            .unwrap(),
        vec![f3]
    );
}

/// `untagged_frame_ids` also excludes frames with a `vision_tag` job in a
/// `pending`/`running`/`dead` state, so a slow batch isn't re-enqueued on the next tick,
/// the timer/idle lanes don't double-queue the same frames, and a poisoned (dead-lettered)
/// frame isn't re-enqueued forever. A finished (`done`) vision_tag job, or a pending job
/// of another kind, leaves a still-untagged frame eligible.
#[tokio::test]
async fn untagged_frame_ids_excludes_in_flight_vision_jobs() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f1 = store.insert_frame(frame_at(100)).await.unwrap();
    let f2 = store.insert_frame(frame_at(200)).await.unwrap();
    let f3 = store.insert_frame(frame_at(300)).await.unwrap();
    let f4 = store.insert_frame(frame_at(400)).await.unwrap();
    let f5 = store.insert_frame(frame_at(500)).await.unwrap();
    let f6 = store.insert_frame(frame_at(600)).await.unwrap();

    let frame_job = |kind, frame_id| NewJob {
        kind,
        frame_id: Some(frame_id),
        priority: 0,
        max_attempts: 3,
        not_before: 0,
    };

    // f3: a vision_tag job that ran to completion — it still has no vision_analysis
    // row, so the frame stays eligible. (Claim then complete; it's the only pending
    // vision_tag job at this point.)
    store
        .enqueue_job(frame_job(JobKind::VisionTag, f3))
        .await
        .unwrap();
    let done = store
        .claim_jobs(&[JobKind::VisionTag], 10, 100)
        .await
        .unwrap();
    assert_eq!(done.len(), 1);
    store.complete_job(done[0].id).await.unwrap();

    // f6: a vision_tag job that exhausted retries and dead-lettered — excluded, so a
    // poisoned frame isn't re-enqueued every tick. (Claim then fail with no retry.)
    store
        .enqueue_job(frame_job(JobKind::VisionTag, f6))
        .await
        .unwrap();
    let dead = store
        .claim_jobs(&[JobKind::VisionTag], 10, 100)
        .await
        .unwrap();
    assert_eq!(dead.len(), 1);
    assert_eq!(dead[0].frame_id, Some(f6));
    store.fail_job(dead[0].id, "poisoned", None).await.unwrap();

    // f2: a vision_tag job left running (claimed, not completed) — excluded.
    store
        .enqueue_job(frame_job(JobKind::VisionTag, f2))
        .await
        .unwrap();
    let running = store
        .claim_jobs(&[JobKind::VisionTag], 10, 100)
        .await
        .unwrap();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].frame_id, Some(f2));

    // f1: a vision_tag job left pending — excluded.
    store
        .enqueue_job(frame_job(JobKind::VisionTag, f1))
        .await
        .unwrap();

    // f4: a pending job of a *different* kind — does not exclude the frame.
    store
        .enqueue_job(frame_job(JobKind::EmbedText, f4))
        .await
        .unwrap();

    // f5: no job at all.

    // Eligible, oldest first: f3 (done), f4 (other kind), f5 (no job).
    // Excluded: f1 (pending), f2 (running), f6 (dead).
    assert_eq!(
        store.untagged_frame_ids(10, None).await.unwrap(),
        vec![f3, f4, f5]
    );
}

/// `ocr_texts` bulk-fetches non-empty OCR text for many frames in one query (the `ask`
/// grounding hydrate). Frames without text — or with empty text — are omitted.
#[tokio::test]
async fn ocr_texts_bulk_fetches_nonempty_only() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f1 = store.insert_frame(frame_at(100)).await.unwrap();
    let f2 = store.insert_frame(frame_at(200)).await.unwrap();
    let f3 = store.insert_frame(frame_at(300)).await.unwrap();
    let ocr = |text: &str| OcrResult {
        text: text.to_string(),
        mean_confidence: -1.0,
        engine: "winrt".to_string(),
        spans: Vec::new(),
    };
    store.insert_ocr(f1, ocr("login screen")).await.unwrap();
    store.insert_ocr(f2, ocr("")).await.unwrap(); // empty → omitted
                                                  // f3 has no OCR row at all → omitted.

    let map = store.ocr_texts(&[f1, f2, f3]).await.unwrap();
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(&f1).map(String::as_str), Some("login screen"));
    assert!(!map.contains_key(&f2));
    assert!(!map.contains_key(&f3));

    // Empty input is a no-op (no query).
    assert!(store.ocr_texts(&[]).await.unwrap().is_empty());
}

/// `timeline_buckets` groups frames into fixed-width, half-open buckets and returns
/// only the **occupied** ones (sparse), ascending by time. Backs `get_timeline`.
#[tokio::test]
async fn timeline_buckets_are_sparse_and_half_open() {
    let store = SqliteStore::open_in_memory().unwrap();
    // bucket width = ceil(200 / 4) = 50 → [0,50) [50,100) [100,150) [150,200).
    for ts in [10, 20, 120] {
        store.insert_frame(frame_at(ts)).await.unwrap();
    }
    // Out of range: a frame before `start` and one exactly at the exclusive `end`
    // are both excluded.
    store.insert_frame(frame_at(-5)).await.unwrap();
    store.insert_frame(frame_at(200)).await.unwrap();

    let buckets = store.timeline_buckets(0, 200, 4).await.unwrap();
    assert_eq!(
        buckets,
        vec![
            TimelineBucket {
                start: 0,
                end: 50,
                count: 2
            },
            TimelineBucket {
                start: 100,
                end: 150,
                count: 1
            },
        ],
    );

    // Invalid / degenerate ranges yield nothing.
    assert!(store.timeline_buckets(200, 0, 4).await.unwrap().is_empty());
    assert!(store.timeline_buckets(0, 200, 0).await.unwrap().is_empty());
}

/// `insights_summary` returns real aggregates over the half-open window: total and
/// vision-tagged counts, the top foreground apps, and the activity-type breakdown.
#[tokio::test]
async fn insights_summary_aggregates_truthfully() {
    let store = SqliteStore::open_in_memory().unwrap();
    let firefox = |ts: i64| {
        let mut f = frame_at(ts);
        f.app_hint = Some("Firefox".to_string());
        f
    };
    // 3 Firefox, 1 Code, 1 with no app hint — all inside [0, 1000).
    let f1 = store.insert_frame(firefox(100)).await.unwrap();
    store.insert_frame(firefox(200)).await.unwrap();
    store.insert_frame(firefox(300)).await.unwrap();
    let mut code = frame_at(400);
    code.app_hint = Some("Code".to_string());
    let f_code = store.insert_frame(code).await.unwrap();
    let mut anon = frame_at(500);
    anon.app_hint = None;
    store.insert_frame(anon).await.unwrap();

    // Tag two frames with vision activity types (writes frames.activity_type).
    let vision = |activity: &str| VisionAnalysis {
        description: "desc".to_string(),
        activity_type: Some(activity.to_string()),
        app_hint: None,
        confidence: 0.9,
        model: "test".to_string(),
    };
    store.insert_vision(f1, vision("browsing")).await.unwrap();
    store.insert_vision(f_code, vision("coding")).await.unwrap();

    let s = store.insights_summary(0, 1000, 48).await.unwrap();
    assert_eq!(s.total_frames, 5);
    assert_eq!(s.tagged_frames, 2);
    // Most-captured app is Firefox (3), ordered by count desc.
    let top = s.top_apps.first().expect("a top app");
    assert_eq!(top.app.as_deref(), Some("Firefox"));
    assert_eq!(top.count, 3);
    // Both tagged activities appear, each once.
    let activities: Vec<(Option<String>, u32)> = s
        .activity_breakdown
        .iter()
        .map(|a| (a.activity.clone(), a.count))
        .collect();
    assert!(activities.contains(&(Some("browsing".to_string()), 1)));
    assert!(activities.contains(&(Some("coding".to_string()), 1)));
    assert!(!s.captures.is_empty(), "capture density buckets present");

    // A window with no frames → honest-empty summary, never fabricated.
    let empty = store.insights_summary(10_000, 20_000, 48).await.unwrap();
    assert_eq!(empty.total_frames, 0);
    assert!(empty.top_apps.is_empty());
    assert!(empty.activity_breakdown.is_empty());
}

/// `timeline_buckets` must not panic on hostile/extreme timestamps (the values come
/// straight from the frontend). An unrepresentable span yields an empty result, and
/// a huge-but-representable span — which the old `(span + n - 1)` ceil would have
/// overflowed — is handled cleanly.
#[tokio::test]
async fn timeline_buckets_survives_extreme_ranges() {
    let store = SqliteStore::open_in_memory().unwrap();
    store.insert_frame(frame_at(0)).await.unwrap();

    // `end - start` overflows i64 → empty window, no panic.
    assert!(store
        .timeline_buckets(i64::MIN, i64::MAX, 4)
        .await
        .unwrap()
        .is_empty());

    // Span is exactly i64::MAX (representable). The previous `(span + n - 1) / n`
    // ceil would overflow here; the current form does not. The single frame at 0
    // lands in bucket 0.
    let buckets = store.timeline_buckets(0, i64::MAX, 4).await.unwrap();
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].start, 0);
    assert_eq!(buckets[0].count, 1);
    // A forward bucket: a `checked_add` overflow would wrap the end negative.
    assert!(buckets[0].end > buckets[0].start);
}

/// `frames_in_range` lists frames in the half-open window, most-recent-first, capped
/// at `limit` — the Timeline thumbnails / Deck recents source. Backs `get_frames`.
#[tokio::test]
async fn frames_in_range_lists_window_recent_first() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f1 = store.insert_frame(frame_at(100)).await.unwrap();
    let f2 = store.insert_frame(frame_at(200)).await.unwrap();
    let f3 = store.insert_frame(frame_at(300)).await.unwrap();
    // Out of [150, 350): one before `start`, one exactly at the exclusive `end`.
    store.insert_frame(frame_at(100)).await.unwrap(); // same ts as f1, still < start
    store.insert_frame(frame_at(350)).await.unwrap();

    // Window [150, 350): f2 and f3 only, newest first.
    let metas = store.frames_in_range(150, 350, 10).await.unwrap();
    let ids: Vec<i64> = metas.iter().map(|m| m.frame_id).collect();
    assert_eq!(ids, vec![f3, f2]);
    // FrameMeta carries the lightweight browsing fields.
    assert_eq!(metas[0].captured_at, 300);
    assert_eq!(metas[0].image_path, "frames/300.jpg");
    assert_eq!(metas[0].app_hint.as_deref(), Some("Firefox"));

    // `limit` caps the result; newest-first means the most recent (@350) is kept.
    let capped = store.frames_in_range(0, 1_000, 1).await.unwrap();
    assert_eq!(capped.len(), 1);
    assert_eq!(capped[0].captured_at, 350);

    // Degenerate windows / zero limit yield nothing — never the whole table.
    assert!(store
        .frames_in_range(350, 150, 10)
        .await
        .unwrap()
        .is_empty());
    assert!(store.frames_in_range(0, 1_000, 0).await.unwrap().is_empty());
    // f1 is reachable in a window that includes it.
    assert!(store
        .frames_in_range(0, 150, 10)
        .await
        .unwrap()
        .iter()
        .any(|m| m.frame_id == f1));
}

/// `nearest_frame` resolves a timestamp to the closest frame on either side, with
/// the at-or-after frame winning an exact tie. Backs `get_nearest_frame` (the
/// Timeline scan-head → concrete frame id). `None` only when the DB is empty.
#[tokio::test]
async fn nearest_frame_picks_closest_with_after_winning_ties() {
    let store = SqliteStore::open_in_memory().unwrap();
    // Empty DB → no nearest frame.
    assert!(store.nearest_frame(1_000).await.unwrap().is_none());

    let f100 = store.insert_frame(frame_at(100)).await.unwrap();
    let f200 = store.insert_frame(frame_at(200)).await.unwrap();
    let f400 = store.insert_frame(frame_at(400)).await.unwrap();

    let id_at = |m: Option<FrameMeta>| m.expect("a frame").frame_id;
    // Before the first frame → the first frame.
    assert_eq!(id_at(store.nearest_frame(0).await.unwrap()), f100);
    // After the last frame → the last frame.
    assert_eq!(id_at(store.nearest_frame(10_000).await.unwrap()), f400);
    // Closer to f200 than f100.
    assert_eq!(id_at(store.nearest_frame(170).await.unwrap()), f200);
    // Closer to f200 than f400.
    assert_eq!(id_at(store.nearest_frame(260).await.unwrap()), f200);
    // Exact midpoint between f200 and f400 (300) → the later frame wins the tie.
    assert_eq!(id_at(store.nearest_frame(300).await.unwrap()), f400);
    // Exactly on a frame → that frame.
    assert_eq!(id_at(store.nearest_frame(200).await.unwrap()), f200);
}

/// Range-scoped nearest-frame lookup must never return a frame outside the visible
/// Timeline window. Regression: the P5 command looked across the whole DB, so opening
/// a sparse Today/7d/30d window could jump to a capture outside the selected range.
#[tokio::test]
async fn nearest_frame_in_range_ignores_frames_outside_window() {
    let store = SqliteStore::open_in_memory().unwrap();
    let outside_before = store.insert_frame(frame_at(100)).await.unwrap();
    let inside = store.insert_frame(frame_at(500)).await.unwrap();
    let outside_after = store.insert_frame(frame_at(900)).await.unwrap();

    let nearest = store
        .nearest_frame_in_range(150, 400, 800)
        .await
        .unwrap()
        .expect("frame inside range");
    assert_eq!(nearest.frame_id, inside);

    let nearest_after = store
        .nearest_frame_in_range(850, 400, 800)
        .await
        .unwrap()
        .expect("frame inside range");
    assert_eq!(nearest_after.frame_id, inside);

    assert!(store
        .nearest_frame_in_range(150, 200, 400)
        .await
        .unwrap()
        .is_none());

    let global = store.nearest_frame(150).await.unwrap().unwrap();
    assert_eq!(
        global.frame_id, outside_before,
        "global nearest remains available for non-windowed callers"
    );
    assert_ne!(global.frame_id, outside_after);
}

/// `neighbour_frames` brackets the anchor with the *closest* captures on each side —
/// not the window edges — so a Moment's prev/next + context strip always point at the
/// adjacent frames. Backs `get_frame_context`. Regression: `frames_in_range`'s
/// newest-first cap would return only the latest frames in a wide forward window and
/// silently drop the anchor's true neighbours (and the anchor itself).
#[tokio::test]
async fn neighbour_frames_brackets_anchor_with_closest_each_side() {
    let store = SqliteStore::open_in_memory().unwrap();
    for t in [100, 200, 300, 400, 500, 600, 700] {
        store.insert_frame(frame_at(t)).await.unwrap();
    }

    // Anchor @400, ±1000 covers every frame, two per side. Closest two before (@300,
    // @200) + closest two after (@500, @600), ascending; anchor (@400) excluded and the
    // far edges (@100/@700) dropped by the 2-each cap — NOT the window's newest frames.
    let metas = store.neighbour_frames(400, 1_000, 2).await.unwrap();
    let times: Vec<i64> = metas.iter().map(|m| m.captured_at).collect();
    assert_eq!(times, vec![200, 300, 500, 600]);

    // prev/next as the Moment screen derives them: closest below / closest above.
    let prev = metas.iter().rfind(|m| m.captured_at < 400).unwrap();
    let next = metas.iter().find(|m| m.captured_at > 400).unwrap();
    assert_eq!(prev.captured_at, 300);
    assert_eq!(next.captured_at, 500);

    // Anchor at the earliest frame → only the after-side; no before frames, no panic.
    let only_after = store.neighbour_frames(100, 1_000, 3).await.unwrap();
    assert!(only_after.iter().all(|m| m.captured_at > 100));
    assert_eq!(only_after.first().map(|m| m.captured_at), Some(200));

    // The half-window clips reach: ±100 keeps only the immediately adjacent @300/@500.
    let near = store.neighbour_frames(400, 100, 5).await.unwrap();
    let near_times: Vec<i64> = near.iter().map(|m| m.captured_at).collect();
    assert_eq!(near_times, vec![300, 500]);

    // Degenerate window / zero limit → empty (never the whole table).
    assert!(store.neighbour_frames(400, 0, 5).await.unwrap().is_empty());
    assert!(store
        .neighbour_frames(400, 1_000, 0)
        .await
        .unwrap()
        .is_empty());
}

/// `set_settings_batch` upserts every pair in one transaction: all keys are present
/// afterward and existing keys are overwritten. (Atomicity-on-failure comes from the
/// single `BEGIN … COMMIT` — a failed write `?`-returns before `commit`, so nothing
/// lands; that path is exercised by `save_settings`' build-then-commit ordering.)
#[tokio::test]
async fn set_settings_batch_writes_all_and_overwrites() {
    let store = SqliteStore::open_in_memory().unwrap();
    // Pre-existing value the batch must overwrite.
    store.set_setting("a", "old").await.unwrap();

    store
        .set_settings_batch(&[
            ("a".to_string(), "new".to_string()),
            ("b".to_string(), "1".to_string()),
            ("c".to_string(), "2".to_string()),
        ])
        .await
        .unwrap();

    assert_eq!(
        store.get_setting("a").await.unwrap().as_deref(),
        Some("new")
    );
    assert_eq!(store.get_setting("b").await.unwrap().as_deref(), Some("1"));
    assert_eq!(store.get_setting("c").await.unwrap().as_deref(), Some("2"));

    // An empty batch is a no-op (opens and commits an empty transaction).
    store.set_settings_batch(&[]).await.unwrap();
}

/// Retention needs a bounded list of purge candidates before deleting files. It
/// should return only frames older than the cutoff, oldest first, capped at `limit`.
#[tokio::test]
async fn frames_older_than_lists_bounded_retention_candidates() {
    let store = SqliteStore::open_in_memory().unwrap();
    let f100 = store.insert_frame(frame_at(100)).await.unwrap();
    let f200 = store.insert_frame(frame_at(200)).await.unwrap();
    store.insert_frame(frame_at(300)).await.unwrap();

    let candidates = store.frames_older_than(250, 1).await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].frame_id, f100);
    assert_eq!(candidates[0].image_path, "frames/100.jpg");

    let candidates = store.frames_older_than(250, 10).await.unwrap();
    let ids: Vec<i64> = candidates.iter().map(|m| m.frame_id).collect();
    assert_eq!(ids, vec![f100, f200]);

    assert!(store.frames_older_than(250, 0).await.unwrap().is_empty());
}

/// Insights bucket density is presentation-driven just like Timeline density; callers
/// can request a coarser/finer chart without a fixed 48-bucket backend ceiling.
#[tokio::test]
async fn insights_summary_uses_requested_bucket_count() {
    let store = SqliteStore::open_in_memory().unwrap();
    for ts in [10, 60, 120, 180] {
        store.insert_frame(frame_at(ts)).await.unwrap();
    }

    let two = store.insights_summary(0, 200, 2).await.unwrap();
    assert_eq!(
        two.captures,
        vec![
            TimelineBucket {
                start: 0,
                end: 100,
                count: 2
            },
            TimelineBucket {
                start: 100,
                end: 200,
                count: 2
            },
        ]
    );

    let four = store.insights_summary(0, 200, 4).await.unwrap();
    assert_eq!(four.captures.len(), 4);
}

// --- PR3 attention filter (insert_ocr_filtered / stats / reconcile) ---------------

/// A frame whose foreground app is the editor (the target app the catalog scopes to).
fn editor_frame(captured_at: i64) -> NewFrame {
    NewFrame {
        captured_at,
        monitor_index: 0,
        width: 1920,
        height: 1080,
        image_path: format!("frames/{captured_at}.jpg"),
        content_hash: format!("h-{captured_at}"),
        app_hint: Some("editor".to_string()),
        window_title: Some("Doc".to_string()),
        browser_url: None,
        capture_trigger: None,
    }
}

/// One OCR line laid left-to-right at `y` on `line_index`.
fn ocr_line(text: &str, line_index: u32, y: f32) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut x = 0.05;
    for w in text.split_whitespace() {
        let width = 0.01 * w.chars().count() as f32;
        let mut s = span(w, x, y, width, 0.02);
        s.line_index = line_index;
        spans.push(s);
        x += width + 0.005;
    }
    spans
}

/// The synthetic editor frame: a fixed top toolbar (short, repeating) + a long body
/// line (interior, protected content). `ZTOOLBARZ` is a unique chrome token.
fn editor_ocr(body: &str) -> OcrResult {
    let mut spans = Vec::new();
    spans.extend(ocr_line("ZTOOLBARZ Home Share View", 0, 0.01));
    spans.extend(ocr_line(body, 1, 0.50));
    OcrResult {
        text: format!("ZTOOLBARZ Home Share View\n{body}"),
        mean_confidence: -1.0,
        engine: "test".to_string(),
        spans,
    }
}

/// Filter context with a low `min_seen` so the threshold flips inside a short test.
/// Full-screen target rect → the toolbar is in-window (not background) but not interior.
fn filter_ctx(min_seen: u32) -> TextFilterContext {
    TextFilterContext {
        target_rect: Some([0.0, 0.0, 1.0, 1.0]),
        chrome_suppress_min_seen: min_seen,
        chrome_protect_min_chars: 48,
        chrome_region_buckets: 8,
    }
}

fn search(text: &str, include_chrome: bool) -> SearchQuery {
    SearchQuery {
        text: text.to_string(),
        limit: 20,
        time_range: None,
        include_chrome,
    }
}

#[tokio::test]
async fn insert_ocr_filtered_suppresses_repeated_chrome_after_threshold() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let bodies = [
        "the quarterly revenue summary shows strong growth across every region",
        "meeting notes capture the decisions and the owners for next sprint here",
        "the migration checklist enumerates each table and its forward only change",
    ];
    let mut ids = Vec::new();
    for (i, body) in bodies.iter().enumerate() {
        let id = store
            .insert_frame(editor_frame(1000 + i as i64))
            .await
            .unwrap();
        store
            .insert_ocr_filtered(id, editor_ocr(body), filter_ctx(2))
            .await
            .unwrap();
        ids.push(id);
    }

    // First appearance is kept (below threshold); later appearances are suppressed.
    let f1 = store.get_frame(ids[0]).await.unwrap().unwrap();
    let f3 = store.get_frame(ids[2]).await.unwrap().unwrap();
    assert!(f1.content_text.as_deref().unwrap().contains("ZTOOLBARZ"));
    assert!(!f3.content_text.as_deref().unwrap().contains("ZTOOLBARZ"));
    // Raw text always preserves it (recoverable).
    assert!(f3.raw_text.as_deref().unwrap().contains("ZTOOLBARZ"));
    // The long body line is never suppressed for repeating.
    assert!(f3
        .content_text
        .as_deref()
        .unwrap()
        .contains("migration checklist"));
    assert!(f3.suppressed_text_count >= 1);

    // The toolbar word on the suppressed frame is classified chrome + non-searchable.
    let spans = store.frame_spans(ids[2]).await.unwrap();
    let toolbar = spans.iter().find(|s| s.text == "ZTOOLBARZ").unwrap();
    assert_eq!(toolbar.role, TextRole::Chrome);
    assert!(!toolbar.is_searchable);

    // Default search no longer surfaces the suppressed frame by the chrome term;
    // include_chrome still finds it via the raw arm (03 §3b acceptance).
    let default = store
        .hybrid_search(&search("ZTOOLBARZ", false))
        .await
        .unwrap();
    let def_ids: HashSet<i64> = default.iter().map(|h| h.frame_id).collect();
    assert!(
        def_ids.contains(&ids[0]),
        "first (kept) frame still matches content"
    );
    assert!(
        !def_ids.contains(&ids[2]),
        "suppressed frame must not match content"
    );
    let chrome = store
        .hybrid_search(&search("ZTOOLBARZ", true))
        .await
        .unwrap();
    let ch_ids: HashSet<i64> = chrome.iter().map(|h| h.frame_id).collect();
    assert!(
        ch_ids.contains(&ids[2]),
        "include_chrome recovers it via raw text"
    );

    // Per-app suppression metric reflects the dropped chrome spans.
    let stats = store.text_filter_stats(FILTER_VERSION).await.unwrap();
    let editor = stats
        .iter()
        .find(|s| s.app.as_deref() == Some("editor"))
        .expect("editor app present in stats");
    assert!(editor.suppressed_spans > 0);
    assert!(editor.total_spans >= editor.suppressed_spans);
    assert!(editor.rate > 0.0 && editor.rate <= 1.0);
}

#[tokio::test]
async fn backfill_filter_version_recleans_old_frames_against_warm_catalog() {
    let store = SqliteStore::open_in_memory().expect("open store");

    // An older frame stored before the chrome catalog warmed up: its content_text still
    // carries the toolbar (filter_version 0, the cold-start / passthrough case the PR3
    // audit flagged — docs/AUDIT_0.2.0_PR3_2026-06-26.md).
    let old_id = store.insert_frame(editor_frame(1000)).await.unwrap();
    store
        .insert_ocr(
            old_id,
            editor_ocr(
                "ALPHAUNIQUEBODY the annual planning doc lists each milestone owner and date",
            ),
        )
        .await
        .unwrap();
    let before = store.get_frame(old_id).await.unwrap().unwrap();
    assert!(before
        .content_text
        .as_deref()
        .unwrap()
        .contains("ZTOOLBARZ"));

    // Two later captures teach the catalog the toolbar signature (seen_count → 2).
    for (i, body) in [
        "meeting notes capture the decisions and the owners for the next sprint here",
        "the migration checklist enumerates each table and its forward only change set",
    ]
    .iter()
    .enumerate()
    {
        let id = store
            .insert_frame(editor_frame(2000 + i as i64))
            .await
            .unwrap();
        store
            .insert_ocr_filtered(id, editor_ocr(body), filter_ctx(2))
            .await
            .unwrap();
    }

    // Backfill re-cleans every sub-current frame against the now-warm catalog.
    let changed = store
        .backfill_filter_version(FILTER_VERSION, 2, 48, 8, true)
        .await
        .unwrap();
    assert_eq!(changed, 1, "the one stale frame is re-cleaned");

    // The toolbar is gone from content_text; the real body survives; raw still has it.
    let after = store.get_frame(old_id).await.unwrap().unwrap();
    assert!(!after.content_text.as_deref().unwrap().contains("ZTOOLBARZ"));
    assert!(after
        .content_text
        .as_deref()
        .unwrap()
        .contains("ALPHAUNIQUEBODY"));
    assert!(after.raw_text.as_deref().unwrap().contains("ZTOOLBARZ"));

    // The toolbar span is now classified chrome + non-searchable.
    let spans = store.frame_spans(old_id).await.unwrap();
    let toolbar = spans.iter().find(|s| s.text == "ZTOOLBARZ").unwrap();
    assert_eq!(toolbar.role, TextRole::Chrome);
    assert!(!toolbar.is_searchable);

    // A re-embed job was enqueued for the changed frame.
    let jobs = store
        .claim_jobs(&[JobKind::EmbedText], 10, 10_000_000)
        .await
        .unwrap();
    assert!(jobs.iter().any(|j| j.frame_id == Some(old_id)));

    // Default content search no longer surfaces the old frame by the chrome term;
    // include_chrome still recovers it via preserved raw text (03 §3b acceptance).
    let default: HashSet<i64> = store
        .hybrid_search(&search("ZTOOLBARZ", false))
        .await
        .unwrap()
        .iter()
        .map(|h| h.frame_id)
        .collect();
    assert!(!default.contains(&old_id));
    let raw: HashSet<i64> = store
        .hybrid_search(&search("ZTOOLBARZ", true))
        .await
        .unwrap()
        .iter()
        .map(|h| h.frame_id)
        .collect();
    assert!(raw.contains(&old_id));

    // Idempotent: the watermark is now current, so a second pass is a no-op.
    assert_eq!(
        store
            .backfill_filter_version(FILTER_VERSION, 2, 48, 8, true)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn backfill_filter_version_invalidates_stale_text_embedding() {
    let store = SqliteStore::open_in_memory().expect("open store");

    // An older frame whose content_text still carries the toolbar (filter_version 0),
    // plus a text embedding built from that stale, chrome-laden content.
    let old_id = store.insert_frame(editor_frame(1000)).await.unwrap();
    store
        .insert_ocr(
            old_id,
            editor_ocr(
                "ALPHAUNIQUEBODY the annual planning doc lists each milestone owner and date",
            ),
        )
        .await
        .unwrap();
    store
        .upsert_text_embedding(
            old_id,
            0,
            "ZTOOLBARZ ...",
            ChunkSource::Ocr,
            &one_hot(0),
            "gemma",
        )
        .await
        .unwrap();
    assert_eq!(store.text_embedding_count().await.unwrap(), 1);
    assert!(store
        .nearest_text_frames(&one_hot(0), 5)
        .await
        .unwrap()
        .contains(&old_id));

    // Two later captures warm the catalog so the toolbar signature is suppressible.
    for (i, body) in [
        "meeting notes capture the decisions and the owners for the next sprint here",
        "the migration checklist enumerates each table and its forward only change set",
    ]
    .iter()
    .enumerate()
    {
        let id = store
            .insert_frame(editor_frame(2000 + i as i64))
            .await
            .unwrap();
        store
            .insert_ocr_filtered(id, editor_ocr(body), filter_ctx(2))
            .await
            .unwrap();
    }

    // Backfill re-cleans the stale frame — and must drop its now-stale text embedding so
    // the dropped chrome can't keep surfacing the frame via hybrid_search's vector arm
    // before the re-embed runs (Codex PR3 review, P1).
    let changed = store
        .backfill_filter_version(FILTER_VERSION, 2, 48, 8, true)
        .await
        .unwrap();
    assert_eq!(changed, 1);

    // The stale embedding (and its vec0 shadow, via the embeddings_ad trigger) is gone;
    // nothing surfaces old_id by the old vector anymore.
    assert_eq!(store.text_embedding_count().await.unwrap(), 0);
    assert!(!store
        .nearest_text_frames(&one_hot(0), 5)
        .await
        .unwrap()
        .contains(&old_id));

    // A re-embed job was enqueued to rebuild it from the cleaned content_text.
    let jobs = store
        .claim_jobs(&[JobKind::EmbedText], 10, 10_000_000)
        .await
        .unwrap();
    assert!(jobs.iter().any(|j| j.frame_id == Some(old_id)));
}

#[tokio::test]
async fn frames_with_app_hint_matches_case_insensitively() {
    let store = SqliteStore::open_in_memory().expect("open store");
    let mk = |at: i64, app: &str| NewFrame {
        captured_at: at,
        monitor_index: 0,
        width: 100,
        height: 100,
        image_path: format!("frames/{at}.jpg"),
        content_hash: format!("h{at}"),
        app_hint: Some(app.to_string()),
        window_title: Some("w".to_string()),
        browser_url: None,
        capture_trigger: None,
    };
    let a = store.insert_frame(mk(1000, "ScreenSearch")).await.unwrap();
    let _b = store.insert_frame(mk(1001, "firefox")).await.unwrap();
    let c = store.insert_frame(mk(1002, "screensearch")).await.unwrap();

    // Backs the one-time self-capture purge: matches own frames regardless of case.
    let ids: HashSet<i64> = store
        .frames_with_app_hint("screensearch", 10)
        .await
        .unwrap()
        .iter()
        .map(|m| m.frame_id)
        .collect();
    assert_eq!(ids, HashSet::from([a, c]));

    // Guards: empty hint or zero limit yields nothing.
    assert!(store.frames_with_app_hint("", 10).await.unwrap().is_empty());
    assert!(store
        .frames_with_app_hint("screensearch", 0)
        .await
        .unwrap()
        .is_empty());
}

// ---- 0.3.0 PR7: streaming export cursor (`export_frames_page`, `03 §7c`) ----

/// The keyset cursor pages through every frame in ascending id order and stops when a
/// page returns fewer than `limit` rows (the terminal condition the export stream uses).
#[tokio::test]
async fn export_frames_page_pages_through_all_frames_in_id_order() {
    let store = SqliteStore::open_in_memory().unwrap();
    let mut ids = Vec::new();
    for at in 0..250 {
        ids.push(seed(&store, 1000 + at, &format!("content {at}"), None).await);
    }

    // Drain in pages of 100 with the keyset cursor advancing to each page's last id.
    let mut seen = Vec::new();
    let mut after = 0_i64;
    loop {
        let page = store
            .export_frames_page(after, None, None, 100)
            .await
            .unwrap();
        if page.is_empty() {
            break;
        }
        // Strictly ascending within and across pages.
        assert!(page.windows(2).all(|w| w[0].frame_id < w[1].frame_id));
        after = page.last().unwrap().frame_id;
        let short = page.len() < 100;
        seen.extend(page.into_iter().map(|r| r.frame_id));
        if short {
            break;
        }
    }
    assert_eq!(
        seen, ids,
        "cursor visits every frame exactly once, in id order"
    );
}

/// The optional `[from, to)` window is half-open and excludes the `to` boundary,
/// matching `TimeRange` semantics everywhere else in the store.
#[tokio::test]
async fn export_frames_page_honors_half_open_time_window() {
    let store = SqliteStore::open_in_memory().unwrap();
    for at in [100, 200, 300, 400, 500] {
        seed(&store, at, &format!("t{at}"), None).await;
    }

    let rows = store
        .export_frames_page(0, Some(200), Some(500), 1000)
        .await
        .unwrap();
    let times: Vec<i64> = rows.iter().map(|r| r.captured_at).collect();
    // 200 included (>= from), 500 excluded (< to).
    assert_eq!(times, vec![200, 300, 400]);
}

/// A frame with no `frame_text` row (never enriched) still exports — the LEFT JOIN
/// yields `content_text = None` rather than dropping the frame.
#[tokio::test]
async fn export_frames_page_left_join_yields_none_content_for_textless_frames() {
    let store = SqliteStore::open_in_memory().unwrap();
    let textless = store.insert_frame(frame_at(100)).await.unwrap();
    let with_text = seed(&store, 200, "has content", None).await;

    let rows = store.export_frames_page(0, None, None, 1000).await.unwrap();
    assert_eq!(rows.len(), 2, "textless frame is not dropped by the join");

    let a = rows.iter().find(|r| r.frame_id == textless).unwrap();
    assert_eq!(a.content_text, None);
    assert_eq!(a.app_hint.as_deref(), Some("Firefox"));

    let b = rows.iter().find(|r| r.frame_id == with_text).unwrap();
    assert_eq!(b.content_text.as_deref(), Some("has content"));
}

/// `limit == 0` is the empty guard (mirrors `purged_frame_ids`), so a caller can't
/// accidentally spin on an unbounded page.
#[tokio::test]
async fn export_frames_page_zero_limit_is_empty() {
    let store = SqliteStore::open_in_memory().unwrap();
    seed(&store, 100, "x", None).await;
    assert!(store
        .export_frames_page(0, None, None, 0)
        .await
        .unwrap()
        .is_empty());
}

// --- 0.4.0 PR6: session-scoped hybrid search (`hybrid_search_in_session`, D12) ---------------
// Retrieval restricted to a session's own frames so `POST /v1/ask?session_id` cites only
// in-session frames. Concurrent sessions can overlap in wall-clock time, so scoping is by
// exclusive `frames.session_id` ownership, never a time window.

/// A closed AI session over `[started_at, ended_at]` with the given context key.
fn session_row(started_at: i64, ended_at: i64, key: &str) -> NewSession {
    NewSession {
        started_at,
        ended_at: Some(ended_at),
        kind: SessionKind::Ai,
        tool: Some("claude-code".to_string()),
        host: Some(SessionHost::Terminal),
        context_key: key.to_string(),
        confidence: 0.9,
        frozen: false,
    }
}

/// FTS arm: two sessions overlapping in wall-clock time each own a frame with the shared term,
/// plus one unowned (session_id NULL) frame that also matches. A scoped search returns only the
/// named session's frame; the unscoped search still sees every match (D10 — scoping is additive).
#[tokio::test]
async fn scoped_fts_search_returns_only_the_named_sessions_frames() {
    let store = SqliteStore::open_in_memory().unwrap();
    let a_frame = seed(&store, 1_000, "shared invoice term", None).await;
    let b_frame = seed(&store, 2_000, "shared invoice term", None).await;
    let orphan = seed(&store, 3_000, "shared invoice term", None).await;

    let a = store
        .insert_session(session_row(500, 4_000, "ai:claude-code"))
        .await
        .unwrap();
    let b = store
        .insert_session(session_row(1_500, 4_500, "ai:codex"))
        .await
        .unwrap();
    store
        .assign_frames_session(&[a_frame], Some(a))
        .await
        .unwrap();
    store
        .assign_frames_session(&[b_frame], Some(b))
        .await
        .unwrap();
    // `orphan` stays unassigned (session_id NULL).

    let q = query("invoice", 10);
    let scoped: Vec<i64> = store
        .hybrid_search_in_session(&q, a)
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.frame_id)
        .collect();
    assert_eq!(
        scoped,
        vec![a_frame],
        "scoped search must return only session A's frame; got {scoped:?}"
    );

    let mut unscoped: Vec<i64> = store
        .hybrid_search(&q)
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.frame_id)
        .collect();
    unscoped.sort();
    let mut want = vec![a_frame, b_frame, orphan];
    want.sort();
    assert_eq!(
        unscoped, want,
        "unscoped search is unchanged and still sees every session's match"
    );
}

/// Vector arm: session A's only frame is buried past the `pool = 50` nearest vectors (all owned
/// by other sessions or unowned). A naive fixed-`k` KNN + session filter would fetch only the
/// nearer non-A frames and drop them all — the session-aware escalation target must still surface
/// A's match (the `07` #8 recall hole in session form).
#[tokio::test]
async fn scoped_vector_search_finds_in_session_match_buried_beyond_pool() {
    let store = SqliteStore::open_in_memory().unwrap();
    let b = store
        .insert_session(session_row(0, 100_000, "ai:codex"))
        .await
        .unwrap();
    // 55 vector-only frames nearer than A's, none owned by A (half to B, half unowned).
    for i in 1..=55 {
        let id = seed(
            &store,
            1_000 + i,
            "filler noise unrelated",
            Some(&vec_at_angle(0.001 * i as f32)),
        )
        .await;
        if i % 2 == 0 {
            store.assign_frames_session(&[id], Some(b)).await.unwrap();
        }
    }
    let a = store
        .insert_session(session_row(0, 100_000, "ai:claude-code"))
        .await
        .unwrap();
    // A's only frame: farther than all 55 (rank 56, beyond the k = 50 first pass), vector-only.
    let a_frame = seed(
        &store,
        60_000,
        "filler noise unrelated",
        Some(&vec_at_angle(0.060)),
    )
    .await;
    store
        .assign_frames_session(&[a_frame], Some(a))
        .await
        .unwrap();

    let mut by_text = HashMap::new();
    by_text.insert("target".to_string(), vec_at_angle(0.0));
    let store = store.with_embedder(Arc::new(FakeEmbedder { by_text }));

    let q = SearchQuery {
        text: "target".to_string(),
        limit: 1, // pool = max(1*5, 50) = 50; A's frame sits at rank 56
        time_range: None,
        include_chrome: false,
    };
    let scoped: Vec<i64> = store
        .hybrid_search_in_session(&q, a)
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.frame_id)
        .collect();
    assert_eq!(
        scoped, vec![a_frame],
        "the in-session vector match must be found even when buried beyond the pool; got {scoped:?}"
    );
}

/// A session that owns no matching frame grounds a scoped ask on nothing — honest empty, never a
/// silent fall-through to the global result set.
#[tokio::test]
async fn scoped_search_with_no_in_session_matches_returns_empty() {
    let store = SqliteStore::open_in_memory().unwrap();
    let _orphan = seed(&store, 1_000, "shared invoice term", None).await;
    let a = store
        .insert_session(session_row(0, 10_000, "ai:claude-code"))
        .await
        .unwrap();
    let scoped = store
        .hybrid_search_in_session(&query("invoice", 10), a)
        .await
        .unwrap();
    assert!(
        scoped.is_empty(),
        "a session that owns no matching frame must ground a scoped ask on nothing"
    );
}
