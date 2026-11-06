//! P9's intelligence commands: the work graph (threads, entities, insights) and Ask
//! conversations (`03 §7h`, `§7i.4`, `§7j`; wire types in `traits::graph_ipc`).
//!
//! Every read here is a thin adapter. It parses the typed request, clamps the page size, calls
//! exactly the `Store` read the HTTP surface (`crates/api/src/graph_reads.rs`) already calls, and
//! returns the typed page — no command holds SQL, and none recomputes a figure the store reports
//! differently.
//!
//! The three overrides (rename / merge / split) are the module's only writes, and they all go
//! through `Store::graph_insert_thread_override`. That method persists the `thread_overrides`
//! row **and** enqueues the `graph_cluster` job in one transaction (`03 §7h.5`), so the fold
//! happens when the queue applies the override — never here. Nothing in this module writes
//! `threads` or `thread_sessions`; the returned [`OverrideReceipt`] names the override, not a
//! finished fold.
//!
//! `ask_turn` is the conversation twin of the legacy single-turn `ask` command (`lib.rs`): the
//! kernel pipeline is the same one, the delta stream is relayed by the same
//! [`crate::relay_answer_deltas`] into the same `answer_delta` event, and the task is registered in
//! the same `ask_tasks` slot so `cancel_ask` still reaches it.

use std::sync::Arc;

use store::SqliteStore;
use tauri::{Emitter, State};
use traits::{
    AnswerOpts, AskConversationRequest, AskThreadScope, AskThreadSummary, AskTurn, AskTurnRequest,
    AskTurnStartError, AskTurnStarted, AskTurnStream, AskWindowOverride, AskWindowScope,
    EntityFilter, EntityListQuery, EntitySummary, EvidenceKind, InsightFilter, InsightKind,
    InsightListPage, InsightListQuery, MergeThreadsRequest, OverrideKind, OverrideReceipt,
    RenameThreadRequest, SplitThreadRequest, ThreadFilter, ThreadListPage, ThreadListQuery,
    ThreadOverrideDraft, ThreadOverrideKind, ThreadOverridePayload, ThreadPage, ThreadSummary,
    TimeRange,
};

use crate::{next_ask_id, now_ms, relay_answer_deltas, AppState};

/// Hard ceiling on every graph list read, matching `crates/api`'s `LIST_LIMIT_MAX`: a larger
/// request is clamped, never rejected, and a `0` clamps up to `1` (an empty page is not
/// expressible, and `1..=1000` is the documented range on every list surface).
const LIST_LIMIT_MAX: u32 = 1000;

/// How many connected threads the Thread page reads (`03 §7h.4`). The store orders them by
/// shared-entity count then recency, so this is the strongest 50 connections rather than a
/// sample; a thread sharing a subject with hundreds of others is coarse clustering, not a list
/// worth rendering.
const THREAD_CONNECTIONS_LIMIT: u32 = 50;

/// How much of the question names a new Ask conversation (`03 §7i.4`): the trimmed question,
/// at most 80 characters, so the Ask list stays a list.
const ASK_THREAD_TITLE_CHARS: usize = 80;

/// The data spine, or the reason every graph command reports when the DB failed to open.
fn open_store(state: &AppState) -> Result<Arc<SqliteStore>, String> {
    state
        .store
        .clone()
        .ok_or_else(|| "database unavailable".to_string())
}

/// Clamp a requested page size into `1..=LIST_LIMIT_MAX`.
fn clamp_limit(limit: u32) -> u32 {
    limit.clamp(1, LIST_LIMIT_MAX)
}

/// A row count on the wire is `u32`; a list that long cannot occur in practice, so saturate
/// rather than report a wrapped number.
fn wire_count(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The store's thread-overlap predicate (`03 §7c` P8 block): `first_seen_at < to AND
/// last_seen_at >= from`, with a missing bound leaving that side open. Mirrored here only for
/// the entity-filtered list, where the store read takes the entity but no window.
fn overlaps_window(thread: &ThreadSummary, from_ms: Option<i64>, to_ms: Option<i64>) -> bool {
    if let Some(from) = from_ms {
        if thread.last_seen_at < from {
            return false;
        }
    }
    if let Some(to) = to_ms {
        if thread.first_seen_at >= to {
            return false;
        }
    }
    true
}

/// The title a started conversation takes from its first question (`03 §7i.4`): trimmed, at most
/// [`ASK_THREAD_TITLE_CHARS`] characters. Char-based so a multi-byte question is never cut
/// mid-character.
fn conversation_title(question: &str) -> String {
    question
        .trim()
        .chars()
        .take(ASK_THREAD_TITLE_CHARS)
        .collect()
}

/// The Work screen's thread page (`list_threads`, `03 §7h.4`): threads overlapping the half-open
/// window, optionally narrowed to threads a given entity was seen in, plus the backfill progress
/// that frames the list.
///
/// Threads with no member sessions are filtered out — those are the emptied merge sources
/// `03 §7h.5` keeps as durability evidence, never user-visible work.
#[tauri::command]
pub async fn list_threads(
    query: ThreadListQuery,
    state: State<'_, AppState>,
) -> Result<ThreadListPage, String> {
    let store = open_store(&state)?;
    let limit = clamp_limit(query.limit);
    let mut threads = match query.entity_id {
        Some(entity_id) => {
            // `threads_for_entity` takes the entity but no window, so the documented window
            // predicate is applied here over the same bounded read. Filtering after the store's
            // `LIMIT` would silently page matches away; reading the full 1000 and truncating to
            // the request keeps "window AND entity" exact.
            let mut threads = store
                .threads_for_entity(entity_id, LIST_LIMIT_MAX)
                .await
                .map_err(|e| e.to_string())?;
            threads.retain(|thread| overlaps_window(thread, query.from_ms, query.to_ms));
            threads
        }
        None => store
            .threads_in_range(ThreadFilter {
                from_ms: query.from_ms,
                to_ms: query.to_ms,
                limit,
            })
            .await
            .map_err(|e| e.to_string())?,
    };
    // An emptied merge source is durability evidence, not work (`03 §7h.5`). Dropping them before
    // the truncation keeps the page full when the newest rows happen to be folded threads.
    threads.retain(|thread| thread.session_count > 0);
    threads.truncate(limit as usize);
    let progress = store.graph_progress().await.map_err(|e| e.to_string())?;
    Ok(ThreadListPage { threads, progress })
}

/// One thread's page (`get_thread_page`, `03 §7h.4`/`§7h.5`/`§7j`): the member sessions and
/// entities, the survivor it was merged into (if any), the threads it connects to, its open loops
/// and the backfill progress.
///
/// A merged-away source thread explains the merge instead of rendering as an empty page, so
/// `detail` is `None` whenever `merged_into` is set. Both are `None` when the thread does not
/// exist at all.
#[tauri::command]
pub async fn get_thread_page(
    thread_id: i64,
    state: State<'_, AppState>,
) -> Result<ThreadPage, String> {
    let store = open_store(&state)?;
    let detail = store
        .thread_detail(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    let merged_into = match store
        .thread_merge_target(thread_id)
        .await
        .map_err(|e| e.to_string())?
    {
        // The survivor's summary is read through its own detail: `thread_detail` is the only
        // read that turns one thread id into a `ThreadSummary`.
        Some(survivor_id) => store
            .thread_detail(survivor_id)
            .await
            .map_err(|e| e.to_string())?
            .map(|survivor| survivor.thread),
        None => None,
    };

    // An open loop belongs to this thread when its evidence names the thread itself or one of the
    // thread's member sessions (`03 §7j`); a merged-away source has no sessions left to name.
    let mut refs: Vec<(EvidenceKind, i64)> = vec![(EvidenceKind::Thread, thread_id)];
    if let Some(detail) = &detail {
        refs.extend(
            detail
                .sessions
                .iter()
                .map(|session| (EvidenceKind::Session, session.session_id)),
        );
    }
    let open_loops = store
        .insights_for_evidence(Some(InsightKind::OpenLoop), &refs)
        .await
        .map_err(|e| e.to_string())?;
    let connected = store
        .connected_threads(thread_id, THREAD_CONNECTIONS_LIMIT)
        .await
        .map_err(|e| e.to_string())?;
    let progress = store.graph_progress().await.map_err(|e| e.to_string())?;
    Ok(ThreadPage {
        detail: if merged_into.is_some() { None } else { detail },
        merged_into,
        connected,
        open_loops,
        progress,
    })
}

/// Entities filtered by an exact kind and/or a case-insensitive name/alias substring
/// (`list_entities`, `03 §7c` P8 block). An empty `kind` or `query` means "no filter", the same
/// reading the HTTP route gives an empty `q`.
#[tauri::command]
pub async fn list_entities(
    query: EntityListQuery,
    state: State<'_, AppState>,
) -> Result<Vec<EntitySummary>, String> {
    let store = open_store(&state)?;
    let filter = EntityFilter {
        kind: query.kind.filter(|kind| !kind.trim().is_empty()),
        query: query.query.filter(|text| !text.trim().is_empty()),
        limit: clamp_limit(query.limit),
    };
    store
        .entities_filtered(filter)
        .await
        .map_err(|e| e.to_string())
}

/// The entities named by `ids` (`get_entities`), for scope chips and evidence labels. Ids that no
/// longer exist are simply absent from the result — the caller tolerates a shorter list.
#[tauri::command]
pub async fn get_entities(
    ids: Vec<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<EntitySummary>, String> {
    let store = open_store(&state)?;
    store.entities_by_ids(&ids).await.map_err(|e| e.to_string())
}

/// Insights overlapping the half-open window, optionally of one kind (`list_insights`, `03 §7j`),
/// with the generation status that decides the partial label: `pending_jobs > 0` means the window
/// is still filling.
#[tauri::command]
pub async fn list_insights(
    query: InsightListQuery,
    state: State<'_, AppState>,
) -> Result<InsightListPage, String> {
    let store = open_store(&state)?;
    let insights = store
        .list_insights(InsightFilter {
            from_ms: query.from_ms,
            to_ms: query.to_ms,
            kind: query.kind,
            limit: clamp_limit(query.limit),
        })
        .await
        .map_err(|e| e.to_string())?;
    let status = store
        .insight_window_status()
        .await
        .map_err(|e| e.to_string())?;
    Ok(InsightListPage { insights, status })
}

/// Rename one thread (`rename_thread`, `03 §7h.5`): a `rename` override that sets `threads.title`
/// when clustering applies it. Membership is untouched, so the receipt moves no sessions.
#[tauri::command]
pub async fn rename_thread(
    request: RenameThreadRequest,
    state: State<'_, AppState>,
) -> Result<OverrideReceipt, String> {
    let store = open_store(&state)?;
    let draft = ThreadOverrideDraft {
        kind: ThreadOverrideKind::Rename,
        thread_id: Some(request.thread_id),
        target_thread_id: None,
        // `ThreadOverrideDraft::validate` rejects a whitespace-only title; trimming here keeps a
        // stray leading/trailing space out of the persisted title.
        payload: ThreadOverridePayload::Rename {
            title: request.title.trim().to_string(),
        },
    };
    draft.validate().map_err(|e| e.to_string())?;
    let override_id = store
        .graph_insert_thread_override(&draft, now_ms())
        .await
        .map_err(|e| e.to_string())?;
    Ok(OverrideReceipt {
        override_id,
        kind: OverrideKind::Rename,
        sessions_moved: 0,
    })
}

/// Fold one thread's sessions into another (`merge_threads`, `03 §7h.5`).
///
/// The payload lists **every** current member session of the source, which is both the audit
/// record and what makes the receipt's `sessions_moved` checkable. The fold itself follows
/// `thread_sessions`, so the payload can never contradict the rows; the source thread is retained
/// (emptied) and reads back through `get_thread_page`'s `merged_into`.
#[tauri::command]
pub async fn merge_threads(
    request: MergeThreadsRequest,
    state: State<'_, AppState>,
) -> Result<OverrideReceipt, String> {
    let store = open_store(&state)?;
    let source = store
        .thread_detail(request.source_thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "the merge source thread does not exist".to_string())?;
    let session_ids: Vec<i64> = source
        .sessions
        .iter()
        .map(|session| session.session_id)
        .collect();
    if session_ids.is_empty() {
        return Err("the merge source thread has no sessions to move".to_string());
    }
    let draft = ThreadOverrideDraft {
        kind: ThreadOverrideKind::Merge,
        thread_id: Some(request.source_thread_id),
        target_thread_id: Some(request.target_thread_id),
        payload: ThreadOverridePayload::Merge { session_ids },
    };
    draft.validate().map_err(|e| e.to_string())?;
    let override_id = store
        .graph_insert_thread_override(&draft, now_ms())
        .await
        .map_err(|e| e.to_string())?;
    Ok(OverrideReceipt {
        override_id,
        kind: OverrideKind::Merge,
        sessions_moved: source.thread.session_count,
    })
}

/// Move the listed sessions out of a thread (`split_thread`, `03 §7h.5`). The new thread is
/// seeded by the override when clustering applies it; at least one session is required and no id
/// may repeat (`ThreadOverrideDraft::validate` enforces both).
#[tauri::command]
pub async fn split_thread(
    request: SplitThreadRequest,
    state: State<'_, AppState>,
) -> Result<OverrideReceipt, String> {
    let store = open_store(&state)?;
    let sessions_moved = wire_count(request.session_ids.len());
    let draft = ThreadOverrideDraft {
        kind: ThreadOverrideKind::Split,
        thread_id: Some(request.thread_id),
        target_thread_id: None,
        payload: ThreadOverridePayload::Split {
            thread_id: request.thread_id,
            session_ids: request.session_ids,
        },
    };
    draft.validate().map_err(|e| e.to_string())?;
    let override_id = store
        .graph_insert_thread_override(&draft, now_ms())
        .await
        .map_err(|e| e.to_string())?;
    Ok(OverrideReceipt {
        override_id,
        kind: OverrideKind::Split,
        sessions_moved,
    })
}

/// Every Ask conversation, newest first (`list_ask_threads`, `03 §7i.4`).
#[tauri::command]
pub async fn list_ask_threads(
    limit: u32,
    state: State<'_, AppState>,
) -> Result<Vec<AskThreadSummary>, String> {
    let store = open_store(&state)?;
    store
        .list_ask_threads(clamp_limit(limit))
        .await
        .map_err(|e| e.to_string())
}

/// Every persisted turn of one conversation, in `ordinal` order (`list_ask_turns`, `03 §7i.4`).
/// This — not a replayed stream — is how a reloaded conversation renders its answers and
/// citations.
#[tauri::command]
pub async fn list_ask_turns(
    thread_id: i64,
    state: State<'_, AppState>,
) -> Result<Vec<AskTurn>, String> {
    let store = open_store(&state)?;
    let turns = store
        .list_ask_turns(thread_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(turns.into_iter().map(AskTurn::from).collect())
}

/// Delete one conversation and its turns (`delete_ask_thread`, `03 §7i.4`). `false` means the row
/// was already gone; no frame, session, thread or insight is touched either way.
#[tauri::command]
pub async fn delete_ask_thread(thread_id: i64, state: State<'_, AppState>) -> Result<bool, String> {
    let store = open_store(&state)?;
    store
        .delete_ask_thread(thread_id)
        .await
        .map_err(|e| e.to_string())
}

/// Rename one conversation (`set_ask_thread_title`, `03 §7i.4`). `false` means the row was
/// already gone.
#[tauri::command]
pub async fn set_ask_thread_title(
    thread_id: i64,
    title: String,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let store = open_store(&state)?;
    let title = title.trim();
    if title.is_empty() {
        return Err("a conversation title must not be empty".to_string());
    }
    store
        .set_ask_thread_title(thread_id, title)
        .await
        .map_err(|e| e.to_string())
}

/// Start one Ask conversation turn (`ask_turn`, `03 §7i.2`/`§7i.4`) and return the ids its deltas
/// stream under.
///
/// The mapping is the whole job: the three editable scope chips become the kernel's request-side
/// overrides, an absent `thread_id` starts a new conversation, and `top_k` falls back to the
/// configured `retrieval.default_top_k` exactly like the legacy `ask` command. The returned stream
/// is drained by [`relay_answer_deltas`] into the same `answer_delta` event, keyed by this
/// request's id, so `cancel_ask` reaches it through the same `ask_tasks` slot.
#[tauri::command]
pub async fn ask_turn(
    request: AskConversationRequest,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<AskTurnStarted, AskTurnStartError> {
    let request_id = request
        .request_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| format!("conversation-{}", next_ask_id()));
    let store = state
        .store
        .clone()
        .ok_or_else(|| AskTurnStartError::Failed {
            message: "database unavailable".to_string(),
        })?;
    let kernel = state
        .kernel
        .clone()
        .ok_or_else(|| AskTurnStartError::Failed {
            message: "kernel unavailable".to_string(),
        })?;

    let settings = kernel::settings::load_settings(store.as_ref()).await;
    let top_k = request.top_k.unwrap_or(settings.retrieval_default_top_k);
    let window_override = match request.window {
        AskWindowScope::Inherit => None,
        AskWindowScope::All => Some(AskWindowOverride::All),
        AskWindowScope::Range { start, end } => {
            Some(AskWindowOverride::Range(TimeRange { start, end }))
        }
    };
    // `clear_thread_scope` wins over a stale id: the UI sends both when the user clears the chip
    // while the previous turn's scope is still loaded.
    let thread_scope = if request.clear_thread_scope {
        AskThreadScope::None
    } else {
        match request.scope_thread_id {
            Some(thread_id) => AskThreadScope::Thread(thread_id),
            None => AskThreadScope::Inherit,
        }
    };
    let new_conversation = request.thread_id.is_none();
    let title = conversation_title(&request.query);

    let stream = kernel
        .ask_turn(AskTurnRequest {
            query: request.query,
            thread_id: request.thread_id,
            // Session scoping is the API/MCP surface (`03 §7c`, D10); a conversation turn is
            // frame-level like the in-app Ask.
            session_id: None,
            top_k,
            window_override,
            entity_override: request.entities,
            thread_scope,
            opts: AnswerOpts {
                thinking: request.thinking,
                max_tokens: request.max_tokens,
            },
        })
        .await
        .map_err(AskTurnStartError::from)?;
    let AskTurnStream {
        thread_id,
        turn_id,
        deltas,
    } = stream;

    // The first question names a new conversation (`§7i.4`). The turn is already streaming, so a
    // failed naming write must not lose it: the title stays NULL (the Ask list falls back to the
    // newest question) and the next turn can name the thread again.
    if new_conversation && !title.is_empty() {
        if let Err(error) = store.set_ask_thread_title(thread_id, &title).await {
            tracing::warn!(error = %error, "naming a started Ask conversation failed");
        }
    }

    let event_request_id = request_id.clone();
    let task_request_id = request_id.clone();
    let tasks = state.ask_tasks.clone();
    let handle = tokio::spawn(async move {
        relay_answer_deltas(deltas, &event_request_id, |event| {
            app.emit("answer_delta", event)
        })
        .await;
        tasks.lock().await.remove(&task_request_id);
    });
    if let Some(previous) = state
        .ask_tasks
        .lock()
        .await
        .insert(request_id.clone(), handle)
    {
        previous.abort();
    }
    Ok(AskTurnStarted {
        request_id,
        thread_id,
        turn_id,
    })
}
