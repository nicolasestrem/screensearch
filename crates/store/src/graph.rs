//! Work-graph persistence (`03 §7h`, 0.4.5 P5): SQL over the six graph tables in
//! `03 §4` plus the small read/write surface the kernel's graph jobs need.
//!
//! The graph is derived-but-persisted: every row here is recomputable from frames,
//! text and sessions, and deleting these tables loses no evidence. All graph-table
//! SQL lives in this module (`crates/store/src/lib.rs` only forwards the trait
//! methods), so the six tables have exactly one writer/reader module.
//!
//! Determinism and idempotency rules implemented here:
//!
//! - `entities` dedup on `UNIQUE(kind, normalized_key)`. The upsert widens the seen
//!   window and raises the confidence but never rewrites `canonical_name`,
//!   `normalized_key` or `source` (`03 §7h.2`: the display form is the first hit that
//!   created the row; a rule's own confidence is never smoothed).
//! - `entity_aliases` are additive: `ON CONFLICT(entity_id, alias) DO NOTHING`.
//! - `session_entities` upserts on `(session_id, entity_id)`: `hit_count` grows by one
//!   per processed frame (a frame contributes at most once per entity), confidence is
//!   the maximum, and the seen window is min/max (`03 §7h.3`).
//! - Frame-to-session resolution for linking is **ownership-only** (`03 §7e`, final ruling
//!   on the `§7h.3` conflict recorded in `06` #41): the frame's `session_id` is the only
//!   source of a session, and the interval rule survives solely as the `graph_session_at`
//!   primitive, which extraction does not use.
//! - `graph_replace_session_links` is the exact-rebuild sibling of that incremental path:
//!   one session's links are replaced with absolute aggregates (so an ownership
//!   reconciliation can never inflate `hit_count`), entities are widened identically,
//!   and an unchanged link set writes only any missing additive aliases.
//! - `threads` upsert by the `UNIQUE` `cluster_key` and keep their existing id, so a
//!   re-run of the deterministic clusterer rewrites the same rows instead of creating
//!   new ones (`03 §7h.4`).
//!
//! Every write method runs in exactly one transaction (large inputs use bounded
//! chunk sizes inside that transaction, matching the rest of the store), so an
//! interrupted backfill always leaves a consistent graph (`03 §7h.6`).
//!
//! `graph_frame_input` reads the frame's filtered `frame_text.content_text`. The v15
//! baseline has no legacy `ocr_text` table (dropped in v3, `07` #51), so a frame
//! without a `frame_text` row reports `None` content text rather than falling back to
//! a table that does not exist.

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::{
    named_params, params, params_from_iter, types::Value, Connection, OptionalExtension,
    Transaction,
};

use traits::{
    AliasSource, ConnectedThread, EntityCatalogRow, EntityKind, GraphBackfillStatus,
    GraphClusterInput, GraphClusterLink, GraphFrameInput, GraphHit, GraphProgress,
    GraphSessionInput, GraphSessionInterval, GraphSessionLinkAggregate, GraphTableCounts,
    NormalizationOutcome, NormalizationProposal, Result, SessionHost, SessionSummaryInput,
    StoredThread, ThreadDraft, ThreadMemberDraft, ThreadOverrideDraft, ThreadOverrideKind,
    ThreadOverridePayload, ThreadOverrideRow, ThreadPlan, ThreadSummary,
};

use crate::insights::{decode_thread_summary, THREAD_SUMMARY_SELECT};
use crate::SqliteStore;

/// Row bound for one multi-row statement. Bounded chunks keep every statement small;
/// the whole plan still commits atomically in one transaction.
const WRITE_CHUNK: usize = 400;

/// The queue's queued-or-running counts per graph job kind (`03 §7h.6`), read once with one
/// grouped statement.
///
/// This is the *only* graph-progress state there is: no counter is kept anywhere else, so
/// [`SqliteStore::graph_backfill_status`] and [`SqliteStore::graph_progress`] both read it and
/// cannot disagree about the backlog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct GraphPendingJobs {
    extract: u64,
    backfill: u64,
    cluster: u64,
    normalize: u64,
}

impl GraphPendingJobs {
    /// `true` while any of the four graph kinds still has work — the "graph is running" flag
    /// ([`GraphProgress::running`]).
    fn any(&self) -> bool {
        self.extract + self.backfill + self.cluster + self.normalize > 0
    }
}

/// Reads [`GraphPendingJobs`] from the jobs table.
fn graph_pending_jobs(conn: &Connection) -> rusqlite::Result<GraphPendingJobs> {
    let mut pending = GraphPendingJobs::default();
    let mut stmt = conn.prepare(
        "SELECT kind, COUNT(*) FROM jobs
         WHERE state IN ('pending', 'running')
           AND kind IN ('graph_extract', 'graph_backfill', 'graph_cluster',
                        'graph_normalize')
         GROUP BY kind",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    for row in rows {
        let (kind, count) = row?;
        let count = u64::try_from(count).unwrap_or(0);
        match kind.as_str() {
            "graph_extract" => pending.extract = count,
            "graph_backfill" => pending.backfill = count,
            "graph_cluster" => pending.cluster = count,
            "graph_normalize" => pending.normalize = count,
            _ => {}
        }
    }
    Ok(pending)
}

/// Parses an `entities.kind` token from the schema's closed CHECK set.
fn entity_kind_from_db(token: &str) -> rusqlite::Result<EntityKind> {
    EntityKind::from_token(token).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown entity kind {token:?}").into(),
        )
    })
}

/// Parses an `entity_aliases.source` token from the schema's closed CHECK set.
fn alias_source_from_db(token: &str) -> rusqlite::Result<AliasSource> {
    match token {
        "heuristic" => Ok(AliasSource::Heuristic),
        "llm" => Ok(AliasSource::Llm),
        "user" => Ok(AliasSource::User),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown entity alias source {other:?}").into(),
        )),
    }
}

/// Parses a `thread_overrides.kind` token from the schema's closed CHECK set.
fn thread_override_kind_from_db(token: &str) -> rusqlite::Result<ThreadOverrideKind> {
    ThreadOverrideKind::from_token(token).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown thread override kind {token:?}").into(),
        )
    })
}

/// Parses a `sessions.host` token (nullable) into the shared host enum.
fn session_host_from_db(value: Option<String>) -> rusqlite::Result<Option<SessionHost>> {
    value
        .map(|value| match value.as_str() {
            "terminal" => Ok(SessionHost::Terminal),
            "desktop" => Ok(SessionHost::Desktop),
            "browser" => Ok(SessionHost::Browser),
            "ide" => Ok(SessionHost::Ide),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid session host {other:?}").into(),
            )),
        })
        .transpose()
}

/// Reads one session row by id, the owner lookup `03 §7e` makes authoritative for a frame
/// (`frames.session_id`).
fn read_session_by_id(
    conn: &Connection,
    session_id: i64,
) -> rusqlite::Result<Option<GraphSessionInput>> {
    conn.query_row(
        "SELECT id, started_at, ended_at, tool, host FROM sessions WHERE id = ?1",
        params![session_id],
        |row| {
            Ok(GraphSessionInput {
                session_id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                tool: row.get(3)?,
                host: session_host_from_db(row.get(4)?)?,
            })
        },
    )
    .optional()
}

/// The interval primitive (`03 §7h.3`): the session whose `[started_at, ended_at)`
/// contains the timestamp, open-ended when `ended_at` is NULL, the lowest session id when
/// intervals overlap. Retained as the documented [`SqliteStore::graph_session_at`]
/// contract; extraction and linking do **not** use it (resolution is ownership-only,
/// `03 §7e`, `06` #41).
fn read_session_at(conn: &Connection, at_ms: i64) -> rusqlite::Result<Option<GraphSessionInput>> {
    conn.query_row(
        "SELECT id, started_at, ended_at, tool, host
         FROM sessions
         WHERE started_at <= ?1 AND (ended_at IS NULL OR ended_at > ?1)
         ORDER BY id
         LIMIT 1",
        params![at_ms],
        |row| {
            Ok(GraphSessionInput {
                session_id: row.get(0)?,
                started_at: row.get(1)?,
                ended_at: row.get(2)?,
                tool: row.get(3)?,
                host: session_host_from_db(row.get(4)?)?,
            })
        },
    )
    .optional()
}

/// The absolute per-link values one session currently holds, compared by
/// `graph_replace_session_links` against an exactly recomputed aggregate set.
#[derive(Debug, Clone, Copy)]
struct LinkState {
    hit_count: i64,
    confidence: f64,
    first_seen_at: i64,
    last_seen_at: i64,
}

impl LinkState {
    fn from_aggregate(link: &GraphSessionLinkAggregate) -> Self {
        Self {
            hit_count: i64::from(link.hit_count),
            confidence: link.confidence,
            first_seen_at: link.first_seen_at,
            last_seen_at: link.last_seen_at,
        }
    }

    /// Bitwise comparison of the confidence keeps "changed or not" deterministic: a
    /// SQLite REAL round-trips an `f64` bit for bit, so an identical rebuild compares equal.
    fn matches(self, other: Self) -> bool {
        self.hit_count == other.hit_count
            && self.confidence.to_bits() == other.confidence.to_bits()
            && self.first_seen_at == other.first_seen_at
            && self.last_seen_at == other.last_seen_at
    }
}

/// One persisted link: the entity id (needed to add aliases when only aliases changed on an
/// otherwise identical rebuild) plus the absolute values compared against the aggregates.
#[derive(Debug, Clone, Copy)]
struct ExistingLink {
    entity_id: i64,
    state: LinkState,
}

/// Reads one session's persisted link state keyed by the entity dedup identity
/// `(kind, normalized_key)` (`03 §7h.3`).
fn read_session_link_state(
    tx: &Transaction<'_>,
    session_id: i64,
) -> rusqlite::Result<BTreeMap<(String, String), ExistingLink>> {
    let mut stmt = tx.prepare(
        "SELECT e.id, e.kind, e.normalized_key, se.hit_count, se.confidence,
                se.first_seen_at, se.last_seen_at
         FROM session_entities se
         JOIN entities e ON e.id = se.entity_id
         WHERE se.session_id = ?1",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok((
            (row.get::<_, String>(1)?, row.get::<_, String>(2)?),
            ExistingLink {
                entity_id: row.get(0)?,
                state: LinkState {
                    hit_count: row.get(3)?,
                    confidence: row.get(4)?,
                    first_seen_at: row.get(5)?,
                    last_seen_at: row.get(6)?,
                },
            },
        ))
    })?;
    let mut state = BTreeMap::new();
    for row in rows {
        let (key, link) = row?;
        state.insert(key, link);
    }
    Ok(state)
}

/// Inserts additive heuristic aliases for one entity, leaving existing rows untouched
/// (`ON CONFLICT(entity_id, alias) DO NOTHING`, `03 §7h.2`). Shared by the entity upsert and
/// by the unchanged exact-rebuild path, where a new alias alone must be recorded without
/// counting as a link change.
fn insert_heuristic_aliases(
    tx: &Transaction<'_>,
    entity_id: i64,
    aliases: &[String],
    confidence: f64,
) -> rusqlite::Result<()> {
    for alias in aliases {
        tx.execute(
            "INSERT INTO entity_aliases (entity_id, alias, source, confidence)
             VALUES (?1, ?2, 'heuristic', ?3)
             ON CONFLICT(entity_id, alias) DO NOTHING",
            params![entity_id, alias, confidence],
        )?;
    }
    Ok(())
}

/// One heuristic entity upsert for [`upsert_heuristic_entity`]: the display identity plus
/// the confidence and seen window observed (one point for a single frame, the aggregate
/// window for an exact rebuild).
struct EntityUpsert<'a> {
    kind: EntityKind,
    canonical_name: &'a str,
    normalized_key: &'a str,
    confidence: f64,
    first_seen_at: i64,
    last_seen_at: i64,
    aliases: &'a [String],
}

/// Upserts one heuristic entity and its aliases, returning the entity id.
///
/// Shared by the incremental (`graph_apply_hits`) and exact-replacement
/// (`graph_replace_session_links`) writers so both dedup on `UNIQUE(kind, normalized_key)`
/// identically: the first creating hit keeps `canonical_name`/`normalized_key`/`source`,
/// while the seen window widens and the confidence takes the maximum (`03 §7h.2`).
fn upsert_heuristic_entity(
    tx: &Transaction<'_>,
    upsert: &EntityUpsert<'_>,
) -> rusqlite::Result<i64> {
    let entity_id: i64 = tx.query_row(
        "INSERT INTO entities
           (kind, canonical_name, normalized_key, source, confidence,
            first_seen_at, last_seen_at)
         VALUES (?1, ?2, ?3, 'heuristic', ?4, ?5, ?6)
         ON CONFLICT(kind, normalized_key) DO UPDATE SET
           first_seen_at = min(entities.first_seen_at, excluded.first_seen_at),
           last_seen_at  = max(entities.last_seen_at, excluded.last_seen_at),
           confidence    = max(entities.confidence, excluded.confidence),
           updated_at    = unixepoch()*1000
         RETURNING id",
        params![
            upsert.kind.as_str(),
            upsert.canonical_name,
            upsert.normalized_key,
            upsert.confidence,
            upsert.first_seen_at,
            upsert.last_seen_at
        ],
        |row| row.get(0),
    )?;
    insert_heuristic_aliases(tx, entity_id, upsert.aliases, upsert.confidence)?;
    Ok(entity_id)
}

impl SqliteStore {
    /// Everything heuristic extraction may read for one frame (`03 §7h.2`), including the
    /// filtered content text and the session the frame's evidence belongs to.
    ///
    /// Session resolution is **ownership-only** (`03 §7e`, the final ruling on the `§7h.3`
    /// conflict recorded in `06` #41): `frames.session_id` is the authoritative owner, and
    /// different tracks may overlap in time, so the owner is the only session a frame may
    /// link its evidence to. A frame with no owner (`frames.session_id` NULL) reports
    /// `session: None`; the interval rule is deliberately not used here (it survives only
    /// as the [`Self::graph_session_at`] primitive). A dangling owner id (the FK's
    /// `ON DELETE SET NULL` normally prevents it) also reports `None` instead of failing
    /// extraction.
    pub async fn graph_frame_input(&self, frame_id: i64) -> Result<Option<GraphFrameInput>> {
        self.with_conn(move |conn| {
            let frame = conn
                .query_row(
                    "SELECT f.id, f.captured_at, f.app_hint, f.window_title, f.browser_url,
                            ft.content_text, f.session_id
                     FROM frames f
                     LEFT JOIN frame_text ft ON ft.frame_id = f.id
                     WHERE f.id = ?1",
                    params![frame_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Option<String>>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, Option<String>>(5)?,
                            row.get::<_, Option<i64>>(6)?,
                        ))
                    },
                )
                .optional()?;
            let Some((
                id,
                captured_at,
                app_hint,
                window_title,
                browser_url,
                content_text,
                owner_session_id,
            )) = frame
            else {
                return Ok(None);
            };
            let session = match owner_session_id {
                Some(owner_session_id) => read_session_by_id(conn, owner_session_id)?,
                // Ownership-only: an unowned frame has no session, even when an interval
                // contains it (`03 §7e` final ruling, `06` #41).
                None => None,
            };
            Ok(Some(GraphFrameInput {
                frame_id: id,
                captured_at,
                app_hint,
                window_title,
                browser_url,
                content_text,
                session,
            }))
        })
        .await
    }

    /// The session whose `[started_at, ended_at)` contains `at_ms`; an open session is
    /// open-ended and the lowest session id wins when intervals overlap (`03 §7h.3`).
    /// This is the retained interval primitive: extraction and linking do **not** use it
    /// (session resolution is ownership-only, `03 §7e`; `06` #41 / #157 review fix).
    pub async fn graph_session_at(&self, at_ms: i64) -> Result<Option<GraphSessionInput>> {
        self.with_conn(move |conn| Ok(read_session_at(conn, at_ms)?))
            .await
    }

    /// Frame ids greater than `after_id` (exclusive; `None` starts from the beginning),
    /// oldest first, bounded. The backfill enqueues these as real `graph_extract` jobs;
    /// the cursor itself is the queue's `MAX(jobs.frame_id)` (`03 §7h.6`).
    pub async fn graph_frame_ids_after(
        &self,
        after_id: Option<i64>,
        limit: u32,
    ) -> Result<Vec<i64>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id FROM frames
                 WHERE (?1 IS NULL OR id > ?1)
                 ORDER BY id
                 LIMIT ?2",
            )?;
            let ids = stmt
                .query_map(params![after_id, i64::from(limit)], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(ids)
        })
        .await
    }

    /// Upserts one frame's hits and its session links in one transaction (`03 §7h.2/.3`):
    /// entities dedup on `UNIQUE(kind, normalized_key)`, links on the
    /// `(session_id, entity_id)` primary key, `hit_count` accumulates one per processed
    /// frame, and link confidence is the maximum of the contributing hits. A frame with
    /// no owning session still contributes its entities, with no link row.
    ///
    /// `frame_id` is the job's evidence handle, not persisted: the schema has no
    /// per-frame ledger, and `hit_count` is the only frame-weight the link row carries.
    pub async fn graph_apply_hits(
        &self,
        session_id: Option<i64>,
        _frame_id: i64,
        captured_at: i64,
        hits: &[GraphHit],
    ) -> Result<()> {
        if hits.is_empty() {
            return Ok(());
        }
        let hits = hits.to_vec();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            // One link update per entity in this frame, carrying the strongest confidence
            // seen for it, so a duplicated hit shape cannot count the same frame twice.
            let mut linked: BTreeMap<i64, f64> = BTreeMap::new();
            for hit in &hits {
                let entity_id = upsert_heuristic_entity(
                    &tx,
                    &EntityUpsert {
                        kind: hit.kind,
                        canonical_name: &hit.canonical_name,
                        normalized_key: &hit.normalized_key,
                        confidence: hit.confidence,
                        first_seen_at: captured_at,
                        last_seen_at: captured_at,
                        aliases: &hit.aliases,
                    },
                )?;
                linked
                    .entry(entity_id)
                    .and_modify(|confidence| *confidence = (*confidence).max(hit.confidence))
                    .or_insert(hit.confidence);
            }
            if let Some(session_id) = session_id {
                for (entity_id, confidence) in linked {
                    tx.execute(
                        "INSERT INTO session_entities
                           (session_id, entity_id, hit_count, confidence,
                            first_seen_at, last_seen_at)
                         VALUES (?1, ?2, 1, ?3, ?4, ?4)
                         ON CONFLICT(session_id, entity_id) DO UPDATE SET
                           hit_count     = session_entities.hit_count + 1,
                           confidence    = max(session_entities.confidence, excluded.confidence),
                           first_seen_at = min(session_entities.first_seen_at,
                                               excluded.first_seen_at),
                           last_seen_at  = max(session_entities.last_seen_at,
                                               excluded.last_seen_at)",
                        params![session_id, entity_id, confidence, captured_at],
                    )?;
                }
            }
            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// Replaces one session's entity links with an exactly recomputed aggregate set in one
    /// transaction (`03 §7h.3`, P5 review fix). The kernel rebuilds a session's links after
    /// an ownership change (frames assigned or moved, session deleted and re-created), so a
    /// link that cascaded away or missed the first extraction is restored and a stale link
    /// from a moved frame is removed. Only this session's `session_entities` rows are
    /// replaced; threads and overrides are not touched.
    ///
    /// Change decision: the session's current rows are compared with the aggregates by the
    /// dedup identity `(kind, normalized_key)` on `hit_count`, `confidence` (bitwise,
    /// deterministic), `first_seen_at` and `last_seen_at`. When the set is identical, only
    /// any aggregate aliases that are not yet present for their entity are inserted
    /// (`ON CONFLICT(entity_id, alias) DO NOTHING`) and `Ok(false)` is returned: an
    /// unchanged rebuild cannot trigger a needless re-cluster, but new aliases are still
    /// recorded. Aliases are additive and are not part of the change decision.
    ///
    /// Otherwise (or when an empty aggregate list clears a non-empty session) the rows are
    /// deleted and re-inserted with the aggregates' absolute values, and the referenced
    /// entities are upserted exactly like the incremental path (`graph_apply_hits`): the
    /// dedup identity, `canonical_name` and `source` are never rewritten, only the seen
    /// window is widened and the confidence raised. A session row that no longer exists is
    /// a no-op `Ok(false)` (the kernel may race a session deletion), never an error.
    ///
    /// Returns `true` only when the persisted link set actually changed, so alias-only
    /// writes keep the `Ok(false)` "no re-cluster needed" answer.
    pub async fn graph_replace_session_links(
        &self,
        session_id: i64,
        links: &[GraphSessionLinkAggregate],
    ) -> Result<bool> {
        let links = links.to_vec();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;

            // A session deleted between the kernel's read and this write must not be
            // resurrected, and a foreign-key error would only surface a race the caller
            // already tolerates: treat it as "nothing to do".
            let session_exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
                params![session_id],
                |row| row.get(0),
            )?;
            if !session_exists {
                return Ok(false);
            }

            let existing = read_session_link_state(&tx, session_id)?;
            let mut incoming: BTreeMap<(String, String), &GraphSessionLinkAggregate> =
                BTreeMap::new();
            for link in &links {
                let key = (link.kind.as_str().to_string(), link.normalized_key.clone());
                if incoming.insert(key, link).is_some() {
                    // No session id (or entity key) in the error text: opaque ids are
                    // CodeQL-cleartext-logging class sensitive data and this reaches a warn log.
                    anyhow::bail!("link rebuild repeats an entity identity");
                }
            }
            let unchanged = existing.len() == incoming.len()
                && incoming.iter().all(|(key, link)| {
                    existing.get(key).is_some_and(|current| {
                        current.state.matches(LinkState::from_aggregate(link))
                    })
                });
            if unchanged {
                // The link values are already exact, but the rebuild may carry aliases the
                // previous pass did not know about. Alias rows are additive display data and
                // do not change the link set, so they are written while the caller still
                // gets the `Ok(false)` no-change answer.
                for (key, link) in &incoming {
                    if let Some(current) = existing.get(key) {
                        insert_heuristic_aliases(
                            &tx,
                            current.entity_id,
                            &link.aliases,
                            link.confidence,
                        )?;
                    }
                }
                tx.commit()?;
                return Ok(false);
            }

            tx.execute(
                "DELETE FROM session_entities WHERE session_id = ?1",
                params![session_id],
            )?;
            for link in &links {
                let entity_id = upsert_heuristic_entity(
                    &tx,
                    &EntityUpsert {
                        kind: link.kind,
                        canonical_name: &link.canonical_name,
                        normalized_key: &link.normalized_key,
                        confidence: link.confidence,
                        first_seen_at: link.first_seen_at,
                        last_seen_at: link.last_seen_at,
                        aliases: &link.aliases,
                    },
                )?;
                tx.execute(
                    "INSERT INTO session_entities
                       (session_id, entity_id, hit_count, confidence,
                        first_seen_at, last_seen_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        session_id,
                        entity_id,
                        i64::from(link.hit_count),
                        link.confidence,
                        link.first_seen_at,
                        link.last_seen_at
                    ],
                )?;
            }
            tx.commit()?;
            Ok(true)
        })
        .await
    }

    /// The complete input to one clustering run (`03 §7h.4`): every session's interval
    /// and every session-entity link with the entity display data the title rule needs.
    /// The clusterer owns the confidence threshold and the decay window; the store only
    /// supplies the persisted rows.
    pub async fn graph_cluster_input(&self, now_ms: i64) -> Result<GraphClusterInput> {
        self.with_conn(move |conn| {
            let sessions = {
                let mut stmt =
                    conn.prepare("SELECT id, started_at, ended_at FROM sessions ORDER BY id")?;
                let rows = stmt.query_map([], |row| {
                    Ok(GraphSessionInterval {
                        session_id: row.get(0)?,
                        started_at: row.get(1)?,
                        ended_at: row.get(2)?,
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let links = {
                let mut stmt = conn.prepare(
                    "SELECT se.session_id, se.entity_id, e.kind, e.canonical_name,
                            se.confidence, se.last_seen_at
                     FROM session_entities se
                     JOIN entities e ON e.id = se.entity_id
                     ORDER BY se.session_id, se.entity_id",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, f64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                })?;
                let mut links = Vec::new();
                for row in rows {
                    let (session_id, entity_id, kind, canonical_name, confidence, last_seen_at) =
                        row?;
                    links.push(GraphClusterLink {
                        session_id,
                        entity_id,
                        entity_kind: entity_kind_from_db(&kind)?,
                        canonical_name,
                        confidence,
                        last_seen_at,
                    });
                }
                links
            };
            Ok(GraphClusterInput {
                now_ms,
                sessions,
                links,
            })
        })
        .await
    }

    /// Persisted threads with their current members, read before planning (`03 §7h.4`).
    pub async fn graph_stored_threads(&self) -> Result<Vec<StoredThread>> {
        self.with_conn(move |conn| {
            let mut threads = {
                let mut stmt = conn.prepare(
                    "SELECT id, cluster_key, title, first_seen_at, last_seen_at
                     FROM threads ORDER BY id",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok(StoredThread {
                        id: row.get(0)?,
                        cluster_key: row.get(1)?,
                        title: row.get(2)?,
                        first_seen_at: row.get(3)?,
                        last_seen_at: row.get(4)?,
                        sessions: Vec::new(),
                    })
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let mut members: HashMap<i64, Vec<ThreadMemberDraft>> = HashMap::new();
            {
                let mut stmt = conn.prepare(
                    "SELECT thread_id, session_id, confidence
                     FROM thread_sessions ORDER BY thread_id, session_id",
                )?;
                let rows = stmt.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        ThreadMemberDraft {
                            session_id: row.get(1)?,
                            confidence: row.get(2)?,
                        },
                    ))
                })?;
                for row in rows {
                    let (thread_id, member) = row?;
                    members.entry(thread_id).or_default().push(member);
                }
            }
            for thread in &mut threads {
                thread.sessions = members.remove(&thread.id).unwrap_or_default();
            }
            Ok(threads)
        })
        .await
    }

    /// `thread_overrides` rows in `(created_at, id)` order (`03 §7h.5`), with the typed
    /// JSON payload decoded against each row's kind.
    pub async fn graph_thread_overrides(&self) -> Result<Vec<ThreadOverrideRow>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, thread_id, target_thread_id, payload, created_at
                 FROM thread_overrides ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?;
            let mut overrides = Vec::new();
            for row in rows {
                let (id, kind, thread_id, target_thread_id, payload, created_at) = row?;
                let kind = thread_override_kind_from_db(&kind)?;
                let payload = ThreadOverridePayload::from_json(kind, &payload)?;
                overrides.push(ThreadOverrideRow {
                    id,
                    kind,
                    thread_id,
                    target_thread_id,
                    payload,
                    created_at,
                });
            }
            Ok(overrides)
        })
        .await
    }

    /// Inserts one typed override and enqueues `graph_cluster` in the same transaction,
    /// so an override always wins on the next clustering pass (`03 §7h.5`). The draft's
    /// kind/thread-id shape and payload are validated before anything is written; a
    /// validation or foreign-key failure rolls the enqueue back with the insert.
    pub async fn graph_insert_thread_override(
        &self,
        draft: &ThreadOverrideDraft,
        now_ms: i64,
    ) -> Result<i64> {
        draft.validate()?;
        let draft = draft.clone();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let payload = draft.payload.to_json()?;
            tx.execute(
                "INSERT INTO thread_overrides
                   (kind, thread_id, target_thread_id, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    draft.kind.as_str(),
                    draft.thread_id,
                    draft.target_thread_id,
                    payload,
                    now_ms
                ],
            )?;
            let override_id = tx.last_insert_rowid();
            // Immediately runnable: the user just changed the model and the scheduler
            // should rebuild threads on its next tick, not wait for a clock edge.
            tx.execute(
                "INSERT INTO jobs (kind, frame_id, priority, max_attempts, not_before)
                 VALUES ('graph_cluster', NULL, 0, 3, 0)",
                [],
            )?;
            tx.commit()?;
            Ok(override_id)
        })
        .await
    }

    /// Persists one planned thread model in a single transaction (`03 §7h.4/.5`):
    /// upserts threads by `cluster_key` keeping their existing ids, syncs the
    /// `thread_sessions` rows, applies merge-override rewrites, and deletes the threads
    /// that no longer exist.
    ///
    /// Membership is rewritten before any insert: the rows of every planned thread and
    /// of every thread about to be deleted are cleared first, so a session reassigned
    /// between threads (the merge fold in particular) cannot collide with
    /// `UNIQUE(thread_sessions.session_id)`. Override rewrites happen before the deletes
    /// because removing an emptied merge source would otherwise cascade-delete its
    /// override row (`03 §7h.5`, P5 contract contradiction recorded in `06`).
    pub async fn graph_apply_thread_plan(&self, plan: &ThreadPlan) -> Result<()> {
        if plan.threads.is_empty()
            && plan.delete_thread_ids.is_empty()
            && plan.override_updates.is_empty()
        {
            return Ok(());
        }
        // A plan is the complete deterministic model, so a repeated cluster_key or a
        // session in two threads is a caller bug, not something to paper over with SQL.
        let mut cluster_keys: HashSet<&str> = HashSet::new();
        let mut planned_sessions: HashSet<i64> = HashSet::new();
        for draft in &plan.threads {
            if !cluster_keys.insert(draft.cluster_key.as_str()) {
                anyhow::bail!("thread plan repeats cluster_key {:?}", draft.cluster_key);
            }
            for member in &draft.sessions {
                if !planned_sessions.insert(member.session_id) {
                    anyhow::bail!(
                        "thread plan assigns session {} to more than one thread",
                        member.session_id
                    );
                }
            }
        }
        let plan = plan.clone();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;

            // 1. Upsert every planned thread. The UNIQUE cluster_key is the identity, so
            // an existing thread keeps its id across rebuilds.
            let mut planned: Vec<(i64, &ThreadDraft)> = Vec::with_capacity(plan.threads.len());
            for draft in &plan.threads {
                let session_count = i64::try_from(draft.sessions.len()).map_err(|_| {
                    anyhow::anyhow!("thread {:?} has too many sessions", draft.cluster_key)
                })?;
                let writer = tx.query_row(
                    "INSERT INTO threads
                       (title, cluster_key, first_seen_at, last_seen_at, session_count)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(cluster_key) DO UPDATE SET
                       title         = excluded.title,
                       first_seen_at = excluded.first_seen_at,
                       last_seen_at  = excluded.last_seen_at,
                       session_count = excluded.session_count,
                       updated_at    = unixepoch()*1000
                     RETURNING id",
                    params![
                        draft.title,
                        draft.cluster_key,
                        draft.first_seen_at,
                        draft.last_seen_at,
                        session_count
                    ],
                    |row| row.get::<_, i64>(0),
                )?;
                planned.push((writer, draft));
            }
            for (thread_id, _) in &planned {
                if plan.delete_thread_ids.contains(thread_id) {
                    anyhow::bail!("thread plan both keeps and deletes thread {thread_id}");
                }
            }

            // 2. Clear the membership this plan owns before inserting. Planned threads are
            // rewritten from scratch; threads that are about to be deleted release their
            // sessions too, because those sessions may move into a planned thread.
            let mut cleared: Vec<i64> = planned.iter().map(|(id, _)| *id).collect();
            cleared.extend(plan.delete_thread_ids.iter().copied());
            cleared.sort_unstable();
            cleared.dedup();
            for chunk in cleared.chunks(WRITE_CHUNK) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql =
                    format!("DELETE FROM thread_sessions WHERE thread_id IN ({placeholders})");
                tx.execute(&sql, params_from_iter(chunk.iter().copied()))?;
            }

            // 3. Insert the planned membership rows.
            for (thread_id, draft) in &planned {
                for chunk in draft.sessions.chunks(WRITE_CHUNK) {
                    let placeholders = std::iter::repeat_n("(?, ?, ?)", chunk.len())
                        .collect::<Vec<_>>()
                        .join(",");
                    let sql = format!(
                        "INSERT INTO thread_sessions (thread_id, session_id, confidence)
                         VALUES {placeholders}"
                    );
                    let mut values: Vec<Value> = Vec::with_capacity(chunk.len() * 3);
                    for member in chunk {
                        values.push(Value::Integer(*thread_id));
                        values.push(Value::Integer(member.session_id));
                        values.push(Value::Real(member.confidence));
                    }
                    tx.execute(&sql, params_from_iter(values))?;
                }
            }

            // 4. Re-point merge override rows at the survivor before the emptied source is
            // deleted; the FK cascade would otherwise remove the override and lose the fold.
            for update in &plan.override_updates {
                let changed = tx.execute(
                    "UPDATE thread_overrides SET thread_id = ?2, target_thread_id = ?3
                     WHERE id = ?1",
                    params![
                        update.override_id,
                        update.thread_id,
                        update.target_thread_id
                    ],
                )?;
                if changed == 0 {
                    anyhow::bail!(
                        "thread plan rewrites override {} but the row does not exist",
                        update.override_id
                    );
                }
            }

            // 5. Delete the threads that no longer exist in the model.
            for chunk in plan.delete_thread_ids.chunks(WRITE_CHUNK) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!("DELETE FROM threads WHERE id IN ({placeholders})");
                tx.execute(&sql, params_from_iter(chunk.iter().copied()))?;
            }

            tx.commit()?;
            Ok(())
        })
        .await
    }

    /// The five table counts logged when the backfill completes (`03 §7h.6`).
    pub async fn graph_table_counts(&self) -> Result<GraphTableCounts> {
        self.with_conn(move |conn| {
            let (entities, entity_aliases, session_entities, threads, thread_sessions) = conn
                .query_row(
                    "SELECT (SELECT COUNT(*) FROM entities),
                            (SELECT COUNT(*) FROM entity_aliases),
                            (SELECT COUNT(*) FROM session_entities),
                            (SELECT COUNT(*) FROM threads),
                            (SELECT COUNT(*) FROM thread_sessions)",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                        ))
                    },
                )?;
            Ok(GraphTableCounts {
                entities: entities as u64,
                entity_aliases: entity_aliases as u64,
                session_entities: session_entities as u64,
                threads: threads as u64,
                thread_sessions: thread_sessions as u64,
            })
        })
        .await
    }

    /// The queue-derived backfill checkpoint and in-flight counts (`03 §7h.6`). There is no
    /// extra state: the cursor is `MAX(jobs.frame_id)` over `graph_extract` jobs in any
    /// state, so a queued frame is already past the cursor and an interrupted run resumes
    /// without duplicating rows.
    pub async fn graph_backfill_status(&self) -> Result<GraphBackfillStatus> {
        self.with_conn(move |conn| {
            let cursor: Option<i64> = conn.query_row(
                "SELECT MAX(frame_id) FROM jobs WHERE kind = 'graph_extract'",
                [],
                |row| row.get(0),
            )?;
            let max_frame_id: Option<i64> =
                conn.query_row("SELECT MAX(id) FROM frames", [], |row| row.get(0))?;
            let done_extract: i64 = conn.query_row(
                "SELECT COUNT(*) FROM jobs
                 WHERE kind = 'graph_extract' AND state = 'done'",
                [],
                |row| row.get(0),
            )?;
            let pending = graph_pending_jobs(conn)?;
            Ok(GraphBackfillStatus {
                cursor,
                max_frame_id,
                pending_extract: pending.extract,
                pending_backfill: pending.backfill,
                pending_cluster: pending.cluster,
                pending_normalize: pending.normalize,
                done_extract: done_extract as u64,
            })
        })
        .await
    }

    /// Coverage of the work graph (`03 §7h.6`): `COUNT(sessions)` clustered into threads, and
    /// whether the queue still holds graph work.
    ///
    /// `sessions_clustered` counts distinct `thread_sessions.session_id`, so it can never
    /// exceed `sessions_total` and new sessions keep arriving while older ones are clustered —
    /// which is exactly the "N of M sessions clustered" line the Work screen renders. The
    /// backlog and `running` come from [`graph_pending_jobs`], the same grouped read
    /// [`Self::graph_backfill_status`] uses, because the queue is the only checkpoint there is.
    pub async fn graph_progress(&self) -> Result<GraphProgress> {
        self.with_conn(|conn| {
            let sessions_total: i64 =
                conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
            let sessions_clustered: i64 = conn.query_row(
                "SELECT COUNT(DISTINCT session_id) FROM thread_sessions",
                [],
                |row| row.get(0),
            )?;
            let pending = graph_pending_jobs(conn)?;
            Ok(GraphProgress {
                sessions_total: u64::try_from(sessions_total).unwrap_or(0),
                sessions_clustered: u64::try_from(sessions_clustered).unwrap_or(0),
                running: pending.any(),
                pending_extract: pending.extract,
                pending_backfill: pending.backfill,
                pending_cluster: pending.cluster,
            })
        })
        .await
    }

    /// Threads holding a member session that touched `entity_id` (`session_entities`, `03
    /// §7h.1`), ordered `last_seen_at DESC, id DESC` exactly like
    /// [`SqliteStore::threads_in_range`] and bounded by `limit` — the Work list's `?entity=`
    /// filter.
    ///
    /// The membership test is an `EXISTS`, so a thread appears once however many of its
    /// sessions touched the entity. `active_ms`/`confidence` come from the shared
    /// [`THREAD_SUMMARY_SELECT`] aggregate, never a second formula.
    pub async fn threads_for_entity(
        &self,
        entity_id: i64,
        limit: u32,
    ) -> Result<Vec<ThreadSummary>> {
        self.with_conn(move |conn| {
            let sql = format!(
                "{THREAD_SUMMARY_SELECT}
                 WHERE EXISTS (
                    SELECT 1 FROM thread_sessions ts
                    JOIN session_entities se ON se.session_id = ts.session_id
                    WHERE ts.thread_id = t.id AND se.entity_id = :entity_id
                 )
                 ORDER BY t.last_seen_at DESC, t.id DESC
                 LIMIT :limit"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(
                named_params! {
                    ":entity_id": entity_id,
                    ":limit": i64::from(limit),
                },
                decode_thread_summary,
            )?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })
        .await
    }

    /// Threads sharing at least one entity with `thread_id` (`03 §7h.4`), ordered by
    /// shared-entity count descending then `last_seen_at` descending, bounded by `limit`.
    ///
    /// Two threads share an entity when a session of each carries the same `session_entities`
    /// row; `shared_entity_ids` **is** the evidence, so the Thread page names those subjects
    /// instead of asserting a relationship. Two reads, never one per row: the ranked threads
    /// (with their shared counts) and then every shared entity id for exactly those threads in
    /// `(thread_id, entity_id)` order, which also makes the id list deterministic.
    pub async fn connected_threads(
        &self,
        thread_id: i64,
        limit: u32,
    ) -> Result<Vec<ConnectedThread>> {
        self.with_conn(move |conn| {
            // `shared` lists, per other thread, how many distinct entities it shares. The join
            // walks this thread's session entities out to any session holding the same entity,
            // then back to that session's thread.
            let shared = "SELECT other.thread_id AS thread_id,
                                 COUNT(DISTINCT mine_se.entity_id) AS shared_count
                          FROM thread_sessions mine
                          JOIN session_entities mine_se ON mine_se.session_id = mine.session_id
                          JOIN session_entities other_se
                            ON other_se.entity_id = mine_se.entity_id
                          JOIN thread_sessions other ON other.session_id = other_se.session_id
                          WHERE mine.thread_id = :thread_id
                            AND other.thread_id <> :thread_id
                          GROUP BY other.thread_id";
            let sql = format!(
                "{THREAD_SUMMARY_SELECT}
                 JOIN ({shared}) shared ON shared.thread_id = t.id
                 ORDER BY shared.shared_count DESC, t.last_seen_at DESC, t.id DESC
                 LIMIT :limit"
            );
            let mut stmt = conn.prepare(&sql)?;
            let ranked = stmt
                .query_map(
                    named_params! {
                        ":thread_id": thread_id,
                        ":limit": i64::from(limit),
                    },
                    decode_thread_summary,
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if ranked.is_empty() {
                return Ok(Vec::new());
            }

            let ids: Vec<i64> = ranked.iter().map(|thread| thread.id).collect();
            let mut shared_ids: HashMap<i64, Vec<i64>> = HashMap::new();
            for chunk in ids.chunks(WRITE_CHUNK) {
                let list = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let mut stmt = conn.prepare(&format!(
                    "SELECT other.thread_id, mine_se.entity_id
                     FROM thread_sessions mine
                     JOIN session_entities mine_se ON mine_se.session_id = mine.session_id
                     JOIN session_entities other_se ON other_se.entity_id = mine_se.entity_id
                     JOIN thread_sessions other ON other.session_id = other_se.session_id
                     WHERE mine.thread_id = ?
                       AND other.thread_id IN ({list})
                     GROUP BY other.thread_id, mine_se.entity_id
                     ORDER BY other.thread_id, mine_se.entity_id"
                ))?;
                let mut values: Vec<i64> = Vec::with_capacity(chunk.len() + 1);
                values.push(thread_id);
                values.extend(chunk.iter().copied());
                let rows = stmt.query_map(params_from_iter(values), |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?;
                for row in rows {
                    let (other, entity_id) = row?;
                    shared_ids.entry(other).or_default().push(entity_id);
                }
            }

            Ok(ranked
                .into_iter()
                .map(|thread| ConnectedThread {
                    shared_entity_ids: shared_ids.remove(&thread.id).unwrap_or_default(),
                    thread,
                })
                .collect())
        })
        .await
    }

    /// The survivor of the latest `merge` override whose source is `thread_id`, from
    /// `thread_overrides` (`03 §7h.5`), or `None` when the thread was never merged away.
    ///
    /// The latest row wins on `(created_at, id)`, matching [`Self::graph_thread_overrides`]'s
    /// order, so a thread merged more than once reports the survivor the following pass used.
    pub async fn thread_merge_target(&self, thread_id: i64) -> Result<Option<i64>> {
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT target_thread_id FROM thread_overrides
                     WHERE kind = 'merge' AND thread_id = ?1
                     ORDER BY created_at DESC, id DESC
                     LIMIT 1",
                    [thread_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()?
                .flatten())
        })
        .await
    }

    /// Sessions with a cached title or summary, most recent first, bounded (`03 §7h.2`
    /// normalization input).
    pub async fn graph_sessions_with_summaries(
        &self,
        limit: u32,
    ) -> Result<Vec<SessionSummaryInput>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, started_at, ended_at, tool, context_key, title, summary
                 FROM sessions
                 WHERE (title IS NOT NULL AND title <> '')
                    OR (summary IS NOT NULL AND summary <> '')
                 ORDER BY started_at DESC, id DESC
                 LIMIT ?1",
            )?;
            let rows = stmt
                .query_map(params![i64::from(limit)], |row| {
                    Ok(SessionSummaryInput {
                        session_id: row.get(0)?,
                        started_at: row.get(1)?,
                        ended_at: row.get(2)?,
                        tool: row.get(3)?,
                        context_key: row.get(4)?,
                        title: row.get(5)?,
                        summary: row.get(6)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    /// The existing entity catalog plus aliases the normalization pass proposes against
    /// (`03 §7h.2`), most recently seen first, bounded. Alias rows keep their persisted
    /// `(entity_id, id)` order.
    pub async fn graph_entity_catalog(&self, limit: u32) -> Result<Vec<EntityCatalogRow>> {
        self.with_conn(move |conn| {
            let mut catalog = {
                let mut stmt = conn.prepare(
                    "SELECT id, kind, canonical_name, normalized_key
                     FROM entities
                     ORDER BY last_seen_at DESC, id DESC
                     LIMIT ?1",
                )?;
                let rows = stmt.query_map(params![i64::from(limit)], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                })?;
                let mut catalog = Vec::new();
                for row in rows {
                    let (id, kind, canonical_name, normalized_key) = row?;
                    catalog.push(EntityCatalogRow {
                        id,
                        kind: entity_kind_from_db(&kind)?,
                        canonical_name,
                        normalized_key,
                        aliases: Vec::new(),
                    });
                }
                catalog
            };
            let entity_ids: Vec<i64> = catalog.iter().map(|row| row.id).collect();
            let mut aliases: HashMap<i64, Vec<(String, AliasSource)>> = HashMap::new();
            for chunk in entity_ids.chunks(WRITE_CHUNK) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT entity_id, alias, source FROM entity_aliases
                     WHERE entity_id IN ({placeholders})
                     ORDER BY entity_id, id"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(params_from_iter(chunk.iter().copied()), |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })?;
                for row in rows {
                    let (entity_id, alias, source) = row?;
                    aliases
                        .entry(entity_id)
                        .or_default()
                        .push((alias, alias_source_from_db(&source)?));
                }
            }
            for row in &mut catalog {
                row.aliases = aliases.remove(&row.id).unwrap_or_default();
            }
            Ok(catalog)
        })
        .await
    }

    /// Applies validated normalization proposals in one transaction (`03 §7h.2`): new
    /// aliases are written with `source='llm'`, new rows are `entities.source='llm'`, and
    /// a heuristic entity's `normalized_key` is never rewritten or deleted. Only the
    /// proposals' own aliases are written; a proposal that matches an existing identity
    /// adds aliases to it and creates no row, so the pass stays additive.
    pub async fn graph_apply_normalization(
        &self,
        proposals: &[NormalizationProposal],
    ) -> Result<NormalizationOutcome> {
        if proposals.is_empty() {
            return Ok(NormalizationOutcome::default());
        }
        let proposals = proposals.to_vec();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            // A proposal carries no timestamp (the pass reads summaries, not frames), so a
            // newly promoted `llm` entity is stamped with the store clock at promotion time.
            let promoted_at: i64 = tx.query_row("SELECT unixepoch()*1000", [], |row| row.get(0))?;
            let mut outcome = NormalizationOutcome::default();
            for proposal in &proposals {
                let entity_id = if let Some(matched_entity_id) = proposal.matched_entity_id {
                    let exists: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM entities WHERE id = ?1)",
                        params![matched_entity_id],
                        |row| row.get(0),
                    )?;
                    if !exists {
                        anyhow::bail!(
                            "normalization matched entity {matched_entity_id} does not exist"
                        );
                    }
                    matched_entity_id
                } else {
                    let inserted: Option<i64> = tx
                        .query_row(
                            "INSERT INTO entities
                               (kind, canonical_name, normalized_key, source, confidence,
                                first_seen_at, last_seen_at)
                             VALUES (?1, ?2, ?3, 'llm', ?4, ?5, ?5)
                             ON CONFLICT(kind, normalized_key) DO NOTHING
                             RETURNING id",
                            params![
                                proposal.kind.as_str(),
                                proposal.canonical_name,
                                proposal.normalized_key,
                                proposal.confidence,
                                promoted_at
                            ],
                            |row| row.get(0),
                        )
                        .optional()?;
                    match inserted {
                        Some(entity_id) => {
                            outcome.entities_created += 1;
                            entity_id
                        }
                        // The identity already exists (a heuristic row in the normal case):
                        // attach the aliases to it and leave its identity untouched.
                        None => tx.query_row(
                            "SELECT id FROM entities WHERE kind = ?1 AND normalized_key = ?2",
                            params![proposal.kind.as_str(), proposal.normalized_key],
                            |row| row.get(0),
                        )?,
                    }
                };
                for alias in &proposal.aliases {
                    outcome.aliases_inserted += tx.execute(
                        "INSERT INTO entity_aliases (entity_id, alias, source, confidence)
                         VALUES (?1, ?2, 'llm', ?3)
                         ON CONFLICT(entity_id, alias) DO NOTHING",
                        params![entity_id, alias, proposal.confidence],
                    )? as u64;
                }
            }
            tx.commit()?;
            Ok(outcome)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::SqliteStore;
    use traits::{
        AliasSource, EntityKind, GraphHit, GraphSessionInterval, GraphSessionLinkAggregate,
        GraphTableCounts, JobKind, NewFrame, NewJob, NewSession, NormalizationOutcome,
        NormalizationProposal, OcrResult, SessionHost, SessionKind, Store, ThreadDraft,
        ThreadMemberDraft, ThreadOverrideDraft, ThreadOverrideKind, ThreadOverridePayload,
        ThreadOverrideUpdate, ThreadPlan,
    };

    /// A representative foreground context for a frame; the graph reads only the stored
    /// columns, so the fixture rows are the source of every expectation below.
    fn frame(at: i64) -> NewFrame {
        NewFrame {
            captured_at: at,
            monitor_index: 0,
            width: 1920,
            height: 1080,
            image_path: format!("frames/{at}.webp"),
            content_hash: format!("hash-{at}"),
            app_hint: Some("Code".to_string()),
            window_title: Some("main.rs - acme".to_string()),
            browser_url: None,
            capture_trigger: None,
        }
    }

    fn session(started_at: i64, ended_at: Option<i64>) -> NewSession {
        NewSession {
            started_at,
            ended_at,
            kind: SessionKind::Focus,
            tool: None,
            host: Some(SessionHost::Ide),
            context_key: "focus:code".to_string(),
            confidence: 0.8,
            frozen: false,
        }
    }

    async fn seed_frame(store: &SqliteStore, at: i64, text: &str) -> i64 {
        let id = store.insert_frame(frame(at)).await.unwrap();
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

    fn hit(kind: EntityKind, name: &str, key: &str, confidence: f64) -> GraphHit {
        GraphHit {
            kind,
            canonical_name: name.to_string(),
            normalized_key: key.to_string(),
            confidence,
            aliases: Vec::new(),
        }
    }

    fn member(session_id: i64, confidence: f64) -> ThreadMemberDraft {
        ThreadMemberDraft {
            session_id,
            confidence,
        }
    }

    fn draft(
        cluster_key: &str,
        title: &str,
        first_seen_at: i64,
        last_seen_at: i64,
        sessions: Vec<ThreadMemberDraft>,
    ) -> ThreadDraft {
        ThreadDraft {
            cluster_key: cluster_key.to_string(),
            title: title.to_string(),
            first_seen_at,
            last_seen_at,
            sessions,
        }
    }

    /// Runs read-only fixture assertions directly against the store connection. This is
    /// the same private connection every store module shares; no production SQL is added.
    fn with_raw<T>(store: &SqliteStore, f: impl FnOnce(&Connection) -> T) -> T {
        let conn = store.conn.lock().unwrap();
        f(&conn)
    }

    fn row_count(store: &SqliteStore, table: &str) -> i64 {
        with_raw(store, |conn| {
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
        })
    }

    fn job_count(store: &SqliteStore, kind: &str, state: &str) -> i64 {
        with_raw(store, |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM jobs WHERE kind = ?1 AND state = ?2",
                rusqlite::params![kind, state],
                |row| row.get(0),
            )
            .unwrap()
        })
    }

    /// A full `entities` row in the column order the assertions read.
    fn entity_row(
        store: &SqliteStore,
        kind: EntityKind,
        normalized_key: &str,
    ) -> (i64, String, String, String, f64, i64, i64) {
        with_raw(store, |conn| {
            conn.query_row(
                "SELECT id, canonical_name, normalized_key, source, confidence,
                        first_seen_at, last_seen_at
                 FROM entities WHERE kind = ?1 AND normalized_key = ?2",
                rusqlite::params![kind.as_str(), normalized_key],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap()
        })
    }

    fn link_row(store: &SqliteStore, session_id: i64, entity_id: i64) -> (i64, f64, i64, i64) {
        with_raw(store, |conn| {
            conn.query_row(
                "SELECT hit_count, confidence, first_seen_at, last_seen_at
                 FROM session_entities WHERE session_id = ?1 AND entity_id = ?2",
                rusqlite::params![session_id, entity_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap()
        })
    }

    fn thread_session_count(store: &SqliteStore, thread_id: i64) -> i64 {
        with_raw(store, |conn| {
            conn.query_row(
                "SELECT session_count FROM threads WHERE id = ?1",
                rusqlite::params![thread_id],
                |row| row.get(0),
            )
            .unwrap()
        })
    }

    fn aggregate(
        kind: EntityKind,
        name: &str,
        key: &str,
        confidence: f64,
        hit_count: u32,
        first_seen_at: i64,
        last_seen_at: i64,
    ) -> GraphSessionLinkAggregate {
        GraphSessionLinkAggregate {
            kind,
            canonical_name: name.to_string(),
            normalized_key: key.to_string(),
            confidence,
            hit_count,
            first_seen_at,
            last_seen_at,
            aliases: Vec::new(),
        }
    }

    fn session_link_count(store: &SqliteStore, session_id: i64) -> i64 {
        with_raw(store, |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM session_entities WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            )
            .unwrap()
        })
    }

    fn session_entity_link_count(store: &SqliteStore, session_id: i64, entity_id: i64) -> i64 {
        with_raw(store, |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM session_entities
                 WHERE session_id = ?1 AND entity_id = ?2",
                rusqlite::params![session_id, entity_id],
                |row| row.get(0),
            )
            .unwrap()
        })
    }

    #[tokio::test]
    async fn entity_dedup_keeps_first_identity_and_widens_the_seen_window() {
        let store = SqliteStore::open_in_memory().unwrap();
        let first = seed_frame(&store, 1_000, "acme/api").await;
        store
            .graph_apply_hits(
                None,
                first,
                1_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.61)],
            )
            .await
            .unwrap();
        let second = seed_frame(&store, 5_000, "ACME/Api").await;
        store
            .graph_apply_hits(
                None,
                second,
                5_000,
                &[hit(EntityKind::Repo, "ACME/Api", "acme/api", 0.92)],
            )
            .await
            .unwrap();
        let third = seed_frame(&store, 3_000, "acme/api").await;
        store
            .graph_apply_hits(
                None,
                third,
                3_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.55)],
            )
            .await
            .unwrap();

        assert_eq!(store.graph_table_counts().await.unwrap().entities, 1);
        let (_, canonical, normalized_key, source, confidence, first_seen, last_seen) =
            entity_row(&store, EntityKind::Repo, "acme/api");
        assert_eq!(canonical, "acme/api", "the first hit owns the display form");
        assert_eq!(
            normalized_key, "acme/api",
            "the dedup identity is immutable"
        );
        assert_eq!(source, "heuristic");
        assert!(
            (confidence - 0.92).abs() < 1e-12,
            "confidence is the maximum"
        );
        assert_eq!(first_seen, 1_000);
        assert_eq!(last_seen, 5_000);

        // The same normalized key under another kind is a different entity.
        store
            .graph_apply_hits(
                None,
                third,
                3_000,
                &[hit(EntityKind::File, "acme/api", "acme/api", 0.4)],
            )
            .await
            .unwrap();
        assert_eq!(store.graph_table_counts().await.unwrap().entities, 2);
    }

    #[tokio::test]
    async fn link_upsert_counts_frames_keeps_max_confidence_and_aliases_dedupe() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(10_000)))
            .await
            .unwrap();
        let first = seed_frame(&store, 2_000, "acme/api").await;
        let mut aliased = hit(EntityKind::Repo, "acme/api", "acme/api", 0.75);
        aliased.aliases = vec!["API".to_string(), "api".to_string()];
        store
            .graph_apply_hits(
                Some(session_id),
                first,
                2_000,
                std::slice::from_ref(&aliased),
            )
            .await
            .unwrap();
        store
            .graph_apply_hits(
                Some(session_id),
                first,
                2_000,
                std::slice::from_ref(&aliased),
            )
            .await
            .unwrap();
        let second = seed_frame(&store, 4_000, "acme/api").await;
        store
            .graph_apply_hits(
                Some(session_id),
                second,
                4_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.93)],
            )
            .await
            .unwrap();
        store
            .graph_apply_hits(
                Some(session_id),
                first,
                2_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.5)],
            )
            .await
            .unwrap();

        let (entity_id, ..) = entity_row(&store, EntityKind::Repo, "acme/api");
        let (hit_count, confidence, first_seen, last_seen) =
            link_row(&store, session_id, entity_id);
        assert_eq!(hit_count, 4, "one evidence unit per processed frame");
        assert!((confidence - 0.93).abs() < 1e-12);
        assert_eq!(first_seen, 2_000);
        assert_eq!(last_seen, 4_000);
        assert_eq!(row_count(&store, "entity_aliases"), 2, "aliases dedupe");
        assert_eq!(row_count(&store, "session_entities"), 1);
    }

    #[tokio::test]
    async fn replace_session_links_updates_a_changed_set_exactly() {
        let store = SqliteStore::open_in_memory().unwrap();
        let target = store
            .insert_session(session(1_000, Some(5_000)))
            .await
            .unwrap();
        let other = store
            .insert_session(session(6_000, Some(9_000)))
            .await
            .unwrap();
        let first_frame = seed_frame(&store, 1_500, "acme/api").await;
        let second_frame = seed_frame(&store, 2_000, "example.com").await;
        let other_frame = seed_frame(&store, 6_500, "other.md").await;
        store
            .graph_apply_hits(
                Some(target),
                first_frame,
                1_500,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.8)],
            )
            .await
            .unwrap();
        store
            .graph_apply_hits(
                Some(target),
                second_frame,
                2_000,
                &[hit(EntityKind::Domain, "example.com", "example.com", 0.7)],
            )
            .await
            .unwrap();
        store
            .graph_apply_hits(
                Some(other),
                other_frame,
                6_500,
                &[hit(EntityKind::File, "other.md", "other.md", 0.55)],
            )
            .await
            .unwrap();
        assert_eq!(session_link_count(&store, target), 2);

        let replacement = vec![
            aggregate(
                EntityKind::Repo,
                "acme/api",
                "acme/api",
                0.92,
                2,
                1_200,
                2_100,
            ),
            aggregate(
                EntityKind::File,
                "notes.md",
                "notes.md",
                0.65,
                3,
                1_300,
                2_200,
            ),
        ];
        assert!(store
            .graph_replace_session_links(target, &replacement)
            .await
            .unwrap());

        // Exactly the incoming set, with the absolute aggregate values.
        assert_eq!(session_link_count(&store, target), 2);
        let (repo_id, ..) = entity_row(&store, EntityKind::Repo, "acme/api");
        assert_eq!(link_row(&store, target, repo_id), (2, 0.92, 1_200, 2_100));
        let (domain_id, ..) = entity_row(&store, EntityKind::Domain, "example.com");
        assert_eq!(
            session_entity_link_count(&store, target, domain_id),
            0,
            "the stale link is removed"
        );
        let (notes_id, canonical, ..) = entity_row(&store, EntityKind::File, "notes.md");
        assert_eq!(canonical, "notes.md");
        assert_eq!(link_row(&store, target, notes_id), (3, 0.65, 1_300, 2_200));

        // The rebuild is scoped to one session: the other session keeps its row.
        assert_eq!(session_link_count(&store, other), 1);
        let (other_id, ..) = entity_row(&store, EntityKind::File, "other.md");
        assert_eq!(link_row(&store, other, other_id), (1, 0.55, 6_500, 6_500));
    }

    #[tokio::test]
    async fn replace_session_links_is_a_no_op_for_an_identical_set() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(5_000)))
            .await
            .unwrap();
        let links = vec![
            aggregate(
                EntityKind::Repo,
                "acme/api",
                "acme/api",
                0.81,
                2,
                1_100,
                2_400,
            ),
            aggregate(
                EntityKind::Domain,
                "example.com",
                "example.com",
                0.6,
                1,
                1_900,
                1_900,
            ),
        ];
        assert!(store
            .graph_replace_session_links(session_id, &links)
            .await
            .unwrap());
        let counts_before = store.graph_table_counts().await.unwrap();
        let (repo_id, ..) = entity_row(&store, EntityKind::Repo, "acme/api");
        let repo_link_before = link_row(&store, session_id, repo_id);
        let entity_before = entity_row(&store, EntityKind::Repo, "acme/api");

        assert!(
            !store
                .graph_replace_session_links(session_id, &links)
                .await
                .unwrap(),
            "an identical rebuild reports no change"
        );
        // The kernel reaches this through `dyn Store`; the forwarding must reach the
        // implementation instead of falling back to the trait's "unsupported" default.
        assert!(
            !Store::graph_replace_session_links(&store, session_id, &links)
                .await
                .unwrap(),
            "the trait-object path reports the same unchanged outcome"
        );
        assert_eq!(store.graph_table_counts().await.unwrap(), counts_before);
        assert_eq!(link_row(&store, session_id, repo_id), repo_link_before);
        assert_eq!(
            entity_row(&store, EntityKind::Repo, "acme/api"),
            entity_before
        );
    }

    #[tokio::test]
    async fn replace_session_links_writes_missing_aliases_without_reporting_a_change() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(5_000)))
            .await
            .unwrap();
        // Seed the exact link state without aliases, as an earlier scan would have.
        let links = vec![aggregate(
            EntityKind::Repo,
            "acme/api",
            "acme/api",
            0.84,
            2,
            1_200,
            2_600,
        )];
        assert!(store
            .graph_replace_session_links(session_id, &links)
            .await
            .unwrap());
        assert_eq!(row_count(&store, "entity_aliases"), 0);
        let (entity_id, ..) = entity_row(&store, EntityKind::Repo, "acme/api");
        let link_before = link_row(&store, session_id, entity_id);

        // Identical hit_count/confidence/seen window plus one new alias: the alias is
        // recorded, but the link set did not change, so the answer stays `false`.
        let mut aliased = links[0].clone();
        aliased.aliases = vec!["ACME API".to_string()];
        assert!(
            !store
                .graph_replace_session_links(session_id, std::slice::from_ref(&aliased))
                .await
                .unwrap(),
            "an alias-only rebuild is not a link change"
        );
        assert_eq!(link_row(&store, session_id, entity_id), link_before);
        assert_eq!(session_link_count(&store, session_id), 1);
        assert_eq!(row_count(&store, "entity_aliases"), 1);
        let aliases: Vec<(String, String, f64)> = with_raw(&store, |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT alias, source, confidence FROM entity_aliases
                     WHERE entity_id = ?1 ORDER BY alias",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![entity_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap();
            rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
        });
        assert_eq!(
            aliases,
            vec![("ACME API".to_string(), "heuristic".to_string(), 0.84)]
        );

        // A second identical rebuild is a clean no-op: still `false`, no duplicate alias.
        assert!(!store
            .graph_replace_session_links(session_id, std::slice::from_ref(&aliased))
            .await
            .unwrap());
        assert_eq!(row_count(&store, "entity_aliases"), 1);
        assert_eq!(link_row(&store, session_id, entity_id), link_before);
    }

    #[tokio::test]
    async fn replace_session_links_does_not_inflate_hit_count_on_rebuild() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(5_000)))
            .await
            .unwrap();
        let first_frame = seed_frame(&store, 1_500, "acme/api").await;
        let second_frame = seed_frame(&store, 2_000, "acme/api").await;
        store
            .graph_apply_hits(
                Some(session_id),
                first_frame,
                1_500,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.8)],
            )
            .await
            .unwrap();
        store
            .graph_apply_hits(
                Some(session_id),
                second_frame,
                2_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.85)],
            )
            .await
            .unwrap();
        let (entity_id, ..) = entity_row(&store, EntityKind::Repo, "acme/api");
        assert_eq!(
            link_row(&store, session_id, entity_id),
            (2, 0.85, 1_500, 2_000)
        );

        let rebuilt = vec![aggregate(
            EntityKind::Repo,
            "acme/api",
            "acme/api",
            0.85,
            2,
            1_500,
            2_000,
        )];
        for _ in 0..3 {
            assert!(
                !store
                    .graph_replace_session_links(session_id, &rebuilt)
                    .await
                    .unwrap(),
                "an exact rebuild writes nothing"
            );
            assert_eq!(
                link_row(&store, session_id, entity_id).0,
                2,
                "hit_count stays the frame count across rebuilds"
            );
        }
        assert_eq!(session_link_count(&store, session_id), 1);
    }

    #[tokio::test]
    async fn replace_session_links_clears_links_with_an_empty_set() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(5_000)))
            .await
            .unwrap();
        let frame_id = seed_frame(&store, 1_500, "acme/api").await;
        store
            .graph_apply_hits(
                Some(session_id),
                frame_id,
                1_500,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.8)],
            )
            .await
            .unwrap();
        let entities_before = store.graph_table_counts().await.unwrap().entities;
        assert_eq!(session_link_count(&store, session_id), 1);

        assert!(
            store
                .graph_replace_session_links(session_id, &[])
                .await
                .unwrap(),
            "clearing an existing link is a change"
        );
        assert_eq!(session_link_count(&store, session_id), 0);
        assert_eq!(
            store.graph_table_counts().await.unwrap().entities,
            entities_before,
            "entity rows are never deleted by a link rebuild"
        );
        assert!(
            !store
                .graph_replace_session_links(session_id, &[])
                .await
                .unwrap(),
            "an already empty session stays a no-op"
        );
    }

    #[tokio::test]
    async fn replace_session_links_missing_session_is_a_no_op() {
        let store = SqliteStore::open_in_memory().unwrap();
        let missing = 4_242;
        let links = vec![aggregate(
            EntityKind::Repo,
            "acme/api",
            "acme/api",
            0.9,
            1,
            1_000,
            1_000,
        )];
        assert!(!store
            .graph_replace_session_links(missing, &links)
            .await
            .unwrap());
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(counts.entities, 0, "a missing session creates no entity");
        assert_eq!(counts.session_entities, 0);

        // Deletion race: the session vanishes between the kernel's read and this write.
        let session_id = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        let frame_id = seed_frame(&store, 1_500, "kept.md").await;
        store
            .graph_apply_hits(
                Some(session_id),
                frame_id,
                1_500,
                &[hit(EntityKind::File, "kept.md", "kept.md", 0.7)],
            )
            .await
            .unwrap();
        assert!(store.delete_unfrozen_session(session_id).await.unwrap());
        let rebuilt = vec![aggregate(
            EntityKind::File,
            "new.md",
            "new.md",
            0.6,
            1,
            1_500,
            1_500,
        )];
        assert!(!store
            .graph_replace_session_links(session_id, &rebuilt)
            .await
            .unwrap());
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(
            counts.entities, 1,
            "the deleted session cannot add an entity"
        );
        assert_eq!(counts.session_entities, 0);
    }

    #[tokio::test]
    async fn replace_session_links_widens_entities_without_rewriting_identity() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(5_000)))
            .await
            .unwrap();
        let frame_id = seed_frame(&store, 1_000, "acme/api").await;
        store
            .graph_apply_hits(
                Some(session_id),
                frame_id,
                1_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.61)],
            )
            .await
            .unwrap();
        let (entity_id, canonical, key, source, confidence, _, _) =
            entity_row(&store, EntityKind::Repo, "acme/api");
        assert_eq!(canonical, "acme/api");
        assert_eq!(source, "heuristic");
        assert!((confidence - 0.61).abs() < 1e-12);

        let mut rebuilt = aggregate(
            EntityKind::Repo,
            "ACME/Api",
            "acme/api",
            0.97,
            2,
            500,
            3_000,
        );
        rebuilt.aliases = vec!["API".to_string()];
        assert!(store
            .graph_replace_session_links(session_id, std::slice::from_ref(&rebuilt))
            .await
            .unwrap());

        let (
            after_id,
            after_canonical,
            after_key,
            after_source,
            after_confidence,
            after_first,
            after_last,
        ) = entity_row(&store, EntityKind::Repo, "acme/api");
        assert_eq!(after_id, entity_id);
        assert_eq!(
            after_canonical, canonical,
            "the first creating hit owns the display form"
        );
        assert_eq!(after_key, key);
        assert_eq!(after_source, source);
        assert!(
            (after_confidence - 0.97).abs() < 1e-12,
            "confidence widens to the maximum"
        );
        assert_eq!(after_first, 500, "the seen window widens earlier");
        assert_eq!(after_last, 3_000, "the seen window widens later");
        assert_eq!(
            link_row(&store, session_id, after_id),
            (2, 0.97, 500, 3_000)
        );
        let aliases: Vec<(String, String, f64)> = with_raw(&store, |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT alias, source, confidence FROM entity_aliases
                     WHERE entity_id = ?1 ORDER BY alias",
                )
                .unwrap();
            let rows = stmt
                .query_map(rusqlite::params![after_id], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })
                .unwrap();
            rows.collect::<rusqlite::Result<Vec<_>>>().unwrap()
        });
        assert_eq!(
            aliases,
            vec![("API".to_string(), "heuristic".to_string(), 0.97)]
        );
    }

    #[tokio::test]
    async fn graph_session_at_keeps_the_interval_contract_but_extraction_is_ownership_only() {
        let store = SqliteStore::open_in_memory().unwrap();
        let first = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        let second = store
            .insert_session(session(1_500, Some(3_000)))
            .await
            .unwrap();
        let open = store.insert_session(session(4_000, None)).await.unwrap();

        assert_eq!(
            store
                .graph_session_at(1_000)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            first
        );
        assert_eq!(
            store
                .graph_session_at(1_500)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            first,
            "the lowest session id wins on an overlap"
        );
        assert_eq!(
            store
                .graph_session_at(1_999)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            first
        );
        assert_eq!(
            store
                .graph_session_at(2_000)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            second
        );
        assert_eq!(
            store
                .graph_session_at(2_999)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            second
        );
        assert!(store.graph_session_at(3_000).await.unwrap().is_none());
        assert_eq!(
            store
                .graph_session_at(i64::MAX)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            open,
            "an open session is open-ended"
        );

        // Extraction and linking do not use the interval primitive: an unowned frame that
        // both intervals contain reports no session.
        let unowned = seed_frame(&store, 2_500, "content text here").await;
        let input = store.graph_frame_input(unowned).await.unwrap().unwrap();
        assert_eq!(input.frame_id, unowned);
        assert_eq!(input.captured_at, 2_500);
        assert_eq!(input.app_hint.as_deref(), Some("Code"));
        assert_eq!(input.window_title.as_deref(), Some("main.rs - acme"));
        assert!(input.browser_url.is_none());
        assert_eq!(input.content_text.as_deref(), Some("content text here"));
        assert!(
            input.session.is_none(),
            "an unowned frame links to no session"
        );

        // Ownership is the only resolution: the same frame owned by `second` reports it.
        assert!(
            store
                .assign_frames_session(&[unowned], Some(second))
                .await
                .unwrap()
                == 1
        );
        let owned = store.graph_frame_input(unowned).await.unwrap().unwrap();
        let owned_session = owned.session.expect("an owned frame reports its owner");
        assert_eq!(owned_session.session_id, second);
        assert_eq!(owned_session.tool, None);
        assert_eq!(owned_session.host, Some(SessionHost::Ide));
        assert_eq!(owned_session.started_at, 1_500);
        assert_eq!(owned_session.ended_at, Some(3_000));

        let bare = store.insert_frame(frame(3_500)).await.unwrap();
        let bare_input = store.graph_frame_input(bare).await.unwrap().unwrap();
        assert!(bare_input.content_text.is_none(), "no frame_text row");
        assert!(bare_input.session.is_none(), "an unowned frame in a gap");
        assert!(store
            .graph_frame_input(i64::MAX - 1)
            .await
            .unwrap()
            .is_none());

        // No owning session still contributes entities, but no link row.
        store
            .graph_apply_hits(
                None,
                bare,
                3_500,
                &[hit(EntityKind::File, "notes.md", "notes.md", 0.8)],
            )
            .await
            .unwrap();
        assert_eq!(
            store.graph_table_counts().await.unwrap().session_entities,
            0
        );
        store
            .graph_apply_hits(
                Some(second),
                unowned,
                2_500,
                &[hit(EntityKind::File, "notes.md", "notes.md", 0.8)],
            )
            .await
            .unwrap();
        assert_eq!(
            store.graph_table_counts().await.unwrap().session_entities,
            1
        );
    }

    #[tokio::test]
    async fn owned_frame_links_to_its_owner_not_the_lowest_overlapping_interval() {
        let store = SqliteStore::open_in_memory().unwrap();
        // Different tracks overlap in time (`03 §7e`): the lower id owns the wider span.
        let lower = store
            .insert_session(session(1_000, Some(9_000)))
            .await
            .unwrap();
        let higher = store
            .insert_session(session(2_000, Some(3_000)))
            .await
            .unwrap();
        assert!(lower < higher);
        let frame_id = seed_frame(&store, 2_500, "acme/api").await;
        assert!(
            store
                .assign_frames_session(&[frame_id], Some(higher))
                .await
                .unwrap()
                == 1
        );
        // The retained interval primitive would answer `lower`; ownership is the only
        // resolution extraction uses (`03 §7e` final ruling).
        assert_eq!(
            store
                .graph_session_at(2_500)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            lower
        );

        let input = store.graph_frame_input(frame_id).await.unwrap().unwrap();
        let owner = input.session.expect("the frame has an authoritative owner");
        assert_eq!(owner.session_id, higher);
        assert_eq!(owner.started_at, 2_000);
        assert_eq!(owner.ended_at, Some(3_000));

        // An otherwise identical unowned frame links to nobody, even though the interval
        // primitive would still answer `lower` for it.
        let unowned_frame = seed_frame(&store, 2_500, "acme/api").await;
        let unowned_input = store
            .graph_frame_input(unowned_frame)
            .await
            .unwrap()
            .unwrap();
        assert!(
            unowned_input.session.is_none(),
            "an unowned frame links to nobody"
        );

        // The evidence links to the owner, so clustering never sees it under the lower id.
        store
            .graph_apply_hits(
                Some(owner.session_id),
                frame_id,
                2_500,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.9)],
            )
            .await
            .unwrap();
        let links = store.graph_cluster_input(9_500).await.unwrap().links;
        let linked_sessions: Vec<i64> = links.iter().map(|link| link.session_id).collect();
        assert_eq!(linked_sessions, vec![higher]);
    }

    #[tokio::test]
    async fn unowned_frame_inside_overlapping_sessions_has_no_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        let lower = store
            .insert_session(session(1_000, Some(9_000)))
            .await
            .unwrap();
        let _higher = store
            .insert_session(session(2_000, Some(3_000)))
            .await
            .unwrap();
        // `insert_frame` leaves `frames.session_id` NULL: linkage is ownership-only, so the
        // frame reports no session even though both intervals contain it.
        let frame_id = seed_frame(&store, 2_500, "notes.md").await;

        let input = store.graph_frame_input(frame_id).await.unwrap().unwrap();
        assert!(input.session.is_none(), "no owner means no session");
        // The retained interval primitive still answers `lower` for its own contract.
        assert_eq!(
            store
                .graph_session_at(2_500)
                .await
                .unwrap()
                .unwrap()
                .session_id,
            lower
        );
    }

    #[tokio::test]
    async fn unowned_frame_between_sessions_has_no_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        store.insert_session(session(4_000, None)).await.unwrap();
        let frame_id = seed_frame(&store, 3_000, "gap").await;

        let input = store.graph_frame_input(frame_id).await.unwrap().unwrap();
        assert!(
            input.session.is_none(),
            "linkage is ownership-only, so an unowned frame has no session"
        );
    }

    #[tokio::test]
    async fn dangling_owner_id_has_no_session() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        let frame_id = seed_frame(&store, 1_500, "x").await;
        assert!(
            store
                .assign_frames_session(&[frame_id], Some(session_id))
                .await
                .unwrap()
                == 1
        );
        // `ON DELETE SET NULL` prevents this state; simulate the stale reference the
        // defensive branch handles so a corrupt row cannot fail the extraction job.
        with_raw(&store, |conn| {
            conn.execute_batch("PRAGMA foreign_keys = OFF").unwrap();
            conn.execute(
                "UPDATE frames SET session_id = ?1 WHERE id = ?2",
                rusqlite::params![session_id + 100, frame_id],
            )
            .unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        });

        let input = store.graph_frame_input(frame_id).await.unwrap().unwrap();
        assert!(
            input.session.is_none(),
            "a dangling owner id reports no session (ownership-only)"
        );
    }

    #[tokio::test]
    async fn frame_ids_after_scans_oldest_first_from_the_cursor() {
        let store = SqliteStore::open_in_memory().unwrap();
        let ids: Vec<i64> = {
            let mut ids = Vec::new();
            for at in [1_000, 2_000, 3_000] {
                ids.push(seed_frame(&store, at, "x").await);
            }
            ids
        };

        assert_eq!(
            store.graph_frame_ids_after(None, 2).await.unwrap(),
            vec![ids[0], ids[1]]
        );
        assert_eq!(
            store.graph_frame_ids_after(Some(ids[0]), 10).await.unwrap(),
            vec![ids[1], ids[2]]
        );
        assert!(store
            .graph_frame_ids_after(Some(ids[2]), 10)
            .await
            .unwrap()
            .is_empty());
        assert!(store
            .graph_frame_ids_after(None, 0)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn typed_override_insert_validates_then_enqueues_one_cluster_job() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        store
            .graph_apply_thread_plan(&ThreadPlan {
                threads: vec![draft(
                    "seed:1",
                    "Thread One",
                    1_000,
                    2_000,
                    vec![member(session_id, 0.9)],
                )],
                ..ThreadPlan::default()
            })
            .await
            .unwrap();
        let thread_id = store.graph_stored_threads().await.unwrap()[0].id;

        // Wrong kind/thread-id shapes and wrong payload variants are rejected before any
        // write, so no override row and no cluster job survive the failure.
        let missing_thread = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: None,
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: "Renamed".to_string(),
            },
        };
        assert!(store
            .graph_insert_thread_override(&missing_thread, 5_000)
            .await
            .is_err());
        let merge_without_target = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Merge,
            thread_id: Some(thread_id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Merge {
                session_ids: vec![session_id],
            },
        };
        assert!(store
            .graph_insert_thread_override(&merge_without_target, 5_000)
            .await
            .is_err());
        let mismatched_payload = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: Some(thread_id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Merge {
                session_ids: vec![session_id],
            },
        };
        assert!(store
            .graph_insert_thread_override(&mismatched_payload, 5_000)
            .await
            .is_err());
        let empty_title = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: Some(thread_id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: "   ".to_string(),
            },
        };
        assert!(store
            .graph_insert_thread_override(&empty_title, 5_000)
            .await
            .is_err());
        // A well-shaped draft whose thread does not exist fails the FK and must roll the
        // enqueued cluster job back with it.
        let unknown_thread = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: Some(thread_id + 100),
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: "Ghost".to_string(),
            },
        };
        assert!(store
            .graph_insert_thread_override(&unknown_thread, 5_000)
            .await
            .is_err());
        assert_eq!(row_count(&store, "thread_overrides"), 0);
        assert_eq!(job_count(&store, "graph_cluster", "pending"), 0);

        // A valid override persists the typed payload and enqueues exactly one job.
        let rename = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: Some(thread_id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: "Renamed thread".to_string(),
            },
        };
        let rename_id = store
            .graph_insert_thread_override(&rename, 6_000)
            .await
            .unwrap();
        assert_eq!(job_count(&store, "graph_cluster", "pending"), 1);
        // A second rename with an earlier creation time must sort before the first one,
        // proving the (created_at, id) read order rather than insertion order.
        let earlier = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: Some(thread_id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: "Final name".to_string(),
            },
        };
        let earlier_id = store
            .graph_insert_thread_override(&earlier, 5_000)
            .await
            .unwrap();
        assert_eq!(job_count(&store, "graph_cluster", "pending"), 2);

        let overrides = store.graph_thread_overrides().await.unwrap();
        assert_eq!(overrides.len(), 2);
        assert_eq!(overrides[0].id, earlier_id);
        assert_eq!(overrides[0].created_at, 5_000);
        assert_eq!(
            overrides[0].payload,
            ThreadOverridePayload::Rename {
                title: "Final name".to_string()
            }
        );
        assert_eq!(overrides[1].id, rename_id);
        assert_eq!(overrides[1].created_at, 6_000);
        assert_eq!(overrides[1].thread_id, Some(thread_id));
        assert_eq!(overrides[1].target_thread_id, None);
    }

    #[tokio::test]
    async fn thread_plan_upserts_syncs_deletes_and_rewrites_overrides_idempotently() {
        let store = SqliteStore::open_in_memory().unwrap();
        let first_session = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        let second_session = store
            .insert_session(session(3_000, Some(4_000)))
            .await
            .unwrap();
        let third_session = store
            .insert_session(session(5_000, Some(6_000)))
            .await
            .unwrap();

        store
            .graph_apply_thread_plan(&ThreadPlan {
                threads: vec![
                    draft(
                        "seed:1,2",
                        "First",
                        1_000,
                        4_000,
                        vec![member(first_session, 0.9), member(second_session, 0.8)],
                    ),
                    draft(
                        "seed:3",
                        "Second",
                        5_000,
                        6_000,
                        vec![member(third_session, 0.7)],
                    ),
                ],
                ..ThreadPlan::default()
            })
            .await
            .unwrap();
        let stored = store.graph_stored_threads().await.unwrap();
        assert_eq!(stored.len(), 2);
        let source_id = stored
            .iter()
            .find(|thread| thread.cluster_key == "seed:1,2")
            .unwrap()
            .id;
        let survivor_id = stored
            .iter()
            .find(|thread| thread.cluster_key == "seed:3")
            .unwrap()
            .id;
        assert_eq!(row_count(&store, "thread_sessions"), 3);

        // A merge override source folds into the survivor; the store must re-point the
        // override at the survivor before deleting the emptied source thread.
        let merge_id = store
            .graph_insert_thread_override(
                &ThreadOverrideDraft {
                    kind: ThreadOverrideKind::Merge,
                    thread_id: Some(source_id),
                    target_thread_id: Some(survivor_id),
                    payload: ThreadOverridePayload::Merge {
                        session_ids: vec![first_session, second_session],
                    },
                },
                7_000,
            )
            .await
            .unwrap();

        let merged = ThreadPlan {
            threads: vec![draft(
                "seed:3",
                "Survivor",
                1_000,
                6_000,
                vec![
                    member(first_session, 0.9),
                    member(second_session, 0.8),
                    member(third_session, 0.7),
                ],
            )],
            delete_thread_ids: vec![source_id],
            override_updates: vec![ThreadOverrideUpdate {
                override_id: merge_id,
                thread_id: survivor_id,
                target_thread_id: survivor_id,
            }],
        };
        store.graph_apply_thread_plan(&merged).await.unwrap();

        let stored = store.graph_stored_threads().await.unwrap();
        assert_eq!(stored.len(), 1);
        let survivor = &stored[0];
        assert_eq!(
            survivor.id, survivor_id,
            "the UNIQUE cluster_key keeps the existing thread id"
        );
        assert_eq!(survivor.title, "Survivor");
        assert_eq!(survivor.first_seen_at, 1_000);
        assert_eq!(survivor.last_seen_at, 6_000);
        assert_eq!(survivor.sessions.len(), 3);
        assert_eq!(thread_session_count(&store, survivor_id), 3);
        assert_eq!(row_count(&store, "thread_sessions"), 3);
        let overrides = store.graph_thread_overrides().await.unwrap();
        assert_eq!(
            overrides.len(),
            1,
            "the merge row survives the source delete"
        );
        assert_eq!(overrides[0].thread_id, Some(survivor_id));
        assert_eq!(overrides[0].target_thread_id, Some(survivor_id));

        // Re-running the same plan is idempotent: same thread, same members, one override.
        store.graph_apply_thread_plan(&merged).await.unwrap();
        let stored_again = store.graph_stored_threads().await.unwrap();
        assert_eq!(stored_again.len(), 1);
        assert_eq!(stored_again[0].id, survivor_id);
        assert_eq!(stored_again[0].sessions.len(), 3);
        assert_eq!(row_count(&store, "thread_sessions"), 3);
        assert_eq!(row_count(&store, "thread_overrides"), 1);
        assert_eq!(thread_session_count(&store, survivor_id), 3);
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(counts.threads, 1);
        assert_eq!(counts.thread_sessions, 3);
        // Applying a plan never enqueues: only the override inserts above did.
        assert_eq!(job_count(&store, "graph_cluster", "pending"), 1);
    }

    /// Documents the delete side of the thread plan: a rename row is not membership management,
    /// so a plan that no longer produces its thread's `cluster_key` deletes the row and the
    /// `thread_overrides.thread_id` cascade removes the user's title. `delete_thread_ids` is only
    /// "safe" for threads the planner does not retain (merge/split referenced ids are always
    /// retained); the rename title loss is recorded as a residual in `07`.
    #[tokio::test]
    async fn deleting_a_thread_cascades_its_rename_override() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        store
            .graph_apply_thread_plan(&ThreadPlan {
                threads: vec![draft(
                    "seed:1",
                    "Renamed candidate",
                    1_000,
                    2_000,
                    vec![member(session_id, 0.9)],
                )],
                ..ThreadPlan::default()
            })
            .await
            .unwrap();
        let thread_id = store.graph_stored_threads().await.unwrap()[0].id;
        store
            .graph_insert_thread_override(
                &ThreadOverrideDraft {
                    kind: ThreadOverrideKind::Rename,
                    thread_id: Some(thread_id),
                    target_thread_id: None,
                    payload: ThreadOverridePayload::Rename {
                        title: "Pinned".to_string(),
                    },
                },
                3_000,
            )
            .await
            .unwrap();
        assert_eq!(row_count(&store, "thread_overrides"), 1);

        // A later clustering run no longer produces the key: the planner names the thread for
        // deletion, the store deletes it, and the rename row cascades away with it.
        store
            .graph_apply_thread_plan(&ThreadPlan {
                delete_thread_ids: vec![thread_id],
                ..ThreadPlan::default()
            })
            .await
            .unwrap();
        assert!(store.graph_stored_threads().await.unwrap().is_empty());
        assert_eq!(row_count(&store, "thread_overrides"), 0);
    }

    #[tokio::test]
    async fn table_counts_and_backfill_status_track_the_queue() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert_eq!(
            store.graph_table_counts().await.unwrap(),
            GraphTableCounts::default()
        );
        let empty = store.graph_backfill_status().await.unwrap();
        assert_eq!(empty.cursor, None);
        assert_eq!(empty.max_frame_id, None);
        assert_eq!(empty.pending_extract, 0);
        assert_eq!(empty.pending_backfill, 0);
        assert_eq!(empty.pending_cluster, 0);
        assert_eq!(empty.pending_normalize, 0);
        assert_eq!(empty.done_extract, 0);

        let first = seed_frame(&store, 1_000, "a").await;
        let second = seed_frame(&store, 2_000, "b").await;
        let third = seed_frame(&store, 3_000, "c").await;
        let session_id = store
            .insert_session(session(1_000, Some(3_000)))
            .await
            .unwrap();
        store
            .graph_apply_hits(
                Some(session_id),
                first,
                1_000,
                &[hit(EntityKind::Domain, "example.com", "example.com", 0.9)],
            )
            .await
            .unwrap();
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(counts.entities, 1);
        assert_eq!(counts.entity_aliases, 0);
        assert_eq!(counts.session_entities, 1);
        assert_eq!(counts.threads, 0);
        assert_eq!(counts.thread_sessions, 0);

        let extract = store
            .enqueue_job(NewJob {
                kind: JobKind::GraphExtract,
                frame_id: Some(first),
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await
            .unwrap();
        let claimed = store
            .claim_jobs(&[JobKind::GraphExtract], 10, 10_000)
            .await
            .unwrap();
        assert_eq!(claimed.len(), 1);
        store.complete_job(extract).await.unwrap();
        store
            .enqueue_job(NewJob {
                kind: JobKind::GraphExtract,
                frame_id: Some(second),
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await
            .unwrap();
        store
            .enqueue_job(NewJob {
                kind: JobKind::GraphBackfill,
                frame_id: None,
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await
            .unwrap();
        store
            .enqueue_job(NewJob {
                kind: JobKind::GraphCluster,
                frame_id: None,
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await
            .unwrap();
        store
            .enqueue_job(NewJob {
                kind: JobKind::GraphNormalize,
                frame_id: None,
                priority: 0,
                max_attempts: 3,
                not_before: 0,
            })
            .await
            .unwrap();

        let running = store
            .claim_jobs(&[JobKind::GraphCluster], 10, 10_000)
            .await
            .unwrap();
        assert_eq!(running.len(), 1, "the cluster job is now running");
        let status = store.graph_backfill_status().await.unwrap();
        assert_eq!(
            status.cursor,
            Some(second),
            "the cursor is MAX over extract jobs in every state"
        );
        assert_eq!(status.max_frame_id, Some(third));
        assert_eq!(status.pending_extract, 1);
        assert_eq!(status.pending_backfill, 1);
        assert_eq!(
            status.pending_cluster, 1,
            "pending plus running graph_cluster"
        );
        assert_eq!(status.pending_normalize, 1);
        assert_eq!(status.done_extract, 1);
    }

    #[tokio::test]
    async fn cluster_input_exposes_sessions_and_links_with_entity_display_data() {
        let store = SqliteStore::open_in_memory().unwrap();
        let open = store.insert_session(session(1_000, None)).await.unwrap();
        let closed = store
            .insert_session(session(5_000, Some(6_000)))
            .await
            .unwrap();
        let frame_id = seed_frame(&store, 2_000, "acme/api").await;
        store
            .graph_apply_hits(
                Some(open),
                frame_id,
                2_000,
                &[hit(EntityKind::Repo, "acme/api", "acme/api", 0.85)],
            )
            .await
            .unwrap();

        let input = store.graph_cluster_input(9_000).await.unwrap();
        assert_eq!(input.now_ms, 9_000);
        assert_eq!(
            input.sessions,
            vec![
                GraphSessionInterval {
                    session_id: open,
                    started_at: 1_000,
                    ended_at: None,
                },
                GraphSessionInterval {
                    session_id: closed,
                    started_at: 5_000,
                    ended_at: Some(6_000),
                },
            ]
        );
        assert_eq!(input.links.len(), 1);
        let link = &input.links[0];
        assert_eq!(link.session_id, open);
        assert_eq!(link.entity_kind, EntityKind::Repo);
        assert_eq!(link.canonical_name, "acme/api");
        assert!((link.confidence - 0.85).abs() < 1e-12);
        assert_eq!(link.last_seen_at, 2_000);
    }

    #[tokio::test]
    async fn sessions_with_summaries_is_newest_first_and_bounded() {
        let store = SqliteStore::open_in_memory().unwrap();
        let older = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        let titled = store
            .insert_session(session(3_000, Some(4_000)))
            .await
            .unwrap();
        let newest = store
            .insert_session(session(5_000, Some(6_000)))
            .await
            .unwrap();
        let bare = store
            .insert_session(session(7_000, Some(8_000)))
            .await
            .unwrap();
        store
            .set_session_title_summary(older, "Earlier", "earlier summary", "test-model")
            .await
            .unwrap();
        store
            .set_session_title_summary(titled, "Only a title", "", "test-model")
            .await
            .unwrap();
        store
            .set_session_title_summary(newest, "Newest", "newest summary", "test-model")
            .await
            .unwrap();
        let _ = bare;

        let rows = store.graph_sessions_with_summaries(2).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].session_id, newest);
        assert_eq!(rows[0].title.as_deref(), Some("Newest"));
        assert_eq!(rows[0].summary.as_deref(), Some("newest summary"));
        assert_eq!(rows[0].started_at, 5_000);
        assert_eq!(rows[0].ended_at, Some(6_000));
        assert_eq!(rows[0].context_key, "focus:code");
        assert_eq!(rows[1].session_id, titled);
        assert_eq!(rows[1].title.as_deref(), Some("Only a title"));
        assert_eq!(
            store.graph_sessions_with_summaries(0).await.unwrap(),
            Vec::new()
        );
    }

    #[tokio::test]
    async fn normalization_is_additive_and_never_rewrites_heuristic_identity() {
        let store = SqliteStore::open_in_memory().unwrap();
        let frame_id = seed_frame(&store, 1_000, "acme/api").await;
        let mut heuristic = hit(EntityKind::Repo, "acme/api", "acme/api", 0.9);
        heuristic.aliases = vec!["ACME API".to_string()];
        store
            .graph_apply_hits(None, frame_id, 1_000, &[heuristic])
            .await
            .unwrap();
        let (repo_id, canonical, normalized_key, source, confidence, first_seen, last_seen) =
            entity_row(&store, EntityKind::Repo, "acme/api");

        let catalog = store.graph_entity_catalog(10).await.unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].id, repo_id);
        assert_eq!(catalog[0].kind, EntityKind::Repo);
        assert_eq!(catalog[0].canonical_name, "acme/api");
        assert_eq!(
            catalog[0].aliases,
            vec![("ACME API".to_string(), AliasSource::Heuristic)]
        );

        let proposals = vec![
            NormalizationProposal {
                kind: EntityKind::Repo,
                canonical_name: "acme/api".to_string(),
                normalized_key: "acme/api".to_string(),
                aliases: vec!["Acme".to_string()],
                matched_entity_id: Some(repo_id),
                confidence: 0.95,
            },
            NormalizationProposal {
                kind: EntityKind::Document,
                canonical_name: "ScreenSearch".to_string(),
                normalized_key: "screensearch".to_string(),
                aliases: vec!["screensearch-v2c".to_string()],
                matched_entity_id: None,
                confidence: 0.7,
            },
            // A "new" proposal whose identity already exists (a heuristic row here) must
            // not create a second row; its alias attaches to the existing entity.
            NormalizationProposal {
                kind: EntityKind::Repo,
                canonical_name: "acme/api".to_string(),
                normalized_key: "acme/api".to_string(),
                aliases: vec!["API".to_string()],
                matched_entity_id: None,
                confidence: 0.5,
            },
        ];
        let outcome = store.graph_apply_normalization(&proposals).await.unwrap();
        assert_eq!(
            outcome,
            NormalizationOutcome {
                entities_created: 1,
                aliases_inserted: 3,
            }
        );

        // The heuristic row keeps its identity, provenance, confidence and seen window.
        let (
            after_id,
            after_canonical,
            after_key,
            after_source,
            after_confidence,
            after_first,
            after_last,
        ) = entity_row(&store, EntityKind::Repo, "acme/api");
        assert_eq!(after_id, repo_id);
        assert_eq!(after_canonical, canonical);
        assert_eq!(after_key, normalized_key);
        assert_eq!(after_source, source);
        assert!((after_confidence - confidence).abs() < 1e-12);
        assert_eq!(after_first, first_seen);
        assert_eq!(after_last, last_seen);

        // The promoted document is an additive `llm` row.
        let (
            _,
            promoted_canonical,
            promoted_key,
            promoted_source,
            promoted_confidence,
            promoted_first,
            promoted_last,
        ) = entity_row(&store, EntityKind::Document, "screensearch");
        assert_eq!(promoted_canonical, "ScreenSearch");
        assert_eq!(promoted_key, "screensearch");
        assert_eq!(promoted_source, "llm");
        assert!((promoted_confidence - 0.7).abs() < 1e-12);
        assert_eq!(promoted_first, promoted_last);
        assert!(promoted_first >= first_seen);

        let catalog = store.graph_entity_catalog(10).await.unwrap();
        let repo_row = catalog.iter().find(|row| row.id == repo_id).unwrap();
        assert_eq!(
            repo_row.aliases,
            vec![
                ("ACME API".to_string(), AliasSource::Heuristic),
                ("Acme".to_string(), AliasSource::Llm),
                ("API".to_string(), AliasSource::Llm),
            ]
        );
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(counts.entities, 2);
        assert_eq!(counts.entity_aliases, 4);

        // Replaying the same proposals is a no-op: no new entity and no new alias.
        let replay = store.graph_apply_normalization(&proposals).await.unwrap();
        assert_eq!(replay, NormalizationOutcome::default());
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(counts.entities, 2);
        assert_eq!(counts.entity_aliases, 4);
    }

    #[tokio::test]
    async fn replayed_hits_batch_cannot_duplicate_entity_or_link_rows() {
        let store = SqliteStore::open_in_memory().unwrap();
        let session_id = store
            .insert_session(session(1_000, Some(2_000)))
            .await
            .unwrap();
        let frame_id = seed_frame(&store, 1_500, "acme/api example.com").await;
        let hits = vec![
            hit(EntityKind::Repo, "acme/api", "acme/api", 0.8),
            hit(EntityKind::Domain, "example.com", "example.com", 0.7),
        ];
        for _ in 0..3 {
            store
                .graph_apply_hits(Some(session_id), frame_id, 1_500, &hits)
                .await
                .unwrap();
        }
        let counts = store.graph_table_counts().await.unwrap();
        assert_eq!(counts.entities, 2);
        assert_eq!(counts.session_entities, 2);
        assert_eq!(counts.entity_aliases, 0);
        let total_hit_count: i64 = with_raw(&store, |conn| {
            conn.query_row("SELECT SUM(hit_count) FROM session_entities", [], |row| {
                row.get(0)
            })
            .unwrap()
        });
        assert_eq!(
            total_hit_count, 6,
            "three processed frames across two links"
        );
    }
}
