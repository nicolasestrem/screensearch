//! Ask v2 pipeline (`03 §7i`, 0.4.5 P6): plan → the four retrieval stages → stream.
//!
//! Public seams:
//!
//! - [`ask_context`] — the legacy frame-level Ask retrieval (`03 §7c`) as a free function, so
//!   the kernel, the composition root's `ApiHost` and the dev eval fixture all run the same
//!   code instead of a copy that can drift.
//! - [`Kernel::ask_context`] — the same retrieval as a `Kernel` method; every `/v1/ask`
//!   without a thread still answers through it. Its results are unchanged: hybrid frame
//!   search (optionally restricted to one session), full OCR text hydrated in one bulk read,
//!   no session context on the chunks.
//! - [`Kernel::ask_turn`] — the multi-turn pipeline. It resolves the turn's conversation,
//!   plans its window and entity scope, persists the turn with a NULL answer, runs stages
//!   1..4, and returns an [`AskTurnStream`] whose terminal sequence is
//!   `Citation* , Citations, Done`.
//!
//! Stage order is normative (`03 §7i.2`) and each stage only narrows the previous one:
//! threads → sessions (at most [`SESSION_K`]) → frames → assembly. The documented
//! non-blocking fallbacks are the point of the seam: an empty work graph leaves stage 1
//! empty, so stage 2 sees the window's sessions; when no session wins (no summaries, no
//! vectors) stage 3 runs the existing hybrid frame search over the window directly. On a
//! database with an empty graph and no session summaries Ask v2 therefore answers exactly
//! like today's frame-level Ask with an optional time filter.
//!
//! Citations are provenance, never prose (`crates/inference/src/answer.rs`): the forwarder
//! derives them from the frames the provider actually cited, restricted to the frames that
//! were placed in the prompt, so a cited frame always belongs to a cited session.
//!
//! This module also owns the on-demand half of `session_embed` (`03 §7i.2`, 0.4.5 D11):
//! a turn with pending session vectors enqueues one bounded backfill job, under the same
//! guard the idle producer uses, and never fails the turn when it cannot.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use traits::{
    AnswerDelta, AnswerOpts, AnswerProvider, AskCitations, AskThreadCandidate, AskThreadScope,
    AskTurnError, AskTurnRequest, AskTurnStream, AskWindowOverride, ChunkSession, NewAskTurn,
    RetrievedChunk, SearchHit, SearchQuery, StageCounts, Store,
};

use crate::ask_plan;
use crate::Kernel;

/// Stage 2 keeps at most this many session winners; a kernel constant, not a settings key
/// (0.4.5 D6).
pub(crate) const SESSION_K: u32 = 10;

/// Legacy frame-level Ask retrieval (`03 §7c`), as a free function so every caller — the
/// kernel's own [`Kernel::ask_context`], the composition root's `ApiHost`
/// (`src-tauri/src/local_api.rs`) and the dev eval fixture's host
/// (`crates/evalfixture/src/serve.rs`) — runs this one implementation. A copy in a caller
/// that can drift from the shipped pipeline measures the wrong thing.
///
/// Hybrid frame retrieval over content text at `top_k` depth (chrome stays opt-in),
/// optionally restricted to one session's own frames, then each hit's full OCR text
/// hydrated in one bulk query. A frame with no stored text falls back to its search
/// snippet. `top_k` is floored at one, matching `store::search::normalized_limit`; no
/// session context is attached, so the prompt and token cost are exactly what they were
/// before Ask v2.
pub async fn ask_context(
    store: &Arc<dyn Store>,
    query: &str,
    top_k: u32,
    session_id: Option<i64>,
) -> anyhow::Result<Vec<RetrievedChunk>> {
    let search = SearchQuery {
        text: query.to_string(),
        limit: top_k.max(1),
        time_range: None,
        // Ask grounds on content text (`03 §3b`); raw/chrome stays opt-in.
        include_chrome: false,
    };
    let hits = match session_id {
        Some(id) => store.hybrid_search_in_session(&search, id).await?,
        None => store.hybrid_search(&search).await?,
    };
    let frame_ids: Vec<i64> = hits.iter().map(|hit| hit.frame_id).collect();
    let mut ocr = store.ocr_texts(&frame_ids).await?;
    Ok(hits
        .into_iter()
        .map(|hit| {
            // `ocr` is consumed here, so each string is moved rather than cloned.
            let text = ocr.remove(&hit.frame_id).unwrap_or(hit.snippet);
            RetrievedChunk {
                frame_id: hit.frame_id,
                text,
                score: hit.score,
                captured_at: hit.captured_at,
                session: None,
            }
        })
        .collect())
}

impl Kernel {
    /// Legacy frame-level Ask (`03 §7c`): delegates to [`ask_context`], the one retrieval
    /// every legacy caller shares.
    pub async fn ask_context(
        &self,
        query: &str,
        top_k: u32,
        session_id: Option<i64>,
    ) -> anyhow::Result<Vec<RetrievedChunk>> {
        ask_context(&self.store, query, top_k, session_id).await
    }

    /// Ask v2 (`03 §7i`): plan → stages 1..4 → stream. The returned stream carries every
    /// existing [`AnswerDelta`] unchanged plus one `Citations` before the terminal `Done`;
    /// dropping it cancels the turn, whose answer then stays NULL (`§7i.4`).
    pub async fn ask_turn(&self, request: AskTurnRequest) -> Result<AskTurnStream, AskTurnError> {
        let question = request.query.trim().to_string();
        let now = crate::worker_pool::now_ms();

        // 1. Conversation context: an explicit thread must exist (404 wins over a missing
        //    model, and no conversation is created when the turn cannot run); a follow-up
        //    inherits the previous turn's window and entity scope (`§7i.4`).
        let previous = match request.thread_id {
            Some(id) => {
                if !self
                    .store
                    .ask_thread_exists(id)
                    .await
                    .map_err(AskTurnError::Failed)?
                {
                    return Err(AskTurnError::ThreadNotFound(id));
                }
                self.store
                    .last_ask_turn(id)
                    .await
                    .map_err(AskTurnError::Failed)?
            }
            None => None,
        };
        // Nothing to answer with: fail before any persistence (the route maps this to 503).
        let provider = self
            .answer_provider()
            .await
            .ok_or(AskTurnError::NoAnswerModel)?;
        let thread_id = match request.thread_id {
            Some(id) => id,
            None => self
                .store
                .create_ask_thread()
                .await
                .map_err(AskTurnError::Failed)?,
        };

        // 2. Planning: an explicit override wins over the planner, which parses a calendar
        //    window in the system local timezone and resolves the entity scope from the
        //    question. Both fall back to the previous turn's values; an unresolved anchor never
        //    produces bounds and a mention with no graph row stays plain text.
        //
        //    `AskWindowScope`'s three cases, one for one (`§7i.4`):
        //    - `Range` is exactly that window and nothing else — the scope chip said so;
        //    - `All` clears it to the whole history (both `ask_turns` window columns NULL);
        //    - no override lets the planner inherit the previous window and parse the question.
        let window = match request.window_override {
            Some(AskWindowOverride::All) => None,
            Some(AskWindowOverride::Range(range)) => Some(range),
            None => ask_plan::plan_turn_window(
                &question,
                now,
                &chrono::Local,
                previous.as_ref().and_then(|turn| turn.window),
            ),
        };
        // Same three cases for the entity scope:
        // - `Some(ids)` is exactly those ids, and the planner does not resolve or inherit —
        //   `Some(vec![])` therefore really clears the scope instead of letting a mention in
        //   the question quietly re-add one;
        // - `None` keeps today's rule: resolve mentions from the question, else inherit.
        let entity_scope = match &request.entity_override {
            Some(ids) => sorted_unique(ids.clone()),
            None => {
                let resolved = self
                    .store
                    .ask_resolve_entities(&question)
                    .await
                    .map_err(AskTurnError::Failed)?;
                let scope: Vec<i64> = if resolved.is_empty() {
                    previous
                        .as_ref()
                        .map(|turn| turn.resolved_entities.clone())
                        .unwrap_or_default()
                } else {
                    resolved.iter().map(|m| m.entity_id).collect()
                };
                sorted_unique(scope)
            }
        };
        // Which work-graph thread stage 1 is scoped to (`§7i.2`, `§7i.4`): a follow-up inherits
        // the previous turn's scope unless the request clears or replaces it.
        let scope_thread_id = match request.thread_scope {
            AskThreadScope::Inherit => previous.as_ref().and_then(|turn| turn.scope_thread_id),
            AskThreadScope::None => None,
            AskThreadScope::Thread(id) => Some(id),
        };

        // 3. Persist the turn with a NULL answer, then run the stages in order. The scope thread
        //    is persisted with the turn so a later follow-up can inherit it.
        let turn = self
            .store
            .append_ask_turn(NewAskTurn {
                thread_id,
                question: question.clone(),
                window,
                resolved_entities: entity_scope.clone(),
                scope_thread_id,
            })
            .await
            .map_err(AskTurnError::Failed)?;

        // Stages 1 and 2 — skipped entirely for the legacy single-session scope (`03 §7c`),
        // where the one named session is the whole retrieval set.
        //
        // `entity_scope` carries the resolved **entity ids**: it is the only identifier
        // planning produces (`AskEntityMatch`), and the store joins it to `session_entities`
        // to reach the "entity-resolved sessions" stages 1 and 2 narrow on (`§7i.2`,
        // `§7h.3`). An empty scope is the documented no-graph fallback.
        let (threads, candidates, winning_sessions) = match request.session_id {
            Some(id) => (Vec::new(), vec![id], vec![id]),
            None => {
                // Stage 1 — threads. With a scope thread, stage 1 is exactly that thread: its
                // member sessions are the candidate set regardless of the window or the entity
                // scope, which stage 2 then narrows. `thread_detail` reads only that thread, so a
                // scoped turn never scans the whole graph; a scope naming a thread that no
                // longer exists yields no candidates, the documented empty-graph fallback rather
                // than an unfiltered answer.
                let threads = match scope_thread_id {
                    Some(id) => match self
                        .store
                        .thread_detail(id)
                        .await
                        .map_err(AskTurnError::Failed)?
                    {
                        Some(detail) => vec![AskThreadCandidate {
                            thread_id: id,
                            session_ids: detail
                                .sessions
                                .into_iter()
                                .map(|session| session.session_id)
                                .collect(),
                        }],
                        None => Vec::new(),
                    },
                    // Stage 1 — threads (empty graph ⇒ empty, the documented fallback).
                    None => self
                        .store
                        .ask_candidate_threads(window, &entity_scope)
                        .await
                        .map_err(AskTurnError::Failed)?,
                };
                // Stage 2 — sessions over the candidate set (no thread candidates ⇒ the
                // window's sessions), keeping at most `SESSION_K` winners.
                let candidates = self
                    .store
                    .ask_candidate_sessions(&threads, window, &entity_scope)
                    .await
                    .map_err(AskTurnError::Failed)?;
                let session_hits = self
                    .store
                    .ask_search_sessions(&question, &candidates, SESSION_K)
                    .await
                    .map_err(AskTurnError::Failed)?;
                let winners = session_hits
                    .iter()
                    .map(|hit| hit.session_id)
                    .collect::<Vec<_>>();
                (threads, candidates, winners)
            }
        };
        let search = SearchQuery {
            text: question.clone(),
            limit: request.top_k.max(1),
            time_range: window,
            include_chrome: false,
        };
        // Stage 3 — frames inside the winning sessions. Reaching "no winner" has two meanings
        // and they must not be conflated:
        //  * no entity resolved and nothing ranked (no summaries, no vectors yet) ⇒ the
        //    mandated non-blocking fallback: the existing frame search over the window alone
        //    (`§7i.1`, "never worse than the previous Ask"), i.e. no session restriction.
        //  * an entity resolved **and its linked sessions are in scope** ⇒ the mention is a
        //    constraint (`§7i.1`), so the search stays inside those candidate sessions even
        //    when stage 2 could not rank them (no summaries or vectors yet); without this the
        //    frame stage would silently discard the constraint and cite unrelated sessions.
        //  * an entity resolved but no candidate session survives the window ⇒ the mention has
        //    no usable match, which `§7i.1` answers with plain-text matching, so the window
        //    fallback applies rather than a dead answer.
        let entity_constrained = !entity_scope.is_empty() && !candidates.is_empty();
        let frame_scope: Vec<i64> = if !winning_sessions.is_empty() {
            winning_sessions.clone()
        } else if entity_constrained {
            candidates.clone()
        } else {
            Vec::new()
        };
        let hits = self
            .store
            .ask_search_frames(&search, &frame_scope)
            .await
            .map_err(AskTurnError::Failed)?;
        let stage_counts = StageCounts {
            threads: u32::try_from(threads.len()).unwrap_or(u32::MAX),
            sessions: u32::try_from(winning_sessions.len()).unwrap_or(u32::MAX),
            frames: u32::try_from(hits.len()).unwrap_or(u32::MAX),
        };

        // Stage 4 — assembly, best evidence first, bounded by the provider's context window
        // when it reports one.
        let frame_ids: Vec<i64> = hits.iter().map(|hit| hit.frame_id).collect();
        let mut ocr = self
            .store
            .ocr_texts(&frame_ids)
            .await
            .map_err(AskTurnError::Failed)?;
        let frame_session = self
            .store
            .ask_frame_sessions(&frame_ids)
            .await
            .map_err(AskTurnError::Failed)?;
        let session_ids = sorted_unique(frame_session.values().copied().collect());
        let labels = self
            .store
            .ask_session_labels(&session_ids)
            .await
            .map_err(AskTurnError::Failed)?;
        // The reply budget is reserved out of the window; the provider's own packer keeps
        // its per-chunk hard bound on top of this (`crates/inference/src/answer.rs`). The
        // reservation must be the one the provider actually uses: `max_tokens` above half the
        // window is capped there, so subtracting the raw request here would pre-drop evidence
        // the packer had room for (a 2048-token window with the default 2048 `max_tokens`
        // would assemble exactly one chunk).
        let budget = assembly_budget(provider.answer_context_budget(), request.opts.max_tokens);
        let (chunks, tokens_assembled) =
            assemble_chunks(hits, &mut ocr, &frame_session, &labels, budget);
        // Citation provenance: the frame → session pairs actually handed to the provider,
        // captured before the context is moved into the provider task.
        let assembled: Vec<(i64, Option<i64>)> = chunks
            .iter()
            .map(|chunk| (chunk.frame_id, chunk.session.as_ref().map(|s| s.id)))
            .collect();

        tracing::info!(
            thread_id,
            turn_id = turn.id,
            threads = stage_counts.threads,
            sessions = stage_counts.sessions,
            frames = stage_counts.frames,
            tokens_assembled,
            window_start = window.map(|w| w.start),
            window_end = window.map(|w| w.end),
            entities = entity_scope.len(),
            "ask turn planned"
        );

        let (tx, rx) = tokio::sync::mpsc::channel::<AnswerDelta>(64);
        let store = self.store.clone();
        let opts = request.opts;
        let tokens = u32::try_from(tokens_assembled).unwrap_or(u32::MAX);
        tokio::spawn(async move {
            forward_turn(ForwardTurn {
                provider,
                store,
                question,
                context: chunks,
                assembled,
                opts,
                tx,
                thread_id,
                turn_id: turn.id,
                stage_counts,
                tokens_assembled: tokens,
            })
            .await;
        });

        // Ask v2's session vectors may still be behind; one bounded job, under the idle
        // producer's guard, never on the capture path and never fatal to the turn. The delta
        // task is already running, so this cannot delay the first delta — it delays only the
        // returned stream handle by one bounded DB round-trip (the enqueue's own guard read).
        crate::session_vectors::enqueue_on_demand(self.store.as_ref()).await;

        Ok(AskTurnStream {
            thread_id,
            turn_id: turn.id,
            deltas: rx,
        })
    }
}

/// Everything the forwarder needs, grouped so the task signature stays readable.
struct ForwardTurn {
    provider: std::sync::Arc<dyn AnswerProvider>,
    store: std::sync::Arc<dyn traits::Store>,
    question: String,
    context: Vec<RetrievedChunk>,
    /// `(frame_id, session_id)` for every chunk handed to the provider, in assembly order.
    assembled: Vec<(i64, Option<i64>)>,
    opts: AnswerOpts,
    tx: tokio::sync::mpsc::Sender<AnswerDelta>,
    thread_id: i64,
    turn_id: i64,
    stage_counts: StageCounts,
    tokens_assembled: u32,
}

/// Owns the provider's delta sender: forwards `Thinking`/`Token`/`Citation`, swallows the
/// provider's own `Done`, then emits exactly one `Citations` derived from the frames the
/// provider cited, persists the answer, and emits the terminal `Done` carrying the turn's
/// diagnostics (`03 §7i.2`, 0.4.5 D12).
///
/// Failure and cancellation are terminal and silent about citations:
/// - provider `Error` ⇒ the error is forwarded and nothing else follows (`§7i.4`: no answer
///   is persisted);
/// - a dropped/aborted consumer ⇒ the provider is aborted and nothing is emitted or
///   persisted, so the turn's answer stays NULL.
async fn forward_turn(turn: ForwardTurn) {
    let ForwardTurn {
        provider,
        store,
        question,
        context,
        assembled,
        opts,
        tx,
        thread_id,
        turn_id,
        stage_counts,
        tokens_assembled,
    } = turn;

    let (ptx, mut prx) = tokio::sync::mpsc::channel::<AnswerDelta>(64);
    let mut provider_task =
        tokio::spawn(async move { provider.answer(&question, &context, opts, ptx).await });

    let mut cited: Vec<i64> = Vec::new();
    let mut answer = String::new();
    let mut failed = false;
    let mut cancelled = false;
    loop {
        // A provider may go quiet (a long generation) while the consumer disconnects, so the
        // consumer's closure is watched directly rather than only through a failed send.
        let delta = tokio::select! {
            biased;
            _ = tx.closed() => {
                cancelled = true;
                break;
            }
            delta = prx.recv() => delta,
        };
        let Some(delta) = delta else {
            break;
        };
        match delta {
            AnswerDelta::Thinking { .. } | AnswerDelta::Token { .. } => {
                if let AnswerDelta::Token { text } = &delta {
                    answer.push_str(text);
                }
                if tx.send(delta).await.is_err() {
                    cancelled = true;
                    break;
                }
            }
            AnswerDelta::Citation { frame_id } => {
                // Provenance first, then the client keeps the per-frame citation event it
                // has always received (`03 §7i.3`: extended, never replaced). A frame the
                // terminal `Citations` list will not carry is not forwarded either, so the
                // stream and the terminal event can never name different evidence (the
                // provider's packer only cites packed frames, so this is a guard, not a filter
                // that drops real evidence today).
                if !assembled.iter().any(|(id, _)| *id == frame_id) {
                    continue;
                }
                if !cited.contains(&frame_id) {
                    cited.push(frame_id);
                }
                if tx.send(AnswerDelta::Citation { frame_id }).await.is_err() {
                    cancelled = true;
                    break;
                }
            }
            // The provider's own terminal `done` is replaced by the turn's
            // `Citations` + `Done` pair; a provider never emits `Citations` itself.
            AnswerDelta::Done { .. } | AnswerDelta::Citations { .. } => {}
            AnswerDelta::Error { .. } => {
                failed = true;
                // A closed consumer just ends the turn; there is nothing left to tell.
                if tx.send(delta).await.is_err() {
                    cancelled = true;
                    break;
                }
            }
        }
    }

    if cancelled {
        // The cooperative signal first: dropping the provider's receiver lets it stop the way
        // it already knows how (`AnswerSidecar` aborts the sidecar generation on a closed
        // consumer channel, `03 §7c`), which is the only path that also stops the child
        // process cleanly.
        drop(prx);
        // A provider still in preparation — downloading a model, waiting in the supervisor's
        // acquire — never looks at the channel, so waiting on cooperation alone would leave it
        // running after the user gave up. Give the cooperative path a bounded moment, then
        // abort rather than detaching the task. No citations, no done, no persistence.
        if tokio::time::timeout(PROVIDER_CANCEL_GRACE, &mut provider_task)
            .await
            .is_err()
        {
            provider_task.abort();
        }
        tracing::info!(
            thread_id,
            turn_id,
            "ask turn cancelled; answer not persisted"
        );
        return;
    }

    match provider_task.await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            // A provider that returns an error without streaming one still owes the client
            // a terminal event, and it must not emit citations for an answer that failed.
            tracing::warn!(thread_id, turn_id, error = %error, "ask provider failed");
            let _ = tx
                .send(AnswerDelta::Error {
                    message: error.to_string(),
                })
                .await;
            return;
        }
        Err(error) => {
            tracing::warn!(thread_id, turn_id, error = %error, "ask provider task failed");
            let _ = tx
                .send(AnswerDelta::Error {
                    message: format!("answer task failed: {error}"),
                })
                .await;
            return;
        }
    }

    if failed {
        tracing::warn!(thread_id, turn_id, "ask turn failed; answer not persisted");
        return;
    }

    // Citations are provenance of what inference packed: the provider cites the frames it
    // placed in the prompt, and this intersects that with the assembled context so a cited
    // frame can never fall outside the turn.
    let (sessions, frames) = derive_citations(&cited, &assembled);
    // The persisted pair is exactly what the delta carries (`§7i.3`), so a reloaded turn
    // re-renders the same evidence the stream named.
    let persisted = AskCitations {
        sessions: sessions.clone(),
        frames: frames.clone(),
    };
    let citations = AnswerDelta::Citations {
        sessions,
        frames,
        thread_id,
        turn_id,
        stage_counts,
        tokens_assembled,
    };
    if tx.send(citations).await.is_err() {
        // The consumer went away between the last token and the citations: the turn is
        // cancelled, so the answer stays NULL and no `Done` follows.
        tracing::info!(
            thread_id,
            turn_id,
            "ask consumer dropped before citations; answer not persisted"
        );
        return;
    }
    match store.finish_ask_turn(turn_id, &answer, &persisted).await {
        Ok(true) => {}
        Ok(false) => tracing::warn!(
            thread_id,
            turn_id,
            "ask turn row vanished before its answer landed"
        ),
        Err(error) => {
            tracing::warn!(thread_id, turn_id, error = %error, "persisting the ask answer failed")
        }
    }
    let _ = tx
        .send(AnswerDelta::Done {
            stage_counts: Some(stage_counts),
            tokens_assembled: Some(tokens_assembled),
        })
        .await;
}

/// The context budget left for assembled chunks once the reply reservation is subtracted from
/// the provider's window. The reservation is [`traits::effective_reply_budget`] — the same cap
/// the provider's own packer applies — so assembly can never pre-drop evidence the packer had room
/// for. It is deliberately not equal to the provider's internal budget: the packer also reserves
/// the system prompt, the question and template overhead, so this figure is the upper bound and
/// the packer's own per-chunk bound remains the hard cap. `None` (window unknown) leaves assembly
/// unbounded, exactly as before.
fn assembly_budget(ctx_size: Option<u32>, max_tokens: u32) -> Option<usize> {
    ctx_size.map(|ctx| {
        (ctx as usize).saturating_sub(traits::effective_reply_budget(max_tokens, ctx) as usize)
    })
}

/// How long a cancelled turn waits for its provider to stop on its own before the task is
/// aborted. Long enough for a provider already streaming to observe the closed channel and run
/// its own cleanup; far too short for a model download or a supervisor acquire to finish, which
/// is the case the abort exists for.
const PROVIDER_CANCEL_GRACE: std::time::Duration = std::time::Duration::from_millis(250);

/// Stage 4 (`03 §7i.2`): turns frame hits into prompt chunks, best evidence first, with the
/// frame's session context attached. `budget` bounds the summed [`RetrievedChunk::prompt_cost`];
/// truncation drops the tail (the lowest-ranked chunks) and never a chunk already kept. The
/// top chunk is kept even when it alone exceeds the budget, mirroring the provider's packer
/// (grounding on a truncated head beats dropping everything). Returns the chunks and the
/// assembled token total.
fn assemble_chunks(
    hits: Vec<SearchHit>,
    ocr: &mut HashMap<i64, String>,
    frame_session: &HashMap<i64, i64>,
    labels: &HashMap<i64, String>,
    budget: Option<usize>,
) -> (Vec<RetrievedChunk>, usize) {
    let mut chunks = Vec::with_capacity(hits.len());
    let mut tokens = 0usize;
    for hit in hits {
        let text = ocr.remove(&hit.frame_id).unwrap_or(hit.snippet);
        let session = frame_session.get(&hit.frame_id).map(|id| ChunkSession {
            id: *id,
            label: labels
                .get(id)
                .cloned()
                .unwrap_or_else(|| format!("session {id}")),
        });
        let chunk = RetrievedChunk {
            frame_id: hit.frame_id,
            text,
            score: hit.score,
            captured_at: hit.captured_at,
            session,
        };
        let cost = chunk.prompt_cost();
        if let Some(budget) = budget {
            if !chunks.is_empty() && tokens + cost > budget {
                break;
            }
        }
        tokens += cost;
        chunks.push(chunk);
    }
    (chunks, tokens)
}

/// Derives the citation pair from the frames the provider cited, in citation order,
/// restricted to the assembled context: `(sessions ascending distinct, frames)`.
fn derive_citations(cited: &[i64], assembled: &[(i64, Option<i64>)]) -> (Vec<i64>, Vec<i64>) {
    let mut frames: Vec<i64> = Vec::new();
    let mut sessions: BTreeSet<i64> = BTreeSet::new();
    for frame_id in cited {
        let Some((_, session)) = assembled.iter().find(|(id, _)| id == frame_id) else {
            continue;
        };
        if !frames.contains(frame_id) {
            frames.push(*frame_id);
        }
        if let Some(session_id) = session {
            sessions.insert(*session_id);
        }
    }
    (sessions.into_iter().collect(), frames)
}

fn sorted_unique(ids: Vec<i64>) -> Vec<i64> {
    let set: BTreeSet<i64> = ids.into_iter().collect();
    set.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(frame_id: i64, score: f32, snippet: &str) -> SearchHit {
        SearchHit {
            frame_id,
            captured_at: 0,
            snippet: snippet.to_string(),
            score,
            image_path: String::new(),
            image_purged: false,
            app_hint: None,
        }
    }

    fn chunk(frame_id: i64, text: &str, session: Option<i64>) -> RetrievedChunk {
        RetrievedChunk {
            frame_id,
            text: text.to_string(),
            score: 0.0,
            captured_at: 0,
            session: session.map(|id| ChunkSession {
                id,
                label: format!("session {id} label"),
            }),
        }
    }

    #[test]
    fn assembly_keeps_best_first_order_and_attaches_session_context() {
        let hits = vec![
            hit(11, 0.9, "compiler error"),
            hit(12, 0.8, "test run"),
            hit(13, 0.7, "unowned frame"),
        ];
        let mut ocr = HashMap::from([
            (11, "the compiler error text".to_string()),
            (12, "the test run text".to_string()),
        ]);
        let frame_session = HashMap::from([(11, 5), (12, 5)]);
        let labels = HashMap::from([(5, "Fix the scheduler".to_string())]);

        let (chunks, tokens) = assemble_chunks(hits, &mut ocr, &frame_session, &labels, None);

        assert_eq!(
            chunks.iter().map(|c| c.frame_id).collect::<Vec<_>>(),
            vec![11, 12, 13],
            "the hit order is the assembly order"
        );
        // Hydrated OCR wins over the snippet; a frame without stored text keeps its snippet.
        assert_eq!(chunks[0].text, "the compiler error text");
        assert_eq!(chunks[2].text, "unowned frame");
        assert_eq!(
            chunks[0].session,
            Some(ChunkSession {
                id: 5,
                label: "Fix the scheduler".to_string()
            })
        );
        assert_eq!(chunks[2].session, None, "a frame outside any session");
        // The assembled total is exactly the sum of the chunks' prompt costs.
        let expected: usize = chunks.iter().map(RetrievedChunk::prompt_cost).sum();
        assert_eq!(tokens, expected);
    }

    #[test]
    fn assembly_drops_the_tail_to_respect_the_budget() {
        let hits = vec![
            hit(1, 0.9, "x".repeat(400).as_str()),
            hit(2, 0.8, "y".repeat(400).as_str()),
            hit(3, 0.7, "z".repeat(400).as_str()),
        ];
        let mut ocr = HashMap::new();
        let frame_session = HashMap::new();
        let labels = HashMap::new();
        // Room for exactly the first two chunks, derived from their own cost.
        let first_cost = chunk(1, &hits[0].snippet, None).prompt_cost();
        let second_cost = chunk(2, &hits[1].snippet, None).prompt_cost();
        let budget = first_cost + second_cost;

        let (chunks, tokens) = assemble_chunks(
            hits.clone(),
            &mut ocr,
            &frame_session,
            &labels,
            Some(budget),
        );
        assert_eq!(
            chunks.iter().map(|c| c.frame_id).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(tokens, budget);

        // One token less and the second chunk no longer fits: only the head is kept.
        let (chunks, tokens) = assemble_chunks(
            hits.clone(),
            &mut ocr,
            &frame_session,
            &labels,
            Some(budget - 1),
        );
        assert_eq!(
            chunks.iter().map(|c| c.frame_id).collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(tokens, first_cost);

        // The top chunk is kept even alone over budget (the packer truncates it).
        let (chunks, _) = assemble_chunks(hits, &mut ocr, &frame_session, &labels, Some(1));
        assert_eq!(chunks.len(), 1, "grounding survives an impossible budget");
    }

    #[test]
    fn assembly_budget_uses_the_providers_capped_reply_reservation() {
        // A small window with the default 2048 `max_tokens`: the provider caps its reply to
        // half the window, so assembly must keep a real context budget. Subtracting the raw
        // request would leave 0 and assemble exactly one chunk — the regression this pins.
        assert_eq!(assembly_budget(Some(2048), 2048), Some(1024));
        assert_ne!(
            assembly_budget(Some(2048), 2048),
            Some(0),
            "the provider would have packed more than one snippet here"
        );
        // The normal 8K window passes the request through unchanged.
        assert_eq!(assembly_budget(Some(8192), 2048), Some(8192 - 2048));
        // An unknown window leaves assembly unbounded, as before.
        assert_eq!(assembly_budget(None, 2048), None);

        // End to end on the budget the pipeline would actually compute: more than one chunk
        // survives the small-window case, which is the regression (a zero budget kept exactly
        // one). Asserted as a count floor rather than an exact list so prompt-cost refactors do
        // not have to touch this test.
        let hits = vec![
            hit(1, 0.9, "x".repeat(200).as_str()),
            hit(2, 0.8, "y".repeat(200).as_str()),
            hit(3, 0.7, "z".repeat(200).as_str()),
        ];
        let mut ocr = HashMap::new();
        let (chunks, _) = assemble_chunks(
            hits,
            &mut ocr,
            &HashMap::new(),
            &HashMap::new(),
            assembly_budget(Some(2048), 2048),
        );
        assert!(
            chunks.len() >= 2,
            "the pre-assembly budget must admit what the provider can pack, got {} chunk(s)",
            chunks.len()
        );
        assert_eq!(chunks[0].frame_id, 1, "best evidence first");
    }

    /// The assembled context as the forwarder sees it: frame → session pairs.
    fn assembled_of(chunks: &[RetrievedChunk]) -> Vec<(i64, Option<i64>)> {
        chunks
            .iter()
            .map(|chunk| (chunk.frame_id, chunk.session.as_ref().map(|s| s.id)))
            .collect()
    }

    #[test]
    fn citations_are_the_cited_frames_sessions_ascending() {
        let context = vec![
            chunk(10, "a", Some(7)),
            chunk(11, "b", Some(3)),
            chunk(12, "c", None),
            chunk(13, "d", Some(7)),
        ];
        let assembled = assembled_of(&context);
        // The provider cited 13, 10 and 12 (in that order) — and one frame it was never
        // given, which must be dropped, never cited.
        let (sessions, frames) = derive_citations(&[13, 10, 12, 99, 13], &assembled);
        assert_eq!(frames, vec![13, 10, 12], "citation order, deduped");
        assert_eq!(
            sessions,
            vec![7],
            "distinct non-NULL session ids, ascending (3 was not cited)"
        );
        // Every cited frame's session is cited: 10 and 13 are in 7, 12 has none.
        for frame in &frames {
            if let Some(session) = assembled
                .iter()
                .find(|(id, _)| id == frame)
                .and_then(|(_, session)| *session)
            {
                assert!(sessions.contains(&session));
            }
        }
    }

    #[test]
    fn citations_of_unowned_frames_are_empty_sessions() {
        let assembled = assembled_of(&[chunk(1, "a", None), chunk(2, "b", None)]);
        let (sessions, frames) = derive_citations(&[1, 2], &assembled);
        assert!(sessions.is_empty());
        assert_eq!(frames, vec![1, 2]);
    }
}
