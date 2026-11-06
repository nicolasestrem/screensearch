//! The P9 intelligence-UI wire contract: work-graph reads and Ask conversations.
//!
//! Every type here derives [`ts_rs::TS`] and exports to `ui/src/bindings/`, exactly like
//! [`crate::ipc`] — same convention, same rule: every `i64`/`u64` field carries
//! `#[ts(type = "number")]`, because Tauri serializes over `serde_json` where 64-bit ints
//! arrive as JS `number` (`03 §7`).
//!
//! Layout:
//! - Work graph (`03 §7c` P8 block, `§7h.4`–`§7h.6`): the thread list page with its backfill
//!   progress, the thread page (detail, merge survivor, connections, open loops), the entity and
//!   insight list queries/pages, and the three override requests with their receipt.
//! - Ask conversations (`03 §7i.3`, `§7i.4`): citations, thread summaries, turns, the
//!   conversation request (with its editable scope chips) and the start receipt / error.
//!
//! Request-side kernel scopes that are *not* IPC wire live in [`crate::ask_ipc`]; this module
//! holds only what crosses the UI boundary.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::domain::{AskTurnError, AskTurnRecord};
use crate::ipc::{Insight, InsightKind, ThreadDetail, ThreadSummary, TimeRange};

/// Query for the `list_threads` command (`03 §7c` P8 block, `§7h.4`): threads overlapping the
/// half-open `[from_ms, to_ms)` window (`first_seen_at < to AND last_seen_at >= from`; a missing
/// bound leaves that side open), optionally restricted to threads with a member session that
/// touched `entity_id`. `limit` arrives already resolved — the command clamps it to `1..=1000`,
/// like `crates/api`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ThreadListQuery {
    #[ts(type = "number | null")]
    pub from_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub to_ms: Option<i64>,
    /// Filter to threads holding a session that touched this `entities.id` (`§7h.1`).
    #[ts(type = "number | null")]
    pub entity_id: Option<i64>,
    pub limit: u32,
}

/// How far the work graph has got (`03 §7h.6`), for the Work screen's "N of M sessions clustered"
/// line while the backfill runs.
///
/// `sessions_total` is `COUNT(sessions)` and `sessions_clustered` is
/// `COUNT(DISTINCT thread_sessions.session_id)` — two independent counts, so the second can never
/// exceed the first and new sessions keep arriving while old ones are clustered. `running` is
/// `true` while the queue still holds work for the graph; the `pending_*` counts are the queue's
/// own resumable checkpoint (the same one `graph_backfill_status` reads), never separate state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct GraphProgress {
    #[ts(type = "number")]
    pub sessions_total: u64,
    #[ts(type = "number")]
    pub sessions_clustered: u64,
    pub running: bool,
    #[ts(type = "number")]
    pub pending_extract: u64,
    #[ts(type = "number")]
    pub pending_backfill: u64,
    #[ts(type = "number")]
    pub pending_cluster: u64,
}

/// `list_threads` output: the page of threads plus the backfill progress that frames it.
///
/// Threads with `session_count == 0` are filtered out. Those rows are the emptied merge sources
/// `03 §7h.5` retains as durability evidence (`06` #37, `07`) — never user-visible work.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ThreadListPage {
    pub threads: Vec<ThreadSummary>,
    pub progress: GraphProgress,
}

/// One connected thread: a thread sharing at least one entity with the thread being viewed
/// (`03 §7h.4`). `shared_entity_ids` **is** the evidence of the connection — the Thread page names
/// those entities rather than asserting a relationship the rows cannot prove.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ConnectedThread {
    pub thread: ThreadSummary,
    /// The entities both threads' member sessions touched, ascending. Never empty.
    #[ts(type = "Array<number>")]
    pub shared_entity_ids: Vec<i64>,
}

/// `get_thread_page` output (`03 §7h.4`, `§7h.5`, `§7j`).
///
/// `detail` is `None` and `merged_into` is `None` when the thread does not exist (the UI renders a
/// missing thread, not an empty page). `merged_into` set means a `merge` override (`§7h.5`) folded
/// this thread into the survivor: the page explains the merge and links the survivor, and `detail`
/// is then `None`. `open_loops` are the `open_loop` insights whose evidence names this thread or
/// one of `detail.sessions`, in the insight list's order. `progress` frames the page the same way
/// the Work list does, so a partially clustered graph is never presented as complete.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct ThreadPage {
    pub detail: Option<ThreadDetail>,
    pub merged_into: Option<ThreadSummary>,
    pub connected: Vec<ConnectedThread>,
    pub open_loops: Vec<Insight>,
    pub progress: GraphProgress,
}

/// Query for the `list_entities` command (`03 §7c` P8 block): an exact `entities.kind` token
/// and/or a case-insensitive substring matched against `canonical_name` or any alias. `limit`
/// arrives already resolved and clamped.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct EntityListQuery {
    pub kind: Option<String>,
    pub query: Option<String>,
    pub limit: u32,
}

/// Query for the `list_insights` command (`03 §7c` P8 block, `§7j`): insights whose window
/// overlaps `[from_ms, to_ms)` (`window_start < to AND window_end > from`). `kind: None` means
/// every kind. `limit` arrives already resolved and clamped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct InsightListQuery {
    #[ts(type = "number | null")]
    pub from_ms: Option<i64>,
    #[ts(type = "number | null")]
    pub to_ms: Option<i64>,
    pub kind: Option<InsightKind>,
    pub limit: u32,
}

/// Whether an insight window is still filling (`03 §7j.5`).
///
/// `pending_jobs` counts the `insight_generate` jobs still queued or running, so a non-zero value
/// renders the list as **partial** rather than pretending a half-generated window is the whole
/// truth. `last_generated_at` is `MAX(insights.generated_at)` over the rows read, `None` before the
/// first pass has written anything.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct InsightWindowStatus {
    #[ts(type = "number")]
    pub pending_jobs: u64,
    #[ts(type = "number | null")]
    pub last_generated_at: Option<i64>,
}

/// `list_insights` output: the insight rows plus the generation status that decides the partial
/// label. Rows keep the store's order (`window_start DESC, kind ASC, id DESC`) — the UI groups by
/// kind itself, so a second sort here would only invite drift.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct InsightListPage {
    pub insights: Vec<Insight>,
    pub status: InsightWindowStatus,
}

/// Which `thread_overrides.kind` the UI just wrote (`03 §7h.5`). Tokens match the column exactly,
/// so the receipt can be compared with the row it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum OverrideKind {
    /// `rename` — sets `threads.title` only; membership untouched.
    Rename,
    /// `merge` — folds the source thread's sessions into the survivor.
    Merge,
    /// `split` — moves the listed sessions into a new thread.
    Split,
}

/// Receipt for a rename / merge / split (`03 §7h.5`), returned by all three override commands.
///
/// `sessions_moved` is `0` for a rename, the source thread's session count for a merge and
/// `session_ids.len()` for a split — the number the UI states when confirming. The override is
/// written **and** a `graph_cluster` job is enqueued in the same transaction, so the rows only
/// change when that job applies the override; this receipt never claims the fold already happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct OverrideReceipt {
    /// The new `thread_overrides.id` (`§4`).
    #[ts(type = "number")]
    pub override_id: i64,
    pub kind: OverrideKind,
    pub sessions_moved: u32,
}

/// Input to `rename_thread` (`03 §7h.5`): sets `threads.title` only. An empty or whitespace-only
/// title is rejected by the store's payload validation, so the UI must not send one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct RenameThreadRequest {
    #[ts(type = "number")]
    pub thread_id: i64,
    pub title: String,
}

/// Input to `merge_threads` (`03 §7h.5`): folds `source_thread_id`'s sessions into
/// `target_thread_id` (the survivor). `payload.session_ids` records every source session for audit
/// and undo; the fold itself follows `thread_sessions`, so the list can never contradict the rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct MergeThreadsRequest {
    #[ts(type = "number")]
    pub source_thread_id: i64,
    #[ts(type = "number")]
    pub target_thread_id: i64,
}

/// Input to `split_thread` (`03 §7h.5`): moves the listed sessions out of `thread_id` into a new
/// thread seeded by the override. At least one session id is required and no id may repeat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct SplitThreadRequest {
    #[ts(type = "number")]
    pub thread_id: i64,
    #[ts(type = "Array<number>")]
    pub session_ids: Vec<i64>,
}

/// Both kinds of Ask v2 evidence in one value (`03 §7i.3`): the exact
/// `{"sessions":[…],"frames":[…]}` shape the terminal `citations` delta already emits, persisted
/// on the finished turn as JSON (`ask_turns.citations`, NULL until then).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AskCitations {
    /// Distinct non-NULL `frames.session_id` of the sessions the answer drew on.
    #[ts(type = "Array<number>")]
    pub sessions: Vec<i64>,
    /// The frame ids in the assembled prompt, in citation (inclusion) order.
    #[ts(type = "Array<number>")]
    pub frames: Vec<i64>,
}

/// One `ask_threads` row for the conversation list (`03 §7i.4`).
///
/// `title` is NULL until the first turn names the thread (the `ask_turn` command trims the
/// question to 80 chars); the UI shows the newest turn's question instead when it is NULL.
/// `turn_count` counts the turns; `last_turn_at` is the newest turn's `created_at`, `None` for a
/// thread that has no turns yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AskThreadSummary {
    #[ts(type = "number")]
    pub id: i64,
    pub title: Option<String>,
    #[ts(type = "number")]
    pub created_at: i64,
    pub turn_count: u32,
    #[ts(type = "number | null")]
    pub last_turn_at: Option<i64>,
}

/// One persisted conversation turn, as the UI renders it (`03 §7i.4`) — the wire form of
/// [`crate::AskTurnRecord`].
///
/// `answer: None` means the turn is still streaming or failed; either way the question stays in
/// the thread and a failed turn keeps it for a retry. `citations` is the persisted `§7i.3` pair,
/// `None` until the turn finishes. `scope_thread_id` is the work-graph thread the turn was scoped
/// to, `None` when it ran unscoped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AskTurn {
    #[ts(type = "number")]
    pub id: i64,
    #[ts(type = "number")]
    pub thread_id: i64,
    /// Turn order within the thread, `UNIQUE(thread_id, ordinal)`.
    #[ts(type = "number")]
    pub ordinal: i64,
    pub question: String,
    pub answer: Option<String>,
    /// The resolved half-open `[start, end)` the turn ran over; `None` = whole history.
    pub window: Option<TimeRange>,
    #[ts(type = "Array<number>")]
    pub resolved_entities: Vec<i64>,
    #[ts(type = "number | null")]
    pub scope_thread_id: Option<i64>,
    pub citations: Option<AskCitations>,
    #[ts(type = "number")]
    pub created_at: i64,
}

impl From<AskTurnRecord> for AskTurn {
    /// The persisted record is the source of truth: every field is copied, and the two JSON
    /// columns arrive already decoded by the store.
    fn from(record: AskTurnRecord) -> Self {
        Self {
            id: record.id,
            thread_id: record.thread_id,
            ordinal: record.ordinal,
            question: record.question,
            answer: record.answer,
            window: record.window,
            resolved_entities: record.resolved_entities,
            scope_thread_id: record.scope_thread_id,
            citations: record.citations,
            created_at: record.created_at,
        }
    }
}

/// The window a conversation turn runs over (`03 §7i.4`), as the UI sends it with `ask_turn`.
///
/// `inherit` reuses the previous turn's window (`§7i.4`: "and the day before?" keeps the entity
/// scope while moving the window); `all` clears it to the whole history; `range` sets an explicit
/// half-open `[start, end)` in unix ms. The command maps this to
/// [`crate::ask_ipc::AskWindowOverride`] before calling the kernel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "mode", rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum AskWindowScope {
    /// Reuse the previous turn's window; the planner's default when the user does not touch the
    /// scope chips.
    #[default]
    Inherit,
    /// Whole history (both `ask_turns` window columns NULL).
    All,
    /// Explicit half-open `[start, end)`, unix epoch ms.
    Range {
        #[ts(type = "number")]
        start: i64,
        #[ts(type = "number")]
        end: i64,
    },
}

/// Input to the `ask_turn` command (`03 §7i.2`, `§7i.4`) — the conversation twin of
/// [`crate::AskRequest`]. The legacy single-turn `ask` command and its wire type are untouched, so
/// an older UI keeps working.
///
/// The three scope fields are the editable scope chips (§4):
/// - `window` — see [`AskWindowScope`].
/// - `entities`: `None` = inherit / let the planner resolve entities from the question;
///   `[]` = clear the entity scope; `[ids]` = explicit scope, and the planner skips resolution.
/// - `scope_thread_id` / `clear_thread_scope`: `Some(id)` scopes stage 1 to that work-graph
///   thread; `None` with `clear_thread_scope: false` inherits the previous turn's scope;
///   `clear_thread_scope: true` runs unscoped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AskConversationRequest {
    pub request_id: Option<String>,
    pub query: String,
    /// `None` starts a new conversation (`§7i.4`); `Some` appends to that `ask_threads` row.
    #[ts(type = "number | null")]
    pub thread_id: Option<i64>,
    pub thinking: bool,
    pub max_tokens: u32,
    /// Per-request retrieval-depth override (`03 §8` `retrieval.default_top_k`); `None` = the
    /// configured default.
    pub top_k: Option<u32>,
    pub window: AskWindowScope,
    #[ts(type = "Array<number> | null")]
    pub entities: Option<Vec<i64>>,
    #[ts(type = "number | null")]
    pub scope_thread_id: Option<i64>,
    pub clear_thread_scope: bool,
}

/// The ids a started `ask_turn` streams under (`03 §7i.4`): the conversation (created when
/// `thread_id` was `None`) and the appended turn.
///
/// Deltas arrive on the existing `answer_delta` event keyed by this `request_id`, so the streaming
/// renderer is the one the legacy `ask` command uses. A reloaded thread re-renders from
/// `list_ask_turns` instead — the persisted turns, not the stream, are the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub struct AskTurnStarted {
    pub request_id: String,
    #[ts(type = "number")]
    pub thread_id: i64,
    #[ts(type = "number")]
    pub turn_id: i64,
}

/// Why `ask_turn` could not start (`03 §7i.4`) — the command's error type, tagged by `kind` so the
/// UI can act: a missing conversation offers a new one, a missing model offers to load it, and a
/// real failure shows its message while keeping the question for a retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../ui/src/bindings/")]
pub enum AskTurnStartError {
    /// The request named an `ask_threads` id that does not exist.
    ThreadNotFound {
        #[ts(type = "number")]
        thread_id: i64,
    },
    /// No answer model is attached — the UI's "answer model not loaded; load it?" state.
    NoAnswerModel,
    /// Planning, retrieval or persistence failed. `message` is the kernel's error text.
    Failed { message: String },
}

impl From<AskTurnError> for AskTurnStartError {
    /// One mapping, so the in-app command and any future caller report the same failure the same
    /// way. The kernel's own [`std::fmt::Display`] wording is kept for `Failed`.
    fn from(error: AskTurnError) -> Self {
        match error {
            AskTurnError::ThreadNotFound(thread_id) => Self::ThreadNotFound { thread_id },
            AskTurnError::NoAnswerModel => Self::NoAnswerModel,
            AskTurnError::Failed(error) => Self::Failed {
                message: format!("{error:#}"),
            },
        }
    }
}

#[cfg(test)]
mod ts_number_guard {
    use super::*;
    use ts_rs::TS;

    /// Every `i64`/`u64` field of a P9 wire type must export as TS `number`, not `bigint` — the
    /// same rule and reason as `ipc::ts_number_guard` (Tauri's JSON wire delivers 64-bit ints as
    /// JS numbers). P8's list stays untouched; this module mirrors it for the graph and
    /// conversation types, including `ThreadSummary`, whose P9 fields are new.
    #[test]
    fn no_bigint_in_graph_ipc_types() {
        let decls = [
            ("ThreadSummary", ThreadSummary::inline()),
            ("ThreadListQuery", ThreadListQuery::inline()),
            ("GraphProgress", GraphProgress::inline()),
            ("ThreadListPage", ThreadListPage::inline()),
            ("ConnectedThread", ConnectedThread::inline()),
            ("ThreadPage", ThreadPage::inline()),
            ("EntityListQuery", EntityListQuery::inline()),
            ("InsightListQuery", InsightListQuery::inline()),
            ("InsightWindowStatus", InsightWindowStatus::inline()),
            ("InsightListPage", InsightListPage::inline()),
            ("OverrideKind", OverrideKind::inline()),
            ("OverrideReceipt", OverrideReceipt::inline()),
            ("RenameThreadRequest", RenameThreadRequest::inline()),
            ("MergeThreadsRequest", MergeThreadsRequest::inline()),
            ("SplitThreadRequest", SplitThreadRequest::inline()),
            ("AskCitations", AskCitations::inline()),
            ("AskThreadSummary", AskThreadSummary::inline()),
            ("AskTurn", AskTurn::inline()),
            ("AskWindowScope", AskWindowScope::inline()),
            ("AskConversationRequest", AskConversationRequest::inline()),
            ("AskTurnStarted", AskTurnStarted::inline()),
            ("AskTurnStartError", AskTurnStartError::inline()),
        ];
        for (name, decl) in decls {
            assert!(
                !decl.contains("bigint"),
                "{name} exports a `bigint` field — add #[ts(type = \"number\")]: {decl}"
            );
        }
    }
}
