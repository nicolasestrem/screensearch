//! Request-side Ask v2 scope overrides (`03 §7i.4`) — the kernel's view of the editable scope
//! chips.
//!
//! These are **not** IPC wire types: the UI sends [`crate::AskWindowScope`], an optional entity id
//! list and an optional work-graph thread id, and the command maps them onto these enums before
//! calling `Kernel::ask_turn`. Keeping the kernel's vocabulary here rather than in
//! [`crate::domain`] leaves the domain types free of UI-shaped unions, and keeps the mapping in
//! one place with the wire type it comes from.

use crate::ipc::TimeRange;

/// Explicit window override on an [`crate::AskTurnRequest`] (`03 §7i.4`).
///
/// Without an override the planner reuses the previous turn's window and parses a relative phrase
/// out of the question; with one, the named window wins — the scope chips show exactly what was
/// applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskWindowOverride {
    /// Whole history: both `ask_turns` window columns stay NULL.
    All,
    /// Explicit half-open `[start, end)` in unix epoch ms.
    Range(TimeRange),
}

/// Thread scope override on an [`crate::AskTurnRequest`] (`03 §7i.2` stage 1, `§7i.4`).
///
/// A follow-up inherits the previous turn's scope unless the question or the UI changes it, so
/// "and the day before?" keeps looking at the same thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AskThreadScope {
    /// Reuse the previous turn's scope (`§7i.4` follow-up rule).
    Inherit,
    /// No thread scope: stage 1 runs over the whole window.
    None,
    /// Scope stage 1 to this `threads.id`.
    Thread(i64),
}
