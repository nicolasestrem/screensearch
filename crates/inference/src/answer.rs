//! [`AnswerProvider`] over the llama.cpp sidecar (`03 §3/§6/§13.5`). Builds a grounded
//! RAG prompt from retrieved chunks, streams the model's reply, and maps it to the
//! typed [`AnswerDelta`] flow: reasoning → `Thinking`, answer text → `Token`, one
//! reviewed source-frame id, then `Done` (or `Error`).
//!
//! Reasoning is surfaced two ways depending on the build: a `reasoning_content` SSE
//! field (handled by the client as [`StreamPiece::Reasoning`]) or inline `<think>…
//! </think>` tags in the content (split here by [`ThinkSplitter`]). Citations are the
//! provided context frames (a reliable grounding set), not parsed from the prose.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::RwLock;

use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::sync::mpsc::{self, Sender};
use traits::{AnswerDelta, AnswerOpts, AnswerProvider, ModelTier, RetrievedChunk, SidecarParams};

use crate::client::{ChatMessage, StreamPiece};
use crate::models::{self, ModelLane, ModelSpec};
use crate::supervisor::ModelSupervisor;

const SYSTEM_PROMPT: &str = "You answer questions about the user's screen history. \
Use ONLY the provided context snippets, each tagged with its source frame id. Ground \
your answer in them and be concise. If the context does not contain the answer, say so \
plainly rather than guessing.";

/// The answer lane provider. Like the vision provider, it owns the active tier and
/// lazily downloads the model on first use.
pub struct AnswerSidecar {
    supervisor: Arc<ModelSupervisor>,
    downloader: Arc<crate::download::ModelDownloader>,
    models_root: PathBuf,
    tier: RwLock<ModelTier>,
    launch: RwLock<SidecarParams>,
    /// Automatic reports pin an installed spec and can never reach the downloader.
    fixed_spec: Option<ModelSpec>,
}

impl AnswerSidecar {
    pub fn new(
        supervisor: Arc<ModelSupervisor>,
        downloader: Arc<crate::download::ModelDownloader>,
        models_root: PathBuf,
        tier: ModelTier,
        params: SidecarParams,
    ) -> Self {
        Self {
            supervisor,
            downloader,
            models_root,
            tier: RwLock::new(tier),
            launch: RwLock::new(params),
            fixed_spec: None,
        }
    }

    /// Updates the active answer tier (next request switches the sidecar model).
    pub fn set_tier(&self, tier: ModelTier) {
        *self.tier.write().expect("answer tier lock") = tier;
    }

    /// Updates launch options for the next request (or the next model restart if a
    /// sidecar is already serving the same spec). A change to any tuning field makes the
    /// next `resolve_spec` differ, so the supervisor relaunches.
    pub fn set_launch_options(&self, params: SidecarParams) {
        *self.launch.write().expect("answer launch lock") = params;
    }

    /// Snapshot an installed model for automatic reports. Subsequent tier changes
    /// cannot redirect this provider; missing/deleted files fail locally without a
    /// download. The normal supervisor still owns loading, leases and shutdown.
    pub fn installed_provider_for(
        &self,
        tier: ModelTier,
        params: SidecarParams,
    ) -> Result<Arc<dyn AnswerProvider>> {
        let spec = models::resolve_spec(&self.models_root, ModelLane::Answer, tier, params.clone())
            .context("selected answer model is not installed")?;
        Ok(Arc::new(Self {
            supervisor: self.supervisor.clone(),
            downloader: self.downloader.clone(),
            models_root: self.models_root.clone(),
            tier: RwLock::new(tier),
            launch: RwLock::new(params),
            fixed_spec: Some(spec),
        }))
    }

    async fn ensure_spec(&self) -> Result<ModelSpec> {
        if let Some(spec) = &self.fixed_spec {
            return Ok(spec.clone());
        }
        let tier = *self.tier.read().expect("answer tier lock");
        let params = self.launch.read().expect("answer launch lock").clone();
        if let Some(spec) =
            models::resolve_spec(&self.models_root, ModelLane::Answer, tier, params.clone())
        {
            return Ok(spec);
        }
        self.downloader
            .ensure(ModelLane::Answer, tier)
            .await
            .context("download answer model")?;
        models::resolve_spec(&self.models_root, ModelLane::Answer, tier, params)
            .context("answer model files missing after download")
    }

    /// Eagerly loads the current answer model into the sidecar (the manual "Load" control)
    /// so the next Ask is instant. Downloads on first use, then keeps it resident until the
    /// idle-TTL or a manual unload reclaims it.
    pub async fn preload(&self) -> Result<()> {
        let spec = self.ensure_spec().await?;
        self.supervisor.preload(spec).await
    }

    /// Runs the request to completion, sending a terminal delta either way. Setup
    /// failures surface as an `AnswerDelta::Error` rather than an `Err`, so the UI
    /// always receives a terminal event.
    async fn run(
        &self,
        query: &str,
        context: &[RetrievedChunk],
        opts: AnswerOpts,
        tx: &Sender<AnswerDelta>,
    ) -> Result<()> {
        let spec = self.ensure_spec().await?;
        let ctx_size = spec.ctx_size;
        let lease = self.supervisor.acquire(spec).await?;
        // Cap the reply budget so a large requested `max_tokens` can't consume the whole
        // context window and leave nothing for grounding snippets when `ctx_size` is small
        // (the UI sends a fixed 2048, but Settings allows ctx down to 512). (Codex review.)
        let max_tokens = effective_reply_budget(opts.max_tokens, ctx_size);
        let (messages, cited) = build_messages(query, context, ctx_size, max_tokens);

        // Bridge the client's low-level SSE pieces onto the typed AnswerDelta stream.
        let (ptx, prx) = mpsc::channel::<StreamPiece>(64);
        let client = lease.client().clone();
        let stream_task =
            tokio::spawn(async move { client.stream(messages, max_tokens, &ptx).await });

        // Drain the stream until it completes — or until the consumer goes away (a UI
        // cancel, or a dropped `/v1/ask` SSE), in which case the sidecar is aborted so it
        // stops generating instead of streaming into a closed socket (`03 §7c`).
        match pump_deltas(stream_task, prx, tx, opts).await {
            // The consumer closed the channel; nothing reads further deltas.
            PumpOutcome::Cancelled => Ok(()),
            PumpOutcome::Failed(e) => {
                let _ = tx
                    .send(AnswerDelta::Error {
                        message: e.to_string(),
                    })
                    .await;
                Ok(())
            }
            PumpOutcome::Completed => {
                // Source-frame provenance: one per included context frame (already
                // deduped, in order). Only frames that fit the context budget are
                // emitted, so each id corresponds to text the model actually saw; the UI
                // labels these as checked context rather than proof of a positive claim.
                for frame_id in &cited {
                    let _ = tx
                        .send(AnswerDelta::Citation {
                            frame_id: *frame_id,
                        })
                        .await;
                }
                let _ = tx.send(AnswerDelta::Done).await;
                Ok(())
            }
        }
    }
}

/// Aborts the wrapped task on drop unless disarmed via [`Self::into_inner`]. Wrapping
/// the sidecar stream task in this guard is what makes cancellation actually free the
/// sidecar: whenever [`pump_deltas`] returns early because the downstream `AnswerDelta`
/// consumer went away, the stream task is aborted, which drops reqwest's response body,
/// closes the HTTP connection to llama.cpp, and stops generation. Without it the
/// detached stream task keeps draining the sidecar to `[DONE]` even though nothing reads
/// the result — the leak `03 §7c` calls out (and the reason the old `cancel_ask`, which
/// only aborted the outer task, never stopped generation).
struct AbortOnDrop<T>(Option<tokio::task::JoinHandle<T>>);

impl<T> AbortOnDrop<T> {
    fn new(handle: tokio::task::JoinHandle<T>) -> Self {
        Self(Some(handle))
    }

    /// Disarms the guard and returns the handle — the normal-completion path awaits it
    /// for the stream's result instead of aborting it.
    fn into_inner(mut self) -> tokio::task::JoinHandle<T> {
        self.0.take().expect("AbortOnDrop handle taken twice")
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        if let Some(handle) = &self.0 {
            handle.abort();
        }
    }
}

/// Outcome of draining the sidecar stream in [`pump_deltas`].
enum PumpOutcome {
    /// The sidecar reached `Done`; the caller emits citations + a terminal `Done`.
    Completed,
    /// The sidecar stream task failed; the caller surfaces the error as a terminal delta.
    Failed(anyhow::Error),
    /// The downstream consumer closed the channel (UI cancel / SSE disconnect); the
    /// sidecar stream task was aborted and no further deltas should be sent.
    Cancelled,
}

/// Drains the client's low-level SSE pieces (`prx`) into typed `AnswerDelta` sends on
/// `tx`, splitting inline `<think>` reasoning as it goes, until the stream completes —
/// or until `tx`'s receiver is dropped, in which case the sidecar `stream_task` is
/// aborted (via the [`AbortOnDrop`] guard) so it stops generating. This is the single
/// place the cancel-on-disconnect contract (`03 §7c`) is enforced; it also fixes the
/// pre-existing `cancel_ask` leak, since aborting the outer answer task drops `tx`,
/// which closes the channel observed here.
async fn pump_deltas(
    stream_task: tokio::task::JoinHandle<Result<()>>,
    mut prx: mpsc::Receiver<StreamPiece>,
    tx: &Sender<AnswerDelta>,
    opts: AnswerOpts,
) -> PumpOutcome {
    let stream_task = AbortOnDrop::new(stream_task);
    let mut splitter = ThinkSplitter::default();
    loop {
        tokio::select! {
            piece = prx.recv() => match piece {
                Some(StreamPiece::Reasoning(text)) => {
                    // A closed channel means the consumer hung up mid-piece: stop pumping
                    // now (the guard aborts the sidecar) rather than draining the backlog.
                    if opts.thinking && tx.send(AnswerDelta::Thinking { text }).await.is_err() {
                        return PumpOutcome::Cancelled;
                    }
                }
                Some(StreamPiece::Content(text)) => {
                    for (is_thinking, chunk) in splitter.push(&text) {
                        if !emit_segment(tx, is_thinking, chunk, opts.thinking).await {
                            return PumpOutcome::Cancelled;
                        }
                    }
                }
                Some(StreamPiece::Done) | None => break,
            },
            // The receiver was dropped: the consumer hung up. Stop consuming and let the
            // guard abort the sidecar stream (freeing GPU/CPU) as this fn returns.
            _ = tx.closed() => {
                tracing::info!("answer stream cancelled by consumer; aborting sidecar generation");
                return PumpOutcome::Cancelled;
            }
        }
    }
    if let Some((is_thinking, rest)) = splitter.flush() {
        emit_segment(tx, is_thinking, rest, opts.thinking).await;
    }
    match stream_task.into_inner().await {
        Ok(Ok(())) => PumpOutcome::Completed,
        Ok(Err(e)) => PumpOutcome::Failed(e),
        Err(e) => PumpOutcome::Failed(anyhow::anyhow!("answer stream task panicked: {e}")),
    }
}

#[async_trait]
impl AnswerProvider for AnswerSidecar {
    async fn answer(
        &self,
        query: &str,
        context: &[RetrievedChunk],
        opts: AnswerOpts,
        tx: Sender<AnswerDelta>,
    ) -> Result<()> {
        if let Err(e) = self.run(query, context, opts, &tx).await {
            // A setup failure (model resolve / sidecar spawn) still gets a terminal
            // delta so the UI never hangs waiting.
            let _ = tx
                .send(AnswerDelta::Error {
                    message: e.to_string(),
                })
                .await;
        }
        Ok(())
    }

    /// One report summarization pass (`03 §8b`): resolves + acquires the answer model
    /// like [`Self::run`], builds bounded report messages with the caller's
    /// `system_prompt`, and collects the streamed reply (thinking dropped). Returns the
    /// summary text + the frame ids actually read. Setup/stream errors propagate as
    /// `Err` (the orchestrator/command decides how to surface them), unlike `answer`'s
    /// terminal-delta swallow.
    async fn summarize(
        &self,
        system_prompt: &str,
        instruction: &str,
        context: &[RetrievedChunk],
        opts: AnswerOpts,
    ) -> Result<(String, Vec<i64>)> {
        let spec = self.ensure_spec().await?;
        let ctx_size = spec.ctx_size;
        let lease = self.supervisor.acquire(spec).await?;
        let max_tokens = effective_reply_budget(opts.max_tokens, ctx_size);
        let (messages, cited) =
            build_summary_messages(system_prompt, instruction, context, ctx_size, max_tokens);
        let client = lease.client().clone();
        let text = collect_stream(&client, messages, max_tokens).await?;
        Ok((text, cited))
    }

    /// The active answer model's GGUF filename for the report footer — resolved
    /// without downloading (`None` if the files aren't present yet).
    async fn answer_model_label(&self) -> Option<String> {
        if let Some(spec) = &self.fixed_spec {
            return Some(report_model_label(&spec.gguf_path));
        }
        let tier = *self.tier.read().expect("answer tier lock");
        let params = self.launch.read().expect("answer launch lock").clone();
        models::resolve_spec(&self.models_root, ModelLane::Answer, tier, params)
            .map(|s| report_model_label(&s.gguf_path))
    }

    /// The answer-lane context window the next `summarize` will run with, resolved from
    /// the launch options exactly as [`models::resolve_spec`] would (an explicit
    /// `ctx_size`, or the per-lane auto default when `0`). Lets the report planner budget
    /// against the user's real `sidecar.ctx_size` instead of assuming the default.
    fn answer_context_budget(&self) -> Option<u32> {
        let params = self.launch.read().expect("answer launch lock");
        Some(if params.ctx_size == 0 {
            models::default_ctx_for(ModelLane::Answer)
        } else {
            params.ctx_size
        })
    }
}

/// Emits one segment. Returns `false` iff the send failed because the consumer dropped the
/// receiver — the caller stops pumping. A skipped (empty/suppressed) segment returns `true`.
async fn emit_segment(
    tx: &Sender<AnswerDelta>,
    is_thinking: bool,
    text: String,
    thinking_on: bool,
) -> bool {
    if text.is_empty() {
        return true;
    }
    let delta = if is_thinking {
        if !thinking_on {
            return true; // thinking suppressed by the request
        }
        AnswerDelta::Thinking { text }
    } else {
        AnswerDelta::Token { text }
    };
    tx.send(delta).await.is_ok()
}

/// Chat-template + role-tag overhead reserved on top of the system prompt and question,
/// so the assembled prompt leaves headroom for llama.cpp's own template tokens.
const TEMPLATE_OVERHEAD_TOKENS: usize = 96;
/// Per-snippet framing cost (`[frame <id>] ` + newline), in estimated tokens.
const ID_FRAMING_TOKENS: usize = 6;

/// Conservative UTF-8 **bytes**-per-token lower bound used to estimate prompt length
/// without a real tokenizer. A *chars*-based ratio under-counts dense scripts (a CJK
/// character is ~3 bytes yet ~1 token for well-merging tokenizers, and up to ~1.5 tokens
/// for Mistral-family ones like the default Ministral answer model) and would re-trigger
/// the `exceed_context_size_error` this budgeting prevents. At 2 bytes/token the estimate
/// stays an *upper* bound on tokens for both scripts — English (~4 bytes/token) is
/// over-reserved (safe, with ample context still admitted) and worst-case CJK is covered.
/// (Gemini/Claude/Codex review, PR #26.)
const BYTES_PER_TOKEN: usize = 2;

/// The reply token budget actually used. Caps a large requested `max_tokens` to half the
/// context window so it can never reserve the *entire* window and force `build_messages` to
/// drop all grounding snippets (the symptom: every Ask answers "(no relevant snippets
/// found)"). For the normal 4K/8K windows the UI's 2048 is well under half and passes
/// through unchanged; only a small `ctx_size` (Settings allows down to 512) shrinks it.
fn effective_reply_budget(requested: u32, ctx_size: u32) -> u32 {
    requested.min((ctx_size / 2).max(1))
}

/// Rough token estimate. Deliberately over-counts (see [`BYTES_PER_TOKEN`]) so the
/// assembled prompt stays under the model's context window.
fn estimate_tokens(text: &str) -> usize {
    text.len() / BYTES_PER_TOKEN + 1
}

/// Truncates `text` to roughly `budget_tokens` worth of UTF-8 bytes, snapped down to a
/// char boundary (so multibyte characters are never split). Mirrors [`estimate_tokens`].
fn truncate_to_tokens(text: &str, budget_tokens: usize) -> String {
    let mut max_bytes = budget_tokens
        .saturating_mul(BYTES_PER_TOKEN)
        .min(text.len());
    while max_bytes > 0 && !text.is_char_boundary(max_bytes) {
        max_bytes -= 1;
    }
    text[..max_bytes].to_string()
}

/// Packs context chunks best-first into `intro`, spending at most `budget_tokens`. Each
/// chunk costs its estimated tokens plus [`ID_FRAMING_TOKENS`]; chunks are appended as
/// `[frame <id>] <text>` until the budget is spent, the rest dropped. If the most relevant
/// chunk alone exceeds the budget it is truncated (grounding on a head beats dropping
/// everything). When nothing fit, a `(no relevant snippets found)` line is appended.
/// Returns the assembled user text and the frame ids actually included — the shared core
/// of [`build_messages`] (Ask) and [`build_summary_messages`] (reports), so citations
/// always cover context the model really saw and the budgeting is bounded once.
fn pack_context(
    intro: &str,
    context: &[&RetrievedChunk],
    budget_tokens: usize,
) -> (String, Vec<i64>) {
    let mut budget = budget_tokens;
    let mut user = String::from(intro);
    let mut included: Vec<i64> = Vec::new();
    for chunk in context {
        if budget == 0 {
            break;
        }
        let text = chunk.text.trim();
        if text.is_empty() {
            continue;
        }
        let cost = estimate_tokens(text) + ID_FRAMING_TOKENS;
        let snippet = if cost <= budget {
            budget -= cost;
            text.to_string()
        } else if included.is_empty() {
            // The most relevant chunk alone exceeds the budget: ground on a truncated head
            // rather than dropping all context, then stop.
            let s = truncate_to_tokens(text, budget.saturating_sub(ID_FRAMING_TOKENS));
            budget = 0;
            s
        } else {
            break;
        };
        if snippet.is_empty() {
            break;
        }
        user.push_str(&format!("[frame {}] {}\n", chunk.frame_id, snippet));
        if !included.contains(&chunk.frame_id) {
            included.push(chunk.frame_id);
        }
    }
    if included.is_empty() {
        user.push_str("(no relevant snippets found)\n");
    }
    (user, included)
}

/// Builds the chat messages: a grounding system prompt + a user message listing the
/// context snippets (tagged with their frame ids) and the question — **bounded** to the
/// model's context window via [`pack_context`] (the reserve is `ctx_size` minus the reply
/// `max_tokens`, the system prompt, the question, and template overhead). Without this the
/// prompt could exceed `n_ctx` and llama-server returns a 400 `exceed_context_size_error`
/// (verified). Returns the messages plus the frame ids actually included, so citations only
/// cover context the model really saw.
fn build_messages(
    query: &str,
    context: &[RetrievedChunk],
    ctx_size: u32,
    max_tokens: u32,
) -> (Vec<ChatMessage>, Vec<i64>) {
    let reserve = max_tokens as usize
        + estimate_tokens(SYSTEM_PROMPT)
        + estimate_tokens(query)
        + TEMPLATE_OVERHEAD_TOKENS;
    let budget = (ctx_size as usize).saturating_sub(reserve);
    let refs: Vec<&RetrievedChunk> = context.iter().collect();
    let (mut user, included) =
        pack_context("Context snippets from my screen history:\n", &refs, budget);
    user.push_str(&format!("\nQuestion: {query}"));
    (
        vec![
            ChatMessage::text("system", SYSTEM_PROMPT),
            ChatMessage::text("user", user),
        ],
        included,
    )
}

/// Builds the chat messages for one report summarization pass (`03 §8b`): the
/// caller-supplied `system_prompt` (map / reduce / final) + a user message of the
/// content snippets (bounded via [`pack_context`]) followed by the `instruction` (e.g.
/// the period label or the user's steering prompt). Returns the frame ids actually
/// included so the orchestrator can carry citations through the map → reduce tree.
fn build_summary_messages(
    system_prompt: &str,
    instruction: &str,
    context: &[RetrievedChunk],
    ctx_size: u32,
    max_tokens: u32,
) -> (Vec<ChatMessage>, Vec<i64>) {
    let reserve = max_tokens as usize
        + estimate_tokens(system_prompt)
        + estimate_tokens(instruction)
        + TEMPLATE_OVERHEAD_TOKENS;
    let budget = (ctx_size as usize).saturating_sub(reserve);
    let refs: Vec<&RetrievedChunk> = context.iter().collect();
    let (mut user, included) = pack_context("Content from my screen history:\n", &refs, budget);
    if !instruction.trim().is_empty() {
        user.push_str(&format!("\n{instruction}"));
    }
    (
        vec![
            ChatMessage::text("system", system_prompt),
            ChatMessage::text("user", user),
        ],
        included,
    )
}

/// Collects a streamed completion into a single `String`, dropping the model's
/// `<think>` reasoning (reports have no streaming UI to show it). Mirrors
/// [`AnswerSidecar::run`]'s stream plumbing minus the [`AnswerDelta`] bridge: the
/// per-chunk idle timeout in [`SidecarClient::stream`] keeps a slow-but-progressing
/// map-reduce pass from spuriously timing out.
async fn collect_stream(
    client: &crate::client::SidecarClient,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
) -> Result<String> {
    let (ptx, mut prx) = mpsc::channel::<StreamPiece>(64);
    let c = client.clone();
    let task = tokio::spawn(async move { c.stream(messages, max_tokens, &ptx).await });
    let mut splitter = ThinkSplitter::default();
    let mut out = String::new();
    while let Some(piece) = prx.recv().await {
        match piece {
            StreamPiece::Reasoning(_) => {}
            StreamPiece::Content(text) => {
                for (is_thinking, chunk) in splitter.push(&text) {
                    if !is_thinking {
                        out.push_str(&chunk);
                    }
                }
            }
            StreamPiece::Done => break,
        }
    }
    if let Some((is_thinking, rest)) = splitter.flush() {
        if !is_thinking {
            out.push_str(&rest);
        }
    }
    task.await
        .unwrap_or_else(|e| Err(anyhow::anyhow!("report stream task panicked: {e}")))?;
    Ok(out)
}

/// The report footer's model provenance: the answer-lane GGUF filename
/// (`ModelSpec` has no display-name field). Falls back to `"answer-model"`.
fn report_model_label(gguf_path: &std::path::Path) -> String {
    gguf_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("answer-model")
        .to_string()
}

/// Splits a streamed content sequence into thinking vs. answer segments by tracking
/// `<think>…</think>` tags across chunk boundaries. Text that could be the *start* of a
/// tag is held back until the next chunk so a tag split across SSE frames isn't missed.
#[derive(Default)]
pub struct ThinkSplitter {
    in_think: bool,
    buf: String,
}

impl ThinkSplitter {
    const OPEN: &'static str = "<think>";
    const CLOSE: &'static str = "</think>";

    /// Feeds more content; returns `(is_thinking, text)` segments ready to emit.
    pub fn push(&mut self, text: &str) -> Vec<(bool, String)> {
        self.buf.push_str(text);
        let mut out = Vec::new();
        loop {
            let marker = if self.in_think {
                Self::CLOSE
            } else {
                Self::OPEN
            };
            if let Some(idx) = self.buf.find(marker) {
                let before: String = self.buf[..idx].to_string();
                if !before.is_empty() {
                    out.push((self.in_think, before));
                }
                self.buf.drain(..idx + marker.len());
                self.in_think = !self.in_think;
            } else {
                // No full marker. Emit all but a trailing tail that might begin one.
                let keep = partial_marker_suffix(&self.buf, marker);
                let emit_len = self.buf.len() - keep;
                if emit_len > 0 {
                    let chunk: String = self.buf.drain(..emit_len).collect();
                    out.push((self.in_think, chunk));
                }
                break;
            }
        }
        out
    }

    /// Emits any buffered remainder at end of stream.
    pub fn flush(&mut self) -> Option<(bool, String)> {
        if self.buf.is_empty() {
            None
        } else {
            Some((self.in_think, std::mem::take(&mut self.buf)))
        }
    }
}

/// Length of the longest suffix of `buf` that is a (proper) prefix of `marker` — the
/// tail to hold back in case the marker is split across chunks.
fn partial_marker_suffix(buf: &str, marker: &str) -> usize {
    let max = marker.len().saturating_sub(1).min(buf.len());
    for k in (1..=max).rev() {
        // `marker` is ASCII, so byte-prefix slicing is valid; guard the buf boundary.
        if buf.is_char_boundary(buf.len() - k) && buf[buf.len() - k..] == marker[..k] {
            return k;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(splitter: &mut ThinkSplitter, parts: &[&str]) -> Vec<(bool, String)> {
        let mut out = Vec::new();
        for p in parts {
            out.extend(splitter.push(p));
        }
        out.extend(splitter.flush());
        out
    }

    #[test]
    fn splits_inline_think_tags() {
        let mut s = ThinkSplitter::default();
        let segs = collect(&mut s, &["<think>reasoning here</think>The answer."]);
        assert_eq!(
            segs,
            vec![
                (true, "reasoning here".to_string()),
                (false, "The answer.".to_string()),
            ]
        );
    }

    #[test]
    fn handles_tag_split_across_chunks() {
        let mut s = ThinkSplitter::default();
        // "<think>" arrives split as "<thi" + "nk>", and "</think>" as "</thin" + "k>".
        let segs = collect(&mut s, &["<thi", "nk>step</thin", "k>done"]);
        assert_eq!(
            segs,
            vec![(true, "step".to_string()), (false, "done".to_string())]
        );
    }

    #[test]
    fn plain_content_is_all_tokens() {
        let mut s = ThinkSplitter::default();
        let segs = collect(&mut s, &["Hello ", "world"]);
        assert_eq!(
            segs,
            vec![(false, "Hello ".to_string()), (false, "world".to_string()),]
        );
    }

    fn chunk(frame_id: i64, text: &str) -> RetrievedChunk {
        RetrievedChunk {
            frame_id,
            text: text.to_string(),
            score: 1.0,
            captured_at: 0,
        }
    }

    #[test]
    fn builds_grounded_prompt_with_frame_tags() {
        let ctx = vec![chunk(7, "login page"), chunk(9, "dashboard")];
        let (msgs, cited) = build_messages("what did I see?", &ctx, 8192, 512);
        assert_eq!(msgs.len(), 2);
        // The user message must reference both frames for grounding.
        let user = serde_json::to_string(&msgs[1]).unwrap();
        assert!(user.contains("[frame 7]"));
        assert!(user.contains("[frame 9]"));
        assert!(user.contains("what did I see?"));
        assert_eq!(cited, vec![7, 9], "both frames fit the budget → both cited");
    }

    #[test]
    fn drops_chunks_that_exceed_the_context_budget() {
        // Many large chunks into a tiny ctx: only a prefix fits, and only those are cited —
        // this is the fix for the verified 400 `exceed_context_size_error`.
        let big = "lorem ipsum dolor sit amet ".repeat(50); // ~1350 chars ≈ 450 tokens
        let ctx: Vec<RetrievedChunk> = (0..20).map(|i| chunk(i, &big)).collect();
        let (msgs, cited) = build_messages("q", &ctx, 1024, 256);
        assert_eq!(msgs.len(), 2);
        assert!(!cited.is_empty(), "at least the top chunk is grounded");
        assert!(
            cited.len() < ctx.len(),
            "the budget must drop the chunks that don't fit (cited {})",
            cited.len()
        );
        // The included frames are exactly the leading ones (best-first order preserved).
        assert_eq!(cited, (0..cited.len() as i64).collect::<Vec<_>>());
    }

    #[test]
    fn truncates_an_oversized_top_chunk_instead_of_dropping_everything() {
        let huge = "x".repeat(100_000);
        let (msgs, cited) = build_messages("q", &[chunk(3, &huge)], 2048, 256);
        assert_eq!(
            cited,
            vec![3],
            "the sole chunk is still grounded (truncated)"
        );
        let user = serde_json::to_string(&msgs[1]).unwrap();
        assert!(
            user.len() < huge.len(),
            "the oversized chunk must be truncated"
        );
    }

    #[test]
    fn reply_budget_leaves_room_for_grounding_in_a_small_context() {
        // Ample window: the UI's 2048 is under half, so it passes through unchanged.
        assert_eq!(effective_reply_budget(2048, 8192), 2048);
        // Small window: capped to half so the prompt/context still has room.
        assert_eq!(effective_reply_budget(2048, 2048), 1024);
        assert!(effective_reply_budget(2048, 512) <= 256);
        // With the cap, a small ctx still grounds instead of dropping every chunk.
        let budget = effective_reply_budget(2048, 2048);
        let (_, cited) = build_messages("q", &[chunk(1, "hello world")], 2048, budget);
        assert_eq!(cited, vec![1], "grounding survives a small context window");
    }

    #[test]
    fn estimate_tokens_does_not_undercount_cjk() {
        // 40 CJK chars = 120 UTF-8 bytes, tokenizing ~1 token/char. A chars/3 ratio would
        // estimate ~14 and overflow the context; the byte ratio must stay >= the char count.
        let cjk = "你好世界".repeat(10);
        assert_eq!(cjk.chars().count(), 40);
        assert!(
            estimate_tokens(&cjk) >= cjk.chars().count(),
            "CJK estimate {} must not undercount {} chars",
            estimate_tokens(&cjk),
            cjk.chars().count()
        );
    }

    #[test]
    fn report_summary_messages_use_the_given_system_prompt_and_tag_frames() {
        let ctx = vec![
            chunk(11, "edited the report draft"),
            chunk(13, "ran the build"),
        ];
        let (msgs, cited) = build_summary_messages(
            "SUMMARIZE PROMPT",
            "Summarize Tuesday (Jun 24).",
            &ctx,
            8192,
            512,
        );
        assert_eq!(msgs.len(), 2);
        let system = serde_json::to_string(&msgs[0]).unwrap();
        assert!(
            system.contains("SUMMARIZE PROMPT"),
            "report system prompt, not Ask's"
        );
        let user = serde_json::to_string(&msgs[1]).unwrap();
        assert!(user.contains("[frame 11]") && user.contains("[frame 13]"));
        assert!(
            user.contains("Summarize Tuesday (Jun 24)."),
            "instruction appended"
        );
        assert_eq!(cited, vec![11, 13], "both frames fit → both cited");
    }

    #[test]
    fn report_summary_drops_overflow_and_cites_only_what_fit() {
        // A tiny ctx: only a prefix of frames fits, and only those are cited.
        let big = "lorem ipsum dolor sit amet ".repeat(40);
        let ctx: Vec<RetrievedChunk> = (0..20).map(|i| chunk(i, &big)).collect();
        let (_msgs, cited) = build_summary_messages("S", "label", &ctx, 1024, 256);
        assert!(!cited.is_empty() && cited.len() < ctx.len());
        assert_eq!(cited, (0..cited.len() as i64).collect::<Vec<_>>());
    }

    #[test]
    fn report_model_label_extracts_the_gguf_filename() {
        use std::path::Path;
        assert_eq!(
            report_model_label(Path::new("/models/answer/Ministral-3-3B-Q4_K_M.gguf")),
            "Ministral-3-3B-Q4_K_M.gguf"
        );
        assert_eq!(report_model_label(Path::new("")), "answer-model");
    }

    #[test]
    fn truncate_to_tokens_never_splits_a_multibyte_char() {
        // 3-byte chars; a byte budget that lands mid-character must snap back to a boundary
        // (a naive byte slice would panic).
        let cjk = "世".repeat(100);
        let out = truncate_to_tokens(&cjk, 10);
        assert!(cjk.starts_with(&out));
        assert!(!out.is_empty() && out.len() <= 10 * BYTES_PER_TOKEN);
        assert!(
            out.chars().all(|c| c == '世'),
            "no split / replacement chars"
        );
    }

    /// The cancel-on-disconnect contract (`03 §7c`): when the `AnswerDelta` consumer
    /// drops its receiver mid-stream, `pump_deltas` returns `Cancelled` promptly and the
    /// still-generating sidecar stream task is aborted (not left running to `[DONE]`).
    /// This is the mechanism that both a dropped `/v1/ask` SSE and `cancel_ask` rely on.
    #[tokio::test]
    async fn pump_deltas_cancels_and_aborts_sidecar_on_consumer_drop() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::time::Duration;

        // Records when the sidecar stream task's future is dropped — i.e. when the
        // AbortOnDrop guard aborts it.
        struct DropFlag(Arc<AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }

        let aborted = Arc::new(AtomicBool::new(false));
        let aborted_in_task = aborted.clone();

        let (ptx, prx) = mpsc::channel::<StreamPiece>(4);
        // A never-ending sidecar stream: keeps emitting content until its channel is gone
        // or it is aborted. Stands in for llama.cpp generating a long answer.
        let stream_task = tokio::spawn(async move {
            let _flag = DropFlag(aborted_in_task);
            loop {
                if ptx
                    .send(StreamPiece::Content("token ".to_string()))
                    .await
                    .is_err()
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            Ok(())
        });

        let (tx, rx) = mpsc::channel::<AnswerDelta>(1);
        let opts = AnswerOpts {
            thinking: false,
            max_tokens: 128,
        };
        let pump = tokio::spawn(async move { pump_deltas(stream_task, prx, &tx, opts).await });

        // Let a little output flow, then simulate the consumer disconnecting.
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(rx);

        let outcome = tokio::time::timeout(Duration::from_secs(2), pump)
            .await
            .expect("pump_deltas returns promptly after the consumer drops")
            .expect("pump task joins");
        assert!(matches!(outcome, PumpOutcome::Cancelled));

        // The sidecar stream task was aborted, so llama.cpp stops generating.
        for _ in 0..40 {
            if aborted.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(
            aborted.load(Ordering::SeqCst),
            "sidecar stream task aborted on consumer disconnect (no generation leak)"
        );
    }

    /// The normal path is unchanged: a stream that reaches `Done` yields `Completed`, so
    /// the caller still emits citations + the terminal `Done`.
    #[tokio::test]
    async fn pump_deltas_completes_when_stream_finishes() {
        let (ptx, prx) = mpsc::channel::<StreamPiece>(8);
        let stream_task = tokio::spawn(async move {
            ptx.send(StreamPiece::Content("hello".to_string()))
                .await
                .unwrap();
            ptx.send(StreamPiece::Done).await.unwrap();
            Ok(())
        });
        let (tx, mut rx) = mpsc::channel::<AnswerDelta>(16);
        let opts = AnswerOpts {
            thinking: false,
            max_tokens: 128,
        };
        let outcome = pump_deltas(stream_task, prx, &tx, opts).await;
        assert!(matches!(outcome, PumpOutcome::Completed));
        // The content token was forwarded before completion.
        let mut got_token = false;
        while let Ok(delta) = rx.try_recv() {
            if matches!(delta, AnswerDelta::Token { .. }) {
                got_token = true;
            }
        }
        assert!(got_token, "content forwarded as a Token delta");
    }
}
