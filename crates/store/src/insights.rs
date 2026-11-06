//! Insights: the P5 activity aggregate for the Insights screen, and the P8 typed insight
//! engine's store seam (`03 §7j`).
//!
//! [`SqliteStore::insights_summary`] is the original P5 read: total + vision-tagged counts,
//! capture density over time (reusing [`SqliteStore::timeline_buckets`]), the top foreground
//! apps, and the vision activity-type breakdown. The spec defines no Insights contract for
//! that aggregate — the shape and these aggregates are the chosen default (logged in
//! `07_KNOWN_GAPS.md`) — and every number is a real DB aggregate, never fabricated
//! (`UI_REFERENCE §4` honest-empty).
//!
//! Everything below the summary is the 0.4.5 P8 seam (`03 §7j`): the evidenced-row writers
//! (`insert_insight`, `replace_insights`), the reader behind `GET /v1/insights`
//! (`list_insights`), the two evidence-invariant sweeps the kernel runs after a generation
//! pass, the thread/entity reads behind `GET /v1/threads`, `GET /v1/threads/{id}` and
//! `GET /v1/entities` (`03 §7c` P8 block), and the five generator input readers.
//!
//! Invariants every method here keeps:
//! - Windows are unix ms and half-open `[start, end)`; an open session (`ended_at IS NULL`)
//!   extends to the window's end wherever an end is needed.
//! - An insight is written with its evidence in ONE transaction, ordinals `0..n` dense, and
//!   an insight without evidence is never written (`03 §7j.2`).
//! - A row whose stored payload cannot be decoded is an error, never a silent skip.
//! - Rows are read, never repaired: `stale` is set by the sweep, and the next pass replaces
//!   the row (`03 §7j.5`, `07` C10).

use std::collections::{HashMap, HashSet};

use rusqlite::{named_params, params, params_from_iter, Connection, OptionalExtension};
use traits::{
    ActivityCount, AppCount, EntityFilter, EntitySessionInput, EntitySummary, EvidenceKind,
    Insight, InsightEvidence, InsightFilter, InsightGenerator, InsightKind, InsightPayload,
    InsightsSummary, NewInsight, OpenMarkInput, Result, SessionNeighbourInput, ThreadActivityInput,
    ThreadDetail, ThreadEntityRef, ThreadFilter, ThreadSessionInput, ThreadSessionRef,
    ThreadSummary,
};

use crate::embeddings::SQLITE_VEC_MAX_K;
use crate::SqliteStore;

/// Rows returned in the top-apps / activity breakdowns (a readable Insights list,
/// not the full long tail).
const TOP_N: i64 = 12;

/// Row shape of [`SqliteStore::list_insights`]: the persisted columns up to `generated_at`.
/// `stale` is not among them on purpose — [`effective_stale_sql`] computes it and the read
/// selects it last, so [`decode_insight_row`]'s indices stay fixed.
const INSIGHT_COLUMNS: &str = "id, kind, window_start, window_end, payload, generator,
                               generated_at";

/// The `stale` flag a reader gets: already flagged, **or** citing something that has gone.
///
/// Reading the stored column alone reports a claim as fresh until the next generation pass sweeps
/// it, and that pass waits for idle at throttle level 0 — so a reader could be handed a claim
/// whose session was deleted hours ago. Computing it here makes the flag honest at every read
/// without turning a GET into a write.
fn effective_stale_sql() -> String {
    format!(
        "(CASE
            WHEN insights.stale = 1 THEN 1
            WHEN EXISTS (
                SELECT 1 FROM insight_evidence ev
                WHERE ev.insight_id = insights.id
                  AND {EVIDENCE_MISSING_SQL}
            ) THEN 1
            ELSE 0
        END)"
    )
}

/// Chunk size for `IN (…)` reads. The neighbour map and the entity/thread joins stay small,
/// but a long history can name thousands of session ids and SQLite's bound-parameter ceiling
/// is finite, so ids are read in bounded batches.
const READ_CHUNK: usize = 500;

/// SQL fragment: this `insight_evidence` row (`ev`) names a target that no longer exists.
///
/// Evidence carries only a kind and an integer, so nothing enforces that the target survives —
/// a deleted session, a frame dropped by retention, a thread erased by a re-cluster all leave the
/// reference dangling. Two callers must agree on what that means, and this is the single copy:
/// [`SqliteStore::mark_insights_stale_where_evidence_missing`] writes the flag with it, and
/// [`SqliteStore::list_insights`] computes the flag it returns with it.
///
/// `report` evidence is deferred ('06' #96: `ref_id` is an integer, `report_runs.run_id` is a
/// UUID), so no such row is ever emitted; if one ever were, it could not be resolved and is
/// therefore missing rather than silently trusted.
const EVIDENCE_MISSING_SQL: &str = "((ev.kind = 'frame'
        AND NOT EXISTS (SELECT 1 FROM frames f WHERE f.id = ev.ref_id))
    OR (ev.kind = 'session'
        AND NOT EXISTS (SELECT 1 FROM sessions s WHERE s.id = ev.ref_id))
    OR (ev.kind = 'thread'
        AND NOT EXISTS (SELECT 1 FROM threads t WHERE t.id = ev.ref_id))
    OR (ev.kind = 'entity'
        AND NOT EXISTS (SELECT 1 FROM entities e WHERE e.id = ev.ref_id))
    OR (ev.kind = 'mark'
        AND NOT EXISTS (SELECT 1 FROM marks m WHERE m.id = ev.ref_id))
    OR ev.kind = 'report')";

impl SqliteStore {
    /// Aggregates frame activity over the half-open window `[start, end)`. Returns an
    /// empty summary (all zeros / empty lists) when the window holds no frames.
    pub async fn insights_summary(
        &self,
        start: i64,
        end: i64,
        bucket_count: u32,
    ) -> Result<InsightsSummary> {
        // Invalid or unrepresentable window → honest-empty summary up front, skipping
        // four queries (and a `timeline_buckets` call) that would all return
        // zero/empty anyway.
        if end <= start || end.checked_sub(start).is_none() || bucket_count == 0 {
            return Ok(InsightsSummary::default());
        }
        let captures = self.timeline_buckets(start, end, bucket_count).await?;
        self.with_conn(move |conn| {
            let total_frames: i64 = conn.query_row(
                "SELECT COUNT(*) FROM frames WHERE captured_at >= ?1 AND captured_at < ?2",
                params![start, end],
                |r| r.get(0),
            )?;
            let tagged_frames: i64 = conn.query_row(
                "SELECT COUNT(*) FROM frames
                 WHERE captured_at >= ?1 AND captured_at < ?2 AND activity_type IS NOT NULL",
                params![start, end],
                |r| r.get(0),
            )?;

            // Top foreground apps by frame count. A NULL `app_hint` groups into one
            // `app: None` row (frames with no resolved foreground window) — truthful,
            // labelled by the UI; ties break by name for a stable order.
            let mut app_stmt = conn.prepare(
                "SELECT app_hint, COUNT(*) AS n FROM frames
                 WHERE captured_at >= ?1 AND captured_at < ?2
                 GROUP BY app_hint ORDER BY n DESC, app_hint LIMIT ?3",
            )?;
            let top_apps = app_stmt
                .query_map(params![start, end, TOP_N], |r| {
                    Ok(AppCount {
                        app: r.get::<_, Option<String>>(0)?,
                        count: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(u32::MAX),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            // Vision activity-type breakdown — only frames that have been tagged
            // (untagged frames are excluded so the breakdown isn't dominated by NULL).
            let mut act_stmt = conn.prepare(
                "SELECT activity_type, COUNT(*) AS n FROM frames
                 WHERE captured_at >= ?1 AND captured_at < ?2 AND activity_type IS NOT NULL
                 GROUP BY activity_type ORDER BY n DESC, activity_type LIMIT ?3",
            )?;
            let activity_breakdown = act_stmt
                .query_map(params![start, end, TOP_N], |r| {
                    Ok(ActivityCount {
                        activity: r.get::<_, Option<String>>(0)?,
                        count: u32::try_from(r.get::<_, i64>(1)?).unwrap_or(u32::MAX),
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;

            Ok(InsightsSummary {
                total_frames,
                tagged_frames,
                captures,
                top_apps,
                activity_breakdown,
            })
        })
        .await
    }

    // ---- Writing evidenced insights (`03 §7j.2`, `§7j.5`) ------------------------------

    /// Inserts one insight with its evidence in ONE transaction and returns the new
    /// `insights.id`.
    ///
    /// `insight.evidence` must be non-empty (`03 §7j.2`) and its ordinals must be exactly
    /// `0..n` in list order; either violation is an error and **no row is written** — the
    /// checks run before the transaction opens, so a refused call cannot leave a half-written
    /// row behind. `generated_at` takes the DDL default (the DB's clock).
    pub async fn insert_insight(&self, insight: NewInsight) -> Result<i64> {
        validate_evidence(&insight.evidence)?;
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let insight_id = write_insight(&tx, &insight)?;
            tx.commit()?;
            Ok(insight_id)
        })
        .await
    }

    /// Replace semantics for one generation pass (`03 §7j.5`): in ONE transaction, delete
    /// every insight of `kind` whose window overlaps `[window_start, window_end)` (its
    /// evidence cascades away) and insert `batch`, returning the new ids in batch order.
    ///
    /// The whole batch is validated first — every entry non-empty-evidenced with dense
    /// ordinals, and every payload stating `kind` — so a rejected batch deletes nothing and
    /// inserts nothing. A regeneration therefore replaces rows; it never mutates one in place
    /// under a reader.
    pub async fn replace_insights(
        &self,
        kind: InsightKind,
        window_start: i64,
        window_end: i64,
        batch: Vec<NewInsight>,
    ) -> Result<Vec<i64>> {
        for insight in &batch {
            validate_evidence(&insight.evidence)?;
            // The row's `kind` column and its payload tag are written together and must
            // agree (`03 §7j.5`); a payload of another kind would make the row read back as
            // a different insight than the pass replaced.
            if insight.payload.kind() != kind {
                anyhow::bail!("insight batch holds a payload that is not the replaced kind");
            }
        }
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "DELETE FROM insights
                 WHERE kind = ?1 AND window_start < ?2 AND window_end > ?3",
                params![kind.as_str(), window_end, window_start],
            )?;
            let mut ids = Vec::with_capacity(batch.len());
            for insight in &batch {
                ids.push(write_insight(&tx, insight)?);
            }
            tx.commit()?;
            Ok(ids)
        })
        .await
    }

    // ---- Reading insights (`03 §7c` P8 block) ------------------------------------------

    /// Insights whose window overlaps `filter`'s (`window_start < to AND window_end > from`;
    /// a missing bound leaves that side open), each with its evidence ordered by `ordinal`,
    /// ordered `window_start DESC, kind ASC, id DESC`.
    ///
    /// A stored payload that does not decode — or that states a different kind than the row's
    /// `insights.kind` column — is an error: a row is the truth of what was generated, and
    /// skipping it would silently hide a real insight.
    ///
    /// `stale` is computed per row rather than read from the column: a row already flagged, or
    /// citing a target that no longer exists, comes back stale ([`effective_stale_sql`]). The
    /// generation sweep writes the same flag, but it only runs on an idle pass, so a reader
    /// cannot rely on it having run since a session, frame, thread, entity or mark went away.
    pub async fn list_insights(&self, filter: InsightFilter) -> Result<Vec<Insight>> {
        self.with_conn(move |conn| {
            let kind = filter.kind.map(InsightKind::as_str);
            let mut stmt = conn.prepare(&format!(
                "SELECT {INSIGHT_COLUMNS}, {} AS stale FROM insights
                 WHERE (:from IS NULL OR window_end > :from)
                   AND (:to IS NULL OR window_start < :to)
                   AND (:kind IS NULL OR kind = :kind)
                 ORDER BY window_start DESC, kind ASC, id DESC
                 LIMIT :limit",
                effective_stale_sql()
            ))?;
            let rows = stmt.query_map(
                named_params! {
                    ":from": filter.from_ms,
                    ":to": filter.to_ms,
                    ":kind": kind,
                    ":limit": i64::from(filter.limit),
                },
                decode_insight_row,
            )?;
            let mut raw = Vec::new();
            for row in rows {
                raw.push(row?);
            }
            let mut insights = Vec::with_capacity(raw.len());
            for row in raw {
                insights.push(decode_insight(conn, row)?);
            }
            Ok(insights)
        })
        .await
    }

    /// Ids of insights with no `insight_evidence` row — the kernel's post-pass invariant
    /// check (`03 §7j.2`). An empty result is the healthy state.
    pub async fn unevidenced_insight_ids(&self) -> Result<Vec<i64>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM insights
                 WHERE id NOT IN (SELECT insight_id FROM insight_evidence)
                 ORDER BY id",
            )?;
            let ids = stmt
                .query_map([], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(ids)
        })
        .await
    }

    /// Marks `stale=1` every insight with at least one evidence row whose referenced row no
    /// longer exists (`03 §7j.5`, `07` C10) and returns how many rows were **newly** marked.
    ///
    /// `frame`, `session`, `thread`, `entity` and `mark` refs are checked; `report` refs are
    /// skipped because no generator emits one (P8 opens loops from unresolved marks and
    /// abandoned threads only). The `stale = 0` guard makes the count a true "changed
    /// 0→1" figure, so a second sweep reports zero.
    pub async fn mark_insights_stale_where_evidence_missing(&self) -> Result<u64> {
        self.with_conn(move |conn| {
            let marked = conn.execute(
                &format!(
                    "UPDATE insights SET stale = 1
                     WHERE stale = 0
                       AND EXISTS (
                         SELECT 1 FROM insight_evidence ev
                         WHERE ev.insight_id = insights.id
                           AND {EVIDENCE_MISSING_SQL}
                       )"
                ),
                [],
            )?;
            Ok(marked as u64)
        })
        .await
    }

    // ---- Threads and entities (`03 §7c` P8 block, `§7h.4`) ----------------------------

    /// Threads overlapping `filter`'s window (`first_seen_at < to AND last_seen_at >= from`;
    /// a missing bound leaves that side open), ordered `last_seen_at DESC, id DESC`.
    /// `session_count` is the persisted figure the clusterer maintains, never recomputed here.
    pub async fn threads_in_range(&self, filter: ThreadFilter) -> Result<Vec<ThreadSummary>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, title, first_seen_at, last_seen_at, session_count
                 FROM threads
                 WHERE (:from IS NULL OR last_seen_at >= :from)
                   AND (:to IS NULL OR first_seen_at < :to)
                 ORDER BY last_seen_at DESC, id DESC
                 LIMIT :limit",
            )?;
            let rows = stmt.query_map(
                named_params! {
                    ":from": filter.from_ms,
                    ":to": filter.to_ms,
                    ":limit": i64::from(filter.limit),
                },
                |row| {
                    Ok(ThreadSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        first_seen_at: row.get(2)?,
                        last_seen_at: row.get(3)?,
                        session_count: count_u32(row.get::<_, i64>(4)?),
                    })
                },
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// One thread with its member sessions (ordered by `started_at` ascending, then
    /// `session_id`) and the entities those sessions touch (ordered by `confidence`
    /// descending, then `canonical_name`, then id), or `None` when the thread does not exist.
    ///
    /// `ThreadEntityRef.session_count` counts the thread's member sessions touching the
    /// entity and `confidence` is the maximum over exactly those rows, so a thread's entity
    /// list never borrows weight from sessions outside it.
    pub async fn thread_detail(&self, thread_id: i64) -> Result<Option<ThreadDetail>> {
        self.with_conn(move |conn| {
            let thread = conn
                .query_row(
                    "SELECT id, title, first_seen_at, last_seen_at, session_count
                     FROM threads WHERE id = ?1",
                    [thread_id],
                    |row| {
                        Ok(ThreadSummary {
                            id: row.get(0)?,
                            title: row.get(1)?,
                            first_seen_at: row.get(2)?,
                            last_seen_at: row.get(3)?,
                            session_count: count_u32(row.get::<_, i64>(4)?),
                        })
                    },
                )
                .optional()?;
            let Some(thread) = thread else {
                return Ok(None);
            };

            let sessions = {
                let mut stmt = conn.prepare(
                    "SELECT ts.session_id, s.started_at, s.ended_at, s.kind, s.title,
                            ts.confidence
                     FROM thread_sessions ts
                     JOIN sessions s ON s.id = ts.session_id
                     WHERE ts.thread_id = ?1
                     ORDER BY s.started_at, ts.session_id",
                )?;
                let rows = stmt.query_map([thread_id], |row| {
                    Ok(ThreadSessionRef {
                        session_id: row.get(0)?,
                        started_at: row.get(1)?,
                        ended_at: row.get(2)?,
                        kind: row.get(3)?,
                        title: row.get(4)?,
                        confidence: row.get(5)?,
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            let entities = {
                let mut stmt = conn.prepare(
                    "SELECT e.id, e.kind, e.canonical_name, COUNT(*) AS session_count,
                            MAX(se.confidence) AS confidence
                     FROM thread_sessions ts
                     JOIN session_entities se ON se.session_id = ts.session_id
                     JOIN entities e ON e.id = se.entity_id
                     WHERE ts.thread_id = ?1
                     GROUP BY e.id, e.kind, e.canonical_name
                     ORDER BY confidence DESC, e.canonical_name, e.id",
                )?;
                let rows = stmt.query_map([thread_id], |row| {
                    Ok(ThreadEntityRef {
                        entity_id: row.get(0)?,
                        kind: row.get(1)?,
                        canonical_name: row.get(2)?,
                        session_count: count_u32(row.get::<_, i64>(3)?),
                        confidence: row.get(4)?,
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };

            Ok(Some(ThreadDetail {
                thread,
                sessions,
                entities,
            }))
        })
        .await
    }

    /// Entities filtered by an exact `kind` token and by a case-insensitive substring over
    /// `canonical_name` or any alias, ordered `last_seen_at DESC, id DESC`.
    ///
    /// The matter is matched with `instr(lower(haystack), lower(needle)) > 0`, so `%`, `_`
    /// and `\` are literal characters — no LIKE, no glob, no FTS. **SQLite's `lower()` folds
    /// ASCII only** (the P6 finding behind `ask_resolve_entities`' Rust-side lowering): both
    /// sides are folded the same way, so the comparison is consistently ASCII-case-insensitive
    /// and is *not* Unicode-case-insensitive. This is documented rather than papered over —
    /// a Unicode-correct prefilter would need Rust-side folding over every candidate row.
    ///
    /// `EntitySummary.session_count` is the entity's all-history `session_entities` count;
    /// `EntityFilter` carries no time window, so nothing narrows it.
    pub async fn entities_filtered(&self, filter: EntityFilter) -> Result<Vec<EntitySummary>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT e.id, e.kind, e.canonical_name, e.first_seen_at, e.last_seen_at,
                        (SELECT COUNT(*) FROM session_entities se WHERE se.entity_id = e.id)
                 FROM entities e
                 WHERE (:kind IS NULL OR e.kind = :kind)
                   AND (:query IS NULL
                        OR instr(lower(e.canonical_name), lower(:query)) > 0
                        OR EXISTS (SELECT 1 FROM entity_aliases a
                                   WHERE a.entity_id = e.id
                                     AND instr(lower(a.alias), lower(:query)) > 0))
                 ORDER BY e.last_seen_at DESC, e.id DESC
                 LIMIT :limit",
            )?;
            let rows = stmt.query_map(
                named_params! {
                    ":kind": filter.kind,
                    ":query": filter.query,
                    ":limit": i64::from(filter.limit),
                },
                |row| {
                    Ok(EntitySummary {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        canonical_name: row.get(2)?,
                        first_seen_at: row.get(3)?,
                        last_seen_at: row.get(4)?,
                        session_count: count_u32(row.get::<_, i64>(5)?),
                        aliases: Vec::new(),
                    })
                },
            )?;
            let mut entities = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            let mut alias_stmt =
                conn.prepare("SELECT alias FROM entity_aliases WHERE entity_id = ?1 ORDER BY id")?;
            for entity in &mut entities {
                entity.aliases = alias_stmt
                    .query_map([entity.id], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
            }
            Ok(entities)
        })
        .await
    }

    // ---- Generator inputs (`03 §7j.1`) ------------------------------------------------

    /// `thread_activity` input: one row per thread × member session whose interval overlaps
    /// the window, ordered `thread_id` then the session's `started_at`.
    ///
    /// Overlap is `started_at < window_end AND COALESCE(ended_at, window_end) > window_start`,
    /// so an open session extends to `window_end`. `ended_at` is returned as stored (`None`
    /// while open) and the generator applies the same `window_end` substitution when it sums.
    pub async fn insight_thread_activity(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Vec<ThreadActivityInput>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.title, s.id, s.started_at, s.ended_at
                 FROM threads t
                 JOIN thread_sessions ts ON ts.thread_id = t.id
                 JOIN sessions s ON s.id = ts.session_id
                 WHERE s.started_at < :window_end
                   AND COALESCE(s.ended_at, :window_end) > :window_start
                 ORDER BY t.id, s.started_at, s.id",
            )?;
            let rows = stmt.query_map(
                named_params! { ":window_start": window_start, ":window_end": window_end },
                |row| {
                    Ok(ThreadActivityInput {
                        thread_id: row.get(0)?,
                        title: row.get(1)?,
                        session_id: row.get(2)?,
                        started_at: row.get(3)?,
                        ended_at: row.get(4)?,
                    })
                },
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// `open_loop` input: every mark still open (`resolved_at IS NULL`) created before
    /// `window_end`, ordered `created_at` then id (`03 §7j.1`). A resolved mark is no longer a
    /// loop and a later mark belongs to a later window, so neither is read.
    pub async fn insight_open_marks(&self, window_end: i64) -> Result<Vec<OpenMarkInput>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, frame_id, created_at, note FROM marks
                 WHERE resolved_at IS NULL AND created_at < ?1
                 ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([window_end], |row| {
                Ok(OpenMarkInput {
                    mark_id: row.get(0)?,
                    frame_id: row.get(1)?,
                    created_at: row.get(2)?,
                    note: row.get(3)?,
                })
            })?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// `recurrence` input: `session_entities` joined to `sessions` and `entities` for the
    /// sessions overlapping the window, ordered `entity_id` then the session's `started_at`
    /// (`03 §7j.1`, `§7h.4`). `last_seen_at` is the entity's own stored value — the generator
    /// timestamps a recurrence from the session rows, never from it.
    pub async fn insight_entity_sessions(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Vec<EntitySessionInput>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT e.id, e.kind, e.canonical_name, s.id, s.started_at, e.last_seen_at
                 FROM session_entities se
                 JOIN sessions s ON s.id = se.session_id
                 JOIN entities e ON e.id = se.entity_id
                 WHERE s.started_at < :window_end
                   AND COALESCE(s.ended_at, :window_end) > :window_start
                 ORDER BY e.id, s.started_at, s.id",
            )?;
            let rows = stmt.query_map(
                named_params! { ":window_start": window_start, ":window_end": window_end },
                |row| {
                    Ok(EntitySessionInput {
                        entity_id: row.get(0)?,
                        entity_kind: row.get(1)?,
                        canonical_name: row.get(2)?,
                        session_id: row.get(3)?,
                        session_started_at: row.get(4)?,
                        last_seen_at: row.get(5)?,
                    })
                },
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// `semantic_neighbour` input: for every session overlapping the window that has a
    /// `session_vectors` row, the `k` nearest other sessions by cosine distance (`03 §7j.1`,
    /// `§7i`), one row per neighbour, ordered by subject `session_id` then distance ascending.
    ///
    /// The KNN asks `vec0` for `k + 1` (`WHERE embedding MATCH … AND k = …`, clamped to
    /// sqlite-vec's ceiling) and drops the self-match, so `k` genuinely means "k neighbours".
    /// Rows come back nearest-first with the neighbour id breaking distance ties. vec0 returns
    /// only its `k` nearest and picks arbitrarily among exactly-equidistant rows, so the
    /// tie-break orders the returned set — it cannot choose a tied row vec0 did not return.
    /// A vector row and its `sessions` row are written and deleted together, so a neighbour
    /// with no `sessions` row can only be corruption; such an edge is dropped rather than
    /// rendered with invented display data.
    pub async fn insight_session_neighbours(
        &self,
        window_start: i64,
        window_end: i64,
        k: u32,
    ) -> Result<Vec<SessionNeighbourInput>> {
        if k == 0 {
            return Ok(Vec::new());
        }
        let fetch_k = k.saturating_add(1).min(SQLITE_VEC_MAX_K);
        let per_subject = usize::try_from(k).unwrap_or(usize::MAX);
        self.with_conn(move |conn| {
            let subjects: Vec<(i64, Option<String>, i64)> = {
                let mut stmt = conn.prepare(
                    "SELECT s.id, s.title, s.started_at
                     FROM sessions s
                     WHERE s.started_at < :window_end
                       AND COALESCE(s.ended_at, :window_end) > :window_start
                       AND s.id IN (SELECT session_id FROM session_vectors)
                     ORDER BY s.id",
                )?;
                let rows = stmt.query_map(
                    named_params! { ":window_start": window_start, ":window_end": window_end },
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            if subjects.is_empty() {
                return Ok(Vec::new());
            }

            // One KNN per subject, nearest first, self dropped.
            let mut edges: HashMap<i64, Vec<(i64, f64)>> = HashMap::new();
            {
                let mut vector_of =
                    conn.prepare("SELECT embedding FROM session_vectors WHERE session_id = ?1")?;
                let mut knn = conn.prepare(
                    "SELECT session_id, distance FROM session_vectors
                     WHERE embedding MATCH ?1 AND k = ?2
                     ORDER BY distance",
                )?;
                for (subject_id, _, _) in &subjects {
                    let blob: Vec<u8> = vector_of.query_row([subject_id], |row| row.get(0))?;
                    let rows = knn.query_map(params![blob, i64::from(fetch_k)], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, f64>(1)?))
                    })?;
                    let nearest = edges.entry(*subject_id).or_default();
                    for row in rows {
                        let (neighbour_id, distance) = row?;
                        if neighbour_id == *subject_id {
                            continue;
                        }
                        nearest.push((neighbour_id, distance));
                    }
                    // vec0 orders by distance but leaves equidistant rows in an unspecified
                    // order, and which of them fit inside `k` must not depend on that: sort
                    // nearest-first with the neighbour id as the tie-break, then keep `k`.
                    nearest.sort_by(|(left_id, left), (right_id, right)| {
                        left.total_cmp(right).then(left_id.cmp(right_id))
                    });
                    nearest.truncate(per_subject);
                }
            }

            // Both sides of every edge, so titles/timestamps come from `sessions` (vec0 keeps
            // no display data). A subject's own row is already known, but reading it in the
            // same batch keeps one code path for both sides.
            let mut wanted: HashSet<i64> = subjects.iter().map(|(id, _, _)| *id).collect();
            for nearest in edges.values() {
                wanted.extend(nearest.iter().map(|(id, _)| *id));
            }
            let mut wanted: Vec<i64> = wanted.into_iter().collect();
            wanted.sort_unstable();
            let mut session_data: HashMap<i64, (Option<String>, i64)> = HashMap::new();
            for chunk in wanted.chunks(READ_CHUNK) {
                let list = placeholders(chunk.len());
                let sql =
                    format!("SELECT id, title, started_at FROM sessions WHERE id IN ({list})");
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params_from_iter(chunk.iter().copied()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?;
                for row in rows {
                    let (session_id, title, started_at) = row?;
                    session_data.insert(session_id, (title, started_at));
                }
            }

            let mut out = Vec::new();
            for (subject_id, subject_title, subject_started_at) in &subjects {
                let Some(nearest) = edges.remove(subject_id) else {
                    continue;
                };
                for (neighbour_id, distance) in nearest {
                    let Some((neighbour_title, neighbour_started_at)) =
                        session_data.get(&neighbour_id)
                    else {
                        continue;
                    };
                    out.push(SessionNeighbourInput {
                        session_id: *subject_id,
                        session_title: subject_title.clone(),
                        session_started_at: *subject_started_at,
                        neighbour_session_id: neighbour_id,
                        neighbour_title: neighbour_title.clone(),
                        neighbour_started_at: *neighbour_started_at,
                        distance,
                    });
                }
            }
            Ok(out)
        })
        .await
    }

    /// `resumption` input: every `thread_sessions` row of the threads overlapping the window,
    /// joined to the session's own fields, ordered `thread_id` then `started_at`
    /// (`03 §7j.1`, `§7h.4`).
    ///
    /// All member rows are returned, not only in-window ones: the *previous* session of a
    /// resumption is typically older than the window, so filtering by session would hide the
    /// gap the generator must measure. Session overlap is judged by the thread's persisted
    /// `[first_seen_at, last_seen_at]` interval, the same predicate [`Self::threads_in_range`]
    /// uses.
    pub async fn insight_thread_sessions(
        &self,
        window_start: i64,
        window_end: i64,
    ) -> Result<Vec<ThreadSessionInput>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT t.id, t.title, s.id, s.started_at, s.ended_at, s.title, s.summary
                 FROM threads t
                 JOIN thread_sessions ts ON ts.thread_id = t.id
                 JOIN sessions s ON s.id = ts.session_id
                 WHERE t.first_seen_at < :window_end AND t.last_seen_at >= :window_start
                 ORDER BY t.id, s.started_at, s.id",
            )?;
            let rows = stmt.query_map(
                named_params! { ":window_start": window_start, ":window_end": window_end },
                |row| {
                    Ok(ThreadSessionInput {
                        thread_id: row.get(0)?,
                        title: row.get(1)?,
                        session_id: row.get(2)?,
                        started_at: row.get(3)?,
                        ended_at: row.get(4)?,
                        session_title: row.get(5)?,
                        summary: row.get(6)?,
                    })
                },
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }
}

// ---- Shared helpers ---------------------------------------------------------------------

/// Enforces the evidence rule every writer shares (`03 §7j.2`): at least one reference, and
/// ordinals exactly `0..n` in list order. Runs before a transaction opens, so a violation
/// cannot leave a row behind.
fn validate_evidence(evidence: &[InsightEvidence]) -> Result<()> {
    if evidence.is_empty() {
        anyhow::bail!("an insight requires at least one evidence row");
    }
    for (position, item) in evidence.iter().enumerate() {
        let expected = u32::try_from(position)
            .map_err(|_| anyhow::anyhow!("insight evidence list is longer than a u32 ordinal"))?;
        if item.ordinal != expected {
            anyhow::bail!("insight evidence ordinals must be dense and ascending from 0");
        }
    }
    Ok(())
}

/// Writes one insight row plus its evidence into `tx`. The caller owns the transaction and
/// the evidence validation, so this is only the two writes.
fn write_insight(tx: &Connection, insight: &NewInsight) -> Result<i64> {
    let payload = serde_json::to_string(&insight.payload)
        .map_err(|error| anyhow::anyhow!("insight payload does not serialize: {error}"))?;
    tx.execute(
        "INSERT INTO insights (kind, window_start, window_end, payload, generator)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            insight.payload.kind().as_str(),
            insight.window_start,
            insight.window_end,
            payload,
            insight.generator.as_str()
        ],
    )?;
    let insight_id = tx.last_insert_rowid();
    let mut evidence = tx.prepare(
        "INSERT INTO insight_evidence (insight_id, ordinal, kind, ref_id)
         VALUES (?1, ?2, ?3, ?4)",
    )?;
    for item in &insight.evidence {
        evidence.execute(params![
            insight_id,
            item.ordinal,
            item.kind.as_str(),
            item.ref_id
        ])?;
    }
    Ok(insight_id)
}

/// One raw `insights` row, tokens and payload still unparsed.
struct RawInsight {
    id: i64,
    kind: String,
    window_start: i64,
    window_end: i64,
    payload: String,
    generator: String,
    generated_at: i64,
    stale: bool,
}

/// Maps a row selected with [`INSIGHT_COLUMNS`] plus the computed `stale` flag as its last
/// column (the order [`SqliteStore::list_insights`] selects).
fn decode_insight_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawInsight> {
    Ok(RawInsight {
        id: row.get(0)?,
        kind: row.get(1)?,
        window_start: row.get(2)?,
        window_end: row.get(3)?,
        payload: row.get(4)?,
        generator: row.get(5)?,
        generated_at: row.get(6)?,
        stale: row.get(7)?,
    })
}

/// Turns a [`RawInsight`] into the read shape, decoding both closed-set tokens, the typed
/// payload (and its `kind` tag, which must agree with the row) and the evidence in `ordinal`
/// order. Errors carry no ids: a persisted id is opaque and these messages reach the job log.
fn decode_insight(conn: &Connection, raw: RawInsight) -> Result<Insight> {
    let kind = InsightKind::from_token(&raw.kind)
        .ok_or_else(|| anyhow::anyhow!("insights row has a kind outside the closed set"))?;
    let generator = InsightGenerator::from_token(&raw.generator)
        .ok_or_else(|| anyhow::anyhow!("insights row has a generator outside the closed set"))?;
    let payload: InsightPayload = serde_json::from_str(&raw.payload)
        .map_err(|error| anyhow::anyhow!("insights row payload does not decode: {error}"))?;
    if payload.kind() != kind {
        anyhow::bail!("insights row payload states a different kind than the row");
    }
    let evidence = {
        let mut stmt = conn.prepare(
            "SELECT ordinal, kind, ref_id FROM insight_evidence
             WHERE insight_id = ?1 ORDER BY ordinal",
        )?;
        let rows = stmt.query_map([raw.id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let mut evidence = Vec::new();
        for row in rows {
            let (ordinal, kind, ref_id) = row?;
            let ordinal = u32::try_from(ordinal)
                .map_err(|_| anyhow::anyhow!("insight evidence ordinal is not a u32"))?;
            let kind = EvidenceKind::from_token(&kind).ok_or_else(|| {
                anyhow::anyhow!("insight evidence kind is outside the closed set")
            })?;
            evidence.push(InsightEvidence {
                ordinal,
                kind,
                ref_id,
            });
        }
        evidence
    };
    Ok(Insight {
        id: raw.id,
        kind,
        window_start: raw.window_start,
        window_end: raw.window_end,
        payload,
        generator,
        generated_at: raw.generated_at,
        stale: raw.stale,
        evidence,
    })
}

/// A stored `COUNT(*)` as a `u32` report figure, saturating at the type's maximum (the same
/// idiom the P5 aggregates above use: a count that cannot fit is still a truthful "many").
fn count_u32(count: i64) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// `?,?,?` for an `IN (…)` list of `count` bound parameters.
fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}
