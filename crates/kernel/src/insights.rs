//! The 0.4.5 P8 insights engine (`03 §7j.1`, D10): five typed, *heuristic* statements over
//! the history the store already holds, plus the producer that regenerates them.
//!
//! Everything a claim says is derived from rows that exist: the `derive_*` functions below
//! are pure (no I/O, no clock read, no model) and take the store's generator reads as plain
//! input slices, so each rule is unit-testable against constructed data and a pass is
//! deterministic for a given window. One pass covers one window — `[now - LOOKBACK_MS, now)`
//! truncated to whole minutes — and **replaces** every row of each kind whose window
//! overlaps it (`03 §7j.5`), so a pass can never accumulate duplicates.
//!
//! Every insight carries at least one evidence reference and the ordinals are 0-based and
//! dense (`03 §7j.2`). The store enforces the non-empty rule; [`run_insight_generate`] sweeps
//! afterwards ([`Store::unevidenced_insight_ids`]) and treats a violation as a defect, which
//! is the one case that dead-letters the job instead of retrying it.
//!
//! Nothing here runs on the capture hot path: the job is enqueued by the idle producer below,
//! and by [`enqueue_on_demand`] (P9's "regenerate now"), never by a capture tick. A failure
//! degrades to "the insights are behind", never to a capture stall.
//!
//! Thresholds are **kernel constants, never settings** (0.4.5 D6, `03 §8`): the shipped app
//! exposes no knob for them, and the numbers come from `03 §7h.4` / `§7j.1`.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::task::JoinHandle;
use traits::{
    EntitySessionInput, EvidenceKind, InsightEvidence, InsightGenerator, InsightKind,
    InsightPayload, JobKind, NewInsight, NewJob, OpenLoopPayload, OpenLoopSource, OpenMarkInput,
    RecurrencePayload, ResumptionPayload, SemanticNeighbourPayload, SessionNeighbour,
    SessionNeighbourInput, Store, ThreadActivityInput, ThreadActivityPayload, ThreadSessionInput,
};

// Re-exported so the kernel's public seam (`run_insight_generate`) and its integration test
// name the same outcome type the worker pool maps onto the queue. The pool's lanes and this
// producer return it for exactly the same reason: one vocabulary for "done / retry /
// dead-letter" (`03 §5`).
pub use crate::worker_pool::Outcome;

/// The window one generation pass covers, ending at the pass clock: `03 §7h.4`'s 30-day
/// cluster horizon is the same "recent history" the clusterer plans over, so an occurrence
/// older than this is no longer part of a statement about now. A kernel constant, never a
/// setting (0.4.5 D6, `03 §8`).
pub const LOOKBACK_MS: i64 = 30 * 24 * 60 * 60 * 1_000;

/// The smallest gap between two occurrences of one entity that reads as a *recurrence* —
/// `03 §7h.4`'s full-weight decay horizon: inside it the clusterer already treats both
/// occurrences as one ongoing presence, so only a gap at or beyond it is "seen again". A
/// kernel constant, never a setting (0.4.5 D6, `03 §8`).
pub const RECURRENCE_GAP_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// The smallest gap after a thread's last activity that reads as a *resumption* (the same
/// full-weight horizon as [`RECURRENCE_GAP_MS`]), and therefore also the "abandoned mid-way"
/// threshold for an `open_loop`: a thread untouched for less than this is still in play, not
/// dropped. A kernel constant, never a setting (0.4.5 D6, `03 §8`).
pub const RESUMPTION_GAP_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// Minimum cosine similarity for a `semantic_neighbour` edge (`03 §7j.1`). `session_vectors`
/// is a cosine `vec0` table, so the store reads a **distance** (`1 - similarity`) and the
/// gate is `1.0 - distance >= NEIGHBOUR_MIN_SIMILARITY` — below it two sessions are merely
/// both about something, which is not a connection worth stating. A kernel constant, never a
/// setting (0.4.5 D6, `03 §8`).
pub const NEIGHBOUR_MIN_SIMILARITY: f64 = 0.80;

/// How many neighbours one `semantic_neighbour` insight lists, strongest first: the pass
/// states the few connections that matter instead of a neighbourhood (`03 §7j.1`). A kernel
/// constant, never a setting (0.4.5 D6, `03 §8`).
pub const NEIGHBOUR_TOP_K: u32 = 5;

/// How often the idle producer re-checks whether a pass is due. A kernel constant, never a
/// setting (0.4.5 D6, `03 §8`).
pub const TICK: Duration = Duration::from_secs(30);

/// Idle dwell that counts as "the machine is idle" for a generation pass — the same value as
/// the graph scheduler's normalization gate (`03 §7h.2`): insights are re-derived while the
/// user is away, never in the middle of work. A kernel constant, never a setting (0.4.5 D6,
/// `03 §8`).
pub const IDLE_MS: u64 = 300_000;

/// Attempts an `insight_generate` job may take before the queue dead-letters it (`03 §5`).
pub const MAX_ATTEMPTS: i64 = 3;

/// One minute in milliseconds: a pass window ends on a whole minute so two passes inside the
/// same minute describe the identical window and the second one replaces the first instead of
/// writing an overlapping twin.
const MINUTE_MS: i64 = 60_000;

/// One UTC day in milliseconds — the calendar bucket the `semantic_neighbour` rule compares
/// (`started_at / DAY_MS`), so "another day" is defined once. UTC is the honest default for a
/// stored unix timestamp; P9 may localise the bucket for display.
const DAY_MS: i64 = 86_400_000;

/// The platform idle source slot, shared with [`crate::graph_scheduler`] (which the
/// composition root fills when inference attaches). It is empty until then, and an empty slot
/// means "not idle", so the producer simply waits rather than guessing.
pub(crate) type IdleSlot = crate::graph_scheduler::IdleSlot;

/// Runs one generation pass over `[now_ms - LOOKBACK_MS, now_ms)`, truncated to whole
/// minutes, then sweeps staleness and the evidence invariant.
///
/// Model-free by construction: it takes no embedder, no inference provider and no vision
/// provider, so it can run on a machine with every model lane unloaded. Each kind is written
/// with one `replace_insights` call (delete-overlapping + insert, `03 §7j.5`), so a pass is
/// atomic per kind and rows are never mutated under a reader.
///
/// Failure policy: a store error is a `Retry` (the queue's backoff and
/// [`MAX_ATTEMPTS`] bound it, and the previous rows of kinds not yet replaced stay readable);
/// an insight without evidence is a `DeadLetter`, because retrying a pass that just wrote
/// unevidenced rows would only produce them again. Logs carry counts and kind tokens only —
/// never a session, thread or entity id (`07:447`).
pub async fn run_insight_generate(store: &Arc<dyn Store>, now_ms: i64) -> Outcome {
    let window_end = now_ms - now_ms.rem_euclid(MINUTE_MS);
    let window_start = window_end - LOOKBACK_MS;

    let activity = match store
        .insight_thread_activity(window_start, window_end)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "insights: thread activity read failed");
            return Outcome::Retry(format!("read thread activity input: {error}"));
        }
    };
    let open_marks = match store.insight_open_marks(window_end).await {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "insights: open marks read failed");
            return Outcome::Retry(format!("read open mark input: {error}"));
        }
    };
    let entity_sessions = match store
        .insight_entity_sessions(window_start, window_end)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "insights: entity sessions read failed");
            return Outcome::Retry(format!("read entity session input: {error}"));
        }
    };
    // The KNN limit and the other-day rule do not commute: a subject with several same-day
    // sessions near it fills `NEIGHBOUR_TOP_K` with rows the rule then rejects, hiding a
    // qualifying neighbour one slot deeper. Widen until every subject either has enough
    // eligible neighbours or its candidate pool is spent, so "the nearest session on another
    // day" means exactly that.
    let mut depth = NEIGHBOUR_TOP_K.max(1);
    let neighbours = loop {
        let rows = match store
            .insight_session_neighbours(window_start, window_end, depth)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(error = %error, "insights: session neighbours read failed");
                return Outcome::Retry(format!("read session neighbour input: {error}"));
            }
        };
        if neighbours_are_complete(&rows, depth) || depth >= NEIGHBOUR_FETCH_CEIL {
            break rows;
        }
        depth = depth.saturating_mul(2).min(NEIGHBOUR_FETCH_CEIL);
        tracing::debug!(depth, "insights: widening the neighbour fetch");
    };
    let thread_sessions = match store
        .insight_thread_sessions(window_start, window_end)
        .await
    {
        Ok(rows) => rows,
        Err(error) => {
            tracing::warn!(error = %error, "insights: thread sessions read failed");
            return Outcome::Retry(format!("read thread session input: {error}"));
        }
    };

    // One batch per kind, derived before anything is written: a derivation that panicked or
    // a reader that returned junk never leaves the store half-regenerated by a later error.
    let passes: [(InsightKind, Vec<NewInsight>); 5] = [
        (
            InsightKind::ThreadActivity,
            derive_thread_activity(&activity, window_start, window_end),
        ),
        (
            InsightKind::OpenLoop,
            derive_open_loops(&open_marks, &thread_sessions, window_start, window_end),
        ),
        (
            InsightKind::Recurrence,
            derive_recurrence(&entity_sessions, window_start, window_end),
        ),
        (
            InsightKind::SemanticNeighbour,
            derive_semantic_neighbours(&neighbours, window_start, window_end),
        ),
        (
            InsightKind::Resumption,
            derive_resumption(&thread_sessions, window_start, window_end),
        ),
    ];

    let mut inserted = 0usize;
    for (kind, batch) in passes {
        let derived = batch.len();
        match store
            .replace_insights(kind, window_start, window_end, batch)
            .await
        {
            Ok(ids) => inserted += ids.len(),
            Err(error) => {
                tracing::warn!(kind = kind.as_str(), error = %error, "insights: replace failed");
                return Outcome::Retry(format!("replace {} insights: {error}", kind.as_str()));
            }
        }
        tracing::debug!(kind = kind.as_str(), derived, "insight kind regenerated");
    }

    // Staleness is a separate pass over every row: a referenced row that no longer exists
    // makes the statement unverifiable, and `07` C10 says such a row is flagged, never served
    // as truth without the flag. The next generation pass replaces it.
    match store.mark_insights_stale_where_evidence_missing().await {
        Ok(0) => {}
        Ok(marked) => tracing::debug!(marked, "insights: rows marked stale"),
        Err(error) => {
            tracing::warn!(error = %error, "insights: staleness sweep failed");
            return Outcome::Retry(format!("mark stale insights: {error}"));
        }
    }

    match store.unevidenced_insight_ids().await {
        Ok(ids) if ids.is_empty() => {}
        Ok(ids) => {
            tracing::error!(count = ids.len(), "insight evidence invariant violated");
            return Outcome::DeadLetter("insight evidence invariant violated".to_string());
        }
        Err(error) => {
            tracing::warn!(error = %error, "insights: evidence sweep failed");
            return Outcome::Retry(format!("sweep unevidenced insights: {error}"));
        }
    }

    tracing::debug!(inserted, "insight generation pass complete");
    Outcome::Complete {
        changed_data: inserted > 0,
    }
}

/// Builds one insight, assigning the evidence ordinals here (0, 1, 2 … in `refs` order) so no
/// generator can produce a gap or a duplicate ordinal by hand (`03 §7j.2`).
fn new_insight(
    window_start: i64,
    window_end: i64,
    payload: InsightPayload,
    refs: &[(EvidenceKind, i64)],
) -> NewInsight {
    NewInsight {
        window_start,
        window_end,
        payload,
        generator: InsightGenerator::Heuristic,
        evidence: refs
            .iter()
            .enumerate()
            .map(|(ordinal, &(kind, ref_id))| InsightEvidence {
                ordinal: ordinal as u32,
                kind,
                ref_id,
            })
            .collect(),
    }
}

/// Cosine **similarity** from the `session_vectors` cosine **distance** the store returns,
/// clamped to the mathematical range `[-1, 1]`.
///
/// vec0's distance is `1 - similarity` computed in floating point, so an identical pair can
/// come back a few ULP below zero and read as `1.0000000000000002`. A similarity above 1 is not
/// a fact about the data, so the clamp keeps the payload truthful instead of publishing float
/// noise as a measurement.
fn cosine_similarity(distance: f64) -> f64 {
    (1.0 - distance).clamp(-1.0, 1.0)
}

/// `thread_activity` (`03 §7j.1`): one statement per thread over the window — how many of its
/// sessions overlapped it, how long it was actually active inside it, and when it was first
/// and last touched.
///
/// The overlap is real arithmetic, not an estimate: a session contributes
/// `min(end, window_end) - max(started_at, window_start)` where `end` is `window_end` for a
/// session that is still open. A thread with no session overlapping the window produces
/// nothing — a zero-minute, zero-session row would be a fabricated statement.
///
/// Evidence: the thread (ordinal 0) then its member sessions in `started_at` order. Rows are
/// emitted in ascending `thread_id`, so the output is deterministic and order-stable.
pub fn derive_thread_activity(
    inputs: &[ThreadActivityInput],
    window_start: i64,
    window_end: i64,
) -> Vec<NewInsight> {
    let mut by_thread: BTreeMap<i64, Vec<&ThreadActivityInput>> = BTreeMap::new();
    for row in inputs {
        by_thread.entry(row.thread_id).or_default().push(row);
    }
    let mut out = Vec::new();
    for (thread_id, mut rows) in by_thread {
        rows.sort_by_key(|row| (row.started_at, row.session_id));
        let title = rows[0].title.clone();
        let mut session_count = 0u32;
        let mut active_ms = 0i64;
        let mut first_touched_at: Option<i64> = None;
        let mut last_touched_at: Option<i64> = None;
        let mut sessions = Vec::new();
        for row in rows {
            let end = row.ended_at.unwrap_or(window_end);
            let overlap_start = row.started_at.max(window_start);
            let overlap_end = end.min(window_end);
            // Half-open windows: a session that ends exactly at `window_start`, or starts
            // exactly at `window_end`, does not overlap the pass window.
            if overlap_start >= overlap_end {
                continue;
            }
            session_count += 1;
            active_ms += overlap_end - overlap_start;
            first_touched_at =
                Some(first_touched_at.map_or(row.started_at, |at| at.min(row.started_at)));
            last_touched_at = Some(last_touched_at.map_or(end, |at| at.max(end)));
            sessions.push(row.session_id);
        }
        let (Some(first_touched_at), Some(last_touched_at)) = (first_touched_at, last_touched_at)
        else {
            continue;
        };
        let mut refs = Vec::with_capacity(sessions.len() + 1);
        refs.push((EvidenceKind::Thread, thread_id));
        refs.extend(sessions.iter().map(|&id| (EvidenceKind::Session, id)));
        out.push(new_insight(
            window_start,
            window_end,
            InsightPayload::ThreadActivity(ThreadActivityPayload {
                thread_id,
                title,
                session_count,
                active_ms,
                first_touched_at,
                last_touched_at,
            }),
            &refs,
        ));
    }
    out
}

/// `open_loop` (`03 §7j.1`): something left unresolved, from two sources.
///
/// * `unresolved_mark` — every mark that is still open (`marks.resolved_at IS NULL`, which the
///   store reads) and was created before `window_end`. Its frame is evidence, so the loop can
///   be reopened from the row.
/// * `abandoned_thread` — a thread whose latest member session ended at or before
///   `window_end - RESUMPTION_GAP_MS`, i.e. it was dropped mid-way. A thread with an open
///   session is still running and is never a loop.
///
/// Evidence: `mark` + `frame` for a mark, `thread` + last `session` for a thread. Rows are
/// ordered marks-first by `mark_id`, then threads by `thread_id`, so the output is stable.
///
/// **Resolving a mark does not delete its `open_loop` row immediately, and that is deliberate.**
/// Staleness only fires when an evidence row itself disappears
/// (`mark_insights_stale_where_evidence_missing`); a resolved mark leaves its row in place, so the
/// insight stays valid-history until the next pass regenerates the window and the mark is simply
/// absent from `marks.resolved_at IS NULL`. The read path never claims freshness beyond
/// `generated_at` and `window_end`, and GETs never generate, so an external checker wanting the
/// post-resolution answer asks for a regenerated window rather than reading a stale row as truth.
pub fn derive_open_loops(
    marks: &[OpenMarkInput],
    threads: &[ThreadSessionInput],
    window_start: i64,
    window_end: i64,
) -> Vec<NewInsight> {
    let mut out = Vec::new();

    let mut open_marks: Vec<&OpenMarkInput> = marks
        .iter()
        .filter(|mark| mark.created_at < window_end)
        .collect();
    open_marks.sort_by_key(|mark| mark.mark_id);
    for mark in open_marks {
        out.push(new_insight(
            window_start,
            window_end,
            InsightPayload::OpenLoop(OpenLoopPayload {
                source: OpenLoopSource::UnresolvedMark,
                mark_id: Some(mark.mark_id),
                frame_id: Some(mark.frame_id),
                note: mark.note.clone(),
                thread_id: None,
                title: None,
                opened_at: mark.created_at,
                idle_ms: window_end - mark.created_at,
            }),
            &[
                (EvidenceKind::Mark, mark.mark_id),
                (EvidenceKind::Frame, mark.frame_id),
            ],
        ));
    }

    let abandoned_before = window_end - RESUMPTION_GAP_MS;
    let mut by_thread: BTreeMap<i64, Vec<&ThreadSessionInput>> = BTreeMap::new();
    for row in threads {
        by_thread.entry(row.thread_id).or_default().push(row);
    }
    for (thread_id, rows) in by_thread {
        // The thread's last activity is its member sessions' latest end (an open session
        // ends at `window_end`). Taking the maximum rather than the last-started row keeps
        // the answer honest when two member sessions overlap.
        let last = rows.iter().max_by_key(|row| {
            (
                row.ended_at.unwrap_or(window_end),
                row.started_at,
                row.session_id,
            )
        });
        let Some(last) = last.copied() else {
            continue;
        };
        // An open session means the thread is being worked on right now.
        let Some(ended_at) = last.ended_at else {
            continue;
        };
        if ended_at > abandoned_before {
            continue;
        }
        out.push(new_insight(
            window_start,
            window_end,
            InsightPayload::OpenLoop(OpenLoopPayload {
                source: OpenLoopSource::AbandonedThread,
                mark_id: None,
                frame_id: None,
                note: None,
                thread_id: Some(thread_id),
                title: Some(last.title.clone()),
                opened_at: ended_at,
                idle_ms: window_end - ended_at,
            }),
            &[
                (EvidenceKind::Thread, thread_id),
                (EvidenceKind::Session, last.session_id),
            ],
        ));
    }
    out
}

/// `recurrence` (`03 §7j.1`): the same entity seen again after at least
/// [`RECURRENCE_GAP_MS`] (`03 §7h.4`'s full-weight horizon).
///
/// One insight per *consecutive* pair of sessions touching the entity (ordered by
/// `started_at`), so an entity visited three times a week apart yields two statements, each
/// naming the two sessions that form the gap. A pair just inside the threshold is not a
/// recurrence — inside that horizon the clusterer treats the occurrences as one presence.
/// `occurrence_count` counts every session touching the entity inside the window, so the
/// payload states both the pair and the total.
///
/// Only sessions that *started* inside `[window_start, window_end)` count: the pass describes
/// the window it covers, and a session that began before it belongs to an earlier window.
///
/// Evidence: entity (0), the earlier session (1), the later session (2). Rows are emitted in
/// ascending `entity_id` then ascending pair order.
pub fn derive_recurrence(
    inputs: &[EntitySessionInput],
    window_start: i64,
    window_end: i64,
) -> Vec<NewInsight> {
    let mut by_entity: BTreeMap<i64, Vec<&EntitySessionInput>> = BTreeMap::new();
    for row in inputs {
        by_entity.entry(row.entity_id).or_default().push(row);
    }
    let mut out = Vec::new();
    for (entity_id, mut rows) in by_entity {
        rows.sort_by_key(|row| (row.session_started_at, row.session_id));
        // One row per (entity, session) link is the schema's primary key; a duplicate would
        // otherwise read as two occurrences of the same session.
        rows.dedup_by_key(|row| row.session_id);
        let occurrences: Vec<&EntitySessionInput> = rows
            .into_iter()
            .filter(|row| {
                row.session_started_at >= window_start && row.session_started_at < window_end
            })
            .collect();
        if occurrences.is_empty() {
            continue;
        }
        let occurrence_count = occurrences.len() as u32;
        for pair in occurrences.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            let gap_ms = second.session_started_at - first.session_started_at;
            if gap_ms < RECURRENCE_GAP_MS {
                continue;
            }
            out.push(new_insight(
                window_start,
                window_end,
                InsightPayload::Recurrence(RecurrencePayload {
                    entity_id,
                    entity_kind: first.entity_kind.clone(),
                    entity_name: first.canonical_name.clone(),
                    first_session_id: first.session_id,
                    first_seen_at: first.session_started_at,
                    second_session_id: second.session_id,
                    second_seen_at: second.session_started_at,
                    gap_ms,
                    occurrence_count,
                }),
                &[
                    (EvidenceKind::Entity, entity_id),
                    (EvidenceKind::Session, first.session_id),
                    (EvidenceKind::Session, second.session_id),
                ],
            ));
        }
    }
    out
}

/// Whether one KNN edge names a *different* session on **another UTC day** — the two conditions
/// that do not follow the distance ordering (the same day is one continuous piece of work, not a
/// connection).
///
/// This is the rule that makes a nearest-first fetch insufficient, so both the derivation and the
/// deepening decision ask it, in one place.
fn is_other_day_edge(row: &SessionNeighbourInput) -> bool {
    row.neighbour_session_id != row.session_id
        && row.session_started_at / DAY_MS != row.neighbour_started_at / DAY_MS
}

/// Whether one KNN edge is usable for `semantic_neighbour`: [`is_other_day_edge`] and a similarity
/// the floor accepts.
///
/// The floor is deliberately **not** part of the deepening decision: distance is monotone, so a row
/// below the floor means every row beyond it is below the floor too — reading deeper cannot rescue
/// it. Only the other-day rule can, because it discards rows regardless of rank.
fn eligible_similarity(row: &SessionNeighbourInput) -> Option<f64> {
    if !is_other_day_edge(row) {
        return None;
    }
    let similarity = cosine_similarity(row.distance);
    // vec0 never produces a NaN distance, but a corrupt row must be dropped here rather than
    // slip through an inverted comparison.
    if similarity.is_nan() || similarity < NEIGHBOUR_MIN_SIMILARITY {
        return None;
    }
    Some(similarity)
}

/// Whether fetching `depth` neighbours per subject already exhausted what a deeper read could
/// return for every subject, so the caller can stop widening.
///
/// The other-day rule and the similarity floor both reject rows *after* the KNN has applied its
/// limit, so a subject whose nearest `depth` rows are all same-day could still have eligible
/// neighbours just beyond it. Two things settle that:
///
/// * the subject already has [`NEIGHBOUR_TOP_K`] distinct eligible neighbours — nothing deeper
///   can enter the list;
/// * the read came back short for that subject (fewer than `depth` rows), which only happens
///   when the candidate pool ran out — the store returns exactly `depth` whenever there are
///   more.
fn neighbours_are_complete(inputs: &[SessionNeighbourInput], depth: u32) -> bool {
    let top_k = NEIGHBOUR_TOP_K as usize;
    let per_subject = usize::try_from(depth).unwrap_or(usize::MAX);
    let mut rows_seen: BTreeMap<i64, usize> = BTreeMap::new();
    let mut eligible: BTreeMap<i64, std::collections::BTreeSet<i64>> = BTreeMap::new();
    for row in inputs {
        *rows_seen.entry(row.session_id).or_default() += 1;
        // Count on the other-day rule, not the similarity floor: see [`eligible_similarity`].
        if is_other_day_edge(row) {
            eligible
                .entry(row.session_id)
                .or_default()
                .insert(row.neighbour_session_id);
        }
    }
    rows_seen.iter().all(|(subject_id, seen)| {
        let usable = eligible.get(subject_id).map_or(0, |set| set.len());
        usable >= top_k || *seen < per_subject
    })
}

/// The deepest fetch the pass will make, mirroring sqlite-vec's own `k` ceiling (the store
/// clamps to the same figure), so widening always terminates.
const NEIGHBOUR_FETCH_CEIL: u32 = 4_096;

/// `semantic_neighbour` (`03 §7j.1`, session vectors `03 §7i`): sessions similar to a given
/// session, on **another day**.
///
/// The store supplies the KNN edges with `1 - cosine` distances; the rule keeps an edge when
/// `1.0 - distance >= NEIGHBOUR_MIN_SIMILARITY` and the two sessions started on different UTC
/// days (the same day is one continuous piece of work, not a connection), then lists the
/// strongest [`NEIGHBOUR_TOP_K`] by descending similarity. A session with no surviving
/// neighbour produces nothing.
///
/// The subjects are the sessions that *started* inside `[window_start, window_end)` — the same
/// rule `recurrence` uses, so a session that began before the window belongs to the pass that
/// covered its start and is not described twice.
///
/// Evidence: the subject session (0) then each listed neighbour in payload order, so the
/// evidence and the payload line up index for index. Rows are emitted in ascending
/// `session_id`.
pub fn derive_semantic_neighbours(
    inputs: &[SessionNeighbourInput],
    window_start: i64,
    window_end: i64,
) -> Vec<NewInsight> {
    let mut by_session: BTreeMap<i64, Vec<&SessionNeighbourInput>> = BTreeMap::new();
    for row in inputs {
        by_session.entry(row.session_id).or_default().push(row);
    }
    let top_k = NEIGHBOUR_TOP_K as usize;
    let mut out = Vec::new();
    for (session_id, rows) in by_session {
        let Some(subject) = rows.first().copied() else {
            continue;
        };
        if subject.session_started_at < window_start || subject.session_started_at >= window_end {
            continue;
        }
        // Keep the strongest edge per neighbour: a duplicated edge must not take two slots
        // of the top-k list or two evidence ordinals.
        let mut best: BTreeMap<i64, &SessionNeighbourInput> = BTreeMap::new();
        for row in rows {
            let Some(similarity) = eligible_similarity(row) else {
                continue;
            };
            match best.get(&row.neighbour_session_id) {
                Some(existing) if cosine_similarity(existing.distance) >= similarity => {}
                _ => {
                    best.insert(row.neighbour_session_id, row);
                }
            }
        }
        if best.is_empty() {
            continue;
        }
        let mut ranked: Vec<(f64, &SessionNeighbourInput)> = best
            .into_values()
            .map(|row| (cosine_similarity(row.distance), row))
            .collect();
        // Similarity descending, then the lower session id first: ties stay deterministic.
        ranked.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then(a.1.neighbour_session_id.cmp(&b.1.neighbour_session_id))
        });
        ranked.truncate(top_k);
        let neighbours: Vec<SessionNeighbour> = ranked
            .iter()
            .map(|&(similarity, row)| SessionNeighbour {
                session_id: row.neighbour_session_id,
                title: row.neighbour_title.clone(),
                started_at: row.neighbour_started_at,
                similarity,
            })
            .collect();
        let mut refs = Vec::with_capacity(neighbours.len() + 1);
        refs.push((EvidenceKind::Session, session_id));
        refs.extend(
            neighbours
                .iter()
                .map(|n| (EvidenceKind::Session, n.session_id)),
        );
        out.push(new_insight(
            window_start,
            window_end,
            InsightPayload::SemanticNeighbour(SemanticNeighbourPayload {
                session_id,
                session_title: subject.session_title.clone(),
                session_started_at: subject.session_started_at,
                neighbours,
            }),
            &refs,
        ));
    }
    out
}

/// `resumption` (`03 §7j.1`): a thread picked up after at least [`RESUMPTION_GAP_MS`], with
/// what it was last doing.
///
/// Consecutive member sessions of one thread (ordered by `started_at`) form a resumption when
/// `resumed.started_at - previous.ended_at >= RESUMPTION_GAP_MS` and the resumed session
/// overlaps the pass window. The previous session must have *ended* — a gap cannot be
/// measured from a session that is still running, so an open session never starts a
/// resumption. `last_doing` is the previous session's summary preferred, then its title, and
/// `None` when it has neither.
///
/// Evidence: thread (0), the previous session (1), the resumed session (2). Rows are emitted
/// in ascending `thread_id` then ascending gap order.
pub fn derive_resumption(
    inputs: &[ThreadSessionInput],
    window_start: i64,
    window_end: i64,
) -> Vec<NewInsight> {
    let mut by_thread: BTreeMap<i64, Vec<&ThreadSessionInput>> = BTreeMap::new();
    for row in inputs {
        by_thread.entry(row.thread_id).or_default().push(row);
    }
    let mut out = Vec::new();
    for (thread_id, mut rows) in by_thread {
        rows.sort_by_key(|row| (row.started_at, row.session_id));
        rows.dedup_by_key(|row| row.session_id);
        for pair in rows.windows(2) {
            let (previous, resumed) = (pair[0], pair[1]);
            let Some(previous_ended_at) = previous.ended_at else {
                continue;
            };
            let gap_ms = resumed.started_at - previous_ended_at;
            if gap_ms < RESUMPTION_GAP_MS {
                continue;
            }
            let resumed_end = resumed.ended_at.unwrap_or(window_end);
            if resumed.started_at >= window_end || resumed_end <= window_start {
                continue;
            }
            out.push(new_insight(
                window_start,
                window_end,
                InsightPayload::Resumption(ResumptionPayload {
                    thread_id,
                    title: resumed.title.clone(),
                    previous_session_id: previous.session_id,
                    previous_ended_at,
                    resumed_session_id: resumed.session_id,
                    resumed_at: resumed.started_at,
                    gap_ms,
                    last_doing: previous
                        .summary
                        .clone()
                        .or_else(|| previous.session_title.clone()),
                }),
                &[
                    (EvidenceKind::Thread, thread_id),
                    (EvidenceKind::Session, previous.session_id),
                    (EvidenceKind::Session, resumed.session_id),
                ],
            ));
        }
    }
    out
}

/// Handle to the running producer loop; stopping mirrors [`crate::session_vectors`]. The
/// `Drop` safety net ends the loop even if its owner returns early.
pub(crate) struct SchedulerHandle {
    stop: watch::Sender<bool>,
    join: Option<JoinHandle<()>>,
}

impl SchedulerHandle {
    pub(crate) async fn stop(mut self) {
        let _ = self.stop.send(true);
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
    }
}

impl Drop for SchedulerHandle {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

/// Starts the idle/on-demand producer (`03 §7j.3`, D10). It needs only the store and the
/// platform idle source (an empty slot until inference attaches), so it is not gated on a
/// model provider.
pub(crate) fn spawn(
    store: Arc<dyn Store>,
    throttle_level: Arc<AtomicU8>,
    idle: IdleSlot,
) -> SchedulerHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    let join = tokio::spawn(async move {
        let mut stop = stop_rx;
        // Idle by construction: wait a full tick before the first check, so starting the
        // producer never races a just-started app (or a test) with an immediate pass.
        loop {
            tokio::select! {
                biased;
                _ = stop.changed() => break,
                _ = tokio::time::sleep(TICK) => {}
            }
            if *stop.borrow() {
                break;
            }
            tick(store.as_ref(), &throttle_level, &idle).await;
        }
    });
    SchedulerHandle {
        stop: stop_tx,
        join: Some(join),
    }
}

/// One producer iteration: paused by the enrichment throttle, only while the machine is idle,
/// and with one queued pass at a time. Infallible by contract — every read error is logged and
/// the next tick retries.
pub(crate) async fn tick(store: &dyn Store, throttle_level: &AtomicU8, idle: &IdleSlot) {
    let level = throttle_level.load(Ordering::Relaxed);
    if level >= 1 {
        tracing::debug!(level, "insights are behind: enrichment throttle active");
        return;
    }
    if !idle_ready(idle) {
        return;
    }
    if !pass_ready(store).await {
        return;
    }
    enqueue(store).await;
}

/// On-demand producer for [`crate::Kernel::request_insights_generation`] (P9's "regenerate
/// now"): the idle and throttle gates belong to the timer, not to an explicit request — but
/// a second pass still never stacks on a queued one. Best-effort: a queue error is logged and
/// swallowed, so a caller is never failed by background bookkeeping.
pub(crate) async fn enqueue_on_demand(store: &dyn Store) {
    if !pass_ready(store).await {
        return;
    }
    enqueue(store).await;
}

/// Whether the platform reports at least [`IDLE_MS`] without user input. An unattached idle
/// source means "unknown", which is treated as "not idle": a pass must never be started on a
/// guess.
fn idle_ready(idle: &IdleSlot) -> bool {
    let source = idle.read().expect("idle slot lock").clone();
    match source.and_then(|source| source()) {
        Some(idle_ms) => idle_ms >= IDLE_MS,
        None => {
            tracing::debug!("insights: no idle source attached yet");
            false
        }
    }
}

/// Whether a pass may be enqueued now: none is already queued (so a second tick cannot stack
/// a redundant regeneration).
async fn pass_ready(store: &dyn Store) -> bool {
    match store.pending_jobs_of_kind(JobKind::InsightGenerate).await {
        Ok(0) => true,
        Ok(_) => false,
        Err(error) => {
            tracing::warn!(error = %error, "insights: queued pass count read failed");
            false
        }
    }
}

/// Enqueues one `insight_generate` job (`frame_id: None`: a pass is not a frame).
async fn enqueue(store: &dyn Store) -> bool {
    match store
        .enqueue_job(NewJob {
            kind: JobKind::InsightGenerate,
            frame_id: None,
            priority: 0,
            max_attempts: MAX_ATTEMPTS,
            not_before: 0,
        })
        .await
    {
        Ok(job_id) => {
            tracing::debug!(job_id, "insight generation job enqueued");
            true
        }
        Err(error) => {
            tracing::warn!(error = %error, "insights: generation enqueue failed");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

    use store::SqliteStore;

    /// A fixed "now" so every window and gap below is arithmetic over the test's own values.
    const NOW: i64 = 1_800_000_000_000;

    fn window() -> (i64, i64) {
        (NOW - LOOKBACK_MS, NOW)
    }

    fn idle_slot(idle_ms: Option<u64>) -> IdleSlot {
        let source: Option<crate::IdleSource> = idle_ms.map(|idle_ms| {
            let source: crate::IdleSource = Arc::new(move || Some(idle_ms));
            source
        });
        Arc::new(RwLock::new(source))
    }

    fn activity(
        thread_id: i64,
        session_id: i64,
        started_at: i64,
        ended_at: Option<i64>,
    ) -> ThreadActivityInput {
        ThreadActivityInput {
            thread_id,
            title: format!("Thread {thread_id}"),
            session_id,
            started_at,
            ended_at,
        }
    }

    fn thread_session(
        thread_id: i64,
        session_id: i64,
        started_at: i64,
        ended_at: Option<i64>,
        title: Option<&str>,
        summary: Option<&str>,
    ) -> ThreadSessionInput {
        ThreadSessionInput {
            thread_id,
            title: format!("Thread {thread_id}"),
            session_id,
            started_at,
            ended_at,
            session_title: title.map(str::to_string),
            summary: summary.map(str::to_string),
        }
    }

    fn entity_session(
        entity_id: i64,
        session_id: i64,
        session_started_at: i64,
    ) -> EntitySessionInput {
        EntitySessionInput {
            entity_id,
            entity_kind: "repo".to_string(),
            canonical_name: format!("entity-{entity_id}"),
            session_id,
            session_started_at,
            last_seen_at: session_started_at,
        }
    }

    fn neighbour(
        session_id: i64,
        session_started_at: i64,
        neighbour_session_id: i64,
        neighbour_started_at: i64,
        distance: f64,
    ) -> SessionNeighbourInput {
        SessionNeighbourInput {
            session_id,
            session_title: Some(format!("session {session_id}")),
            session_started_at,
            neighbour_session_id,
            neighbour_title: Some(format!("session {neighbour_session_id}")),
            neighbour_started_at,
            distance,
        }
    }

    fn payload(insight: &NewInsight) -> &InsightPayload {
        &insight.payload
    }

    /// Deepening is driven by the other-day rule alone. A subject whose nearest rows are all on
    /// other days but below the similarity floor is already settled — distance is monotone, so
    /// every further row is below the floor too, and widening would pay a KNN per doubling for
    /// nothing.
    #[test]
    fn neighbour_depth_stops_when_only_the_similarity_floor_rejects_rows() {
        let depth = NEIGHBOUR_TOP_K;
        let day = |offset: i64| NOW + offset * DAY_MS;
        // Five rows on other days, all far below the floor (distance 1.0 => similarity 0).
        let dissimilar: Vec<SessionNeighbourInput> = (1..=5)
            .map(|offset| neighbour(1, day(0), 100 + offset, day(offset), 1.0))
            .collect();
        assert!(
            dissimilar
                .iter()
                .all(|row| eligible_similarity(row).is_none()),
            "the fixture is below the floor"
        );
        assert!(
            neighbours_are_complete(&dissimilar, depth),
            "the floor alone must not ask for a deeper read"
        );

        // Five same-day rows at the same distance: the rule rejects them, so deeper rows could
        // still qualify and the read must continue.
        let same_day: Vec<SessionNeighbourInput> = (1..=5)
            .map(|offset| neighbour(1, day(0), 100 + offset, day(0) + offset * 60_000, 1.0))
            .collect();
        assert!(
            !neighbours_are_complete(&same_day, depth),
            "same-day rows fill the limit without satisfying the rule"
        );

        // A short read means the pool is spent, whatever the rows say.
        assert!(
            neighbours_are_complete(&same_day[..4], depth),
            "fewer rows than asked for means nothing deeper exists"
        );
    }

    #[test]
    fn insights_generator_thread_activity_sums_window_overlap() {
        let (window_start, window_end) = window();
        let closed_start = NOW - 3 * DAY_MS;
        let closed_end = NOW - 2 * DAY_MS;
        let open_start = NOW - 2 * DAY_MS;
        let inputs = vec![
            activity(7, 101, closed_start, Some(closed_end)),
            activity(7, 102, open_start, None),
            // Entirely before the window: no overlap, no statement.
            activity(
                8,
                201,
                window_start - 2 * DAY_MS,
                Some(window_start - DAY_MS),
            ),
        ];
        let out = derive_thread_activity(&inputs, window_start, window_end);
        assert_eq!(out.len(), 1, "only the thread with an overlapping session");
        let row = &out[0];
        assert_eq!(
            row.payload.kind(),
            InsightKind::ThreadActivity,
            "the payload kind and the row kind agree"
        );
        let InsightPayload::ThreadActivity(payload) = payload(row) else {
            panic!("expected a thread_activity payload");
        };
        assert_eq!(payload.thread_id, 7);
        assert_eq!(payload.title, "Thread 7");
        assert_eq!(payload.session_count, 2);
        assert_eq!(
            payload.active_ms,
            (closed_end - closed_start) + (window_end - open_start),
            "a closed session contributes its own span, an open one ends at window_end"
        );
        assert_eq!(payload.first_touched_at, closed_start);
        assert_eq!(
            payload.last_touched_at, window_end,
            "the open session is active up to the window end"
        );
        assert_eq!(row.window_start, window_start);
        assert_eq!(row.window_end, window_end);
        assert_eq!(row.generator, InsightGenerator::Heuristic);
        assert_eq!(
            row.evidence,
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Thread,
                    ref_id: 7,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: 101,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: EvidenceKind::Session,
                    ref_id: 102,
                },
            ],
            "the thread is ordinal 0 and the sessions follow in started_at order"
        );
    }

    #[test]
    fn insights_generator_thread_activity_half_open_window_boundaries() {
        let (window_start, window_end) = window();
        let inputs = vec![
            // Ends exactly at the window start: outside the half-open window.
            activity(1, 11, window_start - DAY_MS, Some(window_start)),
            // Starts exactly at the window end: also outside.
            activity(2, 21, window_end, None),
            // One millisecond inside: in.
            activity(3, 31, window_start, Some(window_start + 1)),
        ];
        let out = derive_thread_activity(&inputs, window_start, window_end);
        assert_eq!(out.len(), 1, "only the 1 ms session overlaps");
        let InsightPayload::ThreadActivity(payload) = payload(&out[0]) else {
            panic!("expected a thread_activity payload");
        };
        assert_eq!(payload.thread_id, 3);
        assert_eq!(payload.active_ms, 1);
        assert_eq!(payload.session_count, 1);
        assert_eq!(payload.first_touched_at, window_start);
        assert_eq!(payload.last_touched_at, window_start + 1);
    }

    #[test]
    fn insights_generator_open_loop_marks_and_abandoned_threads() {
        let (window_start, window_end) = window();
        let marks = vec![
            // Deliberately out of id order: the generator orders them.
            OpenMarkInput {
                mark_id: 9,
                frame_id: 90,
                created_at: NOW - 2 * DAY_MS,
                note: Some("check the failing gate".to_string()),
            },
            OpenMarkInput {
                mark_id: 4,
                frame_id: 40,
                created_at: NOW - DAY_MS,
                note: None,
            },
            // Created at the window end: a later window's loop.
            OpenMarkInput {
                mark_id: 5,
                frame_id: 50,
                created_at: window_end,
                note: None,
            },
        ];
        let threads = vec![
            // Abandoned: last activity 20 days ago.
            thread_session(
                1,
                10,
                NOW - 30 * DAY_MS,
                Some(NOW - 20 * DAY_MS),
                Some("old"),
                None,
            ),
            // Still open: never a loop.
            thread_session(2, 20, NOW - 40 * DAY_MS, None, Some("running"), None),
            // One millisecond inside the gap: still in play.
            thread_session(
                3,
                30,
                NOW - 9 * DAY_MS,
                Some(window_end - RESUMPTION_GAP_MS + 1),
                None,
                None,
            ),
            // Exactly at the gap: abandoned (the threshold is inclusive).
            thread_session(
                4,
                40,
                NOW - 20 * DAY_MS,
                Some(window_end - RESUMPTION_GAP_MS),
                None,
                None,
            ),
        ];
        let out = derive_open_loops(&marks, &threads, window_start, window_end);
        assert_eq!(out.len(), 4, "two open marks plus two abandoned threads");

        let kinds: Vec<OpenLoopSource> = out
            .iter()
            .map(|row| match payload(row) {
                InsightPayload::OpenLoop(payload) => payload.source,
                other => panic!("unexpected payload {other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                OpenLoopSource::UnresolvedMark,
                OpenLoopSource::UnresolvedMark,
                OpenLoopSource::AbandonedThread,
                OpenLoopSource::AbandonedThread,
            ]
        );

        let InsightPayload::OpenLoop(first) = payload(&out[0]) else {
            panic!("expected an open_loop payload");
        };
        assert_eq!(first.mark_id, Some(4));
        assert_eq!(first.frame_id, Some(40));
        assert_eq!(first.note, None);
        assert_eq!(first.thread_id, None);
        assert_eq!(first.title, None);
        assert_eq!(first.opened_at, NOW - DAY_MS);
        assert_eq!(first.idle_ms, window_end - (NOW - DAY_MS));
        assert_eq!(
            out[0].evidence,
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Mark,
                    ref_id: 4,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Frame,
                    ref_id: 40,
                },
            ]
        );

        let InsightPayload::OpenLoop(second) = payload(&out[1]) else {
            panic!("expected an open_loop payload");
        };
        assert_eq!(second.mark_id, Some(9));
        assert_eq!(second.note.as_deref(), Some("check the failing gate"));

        let InsightPayload::OpenLoop(third) = payload(&out[2]) else {
            panic!("expected an open_loop payload");
        };
        assert_eq!(third.source, OpenLoopSource::AbandonedThread);
        assert_eq!(third.thread_id, Some(1));
        assert_eq!(third.title.as_deref(), Some("Thread 1"));
        assert_eq!(third.mark_id, None);
        assert_eq!(third.frame_id, None);
        assert_eq!(third.opened_at, NOW - 20 * DAY_MS);
        assert_eq!(third.idle_ms, window_end - (NOW - 20 * DAY_MS));
        assert_eq!(
            out[2].evidence,
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Thread,
                    ref_id: 1,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: 10,
                },
            ]
        );

        let InsightPayload::OpenLoop(fourth) = payload(&out[3]) else {
            panic!("expected an open_loop payload");
        };
        assert_eq!(fourth.thread_id, Some(4));
        assert_eq!(fourth.idle_ms, RESUMPTION_GAP_MS, "exactly at the gap");
    }

    #[test]
    fn insights_generator_recurrence_gap_boundary() {
        let (window_start, window_end) = window();
        let first_at = NOW - 20 * DAY_MS;
        let at_gap = first_at + RECURRENCE_GAP_MS;
        let just_under = at_gap - 1;
        let third_at = at_gap + RECURRENCE_GAP_MS;
        let inputs = vec![
            // Exactly at the threshold, plus a third occurrence: two pairs.
            entity_session(1, 101, first_at),
            entity_session(1, 102, at_gap),
            entity_session(1, 103, third_at),
            // One millisecond inside the threshold: not a recurrence.
            entity_session(2, 201, first_at),
            entity_session(2, 202, just_under),
            // A session that began before the window belongs to an earlier pass, so only the
            // two in-window occurrences of entity 3 form a pair.
            entity_session(3, 301, window_start - DAY_MS),
            entity_session(3, 302, window_start + DAY_MS),
            entity_session(3, 303, window_start + DAY_MS + RECURRENCE_GAP_MS),
        ];
        let out = derive_recurrence(&inputs, window_start, window_end);
        assert_eq!(
            out.len(),
            3,
            "two pairs for entity 1, one pair for entity 3 (its pre-window session is ignored)"
        );

        let InsightPayload::Recurrence(first_pair) = payload(&out[0]) else {
            panic!("expected a recurrence payload");
        };
        assert_eq!(first_pair.entity_id, 1);
        assert_eq!(first_pair.entity_kind, "repo");
        assert_eq!(first_pair.entity_name, "entity-1");
        assert_eq!(first_pair.first_session_id, 101);
        assert_eq!(first_pair.first_seen_at, first_at);
        assert_eq!(first_pair.second_session_id, 102);
        assert_eq!(first_pair.second_seen_at, at_gap);
        assert_eq!(first_pair.gap_ms, RECURRENCE_GAP_MS);
        assert_eq!(first_pair.occurrence_count, 3);
        assert_eq!(
            out[0].evidence,
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Entity,
                    ref_id: 1,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: 101,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: EvidenceKind::Session,
                    ref_id: 102,
                },
            ]
        );

        let InsightPayload::Recurrence(second_pair) = payload(&out[1]) else {
            panic!("expected a recurrence payload");
        };
        assert_eq!(second_pair.first_session_id, 102);
        assert_eq!(second_pair.second_session_id, 103);
        assert_eq!(
            second_pair.gap_ms,
            third_at - at_gap,
            "the second pair measures its own gap"
        );

        let InsightPayload::Recurrence(third_pair) = payload(&out[2]) else {
            panic!("expected a recurrence payload");
        };
        assert_eq!(third_pair.entity_id, 3);
        assert_eq!(third_pair.first_session_id, 302);
        assert_eq!(
            third_pair.occurrence_count, 2,
            "the pre-window session is outside the pass, so it is not counted"
        );
    }

    #[test]
    fn insights_generator_semantic_neighbour_clamps_cosine_float_noise() {
        // A real pass over a real DB returned `1.0000000000000002` for two identical vectors:
        // vec0's `1 - similarity` lands a couple of ULP below zero for a cosine of exactly 1.
        // A similarity above 1 is float noise, not a measurement, so the payload is clamped.
        let (window_start, window_end) = window();
        let subject_at = NOW - 2 * DAY_MS;
        let distance = -(2.220_446_049_250_313e-16_f64);
        let inputs = vec![neighbour(20, subject_at, 21, NOW - 5 * DAY_MS, distance)];
        let out = derive_semantic_neighbours(&inputs, window_start, window_end);
        assert_eq!(out.len(), 1, "an identical pair is still a neighbour");
        let InsightPayload::SemanticNeighbour(payload) = payload(&out[0]) else {
            panic!("expected a semantic_neighbour payload");
        };
        assert_eq!(
            payload.neighbours[0].similarity, 1.0,
            "cosine similarity cannot exceed 1; the ULP overshoot is clamped"
        );
        assert_eq!(cosine_similarity(distance), 1.0);
        assert_eq!(cosine_similarity(2.0), -1.0, "the lower bound clamps too");
    }

    #[test]
    fn insights_generator_semantic_neighbour_threshold_and_day_rule() {
        let (window_start, window_end) = window();
        let subject_at = NOW - 2 * DAY_MS;
        let inputs = vec![
            // Exactly at the similarity threshold, on another day: kept.
            neighbour(
                10,
                subject_at,
                11,
                NOW - 5 * DAY_MS,
                1.0 - NEIGHBOUR_MIN_SIMILARITY,
            ),
            // Just under the threshold: dropped.
            neighbour(
                10,
                subject_at,
                12,
                NOW - 6 * DAY_MS,
                1.0 - (NEIGHBOUR_MIN_SIMILARITY - 0.01),
            ),
            // Similar but the same UTC day: dropped (one continuous piece of work).
            neighbour(10, subject_at, 13, subject_at + 60_000, 0.0),
            // Not in the window at all: the pass does not describe it.
            neighbour(14, window_start - DAY_MS, 15, NOW - 8 * DAY_MS, 0.0),
        ];
        let out = derive_semantic_neighbours(&inputs, window_start, window_end);
        assert_eq!(out.len(), 1, "only session 10 has a surviving neighbour");
        let InsightPayload::SemanticNeighbour(payload) = payload(&out[0]) else {
            panic!("expected a semantic_neighbour payload");
        };
        assert_eq!(payload.session_id, 10);
        assert_eq!(payload.session_title.as_deref(), Some("session 10"));
        assert_eq!(payload.session_started_at, subject_at);
        assert_eq!(payload.neighbours.len(), 1);
        assert_eq!(payload.neighbours[0].session_id, 11);
        assert_eq!(payload.neighbours[0].started_at, NOW - 5 * DAY_MS);
        assert_eq!(
            payload.neighbours[0].similarity,
            1.0 - (1.0 - NEIGHBOUR_MIN_SIMILARITY)
        );
        assert_eq!(
            out[0].evidence,
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Session,
                    ref_id: 10,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: 11,
                },
            ],
            "the subject is ordinal 0 and the neighbour follows"
        );
    }

    #[test]
    fn insights_generator_semantic_neighbour_keeps_top_k_by_similarity() {
        let (window_start, window_end) = window();
        let subject_at = NOW - 2 * DAY_MS;
        // Six qualifying neighbours: NEIGHBOUR_TOP_K + 1.
        let mut inputs = Vec::new();
        for index in 0..=(NEIGHBOUR_TOP_K as i64) {
            inputs.push(neighbour(
                20,
                subject_at,
                100 + index,
                NOW - (10 + index) * DAY_MS,
                1.0 - (1.0 - 0.01 * index as f64),
            ));
        }
        // A duplicate edge of the strongest neighbour must not take a second slot.
        inputs.push(neighbour(
            20,
            subject_at,
            100,
            NOW - 10 * DAY_MS,
            1.0 - (1.0 - 0.01),
        ));
        let out = derive_semantic_neighbours(&inputs, window_start, window_end);
        assert_eq!(out.len(), 1);
        let InsightPayload::SemanticNeighbour(payload) = payload(&out[0]) else {
            panic!("expected a semantic_neighbour payload");
        };
        assert_eq!(
            payload.neighbours.len(),
            NEIGHBOUR_TOP_K as usize,
            "at most NEIGHBOUR_TOP_K neighbours"
        );
        let ids: Vec<i64> = payload.neighbours.iter().map(|n| n.session_id).collect();
        assert_eq!(
            ids,
            (100..100 + NEIGHBOUR_TOP_K as i64).collect::<Vec<_>>(),
            "strongest first; the weakest and the duplicate edge are dropped"
        );
        let similarities: Vec<f64> = payload.neighbours.iter().map(|n| n.similarity).collect();
        assert!(
            similarities.windows(2).all(|pair| pair[0] > pair[1]),
            "similarity is strictly descending: {similarities:?}"
        );
        assert_eq!(out[0].evidence.len(), NEIGHBOUR_TOP_K as usize + 1);
        assert_eq!(
            out[0].evidence.last().map(|row| row.ref_id),
            Some(100 + NEIGHBOUR_TOP_K as i64 - 1)
        );
    }

    #[test]
    fn insights_generator_resumption_gap_boundary() {
        let (window_start, window_end) = window();
        let previous_end = NOW - 19 * DAY_MS;
        let resumed_at = previous_end + RESUMPTION_GAP_MS;
        let inputs = vec![
            // Exactly at the gap, summary preferred over title.
            thread_session(
                5,
                501,
                NOW - 20 * DAY_MS,
                Some(previous_end),
                Some("old title"),
                Some("old summary"),
            ),
            thread_session(
                5,
                502,
                resumed_at,
                Some(resumed_at + DAY_MS),
                Some("new"),
                None,
            ),
            // One millisecond inside the gap: still the same piece of work.
            thread_session(6, 601, NOW - 20 * DAY_MS, Some(previous_end), None, None),
            thread_session(
                6,
                602,
                previous_end + RESUMPTION_GAP_MS - 1,
                None,
                None,
                None,
            ),
            // The previous session never ended: no gap can be measured.
            thread_session(7, 701, NOW - 20 * DAY_MS, None, None, None),
            thread_session(7, 702, NOW - 3 * DAY_MS, None, None, None),
            // Resumed entirely before the window: a later pass's statement.
            thread_session(
                8,
                801,
                window_start - 20 * DAY_MS,
                Some(window_start - 10 * DAY_MS),
                None,
                None,
            ),
            thread_session(
                8,
                802,
                window_start - 3 * DAY_MS,
                Some(window_start),
                None,
                None,
            ),
        ];
        let out = derive_resumption(&inputs, window_start, window_end);
        assert_eq!(out.len(), 1, "only thread 5 resumes inside the window");
        let InsightPayload::Resumption(payload) = payload(&out[0]) else {
            panic!("expected a resumption payload");
        };
        assert_eq!(payload.thread_id, 5);
        assert_eq!(payload.title, "Thread 5");
        assert_eq!(payload.previous_session_id, 501);
        assert_eq!(payload.previous_ended_at, previous_end);
        assert_eq!(payload.resumed_session_id, 502);
        assert_eq!(payload.resumed_at, resumed_at);
        assert_eq!(payload.gap_ms, RESUMPTION_GAP_MS);
        assert_eq!(
            payload.last_doing.as_deref(),
            Some("old summary"),
            "the summary is preferred over the title"
        );
        assert_eq!(
            out[0].evidence,
            vec![
                InsightEvidence {
                    ordinal: 0,
                    kind: EvidenceKind::Thread,
                    ref_id: 5,
                },
                InsightEvidence {
                    ordinal: 1,
                    kind: EvidenceKind::Session,
                    ref_id: 501,
                },
                InsightEvidence {
                    ordinal: 2,
                    kind: EvidenceKind::Session,
                    ref_id: 502,
                },
            ]
        );
    }

    #[test]
    fn insights_generator_resumption_last_doing_falls_back_to_the_title_then_none() {
        let (window_start, window_end) = window();
        let previous_end = NOW - 19 * DAY_MS;
        let resumed_at = previous_end + RESUMPTION_GAP_MS;
        let inputs = vec![
            thread_session(
                11,
                1101,
                NOW - 20 * DAY_MS,
                Some(previous_end),
                Some("title only"),
                None,
            ),
            thread_session(11, 1102, resumed_at, Some(resumed_at + DAY_MS), None, None),
            thread_session(12, 1201, NOW - 20 * DAY_MS, Some(previous_end), None, None),
            thread_session(12, 1202, resumed_at, Some(resumed_at + DAY_MS), None, None),
        ];
        let out = derive_resumption(&inputs, window_start, window_end);
        assert_eq!(out.len(), 2);
        let InsightPayload::Resumption(title_only) = payload(&out[0]) else {
            panic!("expected a resumption payload");
        };
        assert_eq!(title_only.last_doing.as_deref(), Some("title only"));
        let InsightPayload::Resumption(neither) = payload(&out[1]) else {
            panic!("expected a resumption payload");
        };
        assert_eq!(
            neither.last_doing, None,
            "when the previous session has neither a summary nor a title, nothing is invented"
        );
    }

    #[test]
    fn insights_generator_evidence_ordinals_are_dense_for_every_kind() {
        let (window_start, window_end) = window();
        let mut produced = Vec::new();
        produced.extend(derive_thread_activity(
            &[
                activity(1, 11, NOW - 3 * DAY_MS, Some(NOW - 2 * DAY_MS)),
                activity(1, 12, NOW - 2 * DAY_MS, None),
            ],
            window_start,
            window_end,
        ));
        produced.extend(derive_open_loops(
            &[OpenMarkInput {
                mark_id: 1,
                frame_id: 2,
                created_at: NOW - DAY_MS,
                note: None,
            }],
            &[
                thread_session(
                    1,
                    11,
                    NOW - 30 * DAY_MS,
                    Some(NOW - 20 * DAY_MS),
                    None,
                    None,
                ),
                thread_session(
                    1,
                    12,
                    NOW - 19 * DAY_MS,
                    Some(NOW - 18 * DAY_MS),
                    None,
                    None,
                ),
            ],
            window_start,
            window_end,
        ));
        produced.extend(derive_recurrence(
            &[
                entity_session(1, 11, NOW - 20 * DAY_MS),
                entity_session(1, 12, NOW - 10 * DAY_MS),
            ],
            window_start,
            window_end,
        ));
        produced.extend(derive_semantic_neighbours(
            &[
                neighbour(1, NOW - 2 * DAY_MS, 11, NOW - 9 * DAY_MS, 0.0),
                neighbour(1, NOW - 2 * DAY_MS, 12, NOW - 8 * DAY_MS, 0.01),
            ],
            window_start,
            window_end,
        ));
        produced.extend(derive_resumption(
            &[
                thread_session(
                    1,
                    11,
                    NOW - 20 * DAY_MS,
                    Some(NOW - 19 * DAY_MS),
                    None,
                    None,
                ),
                thread_session(1, 12, NOW - 10 * DAY_MS, Some(NOW - 9 * DAY_MS), None, None),
            ],
            window_start,
            window_end,
        ));

        assert_eq!(
            produced.len(),
            6,
            "one thread_activity, two open_loop rows (mark + abandoned thread), one \
             recurrence, one semantic_neighbour and one resumption"
        );
        for row in &produced {
            assert!(
                !row.evidence.is_empty(),
                "an insight without evidence is a defect (03 §7j.2)"
            );
            let ordinals: Vec<u32> = row.evidence.iter().map(|ref_| ref_.ordinal).collect();
            let expected: Vec<u32> = (0..row.evidence.len() as u32).collect();
            assert_eq!(ordinals, expected, "ordinals are 0-based and dense");
            assert_eq!(row.window_start, window_start);
            assert_eq!(row.window_end, window_end);
        }
    }

    #[tokio::test]
    async fn insights_scheduler_enqueues_one_pass_when_idle_and_throttled_gate_holds() {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().expect("store"));
        let throttle = AtomicU8::new(0);

        // Not idle: nothing is enqueued.
        tick(store.as_ref(), &throttle, &idle_slot(Some(IDLE_MS - 1))).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            0,
            "a pass waits for a full idle dwell"
        );

        // No idle source attached: unknown is treated as busy.
        tick(store.as_ref(), &throttle, &idle_slot(None)).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            0
        );

        // Idle exactly at the dwell: one pass is enqueued.
        tick(store.as_ref(), &throttle, &idle_slot(Some(IDLE_MS))).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            1,
            "exactly IDLE_MS counts as idle"
        );

        // A second tick must not stack a redundant pass.
        tick(store.as_ref(), &throttle, &idle_slot(Some(u64::MAX))).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            1,
            "one pending pass at a time"
        );

        // Drain it, then prove the enrichment throttle pauses the producer.
        let jobs = store
            .claim_jobs(&[JobKind::InsightGenerate], 1, NOW)
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
        store.complete_job(jobs[0].id).await.unwrap();

        throttle.store(1, Ordering::Relaxed);
        tick(store.as_ref(), &throttle, &idle_slot(Some(u64::MAX))).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            0,
            "the enrichment throttle pauses the insights producer"
        );
    }

    #[tokio::test]
    async fn insights_on_demand_enqueue_ignores_idle_but_not_the_pending_guard() {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open_in_memory().expect("store"));
        enqueue_on_demand(store.as_ref()).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            1,
            "an explicit request does not wait for idle"
        );
        enqueue_on_demand(store.as_ref()).await;
        assert_eq!(
            store
                .pending_jobs_of_kind(JobKind::InsightGenerate)
                .await
                .unwrap(),
            1,
            "a queued pass is never stacked"
        );
    }
}
