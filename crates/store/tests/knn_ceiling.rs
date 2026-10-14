//! Regression coverage for sqlite-vec's hard KNN `k` ceiling (usage review 2026-08-01 §7.4).

use std::sync::Arc;

use rusqlite::{params, Connection};
use store::{SqliteStore, EMBEDDING_DIM};
use traits::{Embedding, EmbeddingProvider, SearchQuery, TimeRange};

const TOTAL_FRAMES: usize = 4_200;
const SECOND_DAY_START: usize = TOTAL_FRAMES - 20;
const DAY_MS: i64 = 86_400_000;
const DAY_ONE: i64 = 1_700_000_000_000;
const DAY_TWO: i64 = DAY_ONE + DAY_MS;
const SESSION_ID: i64 = 1;

/// Deterministic query embedding: no model download or inference belongs in this regression.
struct FakeEmbedder(Embedding);

#[async_trait::async_trait]
impl EmbeddingProvider for FakeEmbedder {
    fn dim(&self) -> usize {
        EMBEDDING_DIM
    }

    async fn embed_texts(&self, inputs: &[String]) -> traits::Result<Vec<Embedding>> {
        Ok(vec![self.0.clone(); inputs.len()])
    }
}

fn vector_blob(first: f32, second: f32) -> Vec<u8> {
    // The schema fixes vec0 at 768 dimensions, so the test uses the production dimension.
    let mut vector = vec![0.0_f32; EMBEDDING_DIM];
    vector[0] = first;
    vector[1] = second;
    let mut blob = Vec::with_capacity(EMBEDDING_DIM * std::mem::size_of::<f32>());
    for value in vector {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
}

#[tokio::test]
async fn bounded_and_session_knn_never_exceed_sqlite_vec_k_limit() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db_path = temp.path().join("knn-ceiling.sqlite");
    let store = SqliteStore::open_path(&db_path).expect("open store");

    // Seed in one transaction through a second connection: 4,200 × 768-dimensional vectors
    // reproduce the scale-dependent failure without loading the real EmbeddingGemma model.
    let mut conn = Connection::open(&db_path).expect("open seed connection");
    conn.execute_batch("PRAGMA foreign_keys=ON;")
        .expect("enable foreign keys");
    let tx = conn.transaction().expect("begin seed transaction");
    tx.execute(
        "INSERT INTO sessions
           (id, started_at, ended_at, kind, tool, host, context_key, confidence, frozen)
         VALUES (?1, ?2, ?3, 'ai', 'codex', 'desktop', 'ai:codex', 1.0, 0)",
        params![SESSION_ID, DAY_TWO, DAY_TWO + DAY_MS],
    )
    .expect("insert session");

    let exact = vector_blob(1.0, 0.0);
    let outside = vector_blob(
        std::f32::consts::FRAC_1_SQRT_2,
        std::f32::consts::FRAC_1_SQRT_2,
    );
    let buried = vector_blob(0.0, 1.0);
    {
        let mut insert_frame = tx
            .prepare(
                "INSERT INTO frames
                   (id, captured_at, monitor_index, width, height, image_path, content_hash,
                    session_id)
                 VALUES (?1, ?2, 0, 1, 1, ?3, ?4, ?5)",
            )
            .expect("prepare frame insert");
        let mut insert_embedding = tx
            .prepare(
                "INSERT INTO embeddings
                   (id, frame_id, chunk_index, chunk_text, source, model, dim, content_hash)
                 VALUES (?1, ?1, 0, 'synthetic', 'ocr', 'synthetic', ?2, 'synthetic')",
            )
            .expect("prepare embedding insert");
        let mut insert_vector = tx
            .prepare("INSERT INTO embedding_vectors (embedding_id, embedding) VALUES (?1, ?2)")
            .expect("prepare vector insert");

        for index in 0..TOTAL_FRAMES {
            let id = index as i64 + 1;
            let on_second_day = index >= SECOND_DAY_START;
            let captured_at = if on_second_day {
                DAY_TWO + (index - SECOND_DAY_START) as i64
            } else {
                DAY_ONE + index as i64
            };
            // Five exact in-window matches surface immediately. The other fifteen are ranked
            // behind >4096 out-of-window vectors, forcing 100 → 800 → ceiling escalation.
            let blob = if (SECOND_DAY_START..SECOND_DAY_START + 5).contains(&index) {
                &exact
            } else if on_second_day {
                &buried
            } else {
                &outside
            };
            // The session owns one surfaced match and four buried matches, so its target cannot
            // be filled before the same ceiling is reached.
            let session_id =
                (index == SECOND_DAY_START || index >= TOTAL_FRAMES - 4).then_some(SESSION_ID);

            insert_frame
                .execute(params![
                    id,
                    captured_at,
                    format!("frame-{id}.jpg"),
                    format!("frame-{id}"),
                    session_id
                ])
                .expect("insert frame");
            insert_embedding
                .execute(params![id, EMBEDDING_DIM as i64])
                .expect("insert embedding metadata");
            insert_vector
                .execute(params![id, blob.as_slice()])
                .expect("insert vector");
        }
    }
    tx.commit().expect("commit seed transaction");
    drop(conn);

    let query_embedding = Embedding({
        let mut vector = vec![0.0; EMBEDDING_DIM];
        vector[0] = 1.0;
        vector
    });
    store.set_embedder(Arc::new(FakeEmbedder(query_embedding.clone())));

    let ranged_query = SearchQuery {
        text: "ceiling probe".to_string(),
        limit: 20,
        time_range: Some(TimeRange {
            start: DAY_TWO,
            end: DAY_TWO + DAY_MS,
        }),
        include_chrome: false,
    };
    // Guards sqlite-vec's former `k value in knn query too large` error at k=6400.
    let ranged = store
        .hybrid_search(&ranged_query)
        .await
        .expect("time-ranged search must degrade to fewer candidates, not error");
    assert!(
        !ranged.is_empty(),
        "the surfaced in-window candidates remain usable"
    );

    let session = store
        .hybrid_search_in_session(
            &SearchQuery {
                time_range: None,
                ..ranged_query.clone()
            },
            SESSION_ID,
        )
        .await
        .expect("session-scoped search must degrade to fewer candidates, not error");
    assert!(
        !session.is_empty(),
        "the surfaced session candidate remains usable"
    );

    // The public direct-KNN building block clamps caller-controlled k at its SQL boundary too.
    let direct = store
        .nearest_text_frames(&query_embedding, u32::MAX)
        .await
        .expect("direct KNN must clamp k before sqlite-vec");
    assert_eq!(direct.len(), 4_096);
}
