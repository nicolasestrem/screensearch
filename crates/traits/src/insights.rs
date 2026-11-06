//! Generator inputs for the 0.4.5 P8 insights engine (`03 §7j.1`, `03 §7h`).
//!
//! These are **read shapes, not IPC types**: [`crate::Store`] exposes one reader per
//! generator family, the pure `derive_<kind>` functions in `kernel::insights` turn those rows
//! into [`crate::NewInsight`] values, and nothing here crosses the UI boundary. So the structs
//! derive plain `Debug, Clone, PartialEq` (`Eq` where no float is involved) and no `ts-rs`
//! derive — the UI only ever sees the rendered payloads in [`crate::ipc`].
//!
//! Windows are unix ms and half-open, `[window_start, window_end)`, matching the insight rows
//! themselves (`03 §7j.5`) and every other history read. Each row is a raw fact and nothing
//! more: the generator decides what a fact *states*, and every rendered claim carries at least
//! one evidence reference (`03 §7j.2`).

/// One (thread, member session) pair for the `thread_activity` generator (`03 §7j.1`).
///
/// The store returns one row per thread × member session **overlapping the pass window**;
/// the generator groups by `thread_id` and sums each session's overlap with that window, using
/// `window_end` in place of a missing `ended_at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadActivityInput {
    /// `threads.id` of the member session's thread.
    pub thread_id: i64,
    /// `threads.title` (the cluster seed's name, or the rename override's text).
    pub title: String,
    /// `sessions.id` of the member session.
    pub session_id: i64,
    /// `sessions.started_at`, unix ms.
    pub started_at: i64,
    /// `sessions.ended_at`, unix ms; `None` while the session is still open.
    pub ended_at: Option<i64>,
}

/// One unresolved mark for the `open_loop` generator (`03 §7j.1`).
///
/// Only marks still open (`marks.resolved_at IS NULL`) created before `window_end` are read: a
/// resolved mark is no longer a loop, and a mark created later belongs to a later window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenMarkInput {
    /// `marks.id` — the `mark` evidence the insight carries.
    pub mark_id: i64,
    /// `marks.frame_id` — the `frame` evidence, so the loop opens on the marked moment.
    pub frame_id: i64,
    /// `marks.created_at`, unix ms: when the loop opened.
    pub created_at: i64,
    /// The mark's optional intention note, verbatim.
    pub note: Option<String>,
    /// `frames.session_id` — the session the marked frame belonged to, if the frame has one.
    ///
    /// The open-loop payload carries only `mark` and `frame` evidence, which is enough for the
    /// Insights list but not for the Thread page: that read matches a loop to a thread by `thread`
    /// and member-`session` evidence (`src-tauri/src/graph.rs::get_thread_page`). Without the
    /// session the derivation cannot attach the loop to the work the mark interrupted, so a thread
    /// with an unresolved mark in its history reported "No open loops in this thread".
    pub session_id: Option<i64>,
}

/// One (entity, session) link for the `recurrence` generator (`03 §7j.1`, `03 §7h.4`).
///
/// The store returns the `session_entities` rows joined to `sessions` and `entities` for
/// sessions inside the pass window, ordered by `entity_id` then the session's `started_at`, so
/// the generator can walk consecutive sessions per entity without re-sorting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySessionInput {
    /// `entities.id` — the `entity` evidence the insight carries.
    pub entity_id: i64,
    /// The `entities.kind` token (`03 §7h.1`); carried as the payload's string token.
    pub entity_kind: String,
    /// `entities.canonical_name`, as displayed.
    pub canonical_name: String,
    /// `sessions.id` of a session touching this entity.
    pub session_id: i64,
    /// That session's `started_at`, unix ms — the recurrence gap is measured between these.
    pub session_started_at: i64,
    /// The entity's `last_seen_at`, unix ms, as stored. The generator timestamps a recurrence
    /// claim from the session rows, never from this value.
    pub last_seen_at: i64,
}

/// One KNN edge between two sessions for the `semantic_neighbour` generator (`03 §7j.1`,
/// `03 §7i` session vectors).
///
/// The store runs the KNN over `session_vectors` for each session inside the window with
/// `k + 1` results and drops the self-match, so `session_id != neighbour_session_id` always
/// holds and `k` genuinely means "k neighbours".
#[derive(Debug, Clone, PartialEq)]
pub struct SessionNeighbourInput {
    /// The session being described (the insight's subject).
    pub session_id: i64,
    /// Its `sessions.title`.
    pub session_title: Option<String>,
    /// Its `sessions.started_at`, unix ms.
    pub session_started_at: i64,
    /// The neighbour session's `sessions.id`.
    pub neighbour_session_id: i64,
    /// The neighbour's `sessions.title`.
    pub neighbour_title: Option<String>,
    /// The neighbour's `sessions.started_at`, unix ms.
    pub neighbour_started_at: i64,
    /// vec0 cosine **distance** (`1 - similarity`). The generator keeps a pair only when
    /// `1.0 - distance >= NEIGHBOUR_MIN_SIMILARITY`.
    pub distance: f64,
}

/// One (thread, member session) row for the `resumption` generator (`03 §7j.1`, `03 §7h.4`).
///
/// The store returns every `thread_sessions` row of the threads overlapping the pass window,
/// joined to the session's own fields, ordered by `thread_id` then `started_at`, so
/// consecutive member sessions pair up directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSessionInput {
    /// `threads.id` of the member session's thread.
    pub thread_id: i64,
    /// `threads.title`.
    pub title: String,
    /// `sessions.id` of the member session.
    pub session_id: i64,
    /// `sessions.started_at`, unix ms.
    pub started_at: i64,
    /// `sessions.ended_at`, unix ms; `None` while the session is still open, which can never
    /// be the *previous* session of a resumption.
    pub ended_at: Option<i64>,
    /// `sessions.title`.
    pub session_title: Option<String>,
    /// `sessions.summary` — preferred over the title for the payload's `last_doing`.
    pub summary: Option<String>,
}
