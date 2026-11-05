//! Ask v2 (`03 §7i`): entity mentions, the four retrieval stages' store side, the
//! conversation rows, and the `session_vectors` backfill.
//!
//! ## Why stage 2 builds a `temp.` FTS5 index
//! The v14 baseline (`03 §4`) has no FTS index over `sessions.title`/`summary`, and adding a
//! persisted one is a P3 DDL change (0.4.5 D2). Stage 2 therefore builds a per-query
//! `temp.ask_session_fts` FTS5 index over exactly the candidate sessions' concatenated
//! title/summary, ranks it with `bm25()`, and drops it before returning. A temp table is
//! connection scratch state, not persisted schema: it needs no migration, leaves no row
//! behind, and its cost is bounded by the candidate set stage 1 already narrowed. The same
//! `temp.` schema hosts nothing else, so the fixed name cannot collide with a persisted
//! object.
//!
//! ## Stage 3 mirrors, but never edits, the frame search
//! [`SqliteStore::ask_search_frames`] repeats the two arms of
//! [`crate::search`] with a `frames.session_id IN (…)` predicate instead of the
//! single-session scope. The existing search methods keep their exact SQL and semantics
//! (`03 §7c`, D12); an empty session set delegates to the existing hybrid search, which is
//! the documented non-blocking fallback (`03 §7i.1`).
//!
//! ## Vector rows are cleaned here because `vec0` has no FK
//! [`crate::SqliteStore::delete_unfrozen_session`] drops a deleted session's
//! `session_vectors` row in the same transaction (see `sessions.rs`): a `vec0` table carries
//! no foreign key and a DDL trigger would be a schema change (P3 owns the DDL).

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, types::Value, OptionalExtension, Row};
use traits::{
    AskEntityMatch, AskSessionHit, AskThreadCandidate, AskTurnRecord, Embedding, NewAskTurn,
    Result, SearchHit, SearchQuery, SessionEmbedInput, TimeRange,
};

use crate::embeddings::{f32_blob, SQLITE_VEC_MAX_K};
use crate::search::{
    candidate_pool, escalate_in_range_knn, fts_match_query, normalized_limit, rrf_fuse,
};
use crate::{SqliteStore, EMBEDDING_DIM};

/// Maximum ids bound into one `IN (…)` predicate. SQLite's bound-parameter ceiling is per
/// statement, so a wide candidate set (every session of an unbounded window) is chunked
/// rather than truncated.
const IN_CHUNK: usize = 400;

/// Upper bound on a session label's length in the assembled prompt (`03 §7i.2` stage 4).
const MAX_SESSION_LABEL: usize = 60;

/// Frames the stage-3 pre-count will examine before it assumes a dense session set. The
/// count only decides how far the KNN escalation aims, exactly like
/// `search::COUNT_SCAN_CAP`: past the budget the answer is "assume dense", which can only
/// widen the escalation, never drop an in-range frame.
const COUNT_SCAN_CAP: usize = SQLITE_VEC_MAX_K as usize;

/// Connection-scratch FTS5 index for stage 2 (never persisted; see the module docs).
const TEMP_SESSION_FTS: &str = "temp.ask_session_fts";

/// The SQL twin of [`session_text`], used by [`SqliteStore::session_embed_candidates`] and
/// [`SqliteStore::session_embed_pending`] so the candidate predicate and the write gate in
/// [`SqliteStore::upsert_session_vector`] cannot disagree. `NULL` when both columns are
/// `NULL`, which the `<> ''` comparison rejects (a `NULL` comparison is not true).
///
/// `crate`-visible because the title/summary writers in `sessions.rs` compare the text they
/// produced against the text the stored vector was built from, to invalidate a stale vector.
pub(crate) const SESSION_TEXT_SQL: &str = "CASE
         WHEN title IS NOT NULL AND summary IS NOT NULL THEN title || char(10) || summary
         WHEN title IS NOT NULL THEN title
         ELSE summary
     END";

/// The exact text embedded for a session: its non-`NULL` `title` and `summary`, joined by a
/// newline when both exist (`03 §7i.2`). Both the candidate query and the upsert gate use
/// this function, so a text the backfill will hand out is exactly a text the write accepts.
fn session_text(title: Option<&str>, summary: Option<&str>) -> String {
    match (title, summary) {
        (Some(title), Some(summary)) => format!("{title}\n{summary}"),
        (Some(title), None) => title.to_string(),
        (None, Some(summary)) => summary.to_string(),
        (None, None) => String::new(),
    }
}

/// Whether `needle` occurs in `haystack` as a **whole mention**: neither character flanking the
/// occurrence is alphanumeric, so a short workspace name such as `api` does not resolve out of
/// "capital expenditure" (`03 §7i.1`). Both sides must already be lowercased by the caller.
///
/// The DB prefilter stays a substring `instr` so the scan is bounded by the graph's size; this
/// is the precision pass over its result, and it applies identically to canonical names and to
/// aliases.
fn bounded_mention(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let mut search_from = 0usize;
    while let Some(found) = haystack[search_from..].find(needle) {
        let start = search_from + found;
        let end = start + needle.len();
        let before_ok = haystack[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let after_ok = haystack[end..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_alphanumeric());
        if before_ok && after_ok {
            return true;
        }
        // Advance one character past this occurrence's start so overlapping candidates are
        // still considered (`snake_api` → `api` must not match, `api_v2` is a separate name).
        search_from = start
            + haystack[start..]
                .chars()
                .next()
                .map(char::len_utf8)
                .unwrap_or(1);
        if search_from >= haystack.len() {
            break;
        }
    }
    false
}

/// `?first,?first+1,…` — a run of **explicitly numbered** placeholders. Explicit numbering
/// keeps a chunked `IN (…)` list usable together with later bound values regardless of the
/// textual order of the clauses (a bare `?` is numbered by its position in the statement).
fn numbered_placeholders(first: usize, count: usize) -> String {
    (first..first + count)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Sorted, de-duplicated copy of an id slice.
fn sorted_ids(ids: &[i64]) -> Vec<i64> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Cuts a label to [`MAX_SESSION_LABEL`] chars on a char boundary. No ellipsis: the budget is
/// the prompt, not a display string.
fn short_label(text: &str) -> String {
    if text.chars().count() <= MAX_SESSION_LABEL {
        return text.to_string();
    }
    text.chars().take(MAX_SESSION_LABEL).collect()
}

/// A short session label for the prompt: trimmed `title`, else `tool`, else the session
/// `kind`, else a start-instant discriminator. Deliberately timezone-free — the instant is
/// raw unix ms, never locally formatted, so a label cannot depend on the host's zone.
fn session_label(title: Option<&str>, tool: Option<&str>, kind: &str, started_at: i64) -> String {
    if let Some(title) = title.map(str::trim).filter(|text| !text.is_empty()) {
        return short_label(title);
    }
    if let Some(tool) = tool.map(str::trim).filter(|text| !text.is_empty()) {
        return short_label(tool);
    }
    let kind = kind.trim();
    if !kind.is_empty() {
        return short_label(kind);
    }
    short_label(&format!("session {started_at}"))
}

/// Every session (ascending, de-duplicated) linked to any of `entity_ids` through
/// `session_entities` (`03 §7h.3`). The planner resolves a mention to an **entity**
/// (`ask_resolve_entities`) and `03 §7i.1` constrains retrieval to the sessions linked to that
/// entity, so this join belongs to the store: no caller needs to know the graph's link table,
/// and an empty `entity_ids` yields an empty scope (the graph-less fallback, `§7i.1`).
fn entity_scope_sessions(
    conn: &rusqlite::Connection,
    entity_ids: &[i64],
) -> rusqlite::Result<Vec<i64>> {
    if entity_ids.is_empty() {
        return Ok(Vec::new());
    }
    let list = numbered_placeholders(1, entity_ids.len());
    let mut stmt = conn.prepare(&format!(
        "SELECT DISTINCT session_id FROM session_entities
         WHERE entity_id IN ({list}) ORDER BY session_id"
    ))?;
    let ids = stmt
        .query_map(params_from_iter(entity_ids.iter().copied()), |row| {
            row.get::<_, i64>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// Every session (ascending) overlapping `window` under the half-open rule already used by
/// [`SqliteStore::sessions_in_range`]: `[started_at, COALESCE(ended_at, now))` intersects
/// `[window.start, window.end)`. The clock is the database's own (`unixepoch()*1000`), so an
/// open session (`ended_at IS NULL`) is treated as continuing to the present. No window means
/// the whole history (`03 §7i.2`).
fn window_sessions(
    conn: &rusqlite::Connection,
    window: Option<TimeRange>,
) -> rusqlite::Result<Vec<i64>> {
    let Some(window) = window else {
        let mut stmt = conn.prepare("SELECT id FROM sessions ORDER BY id")?;
        let ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        return Ok(ids);
    };
    let mut stmt = conn.prepare(
        "SELECT id FROM sessions
         WHERE COALESCE(ended_at, unixepoch()*1000) > ?1 AND started_at < ?2
         ORDER BY id",
    )?;
    let ids = stmt
        .query_map(params![window.start, window.end], |row| {
            row.get::<_, i64>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// `ids` restricted to the sessions that exist and overlap `window` (all of them when the
/// window is unbounded), ascending and de-duplicated.
fn overlapping_sessions(
    conn: &rusqlite::Connection,
    ids: &[i64],
    window: Option<TimeRange>,
) -> rusqlite::Result<Vec<i64>> {
    let Some(window) = window else {
        return Ok(sorted_ids(ids));
    };
    let mut out = Vec::new();
    for chunk in ids.chunks(IN_CHUNK) {
        let list = numbered_placeholders(1, chunk.len());
        let sql = format!(
            "SELECT id FROM sessions
             WHERE id IN ({list})
               AND COALESCE(ended_at, unixepoch()*1000) > ?{start}
               AND started_at < ?{end}
             ORDER BY id",
            start = chunk.len() + 1,
            end = chunk.len() + 2
        );
        let mut values: Vec<Value> = chunk.iter().copied().map(Value::Integer).collect();
        values.push(Value::Integer(window.start));
        values.push(Value::Integer(window.end));
        let mut stmt = conn.prepare(&sql)?;
        out.extend(
            stmt.query_map(params_from_iter(values), |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        );
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// The escalation target for a session-restricted KNN: `Some(min(pool, embedded frames in the
/// session set inside the window))`, or `None` when the set provably holds no embedded frame
/// (the caller then skips the KNN). Same budget rule as `search::count_embedded_frames_in_range`
/// — a scan that hits [`COUNT_SCAN_CAP`] cannot prove sparsity, so it assumes a dense set and
/// aims at `pool`, which only widens the escalation.
fn embedded_frames_in_sessions(
    conn: &rusqlite::Connection,
    sessions: &[i64],
    start: i64,
    end: i64,
    pool: usize,
) -> rusqlite::Result<Option<usize>> {
    let mut scanned_total = 0usize;
    let mut embedded_total = 0usize;
    for chunk in sessions.chunks(IN_CHUNK) {
        let list = numbered_placeholders(1, chunk.len());
        let sql = format!(
            "SELECT COUNT(*), COALESCE(SUM(has_emb), 0) FROM (
                 SELECT EXISTS (SELECT 1 FROM embeddings m WHERE m.frame_id = fr.id) AS has_emb
                 FROM frames fr
                 WHERE fr.captured_at >= ?{start} AND fr.captured_at < ?{end}
                   AND fr.session_id IN ({list})
                 LIMIT ?{limit_param}
             )",
            start = chunk.len() + 1,
            end = chunk.len() + 2,
            limit_param = chunk.len() + 3
        );
        let mut values: Vec<Value> = chunk.iter().copied().map(Value::Integer).collect();
        values.push(Value::Integer(start));
        values.push(Value::Integer(end));
        values.push(Value::Integer(COUNT_SCAN_CAP as i64));
        let (scanned, embedded): (i64, i64) =
            conn.query_row(&sql, params_from_iter(values), |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
        scanned_total += scanned as usize;
        embedded_total += embedded as usize;
        if scanned_total >= COUNT_SCAN_CAP {
            return Ok(Some(pool));
        }
    }
    Ok((embedded_total > 0).then(|| embedded_total.min(pool)))
}

/// Maps an `ask_turns` row (column order of [`ASK_TURN_COLUMNS`]) to an [`AskTurnRecord`].
fn row_to_ask_turn(row: &Row<'_>) -> rusqlite::Result<AskTurnRecord> {
    let window_start: Option<i64> = row.get(5)?;
    let window_end: Option<i64> = row.get(6)?;
    let resolved: Option<String> = row.get(7)?;
    // Both columns are written together or not at all (`03 §7i.1`); a half-written window is
    // read back as "no window" rather than a fabricated bound.
    let window = match (window_start, window_end) {
        (Some(start), Some(end)) => Some(TimeRange { start, end }),
        _ => None,
    };
    let resolved_entities = match resolved {
        None => Vec::new(),
        Some(json) => serde_json::from_str::<Vec<i64>>(&json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    };
    Ok(AskTurnRecord {
        id: row.get(0)?,
        thread_id: row.get(1)?,
        ordinal: row.get(2)?,
        question: row.get(3)?,
        answer: row.get(4)?,
        window,
        resolved_entities,
        created_at: row.get(8)?,
    })
}

/// The column order [`row_to_ask_turn`] expects.
const ASK_TURN_COLUMNS: &str =
    "id, thread_id, ordinal, question, answer, window_start, window_end, resolved_entities, created_at";

impl SqliteStore {
    /// Embeds `text` with the store's injected embedder, or `None` when no embedder is
    /// attached or the text is blank — the two conditions under which every hybrid arm in this
    /// crate degrades to FTS only. The `Arc` is cloned out before the `.await`, so the read
    /// lock is never held across it.
    async fn query_embedding(&self, text: &str) -> Result<Option<Embedding>> {
        let embedder = self
            .embedder
            .read()
            .expect("store embedder lock poisoned")
            .clone();
        let Some(embedder) = embedder else {
            return Ok(None);
        };
        if text.trim().is_empty() {
            return Ok(None);
        }
        let input = text.to_string();
        let mut embeddings = embedder.embed_texts(std::slice::from_ref(&input)).await?;
        let embedding = embeddings
            .pop()
            .ok_or_else(|| anyhow::anyhow!("embedder returned no vector for the query"))?;
        Ok(Some(embedding))
    }

    // ---- Stage 1: entity mentions and thread candidates (`03 §7i.1`, `§7i.2`) -----------

    /// Entity mentions in `question` resolved against `entities.canonical_name` and
    /// `entity_aliases.alias`, case-insensitively (`03 §7i.1`, `§7h.2`). A mention with no
    /// matching graph row is simply absent, so a database whose graph tables are empty returns
    /// nothing and the pipeline falls back to plain text matching. Matches come longest first;
    /// each entity appears once, with its longest matched form.
    pub async fn ask_resolve_entities(&self, question: &str) -> Result<Vec<AskEntityMatch>> {
        let question = question.to_string();
        self.with_conn(move |conn| {
            // No SQL `instr`/`lower` prefilter: SQLite's `lower()` folds ASCII only, so a
            // prefilter would drop a valid mention before the boundary check could see it
            // (`Équipe` in `équipe`). Both sides are lowered by Rust below, which is
            // Unicode-aware, and the scan is bounded by the graph's own size — a few thousand
            // rows for a long history, not by the frame table.
            let mut stmt = conn.prepare(
                "SELECT e.id, e.canonical_name
                 FROM entities e
                 WHERE length(e.canonical_name) > 0
                 UNION ALL
                 SELECT a.entity_id, a.alias
                 FROM entity_aliases a
                 WHERE length(a.alias) > 0",
            )?;
            let mut rows = stmt
                .query_map([], |row| {
                    Ok(AskEntityMatch {
                        entity_id: row.get(0)?,
                        matched: row.get(1)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            // A substring hit is not a mention: `api` inside "capital" must not resolve, or the
            // turn would be scoped (and persisted) to an unrelated workspace (`03 §7i.1`).
            let lowered = question.to_lowercase();
            rows.retain(|candidate| bounded_mention(&lowered, &candidate.matched.to_lowercase()));
            // Longest match first, ties by entity id. The first row of an entity is then that
            // entity's longest matched form, which is the one the turn records.
            rows.sort_by(|left, right| {
                right
                    .matched
                    .chars()
                    .count()
                    .cmp(&left.matched.chars().count())
                    .then(left.entity_id.cmp(&right.entity_id))
            });
            let mut seen = HashSet::new();
            rows.retain(|matched| seen.insert(matched.entity_id));
            Ok(rows)
        })
        .await
    }

    /// Stage 1 (`03 §7i.2`): every thread whose closed `[first_seen_at, last_seen_at]`
    /// interval overlaps `window`, plus every thread holding a session linked to one of
    /// `entity_ids` (the entity ids an entity mention resolved to). `window: None` is the
    /// whole history, so every thread is a candidate. Members are ascending; a thread without
    /// members is still a candidate (empty `session_ids`).
    ///
    /// `entity_ids` carries **entity** ids, not session ids: `03 §7i.1` resolves a mention to
    /// an entity and constrains retrieval to the sessions linked to it, so the
    /// `session_entities` join lives here and no caller has to know the graph's link table.
    pub async fn ask_candidate_threads(
        &self,
        window: Option<TimeRange>,
        entity_ids: &[i64],
    ) -> Result<Vec<AskThreadCandidate>> {
        let entity_ids = sorted_ids(entity_ids);
        self.with_conn(move |conn| {
            let linked = entity_scope_sessions(conn, &entity_ids)?;
            let mut values: Vec<Value> = vec![
                window.map_or(Value::Null, |range| Value::Integer(range.start)),
                window.map_or(Value::Null, |range| Value::Integer(range.end)),
            ];
            // `?1 IS NULL` is the "no window" switch: with no planned bound, every thread
            // qualifies (`03 §7i.1`: a planner with no anchor must not narrow the history).
            let mut sql = String::from(
                "SELECT t.id, ts.session_id
                 FROM threads t
                 LEFT JOIN thread_sessions ts ON ts.thread_id = t.id
                 WHERE (?1 IS NULL OR (t.first_seen_at <= ?2 AND t.last_seen_at >= ?1))",
            );
            if !linked.is_empty() {
                let list = numbered_placeholders(3, linked.len());
                sql.push_str(&format!(
                    " OR EXISTS (SELECT 1 FROM thread_sessions matched
                                 WHERE matched.thread_id = t.id
                                   AND matched.session_id IN ({list}))"
                ));
                values.extend(linked.iter().copied().map(Value::Integer));
            }
            sql.push_str(" ORDER BY t.id, ts.session_id");

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(values), |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut candidates: Vec<AskThreadCandidate> = Vec::new();
            for (thread_id, session_id) in rows {
                match candidates.last_mut() {
                    Some(current) if current.thread_id == thread_id => {
                        if let Some(session_id) = session_id {
                            current.session_ids.push(session_id);
                        }
                    }
                    _ => candidates.push(AskThreadCandidate {
                        thread_id,
                        session_ids: session_id.into_iter().collect(),
                    }),
                }
            }
            Ok(candidates)
        })
        .await
    }

    /// The stage-2 candidate set (`03 §7i.2`): the member sessions of `threads` that overlap
    /// `window`, intersected with the sessions linked to `entity_ids` when an entity
    /// resolved. With no thread candidates the window's sessions are the set — the documented
    /// non-blocking fallback (`§7i.1`). Ascending and de-duplicated.
    pub async fn ask_candidate_sessions(
        &self,
        threads: &[AskThreadCandidate],
        window: Option<TimeRange>,
        entity_ids: &[i64],
    ) -> Result<Vec<i64>> {
        let members = sorted_ids(
            &threads
                .iter()
                .flat_map(|thread| thread.session_ids.iter().copied())
                .collect::<Vec<_>>(),
        );
        let entity_ids = sorted_ids(entity_ids);
        self.with_conn(move |conn| {
            let entity_scope: HashSet<i64> = entity_scope_sessions(conn, &entity_ids)?
                .into_iter()
                .collect();
            let mut sessions = if members.is_empty() {
                window_sessions(conn, window)?
            } else {
                overlapping_sessions(conn, &members, window)?
            };
            if !entity_scope.is_empty() {
                sessions.retain(|session_id| entity_scope.contains(session_id));
            }
            Ok(sessions)
        })
        .await
    }

    // ---- Stage 2: hybrid session retrieval (`03 §7i.2`) ---------------------------------

    /// Stage 2: hybrid retrieval over `candidates`, fusing an FTS5 arm on session
    /// `title`/`summary` with the `session_vectors` cosine KNN through the crate's RRF rule
    /// (`k = 60`, ties toward the larger id). Sessions matched by neither arm are excluded;
    /// at most `limit` hits are returned, best first.
    ///
    /// The FTS arm indexes the candidates' concatenated text into a `temp.` FTS5 table for the
    /// duration of this query only (see the module docs: a persisted session index would be a
    /// P3 schema change). The KNN arm runs directly against `session_vectors` and post-filters
    /// to the candidate set in Rust, because a `vec0` table carries neither metadata nor a
    /// filterable predicate (`03 §4`).
    pub async fn ask_search_sessions(
        &self,
        question: &str,
        candidates: &[i64],
        limit: u32,
    ) -> Result<Vec<AskSessionHit>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let limit = normalized_limit(limit);
        let pool = candidate_pool(limit);
        let candidates = sorted_ids(candidates);
        let question = question.to_string();
        let query_embedding = self.query_embedding(&question).await?;

        self.with_conn(move |conn| {
            conn.execute_batch(&format!(
                "DROP TABLE IF EXISTS {TEMP_SESSION_FTS};
                 CREATE VIRTUAL TABLE {TEMP_SESSION_FTS}
                   USING fts5(body, tokenize='porter unicode61');"
            ))?;

            let outcome: Result<(Vec<i64>, Vec<i64>)> = (|| {
                // --- FTS arm: index exactly the candidates' title/summary text ---
                let fts_ids = {
                    let mut ids = Vec::new();
                    if let Some(match_query) = fts_match_query(&question) {
                        for chunk in candidates.chunks(IN_CHUNK) {
                            let list = numbered_placeholders(1, chunk.len());
                            let sql = format!(
                                "SELECT id, title, summary FROM sessions WHERE id IN ({list})"
                            );
                            let rows = {
                                let mut stmt = conn.prepare(&sql)?;
                                let rows = stmt
                                    .query_map(
                                        params_from_iter(chunk.iter().copied().map(Value::Integer)),
                                        |row| {
                                            Ok((
                                                row.get::<_, i64>(0)?,
                                                row.get::<_, Option<String>>(1)?,
                                                row.get::<_, Option<String>>(2)?,
                                            ))
                                        },
                                    )?
                                    .collect::<rusqlite::Result<Vec<_>>>()?;
                                rows
                            };
                            let mut insert = conn.prepare(&format!(
                                "INSERT INTO {TEMP_SESSION_FTS} (rowid, body) VALUES (?1, ?2)"
                            ))?;
                            for (session_id, title, summary) in rows {
                                let body = session_text(title.as_deref(), summary.as_deref());
                                // A session with no text is never embedded and never matches
                                // (an empty body would match nothing anyway).
                                if !body.is_empty() {
                                    insert.execute(params![session_id, body])?;
                                }
                            }
                        }
                        let mut stmt = conn.prepare(
                            "SELECT rowid FROM ask_session_fts
                             WHERE ask_session_fts MATCH ?1
                             ORDER BY bm25(ask_session_fts)
                             LIMIT ?2",
                        )?;
                        ids = stmt
                            .query_map(params![match_query, i64::from(pool)], |row| {
                                row.get::<_, i64>(0)
                            })?
                            .collect::<rusqlite::Result<Vec<_>>>()?;
                    }
                    ids
                };

                // --- vector arm: cosine KNN, post-filtered to the candidate set ---
                // `vec0` cannot filter inside `MATCH`, so the nearest vectors are fetched and the
                // candidate set is applied in Rust. A candidate set can rank far past the top
                // `pool` nearest across the whole history (the same recall hole `07` #8 names for
                // time windows), so `k` escalates geometrically until it has gathered the set, the
                // table is exhausted, or `k` reaches sqlite-vec's ceiling.
                let vec_ids = {
                    let mut ids = Vec::new();
                    if let Some(embedding) = query_embedding {
                        let blob = f32_blob(&embedding.0);
                        let candidate_set: HashSet<i64> = candidates.iter().copied().collect();
                        let target = candidates.len().min(pool as usize);
                        let mut stmt = conn.prepare(
                            "SELECT session_id, distance FROM session_vectors
                             WHERE embedding MATCH ?1 AND k = ?2
                             ORDER BY distance",
                        )?;
                        let fetch = |k: u32| -> rusqlite::Result<(usize, Vec<i64>)> {
                            let k = k.min(SQLITE_VEC_MAX_K);
                            let mut raw = 0usize;
                            let mut in_scope = Vec::new();
                            let rows = stmt.query_map(params![blob, i64::from(k)], |row| {
                                Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
                            })?;
                            for row in rows {
                                let (session_id, _distance) = row?;
                                raw += 1;
                                if candidate_set.contains(&session_id) {
                                    in_scope.push(session_id);
                                }
                            }
                            Ok((raw, in_scope))
                        };
                        ids = escalate_in_range_knn(pool, target, fetch)?;
                    }
                    ids
                };
                Ok((fts_ids, vec_ids))
            })();

            // The index is scratch state for this one query: drop it even when an arm failed,
            // so a later query can never MATCH a stale candidate set.
            let _ = conn.execute_batch(&format!("DROP TABLE IF EXISTS {TEMP_SESSION_FTS};"));
            let (fts_ids, vec_ids) = outcome?;
            Ok(rrf_fuse(&[fts_ids, vec_ids], limit))
        })
        .await
        .map(|fused| {
            fused
                .into_iter()
                .map(|(session_id, score)| AskSessionHit { session_id, score })
                .collect()
        })
    }

    // ---- Stage 3: session-restricted frame retrieval (`03 §7i.2`) -----------------------

    /// Stage 3: the existing hybrid frame search restricted to frames owned by `sessions`. An
    /// empty `sessions` slice is the documented fallback — the existing hybrid search over the
    /// query's own window, byte-identical to today's behaviour (`§7i.1`). This is a new method:
    /// [`Self::hybrid_search`] and its scoped siblings keep their exact SQL and semantics.
    pub async fn ask_search_frames(
        &self,
        query: &SearchQuery,
        sessions: &[i64],
    ) -> Result<Vec<SearchHit>> {
        let sessions = sorted_ids(sessions);
        if sessions.is_empty() {
            return self.hybrid_search(query).await;
        }
        let limit = normalized_limit(query.limit);
        let pool = candidate_pool(limit);
        // Half-open `[start, end)` like every other search path.
        let (start, end) = query
            .time_range
            .map(|range| (range.start, range.end))
            .unwrap_or((i64::MIN, i64::MAX));

        // --- content FTS arm, restricted to the session set ---
        let content = self
            .session_frame_fts_arm("frame_text_fts", &query.text, start, end, &sessions, pool)
            .await?;
        let content_ids: Vec<i64> = content.iter().map(|(id, _)| *id).collect();
        let mut snippets: HashMap<i64, String> = content.into_iter().collect();

        // --- raw FTS arm, only when the caller opts into chrome/raw text (`03 §3b`) ---
        let raw_ids: Vec<i64> = if query.include_chrome {
            let raw = self
                .session_frame_fts_arm(
                    "frame_text_raw_fts",
                    &query.text,
                    start,
                    end,
                    &sessions,
                    pool,
                )
                .await?;
            let ids: Vec<i64> = raw.iter().map(|(id, _)| *id).collect();
            for (id, snippet) in raw {
                snippets.entry(id).or_insert(snippet);
            }
            ids
        } else {
            Vec::new()
        };

        // --- vector arm, restricted to the session set ---
        let vector_ids = match self.query_embedding(&query.text).await? {
            Some(embedding) => {
                self.session_frame_knn(embedding.0, pool, start, end, &sessions)
                    .await?
            }
            None => Vec::new(),
        };

        let mut arms: Vec<Vec<i64>> = vec![content_ids];
        if query.include_chrome {
            arms.push(raw_ids);
        }
        arms.push(vector_ids);
        let fused = rrf_fuse(&arms, limit);
        self.hydrate(fused, snippets).await
    }

    /// BM25-ranked FTS hits inside the window and the session set, with highlighted snippets.
    /// Mirrors `search::fts_arm`'s query shape with a `frames.session_id IN (…)` predicate;
    /// the table name is a fixed internal identifier (never user input), so interpolating it is
    /// injection-safe. A candidate set wider than [`IN_CHUNK`] is queried in chunks and merged
    /// by best rank — `bm25()` values are comparable across the chunks because the table and
    /// the match expression are identical.
    async fn session_frame_fts_arm(
        &self,
        table: &str,
        text: &str,
        start: i64,
        end: i64,
        sessions: &[i64],
        pool: u32,
    ) -> Result<Vec<(i64, String)>> {
        let Some(match_query) = fts_match_query(text) else {
            return Ok(Vec::new());
        };
        let sessions = sessions.to_vec();
        let table = table.to_string();
        self.with_conn(move |conn| {
            let mut best: HashMap<i64, (f64, String)> = HashMap::new();
            for chunk in sessions.chunks(IN_CHUNK) {
                let list = numbered_placeholders(1, chunk.len());
                let pool_param = chunk.len() + 4;
                let sql = format!(
                    "SELECT fts.rowid,
                            snippet({table}, 0, '[', ']', '…', 12) AS snip,
                            bm25({table}) AS rank
                     FROM {table} fts
                     JOIN frames fr ON fr.id = fts.rowid
                     WHERE {table} MATCH ?{match_param}
                       AND fr.captured_at >= ?{start_param}
                       AND fr.captured_at < ?{end_param}
                       AND fr.session_id IN ({list})
                     ORDER BY rank
                     LIMIT ?{pool_param}",
                    match_param = chunk.len() + 1,
                    start_param = chunk.len() + 2,
                    end_param = chunk.len() + 3,
                );
                let mut values: Vec<Value> = chunk.iter().copied().map(Value::Integer).collect();
                values.push(Value::Text(match_query.clone()));
                values.push(Value::Integer(start));
                values.push(Value::Integer(end));
                values.push(Value::Integer(i64::from(pool)));
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt
                    .query_map(params_from_iter(values), |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, f64>(2)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                for (frame_id, snippet, rank) in rows {
                    match best.get_mut(&frame_id) {
                        Some(held) if rank < held.0 => *held = (rank, snippet),
                        Some(_) => {}
                        None => {
                            best.insert(frame_id, (rank, snippet));
                        }
                    }
                }
            }
            let mut ranked: Vec<(i64, f64, String)> = best
                .into_iter()
                .map(|(frame_id, (rank, snippet))| (frame_id, rank, snippet))
                .collect();
            ranked.sort_by(|left, right| {
                left.1
                    .partial_cmp(&right.1)
                    .unwrap_or(Ordering::Equal)
                    .then(left.0.cmp(&right.0))
            });
            ranked.truncate(pool as usize);
            Ok(ranked
                .into_iter()
                .map(|(frame_id, _, snippet)| (frame_id, snippet))
                .collect())
        })
        .await
    }

    /// Text-embedding cosine KNN restricted to the session set and the window. `vec0` cannot
    /// filter inside `MATCH`, so the `k` nearest vectors are fetched and post-filtered in Rust;
    /// because a session's frames are a small slice of the vector table and can rank far past
    /// the top-`pool` nearest, `k` escalates geometrically (the same recall hole as the
    /// session-scoped frame search, `07` #8) until it has gathered the session set's embedded
    /// frames, the table is exhausted, or `k` hits sqlite-vec's ceiling.
    async fn session_frame_knn(
        &self,
        query: Vec<f32>,
        pool: u32,
        start: i64,
        end: i64,
        sessions: &[i64],
    ) -> Result<Vec<i64>> {
        let blob = f32_blob(&query);
        let sessions = sessions.to_vec();
        self.with_conn(move |conn| {
            let Some(target) =
                embedded_frames_in_sessions(conn, &sessions, start, end, pool as usize)?
            else {
                return Ok(Vec::new());
            };
            let session_set: HashSet<i64> = sessions.iter().copied().collect();
            let mut stmt = conn.prepare(
                "SELECT fr.id, fr.captured_at, fr.session_id FROM (
                     SELECT embedding_id AS vid, distance FROM embedding_vectors
                     WHERE embedding MATCH ?1 AND k = ?2 ORDER BY distance
                 ) knn
                 JOIN embeddings m ON m.id = knn.vid
                 JOIN frames fr ON fr.id = m.frame_id
                 ORDER BY knn.distance",
            )?;
            let fetch = |k: u32| -> rusqlite::Result<(usize, Vec<i64>)> {
                let k = k.min(SQLITE_VEC_MAX_K);
                let mut raw = 0usize;
                let mut in_scope = Vec::new();
                let rows = stmt.query_map(params![blob, i64::from(k)], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                })?;
                for row in rows {
                    let (frame_id, captured_at, frame_session) = row?;
                    raw += 1;
                    let in_window = captured_at >= start && captured_at < end;
                    let in_sessions = frame_session.is_some_and(|id| session_set.contains(&id));
                    if in_window && in_sessions {
                        in_scope.push(frame_id);
                    }
                }
                Ok((raw, in_scope))
            };
            let mut ids = escalate_in_range_knn(pool, target, fetch)?;
            ids.truncate(pool as usize);
            Ok(ids)
        })
        .await
    }

    /// `frames.session_id` for each of `frame_ids` that belongs to a session (`03 §7i.2`
    /// stage 4). Frames outside any session are absent from the map.
    pub async fn ask_frame_sessions(&self, frame_ids: &[i64]) -> Result<HashMap<i64, i64>> {
        let frame_ids = sorted_ids(frame_ids);
        self.with_conn(move |conn| {
            let mut map = HashMap::new();
            for chunk in frame_ids.chunks(IN_CHUNK) {
                let list = numbered_placeholders(1, chunk.len());
                let sql = format!(
                    "SELECT id, session_id FROM frames
                     WHERE id IN ({list}) AND session_id IS NOT NULL"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    params_from_iter(chunk.iter().copied().map(Value::Integer)),
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
                )?;
                for row in rows {
                    let (frame_id, session_id) = row?;
                    map.insert(frame_id, session_id);
                }
            }
            Ok(map)
        })
        .await
    }

    /// A short display label per requested session for the prompt's session context
    /// (`03 §7i.2` stage 4): trimmed `title`, else `tool`, else `kind`, else a start-instant
    /// discriminator; at most 60 chars and never locally formatted. Sessions that no longer
    /// exist are absent from the map.
    pub async fn ask_session_labels(&self, session_ids: &[i64]) -> Result<HashMap<i64, String>> {
        let session_ids = sorted_ids(session_ids);
        self.with_conn(move |conn| {
            let mut labels = HashMap::new();
            for chunk in session_ids.chunks(IN_CHUNK) {
                let list = numbered_placeholders(1, chunk.len());
                let sql = format!(
                    "SELECT id, title, tool, kind, started_at FROM sessions WHERE id IN ({list})"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    params_from_iter(chunk.iter().copied().map(Value::Integer)),
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )?;
                for row in rows {
                    let (id, title, tool, kind, started_at) = row?;
                    labels.insert(
                        id,
                        session_label(title.as_deref(), tool.as_deref(), &kind, started_at),
                    );
                }
            }
            Ok(labels)
        })
        .await
    }

    // ---- Conversations (`03 §7i.4`) ------------------------------------------------------

    /// Whether an `ask_threads` row exists — the kernel's `ThreadNotFound` check before it
    /// appends a turn to a conversation the caller named.
    pub async fn ask_thread_exists(&self, thread_id: i64) -> Result<bool> {
        self.with_conn(move |conn| {
            let exists: i64 = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM ask_threads WHERE id = ?1)",
                [thread_id],
                |row| row.get(0),
            )?;
            Ok(exists != 0)
        })
        .await
    }

    /// Creates a conversation with a `NULL` title (naming threads is P9's job, `§7i.4`) and
    /// returns its id.
    pub async fn create_ask_thread(&self) -> Result<i64> {
        self.with_conn(|conn| {
            conn.execute("INSERT INTO ask_threads (title) VALUES (NULL)", [])?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    /// Deletes a conversation; `ON DELETE CASCADE` removes its turns and touches no frame,
    /// session, graph row or insight (`§7i.4`). Returns whether a row was removed.
    pub async fn delete_ask_thread(&self, thread_id: i64) -> Result<bool> {
        self.with_conn(move |conn| {
            Ok(conn.execute("DELETE FROM ask_threads WHERE id = ?1", [thread_id])? > 0)
        })
        .await
    }

    /// The highest-ordinal turn of a thread — the follow-up's window and entity source
    /// (`§7i.4`). `None` for an empty conversation.
    pub async fn last_ask_turn(&self, thread_id: i64) -> Result<Option<AskTurnRecord>> {
        self.with_conn(move |conn| {
            let sql = format!(
                "SELECT {ASK_TURN_COLUMNS} FROM ask_turns
                 WHERE thread_id = ?1 ORDER BY ordinal DESC LIMIT 1"
            );
            Ok(conn
                .query_row(&sql, [thread_id], row_to_ask_turn)
                .optional()?)
        })
        .await
    }

    /// Appends a turn at the next `ordinal` with a `NULL` answer (`§7i.4`): the next ordinal is
    /// `MAX(ordinal) + 1` read inside the same transaction as the insert, so two appends for one
    /// thread cannot pick the same value (`UNIQUE(thread_id, ordinal)`). `window` writes both
    /// columns or neither; `resolved_entities` is stored as a JSON array.
    pub async fn append_ask_turn(&self, turn: NewAskTurn) -> Result<AskTurnRecord> {
        let NewAskTurn {
            thread_id,
            question,
            window,
            resolved_entities,
        } = turn;
        let (window_start, window_end) = match window {
            Some(range) => (Some(range.start), Some(range.end)),
            None => (None, None),
        };
        let resolved = serde_json::to_string(&resolved_entities)?;
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let ordinal: i64 = tx.query_row(
                "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM ask_turns WHERE thread_id = ?1",
                [thread_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "INSERT INTO ask_turns
                   (thread_id, ordinal, question, answer, window_start, window_end,
                    resolved_entities)
                 VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6)",
                params![
                    thread_id,
                    ordinal,
                    question,
                    window_start,
                    window_end,
                    resolved
                ],
            )?;
            let turn_id = tx.last_insert_rowid();
            let sql = format!("SELECT {ASK_TURN_COLUMNS} FROM ask_turns WHERE id = ?1");
            let record = tx
                .query_row(&sql, [turn_id], row_to_ask_turn)
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("ask turn {turn_id} vanished after its insert"))?;
            tx.commit()?;
            Ok(record)
        })
        .await
    }

    /// Stores the final answer of a streamed turn. `§7i.4`: the column stays `NULL` while the
    /// answer streams and on failure, so only a completed answer is written. Returns whether a
    /// turn row was updated.
    pub async fn finish_ask_turn(&self, turn_id: i64, answer: &str) -> Result<bool> {
        let answer = answer.to_string();
        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE ask_turns SET answer = ?2 WHERE id = ?1",
                params![turn_id, answer],
            )? > 0)
        })
        .await
    }

    /// Every turn of a thread in `ordinal` order.
    pub async fn list_ask_turns(&self, thread_id: i64) -> Result<Vec<AskTurnRecord>> {
        self.with_conn(move |conn| {
            let sql = format!(
                "SELECT {ASK_TURN_COLUMNS} FROM ask_turns
                 WHERE thread_id = ?1 ORDER BY ordinal"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([thread_id], row_to_ask_turn)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    // ---- Session vectors (`03 §7i.2` stage 2) -------------------------------------------

    /// Sessions whose concatenated non-`NULL` `title`/`summary` has no `session_vectors` row,
    /// most recently updated first. A session with both columns `NULL` (or with an empty
    /// concatenation) is never returned: it is never embedded as an empty string, and the
    /// backfill must not spin on it. Replay is idempotent — the write gate in
    /// [`Self::upsert_session_vector`] re-clears the same predicate.
    pub async fn session_embed_candidates(&self, limit: u32) -> Result<Vec<SessionEmbedInput>> {
        self.with_conn(move |conn| {
            let sql = format!(
                "SELECT id, title, summary FROM sessions
                 WHERE ({SESSION_TEXT_SQL}) <> ''
                   AND id NOT IN (SELECT session_id FROM session_vectors)
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?1"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([i64::from(limit)], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut candidates = Vec::with_capacity(rows.len());
            for (session_id, title, summary) in rows {
                let text = session_text(title.as_deref(), summary.as_deref());
                if !text.is_empty() {
                    candidates.push(SessionEmbedInput { session_id, text });
                }
            }
            Ok(candidates)
        })
        .await
    }

    /// How many sessions still need a `session_vectors` row — the backfill's drain condition.
    pub async fn session_embed_pending(&self) -> Result<u64> {
        self.with_conn(move |conn| {
            let sql = format!(
                "SELECT COUNT(*) FROM sessions
                 WHERE ({SESSION_TEXT_SQL}) <> ''
                   AND id NOT IN (SELECT session_id FROM session_vectors)"
            );
            let count: i64 = conn.query_row(&sql, [], |row| row.get(0))?;
            Ok(count as u64)
        })
        .await
    }

    /// Replaces the `session_vectors` row for `session_id` with `emb` and reports whether a row
    /// was written. A `vec0` table supports neither `ON CONFLICT` nor a delete trigger, so the
    /// replace is a delete-then-insert in one transaction.
    ///
    /// The write is skipped when the session no longer exists, when its current
    /// title/summary concatenation no longer equals `text` (a concurrent summary edit must never
    /// leave a stale vector), and when that concatenation is empty (a session is never embedded
    /// as an empty string). Replaying the same `(session_id, text)` is therefore idempotent.
    pub async fn upsert_session_vector(
        &self,
        session_id: i64,
        text: &str,
        emb: &Embedding,
    ) -> Result<bool> {
        if emb.len() != EMBEDDING_DIM {
            anyhow::bail!(
                "session embedding has {} dims, expected {EMBEDDING_DIM}",
                emb.len()
            );
        }
        let blob = f32_blob(&emb.0);
        let text = text.to_string();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let current: Option<(Option<String>, Option<String>)> = tx
                .query_row(
                    "SELECT title, summary FROM sessions WHERE id = ?1",
                    [session_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((title, summary)) = current else {
                return Ok(false);
            };
            let current_text = session_text(title.as_deref(), summary.as_deref());
            if current_text.is_empty() || current_text != text {
                return Ok(false);
            }
            tx.execute(
                "DELETE FROM session_vectors WHERE session_id = ?1",
                [session_id],
            )?;
            tx.execute(
                "INSERT INTO session_vectors (session_id, embedding) VALUES (?1, ?2)",
                params![session_id, blob],
            )?;
            tx.commit()?;
            Ok(true)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_text_joins_only_the_present_columns() {
        assert_eq!(
            session_text(Some("title"), Some("summary")),
            "title\nsummary"
        );
        assert_eq!(session_text(Some("title"), None), "title");
        assert_eq!(session_text(None, Some("summary")), "summary");
        assert_eq!(session_text(None, None), "");
    }

    #[test]
    fn session_label_prefers_title_then_tool_then_kind_and_stays_short() {
        assert_eq!(
            session_label(Some("  Deploy  "), Some("codex"), "ai", 1),
            "Deploy"
        );
        assert_eq!(
            session_label(Some("   "), Some(" codex "), "ai", 1),
            "codex"
        );
        assert_eq!(session_label(None, None, "focus", 1), "focus");
        assert_eq!(session_label(None, None, "  ", 7), "session 7");
        let long = "x".repeat(MAX_SESSION_LABEL + 10);
        let label = session_label(Some(&long), None, "focus", 1);
        assert_eq!(label.chars().count(), MAX_SESSION_LABEL);
    }

    #[test]
    fn numbered_placeholders_are_explicitly_indexed() {
        assert_eq!(numbered_placeholders(1, 3), "?1,?2,?3");
        assert_eq!(numbered_placeholders(4, 2), "?4,?5");
    }
}
