//! Hybrid retrieval: an FTS5 (BM25) arm and a sqlite-vec (cosine KNN) arm fused
//! with Reciprocal Rank Fusion (`02 §4`, `03 §4/§13`).
//!
//! The vector arm is only active when an `Arc<dyn EmbeddingProvider>` was injected
//! (`SqliteStore::with_embedder`); otherwise search degrades to FTS-only. The query
//! text is embedded once (async) up front, then both arms run as DB queries.

use std::collections::HashMap;

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter};
use traits::{Result, SearchHit, SearchQuery};

use crate::embeddings::{dedup_keep_order, f32_blob, SQLITE_VEC_MAX_K};
use crate::SqliteStore;

/// RRF damping constant (the conventional value). A larger `k` flattens the
/// contribution of top ranks; 60 is the de-facto standard.
const RRF_K: f64 = 60.0;
/// Backend ceiling for one search response. The Recall **search** UI sends its own
/// smaller limit (`SEARCH_LIMIT = 100`); this higher bound exists so report retrieval
/// (Custom-with-prompt, `kernel::reports`) can honor `reports.weekly_top_k` (up to 2000)
/// instead of being silently clamped to the UI's max and dropping relevant evidence.
const MAX_SEARCH_LIMIT: usize = 2_000;
/// Per-arm candidate over-fetch ceiling — kept **independent** of `MAX_SEARCH_LIMIT` so a
/// large report limit can't explode each arm's scan (UI search limits keep the pool small
/// either way: `candidate_pool` only over-fetches `5×` the requested limit).
const MAX_CANDIDATE_POOL: usize = 2_000;
/// Vector-arm KNN escalation for time-windowed search (`07` #8). sqlite-vec can't filter
/// inside `MATCH`, so a fixed `k = pool` KNN + time post-filter drops in-range matches that
/// happen to rank beyond the top-`pool` nearest vectors. When a time range is set and the
/// post-filter under-fills the pool, we re-run the KNN with a geometrically larger `k` until
/// the pool fills, the vector table is exhausted, or `k` hits [`MAX_TIME_RANGE_KNN`].
const KNN_ESCALATION_FACTOR: u32 = 8;
/// Hard ceiling on the escalated KNN `k` (see [`KNN_ESCALATION_FACTOR`]). sqlite-vec
/// rejects `k` above [`SQLITE_VEC_MAX_K`], so this ceiling is pinned to that external
/// constraint. Past it recall can still under-count, but bounded/session-scoped retrieval
/// degrades to fewer candidates rather than an SQL error (usage review 2026-08-01 §7.4).
const MAX_TIME_RANGE_KNN: u32 = SQLITE_VEC_MAX_K;
/// Upper bound on frames the sparse-window pre-count ([`count_embedded_frames_in_range`]) will
/// examine. The count exists only to detect a *sparse* window (fewer embedded frames than `pool`)
/// so the KNN escalation doesn't climb to [`MAX_TIME_RANGE_KNN`] needlessly. But with sparse
/// embeddings the inner `LIMIT` can't short-circuit — it only stops after `cap` *matches*, so a
/// window with many captured frames but few embedded ones (embed backlog, or a wide multi-day
/// range) would make the count walk the whole frame range: O(frames-in-window), not O(cap)
/// (`07` #8 review, P2). Capping the frames scanned keeps it O(cap): past this budget we can't
/// prove the window is sparse, so we assume it is dense (target = `pool`) — which can only
/// *raise* the escalation target, never drop an in-range match. This remains deliberately linked
/// to the KNN ceiling: the pre-count never costs more than one ceiling pass, and lowering the
/// scan budget to sqlite-vec's limit only makes sparse detection give up sooner and choose that
/// safe, higher target (usage review 2026-08-01 §7.4).
const COUNT_SCAN_CAP: usize = MAX_TIME_RANGE_KNN as usize;

fn normalized_limit(limit: u32) -> usize {
    (limit as usize).clamp(1, MAX_SEARCH_LIMIT)
}

/// Per-arm candidate pool. We over-fetch beyond `limit` so fusion (and the vector
/// arm's time-range post-filter) have material to work with.
fn candidate_pool(limit: usize) -> u32 {
    limit.saturating_mul(5).clamp(50, MAX_CANDIDATE_POOL) as u32
}

/// Runs the escalating time-window KNN. `fetch(k)` returns `(raw_row_count, in_range_frame_ids)`
/// for the top-`k` KNN in distance order; this widens `k` geometrically until it has gathered
/// `target` distinct in-range frames, the KNN exhausts the table (`raw < k`), or `k` reaches
/// [`MAX_TIME_RANGE_KNN`], then returns the deduped in-range ids truncated to `pool`.
fn escalate_in_range_knn(
    pool: u32,
    target: usize,
    mut fetch: impl FnMut(u32) -> rusqlite::Result<(usize, Vec<i64>)>,
) -> rusqlite::Result<Vec<i64>> {
    // A caller-provided pool can exceed the engine limit; clamp the very first pass as well as
    // every geometric step (usage review 2026-08-01 §7.4).
    let mut k = pool.min(MAX_TIME_RANGE_KNN);
    loop {
        let (raw, in_range) = fetch(k)?;
        let mut ids = dedup_keep_order(in_range);
        // Stop when the window's frames are all gathered (`target`, capped at the count of
        // distinct embedded frames in the window), the KNN drained the table (`raw < k`), or
        // `k` hit the ceiling. The `target` cap is what keeps a sparse window off the ceiling.
        if ids.len() >= target || raw < k as usize || k >= MAX_TIME_RANGE_KNN {
            ids.truncate(pool as usize);
            return Ok(ids);
        }
        k = k
            .saturating_mul(KNN_ESCALATION_FACTOR)
            .min(MAX_TIME_RANGE_KNN);
    }
}

/// The KNN escalation target for a bounded `[start, end)` window, or `None` when the window
/// provably holds **no** embedded frames (the caller then skips the KNN entirely).
///
/// `Some(target)` is `min(pool, distinct_embedded_frames_in_window)` — the most in-range frames
/// the KNN post-filter could ever return — so a *sparse* window (fewer embedded frames than
/// `pool`) stops escalating as soon as it has them all instead of climbing to
/// [`MAX_TIME_RANGE_KNN`] (`07` #8 review).
///
/// The scan is bounded to `scan_cap` frames. A `LIMIT` on *matches* can't short-circuit when the
/// window is sparsely embedded (it only stops after `scan_cap` matches), so capping the frames
/// *examined* is what keeps this O(cap) rather than O(frames-in-window) on a wide, backlog-sparse
/// window (`07` #8 review, P2). If the scan hits its budget we can't prove the window is sparse,
/// so we assume it is **dense** and return `Some(pool)` — a safe over-estimate that only *raises*
/// the escalation target, never dropping an in-range match. Indexed via `idx_frames_captured_at`
/// (range scan) + `idx_embeddings_frame` (the `EXISTS` semi-join).
fn count_embedded_frames_in_range(
    conn: &rusqlite::Connection,
    start: i64,
    end: i64,
    session_id: Option<i64>,
    pool: usize,
    scan_cap: usize,
) -> rusqlite::Result<Option<usize>> {
    // Scoped counting (0.4.0 PR6) restricts to the session's own frames — indexed by
    // `idx_frames_session`, so the target becomes the exact `min(pool, embedded-in-session)`.
    let session_clause = if session_id.is_some() {
        " AND fr.session_id = ?4"
    } else {
        ""
    };
    let sql = format!(
        "SELECT COUNT(*), COALESCE(SUM(has_emb), 0) FROM (
             SELECT EXISTS (SELECT 1 FROM embeddings m WHERE m.frame_id = fr.id) AS has_emb
             FROM frames fr
             WHERE fr.captured_at >= ?1 AND fr.captured_at < ?2{session_clause}
             LIMIT ?3
         )"
    );
    let mut values: Vec<Value> = vec![
        Value::Integer(start),
        Value::Integer(end),
        Value::Integer(scan_cap as i64),
    ];
    if let Some(sid) = session_id {
        values.push(Value::Integer(sid));
    }
    // `scanned` = frames examined (≤ scan_cap); `embedded` = how many of those hold an embedding.
    let (scanned, embedded): (i64, i64) = conn.query_row(&sql, params_from_iter(values), |r| {
        Ok((r.get(0)?, r.get(1)?))
    })?;
    let (scanned, embedded) = (scanned as usize, embedded as usize);
    if scanned >= scan_cap {
        // Scan budget exhausted: `embedded` is only a lower bound on the window's true count, so
        // we can't conclude it's sparse. Assume dense — target `pool`. Safe: it can only widen the
        // escalation, never stop it early on an in-range match still out there.
        return Ok(Some(pool));
    }
    // Whole window scanned: `embedded` is exact. No embeddings ⇒ skip the KNN.
    if embedded == 0 {
        return Ok(None);
    }
    Ok(Some(embedded.min(pool)))
}

/// Builds a safe FTS5 MATCH expression from free user text: each whitespace term
/// is quoted (so FTS operators in the input can't inject), terms AND together.
/// Returns `None` for blank input.
fn fts_match_query(text: &str) -> Option<String> {
    let terms: Vec<String> = text
        .split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// First ~160 chars of `text` on a char boundary, with an ellipsis if truncated.
/// Fallback snippet for vector-only hits (which have no FTS highlight).
fn truncate_snippet(text: &str) -> String {
    const MAX: usize = 160;
    if text.chars().count() <= MAX {
        return text.to_string();
    }
    let mut s: String = text.chars().take(MAX).collect();
    s.push('…');
    s
}

/// Reciprocal Rank Fusion over per-arm ranked id lists. Returns ids with fused
/// scores, highest first; ties break toward the newer (larger) id.
fn rrf_fuse(arms: &[Vec<i64>], limit: usize) -> Vec<(i64, f64)> {
    let mut scores: HashMap<i64, f64> = HashMap::new();
    for arm in arms {
        for (rank, &id) in arm.iter().enumerate() {
            *scores.entry(id).or_insert(0.0) += 1.0 / (RRF_K + (rank + 1) as f64);
        }
    }
    let mut fused: Vec<(i64, f64)> = scores.into_iter().collect();
    fused.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.0.cmp(&a.0))
    });
    fused.truncate(limit);
    fused
}

impl SqliteStore {
    /// Hybrid search over OCR text + (optional) text embeddings, fused via RRF
    /// (`03 §3/§13`).
    pub async fn hybrid_search(&self, q: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.hybrid_search_scoped(q, None).await
    }

    /// Session-scoped hybrid search (0.4.0 PR6, D12): identical fusion to
    /// [`Self::hybrid_search`] but every arm additionally restricts to frames owned by
    /// `session_id`, so a scoped answer cites only in-session frames. Concurrent sessions
    /// can overlap in wall-clock time, so scoping is by exclusive `frames.session_id`
    /// ownership — a time window alone would let an overlapping session's frames leak in.
    pub async fn hybrid_search_in_session(
        &self,
        q: &SearchQuery,
        session_id: i64,
    ) -> Result<Vec<SearchHit>> {
        self.hybrid_search_scoped(q, Some(session_id)).await
    }

    /// The shared retrieval core. `session_id = None` is the frame-level path (byte-identical
    /// to pre-0.4.0 behavior — the existing search tests are its regression guard); `Some(id)`
    /// threads a `frames.session_id` predicate through both arms.
    async fn hybrid_search_scoped(
        &self,
        q: &SearchQuery,
        session_id: Option<i64>,
    ) -> Result<Vec<SearchHit>> {
        let limit = normalized_limit(q.limit);
        let pool = candidate_pool(limit);
        // Half-open `[start, end)` per the `TimeRange` contract: both arms filter
        // `captured_at >= start AND captured_at < end`. No range → the full line.
        let (start, end) = q
            .time_range
            .map(|t| (t.start, t.end))
            .unwrap_or((i64::MIN, i64::MAX));

        // --- content FTS arm (default retrieval text; carries the highlighted snippets) ---
        let fts = self
            .fts_arm("frame_text_fts", &q.text, start, end, pool, session_id)
            .await?;
        let fts_ids: Vec<i64> = fts.iter().map(|(id, _)| *id).collect();
        let mut snippets: HashMap<i64, String> = fts.into_iter().collect();

        // --- raw FTS arm (only when the caller opts into chrome/raw text, `03 §3b`) ---
        // Searches `frame_text.raw_text` so static chrome the content filter drops is
        // still reachable. The content snippet wins; raw snippets fill only ids the
        // content arm didn't match.
        let raw_ids: Vec<i64> = if q.include_chrome {
            let raw = self
                .fts_arm("frame_text_raw_fts", &q.text, start, end, pool, session_id)
                .await?;
            let ids: Vec<i64> = raw.iter().map(|(id, _)| *id).collect();
            for (id, snip) in raw {
                snippets.entry(id).or_insert(snip);
            }
            ids
        } else {
            Vec::new()
        };

        // --- vector arm (only when an embedder is present and the query is non-blank) ---
        // Clone the Arc out from under the lock first — the read guard must never be
        // held across the `.await` on `embed_texts`.
        let embedder = self
            .embedder
            .read()
            .expect("store embedder lock poisoned")
            .clone();
        let vec_ids = match (embedder, q.text.trim().is_empty()) {
            (Some(embedder), false) => {
                let mut embs = embedder.embed_texts(std::slice::from_ref(&q.text)).await?;
                let query_emb = embs
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("embedder returned no vector for the query"))?;
                self.text_knn_in_range(query_emb.0, pool, start, end, session_id)
                    .await?
            }
            _ => Vec::new(),
        };

        // Fuse the active arms (the raw arm only participates when opted in).
        let mut arms: Vec<Vec<i64>> = vec![fts_ids];
        if q.include_chrome {
            arms.push(raw_ids);
        }
        arms.push(vec_ids);
        let fused = rrf_fuse(&arms, limit);
        self.hydrate(fused, snippets).await
    }

    /// BM25-ranked FTS hits within the time window, with highlighted snippets, over
    /// the given external-content FTS5 table (`frame_text_fts` for content text,
    /// `frame_text_raw_fts` for raw text). `table` is a fixed internal identifier, not
    /// user input, so interpolating it is injection-safe.
    async fn fts_arm(
        &self,
        table: &str,
        text: &str,
        start: i64,
        end: i64,
        pool: u32,
        session_id: Option<i64>,
    ) -> Result<Vec<(i64, String)>> {
        let Some(match_q) = fts_match_query(text) else {
            return Ok(Vec::new());
        };
        // Scoped path (0.4.0 PR6) appends the session predicate as `?5`; the unscoped path
        // leaves the SQL byte-identical to the pre-0.4.0 query.
        let session_clause = if session_id.is_some() {
            " AND fr.session_id = ?5"
        } else {
            ""
        };
        let sql = format!(
            "SELECT fts.rowid,
                    snippet({table}, 0, '[', ']', '…', 12) AS snip,
                    bm25({table}) AS rank
             FROM {table} fts
             JOIN frames fr ON fr.id = fts.rowid
             WHERE {table} MATCH ?1 AND fr.captured_at >= ?2 AND fr.captured_at < ?3{session_clause}
             ORDER BY rank
             LIMIT ?4"
        );
        self.with_conn(move |conn| {
            let mut values: Vec<Value> = vec![
                Value::Text(match_q),
                Value::Integer(start),
                Value::Integer(end),
                Value::Integer(pool as i64),
            ];
            if let Some(sid) = session_id {
                values.push(Value::Integer(sid));
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(values), |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// Text-embedding cosine KNN, nearest-first, de-duped by frame, restricted to the
    /// time window. vec0 can't filter inside `MATCH`, so we fetch the `k` nearest vectors
    /// and post-filter the time window on the join. To avoid missing in-range matches that
    /// rank beyond the top-`pool` nearest (`07` #8), a bounded time range **escalates** `k`
    /// geometrically until it has gathered its `target` in-range frames, the vector table is
    /// exhausted (the KNN returned `< k` rows), or `k` reaches [`MAX_TIME_RANGE_KNN`]. The
    /// `target` is capped at the count of distinct embedded frames actually in the window
    /// (`07` #8 review): a **sparse** window — one with fewer embedded frames than `pool` —
    /// then stops as soon as it has them all, instead of climbing to the ceiling on every
    /// query when the DB holds more than [`MAX_TIME_RANGE_KNN`] vectors (neither the pool-fill
    /// nor the exhaustion gate can fire in that case). An unbounded range keeps the single
    /// `k = pool` pass (the time filter is then a no-op).
    ///
    /// `session_id = Some(id)` (0.4.0 PR6) adds an exclusive `frames.session_id` post-filter
    /// alongside the window check and forces the escalation path (a session's frames are a tiny
    /// fraction of the DB and can rank far past the top-`pool` nearest — the same recall hole as
    /// `07` #8, in session form). The escalation target is then `min(pool, embedded-in-session)`.
    async fn text_knn_in_range(
        &self,
        query: Vec<f32>,
        pool: u32,
        start: i64,
        end: i64,
        session_id: Option<i64>,
    ) -> Result<Vec<i64>> {
        let blob = f32_blob(&query);
        // A full `[i64::MIN, i64::MAX)` line means "no time filter"; a session scope also needs
        // the escalating path (its frames can rank arbitrarily far in global distance order).
        let bounded = start != i64::MIN || end != i64::MAX || session_id.is_some();
        self.with_conn(move |conn| {
            // Escalation target: for a bounded window, never chase more frames than the window
            // (and session, if scoped) holds — an indexed count of distinct embedded frames,
            // capped at `pool` (all we ever need is `min(pool, n)`). An empty window skips the
            // KNN entirely; an unbounded, unscoped query uses `pool` (single pass).
            let target = if bounded {
                match count_embedded_frames_in_range(
                    conn,
                    start,
                    end,
                    session_id,
                    pool as usize,
                    COUNT_SCAN_CAP,
                )? {
                    // Provably no embedded frames in the window/session — nothing for the KNN.
                    None => return Ok(Vec::new()),
                    // min(pool, embedded-in-window/session), or `pool` when too large to prove
                    // sparse within the scan budget (safe dense assumption).
                    Some(target) => target,
                }
            } else {
                pool as usize
            };

            // Return each KNN row's frame + capture time (+ owning session only when scoped) in
            // distance order and post-filter window + session in Rust, so we can see both the
            // in-range count and the *raw* KNN row count (the exhaustion signal — a KNN that
            // returned `< k` rows has no more vectors). Session-filtering in SQL would corrupt that
            // `raw < k` signal. The unscoped SELECT omits `session_id` entirely, keeping the query
            // byte-identical to the pre-0.4.0 arm (and valid against pre-session schemas).
            let scoped = session_id.is_some();
            let sql = if scoped {
                "SELECT fr.id, fr.captured_at, fr.session_id FROM (
                     SELECT embedding_id AS vid, distance FROM embedding_vectors
                     WHERE embedding MATCH ?1 AND k = ?2 ORDER BY distance
                 ) knn
                 JOIN embeddings m ON m.id = knn.vid
                 JOIN frames fr ON fr.id = m.frame_id
                 ORDER BY knn.distance"
            } else {
                "SELECT fr.id, fr.captured_at FROM (
                     SELECT embedding_id AS vid, distance FROM embedding_vectors
                     WHERE embedding MATCH ?1 AND k = ?2 ORDER BY distance
                 ) knn
                 JOIN embeddings m ON m.id = knn.vid
                 JOIN frames fr ON fr.id = m.frame_id
                 ORDER BY knn.distance"
            };
            let mut stmt = conn.prepare(sql)?;
            let mut fetch = |k: u32| -> rusqlite::Result<(usize, Vec<i64>)> {
                // Defense in depth at the SQL boundary also covers the unbounded single-pass
                // path, which does not use the escalation ladder.
                let k = k.min(SQLITE_VEC_MAX_K);
                let mut raw = 0_usize;
                let mut in_range: Vec<i64> = Vec::new();
                let rows = stmt.query_map(params![blob, k as i64], |r| {
                    let frame_id = r.get::<_, i64>(0)?;
                    let captured_at = r.get::<_, i64>(1)?;
                    let frame_session = if scoped {
                        r.get::<_, Option<i64>>(2)?
                    } else {
                        None
                    };
                    Ok((frame_id, captured_at, frame_session))
                })?;
                for row in rows {
                    let (frame_id, captured_at, frame_session) = row?;
                    raw += 1;
                    let in_window = captured_at >= start && captured_at < end;
                    // No scope → every frame qualifies; scoped → only this session's frames.
                    let in_session = session_id.is_none_or(|want| frame_session == Some(want));
                    if in_window && in_session {
                        in_range.push(frame_id);
                    }
                }
                Ok((raw, in_range))
            };

            if !bounded {
                // No time filter: the pool nearest vectors are all in range, so one pass
                // suffices (escalating could over-fetch when several vectors map to one frame).
                let (_raw, in_range) = fetch(pool)?;
                let mut ids = dedup_keep_order(in_range);
                ids.truncate(pool as usize);
                return Ok(ids);
            }
            Ok(escalate_in_range_knn(pool, target, fetch)?)
        })
        .await
    }

    /// Resolves fused `(frame_id, score)` rows into [`SearchHit`]s, preferring the
    /// FTS snippet and falling back to a truncated OCR-text snippet. Frame context
    /// and fallback snippets are fetched in two bulk `IN` queries (not per-row), so
    /// hydration is at most two round-trips regardless of result count.
    async fn hydrate(
        &self,
        fused: Vec<(i64, f64)>,
        snippets: HashMap<i64, String>,
    ) -> Result<Vec<SearchHit>> {
        if fused.is_empty() {
            return Ok(Vec::new());
        }
        self.with_conn(move |conn| {
            let ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();

            // bulk-fetch frame context for every candidate
            let frames_sql = format!(
                "SELECT id, captured_at, image_path, image_purged, app_hint
                 FROM frames WHERE id IN ({})",
                placeholders(ids.len())
            );
            let frames: HashMap<i64, (i64, String, bool, Option<String>)> = conn
                .prepare(&frames_sql)?
                .query_map(params_from_iter(ids.iter()), |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        (
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, i64>(3)? != 0,
                            r.get::<_, Option<String>>(4)?,
                        ),
                    ))
                })?
                .collect::<rusqlite::Result<_>>()?;

            // bulk-fetch content text only for hits lacking an FTS snippet (the fallback)
            let need_text: Vec<i64> = ids
                .iter()
                .copied()
                .filter(|id| !snippets.contains_key(id))
                .collect();
            let texts: HashMap<i64, String> = if need_text.is_empty() {
                HashMap::new()
            } else {
                let ocr_sql = format!(
                    "SELECT frame_id, content_text FROM frame_text WHERE frame_id IN ({})",
                    placeholders(need_text.len())
                );
                conn.prepare(&ocr_sql)?
                    .query_map(params_from_iter(need_text.iter()), |r| {
                        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<_>>()?
            };

            // assemble in fused (RRF) order; skip frames that vanished between arms
            let mut hits = Vec::with_capacity(fused.len());
            for (frame_id, score) in fused {
                let Some((captured_at, image_path, image_purged, app_hint)) = frames.get(&frame_id)
                else {
                    continue;
                };
                let snippet = match snippets.get(&frame_id) {
                    Some(s) => s.clone(),
                    None => texts
                        .get(&frame_id)
                        .map(|t| truncate_snippet(t))
                        .unwrap_or_default(),
                };
                hits.push(SearchHit {
                    frame_id,
                    captured_at: *captured_at,
                    snippet,
                    score: score as f32,
                    image_path: image_path.clone(),
                    image_purged: *image_purged,
                    app_hint: app_hint.clone(),
                });
            }
            Ok(hits)
        })
        .await
    }
}

/// `?,?,…,?` — a comma-joined run of `n` positional placeholders for an `IN (…)`.
fn placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::{NewFrame, OcrResult};

    fn frame(at: i64) -> NewFrame {
        NewFrame {
            captured_at: at,
            monitor_index: 0,
            width: 1920,
            height: 1080,
            image_path: format!("frames/{at}.jpg"),
            content_hash: format!("h{at}"),
            app_hint: None,
            window_title: None,
            browser_url: None,
            capture_trigger: None,
        }
    }

    fn q(text: &str, include_chrome: bool) -> SearchQuery {
        SearchQuery {
            text: text.to_string(),
            limit: 10,
            time_range: None,
            include_chrome,
        }
    }

    #[test]
    fn normalized_limit_clamps_to_the_backend_ceiling() {
        assert_eq!(normalized_limit(0), 1, "zero floors to one");
        assert_eq!(normalized_limit(50), 50, "in-range passes through");
        // A report can ask for up to `reports.weekly_top_k` (2000); it is honored, not
        // clamped to the search-UI's 100 max (the prompted-report retrieval regression).
        assert_eq!(normalized_limit(2_000), MAX_SEARCH_LIMIT);
        assert_eq!(
            normalized_limit(u32::MAX),
            MAX_SEARCH_LIMIT,
            "an absurd limit clamps to the ceiling, never panics"
        );
    }

    /// `07` #8 review (P2): a *sparse* window — fewer distinct embedded frames than `pool` —
    /// must stop as soon as it has gathered all of them, not climb to the k ceiling. Here the
    /// mock KNN never exhausts (`raw == k` every pass) and never fills the pool, so only the
    /// count-derived `target` can stop it. Without the target cap this ran a full ceiling pass
    /// on every sparse-window query/report.
    #[test]
    fn escalating_knn_stops_at_window_count_not_ceiling() {
        let mut calls = 0;
        let ids = escalate_in_range_knn(500, 2, |k| {
            calls += 1;
            Ok((k as usize, vec![10, 20])) // 2 in-range frames, KNN never exhausts
        })
        .unwrap();
        assert_eq!(ids, vec![10, 20]);
        assert_eq!(
            calls, 1,
            "a sparse window must not escalate past its own frame count"
        );
    }

    /// Escalation still digs: when the in-range matches are buried beyond the first `k`, widen
    /// (×`KNN_ESCALATION_FACTOR`) until `target` distinct in-range frames surface.
    #[test]
    fn escalating_knn_widens_until_target_reached() {
        let mut calls = 0;
        let ids = escalate_in_range_knn(50, 3, |k| {
            calls += 1;
            let found = if k <= 50 { vec![1] } else { vec![1, 2, 3] };
            Ok((k as usize, found)) // raw == k → only the target gate can stop it
        })
        .unwrap();
        assert_eq!(ids, vec![1, 2, 3]);
        assert_eq!(
            calls, 2,
            "escalate once (50 → 400) to reach the buried matches"
        );
    }

    /// A KNN that returns `< k` rows has no more vectors — stop even if `target` is unmet
    /// (the window genuinely holds fewer embedded frames than the count suggested / a race).
    #[test]
    fn escalating_knn_stops_when_table_exhausted() {
        let mut calls = 0;
        let ids = escalate_in_range_knn(50, 10, |_k| {
            calls += 1;
            Ok((7, vec![1, 2])) // raw = 7 < k = 50 → exhausted; only 2 found
        })
        .unwrap();
        assert_eq!(ids, vec![1, 2]);
        assert_eq!(calls, 1, "raw < k means the vector table is drained — stop");
    }

    /// Pathological: never exhausts, never reaches `target`. Escalation must terminate at
    /// [`MAX_TIME_RANGE_KNN`] (50 → 400 → 3200 → 4096), never loop forever.
    #[test]
    fn escalating_knn_caps_at_the_k_ceiling() {
        let mut ks = Vec::new();
        let ids = escalate_in_range_knn(50, usize::MAX, |k| {
            ks.push(k);
            Ok((k as usize, vec![])) // nothing in range, never exhausts
        })
        .unwrap();
        assert!(ids.is_empty());
        assert_eq!(
            ks,
            vec![50, 400, 3_200, MAX_TIME_RANGE_KNN],
            "geometric climb, capped"
        );
    }

    /// A caller-controlled initial pool above sqlite-vec's limit is clamped before the first
    /// fetch, not only when the geometric ladder advances (usage review 2026-08-01 §7.4).
    #[test]
    fn escalating_knn_clamps_initial_pool_to_the_k_ceiling() {
        let mut ks = Vec::new();
        let ids = escalate_in_range_knn(u32::MAX, usize::MAX, |k| {
            ks.push(k);
            Ok((k as usize, vec![]))
        })
        .unwrap();
        assert!(ids.is_empty());
        assert_eq!(ks, vec![MAX_TIME_RANGE_KNN]);
    }

    /// The returned list is truncated to `pool`, nearest-first, even when the window holds
    /// more in-range frames than the caller asked for.
    #[test]
    fn escalating_knn_truncates_to_pool() {
        let ids = escalate_in_range_knn(3, 3, |k| Ok((k as usize, vec![1, 2, 3, 4, 5]))).unwrap();
        assert_eq!(ids, vec![1, 2, 3], "never return more than the pool");
    }

    /// `07` #8 review (P2): the escalation target counts *distinct* embedded frames (chunks
    /// deduped), honors the half-open window, is capped at `pool`, returns `None` for a window
    /// with no embeddings (KNN-skip fast path), and — the fix for the residual cost — never scans
    /// more than `scan_cap` frames: a window too large to prove sparse within budget falls back to
    /// the dense assumption (`Some(pool)`) instead of walking every frame.
    #[tokio::test]
    async fn count_embedded_frames_dedups_chunks_caps_and_bounds_the_scan() {
        let store = crate::SqliteStore::open_in_memory().unwrap();
        let f1 = store.insert_frame(frame(100)).await.unwrap(); // in window, 2 chunks
        let f2 = store.insert_frame(frame(150)).await.unwrap(); // in window, 1 chunk
        let _f3 = store.insert_frame(frame(160)).await.unwrap(); // in window, NOT embedded
        let f3e = store.insert_frame(frame(170)).await.unwrap(); // in window, embedded
        let f4 = store.insert_frame(frame(500)).await.unwrap(); // embedded, out of window

        store
            .with_conn(move |conn| {
                for (fid, chunk) in [(f1, 0), (f1, 1), (f2, 0), (f3e, 0), (f4, 0)] {
                    conn.execute(
                        "INSERT INTO embeddings(frame_id, chunk_index, chunk_text, source, model, dim, content_hash)
                         VALUES (?1, ?2, 't', 'ocr', 'm', 768, 'h')",
                        params![fid, chunk],
                    )?;
                }
                Ok(())
            })
            .await
            .unwrap();

        // Window [100, 200): f1 (2 chunks → counted once) + f2 + f3e = 3 distinct embedded frames;
        // the un-embedded f3 and out-of-window f4 don't count. Scan budget far exceeds the 4
        // in-window frames, so the count is exact.
        let n = store
            .with_conn(|conn| {
                Ok(count_embedded_frames_in_range(
                    conn, 100, 200, None, 50, 1_000,
                )?)
            })
            .await
            .unwrap();
        assert_eq!(
            n,
            Some(3),
            "distinct embedded frames, chunks deduped, un-embedded excluded"
        );

        // Capped at `pool`: the caller only ever needs min(pool, n).
        let capped = store
            .with_conn(|conn| {
                Ok(count_embedded_frames_in_range(
                    conn, 100, 200, None, 1, 1_000,
                )?)
            })
            .await
            .unwrap();
        assert_eq!(capped, Some(1), "the target is capped at `pool`");

        // Scan budget hit (scan_cap = 2 < 4 in-window frames): the window can't be proven sparse
        // within budget, so it falls back to the dense assumption (`pool`) instead of returning
        // the small exact count — this is what keeps the pre-count O(cap) on a wide, sparsely
        // embedded window (the P2 residual). Without the bound this returned Some(2).
        let bounded = store
            .with_conn(|conn| {
                Ok(count_embedded_frames_in_range(
                    conn, 100, 200, None, 500, 2,
                )?)
            })
            .await
            .unwrap();
        assert_eq!(
            bounded,
            Some(500),
            "over-budget window assumes dense (pool), never walks every frame"
        );

        // A window with no embedded frames returns `None` (drives the KNN-skip fast path).
        let empty = store
            .with_conn(|conn| {
                Ok(count_embedded_frames_in_range(
                    conn, 200, 300, None, 50, 1_000,
                )?)
            })
            .await
            .unwrap();
        assert_eq!(empty, None, "no embedded frames in the window");
    }

    /// `include_chrome` searches `raw_text` via the raw FTS arm, independent of
    /// `content_text` (`03 §3b`). PR2's populator writes content == raw, so we widen
    /// `raw_text` directly to simulate PR3's filter dropping a term to the chrome
    /// layer: the term is then reachable only with `include_chrome = true`, while
    /// content text stays searchable in both modes.
    #[tokio::test]
    async fn include_chrome_searches_raw_text_independently_of_content() {
        let store = crate::SqliteStore::open_in_memory().unwrap();
        let fid = store.insert_frame(frame(1_000)).await.unwrap();
        store
            .insert_ocr(
                fid,
                OcrResult {
                    text: "alpha".to_string(),
                    mean_confidence: -1.0,
                    engine: "test".to_string(),
                    spans: Vec::new(),
                },
            )
            .await
            .unwrap();
        // Diverge raw from content: "bravo" now lives only in raw_text. The raw FTS
        // update trigger keeps frame_text_raw_fts in sync.
        store
            .with_conn(move |conn| {
                conn.execute(
                    "UPDATE frame_text SET raw_text = ?2 WHERE frame_id = ?1",
                    params![fid, "alpha bravo"],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        // Default (content only): "bravo" is not in content_text → no hit.
        assert!(store
            .hybrid_search(&q("bravo", false))
            .await
            .unwrap()
            .is_empty());
        // include_chrome: the raw arm finds it.
        let hits = store.hybrid_search(&q("bravo", true)).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].frame_id, fid);
        // content text stays searchable regardless of the flag.
        assert_eq!(
            store.hybrid_search(&q("alpha", false)).await.unwrap().len(),
            1
        );
        assert_eq!(
            store.hybrid_search(&q("alpha", true)).await.unwrap().len(),
            1
        );
    }
}
