//! Hybrid-search latency fixture (`03 §13.4`, `07` gap #7).
//!
//! P1/P2 only tested tiny `:memory:` fixtures, leaving the DoD's "< ~200 ms on a
//! realistic DB" bar unverified. This seeds 10 000 frames — each with realistic OCR
//! text and a 768-dim text embedding — then measures `hybrid_search` (FTS5 arm +
//! sqlite-vec KNN arm → RRF) across a spread of queries and asserts the p95 stays
//! under 200 ms.
//!
//! Deterministic: an inline LCG generates the vectors and OCR text from fixed seeds
//! (no `rand` dep, no clock dependence), so the DB is identical every run.
//!
//! `#[ignore]`d — seeding 10k rows takes a few seconds and the latency bar is
//! hardware-dependent (CI runners vary). Run locally:
//! `cargo test -p store --test perf -- --ignored --nocapture`.
//! If it regresses, the tuning levers are RRF `k` / `candidate_pool` (`search.rs`)
//! and gap #8 (the vec arm's post-KNN time filter).

use async_trait::async_trait;
use store::{SqliteStore, EMBEDDING_DIM};
use traits::{
    ChunkSource, Embedding, EmbeddingProvider, NewFrame, OcrResult, Result, SearchQuery, TimeRange,
};

const FRAMES: usize = 10_000;

/// A 64-bit LCG step (Knuth's MMIX constants) — deterministic pseudo-randomness.
fn lcg(state: u64) -> u64 {
    state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407)
}

/// FNV-1a hash of a string → a seed (so a query maps to a stable vector).
fn hash_str(s: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// A deterministic 768-dim vector in [-1, 1) seeded by `seed`.
fn pseudo_vec(seed: u64) -> Vec<f32> {
    let mut state = seed.wrapping_add(0x9E3779B97F4A7C15);
    let mut v = Vec::with_capacity(EMBEDDING_DIM);
    for _ in 0..EMBEDDING_DIM {
        state = lcg(state);
        let unit = ((state >> 40) as f32) / ((1u64 << 24) as f32); // [0, 1)
        v.push(unit.mul_add(2.0, -1.0)); // [-1, 1)
    }
    v
}

/// Small vocabulary so FTS has a realistic term distribution (not all-identical rows).
const WORDS: &[&str] = &[
    "invoice",
    "kubernetes",
    "dashboard",
    "email",
    "meeting",
    "report",
    "budget",
    "login",
    "error",
    "search",
    "customer",
    "database",
    "pipeline",
    "deploy",
    "review",
    "ticket",
    "latency",
    "vector",
    "embedding",
    "timeline",
    "capture",
    "monitor",
    "privacy",
    "settings",
    "model",
    "query",
    "index",
    "result",
    "frame",
    "screen",
];

/// ~12 words drawn deterministically from `WORDS` for frame `i`.
fn ocr_text(i: usize) -> String {
    let mut state = (i as u64).wrapping_add(1);
    let mut words = Vec::with_capacity(12);
    for _ in 0..12 {
        state = lcg(state);
        words.push(WORDS[(state >> 33) as usize % WORDS.len()]);
    }
    words.join(" ")
}

/// Generates a stable vector per query string — so the vector arm runs a real KNN
/// over the seeded index without needing the actual model.
struct LcgEmbedder;

#[async_trait]
impl EmbeddingProvider for LcgEmbedder {
    fn dim(&self) -> usize {
        EMBEDDING_DIM
    }
    async fn embed_texts(&self, inputs: &[String]) -> Result<Vec<Embedding>> {
        Ok(inputs
            .iter()
            .map(|s| Embedding(pseudo_vec(hash_str(s))))
            .collect())
    }
}

#[tokio::test]
#[ignore = "seeds 10k frames + 768-dim vectors; run locally: cargo test -p store --test perf -- --ignored --nocapture"]
async fn hybrid_search_under_200ms_on_realistic_db() {
    let store = SqliteStore::open_in_memory()
        .unwrap()
        .with_embedder(std::sync::Arc::new(LcgEmbedder));

    let base = 1_700_000_000_000_i64; // a fixed epoch-ms anchor
    for i in 0..FRAMES {
        let captured_at = base + (i as i64) * 1_000;
        let fid = store
            .insert_frame(NewFrame {
                captured_at,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: format!("frames/{i}.jpg"),
                content_hash: format!("hash-{i}"),
                app_hint: None,
                window_title: None,
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .unwrap();
        let text = ocr_text(i);
        store
            .insert_ocr(
                fid,
                OcrResult {
                    text: text.clone(),
                    mean_confidence: -1.0,
                    engine: "perf".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .unwrap();
        store
            .upsert_text_embedding(
                fid,
                0,
                &text,
                ChunkSource::Ocr,
                &Embedding(pseudo_vec(fid as u64)),
                "perf",
            )
            .await
            .unwrap();
    }

    // A spread of queries: common terms, term pairs, and time-windowed variants.
    let mid = TimeRange {
        start: base + 2_500 * 1_000,
        end: base + 7_500 * 1_000,
    };
    let queries: Vec<SearchQuery> = [
        "invoice",
        "kubernetes deploy",
        "email meeting",
        "budget report",
        "database query",
        "latency vector",
        "login error",
        "customer ticket",
        "dashboard timeline",
        "deploy pipeline",
        "review settings",
        "monitor privacy",
        "model embedding",
        "search index",
        "frame capture",
        "screen result",
        "report budget meeting",
        "vector index query",
        "error login ticket",
        "kubernetes pipeline deploy",
    ]
    .iter()
    .enumerate()
    .map(|(n, t)| SearchQuery {
        text: (*t).to_string(),
        limit: 20,
        time_range: n.is_multiple_of(3).then_some(mid),
        include_chrome: false,
    })
    .collect();

    let mut timings = Vec::with_capacity(queries.len());
    for q in &queries {
        let t0 = std::time::Instant::now();
        let hits = store.hybrid_search(q).await.unwrap();
        timings.push(t0.elapsed());
        assert!(
            !hits.is_empty(),
            "query {:?} returned nothing — the vector arm should always surface candidates",
            q.text
        );
        // Parity digest: FNV-1a over the ranked `frame_id`s. The fixture is
        // LCG-deterministic, so this digest is stable run-to-run and pins the exact
        // hybrid-search ranking. 0.3.0 PR4 (image-lane removal) uses it to *show* that
        // removing the flag-off image lane leaves results unchanged — the image arm was
        // never fused into `hybrid_search`, so every digest here must match verbatim
        // before and after the removal (`docs/0.3.0.md` PR4, `03 §13b.3`).
        let mut digest = 0xcbf29ce484222325u64;
        for hit in &hits {
            for byte in hit.frame_id.to_le_bytes() {
                digest ^= u64::from(byte);
                digest = digest.wrapping_mul(0x100000001b3);
            }
        }
        println!("parity-digest {:?}: {digest:016x}", q.text);
    }

    timings.sort();
    let p95 = timings[(timings.len() * 95 / 100).min(timings.len() - 1)];
    let median = timings[timings.len() / 2];
    println!(
        "hybrid_search over {FRAMES} frames: median = {median:?}, p95 = {p95:?} ({} queries)",
        queries.len()
    );
    assert!(
        p95 < std::time::Duration::from_millis(200),
        "p95 {p95:?} exceeds the 200 ms bar (03 §13.4)"
    );
}
