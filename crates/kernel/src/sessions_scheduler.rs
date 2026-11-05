//! Incremental session reconciliation + one-shot resumable historical backfill (`03 §7e`).
//! Every error is logged and swallowed by the scheduler loop: capture/OCR/storage never wait on
//! sessions, so the D10 degradation mode is always "no sessions", never lost frames.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use traits::{
    NewSessionArtifact, SegmentationParams, SegmenterFrame, Session, SessionArtifactKind,
    SessionDraft, SessionFilter, SessionKind, SessionSegmenter, Settings, Store,
};

use crate::graph_reconcile::{mark_dirty, DirtySessions};
use crate::{settings, worker_pool, KernelEvent};

const CADENCE: Duration = Duration::from_secs(60);
const BACKFILL_CHUNK_MS: i64 = 6 * 60 * 60 * 1000;
/// The internal setting that stores the historical sessions checkpoint. `pub(crate)` so the
/// work-graph gate, its tests and the graph scheduler tests all reference one spelling.
pub(crate) const BACKFILL_MARKER: &str = "sessions.backfill_done_until";

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

pub(crate) fn spawn(
    store: Arc<dyn Store>,
    segmenter: Arc<dyn SessionSegmenter>,
    throttle_level: Arc<AtomicU8>,
    events: broadcast::Sender<KernelEvent>,
    dirty: DirtySessions,
) -> SchedulerHandle {
    let (stop_tx, stop_rx) = watch::channel(false);
    let join = tokio::spawn(scheduler_loop(
        store,
        segmenter,
        throttle_level,
        events,
        dirty,
        stop_rx,
    ));
    SchedulerHandle {
        stop: stop_tx,
        join: Some(join),
    }
}

async fn scheduler_loop(
    store: Arc<dyn Store>,
    segmenter: Arc<dyn SessionSegmenter>,
    throttle_level: Arc<AtomicU8>,
    events: broadcast::Sender<KernelEvent>,
    dirty: DirtySessions,
    mut stop: watch::Receiver<bool>,
) {
    loop {
        let now = worker_pool::now_ms();
        let current = settings::load_settings(store.as_ref()).await;
        run_scheduler_pass(
            store.as_ref(),
            segmenter.as_ref(),
            now,
            &current,
            throttle_level.load(Ordering::Relaxed) != 0,
            &events,
            &dirty,
        )
        .await;
        if wait_or_stop(&mut stop, CADENCE).await {
            break;
        }
    }
}

/// `true` when the one-shot historical sessions pass has reached its fixed target
/// (`sessions.backfill_done_until`: `cursor_ms >= target_ms`). The work graph must not extract
/// frames before their sessions exist (P5 review finding), so the graph scheduler waits for
/// this marker. A missing or unparsable marker is incomplete: the sessions scheduler writes
/// it on its first pass.
pub(crate) async fn history_backfill_complete(store: &dyn Store) -> bool {
    let value = match store.get_setting(BACKFILL_MARKER).await {
        Ok(Some(value)) => value,
        Ok(None) => return false,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "sessions: reading the historical backfill marker failed; treated as incomplete"
            );
            return false;
        }
    };
    let checkpoint: BackfillCheckpoint = match serde_json::from_str(&value) {
        Ok(checkpoint) => checkpoint,
        Err(error) => {
            tracing::warn!(
                error = %error,
                "sessions: the historical backfill marker is not a checkpoint; treated as incomplete"
            );
            return false;
        }
    };
    checkpoint.cursor_ms >= checkpoint.target_ms
}

/// Runs one scheduler iteration and broadcasts only after at least one pass reports a semantic
/// session, ownership, freeze, or artifact change. Individual failures retain the scheduler's
/// additive degradation contract: the other passes still run, and a failed multi-write pass
/// conservatively invalidates readers because an earlier write may already have committed.
pub(crate) async fn run_scheduler_pass(
    store: &dyn Store,
    segmenter: &dyn SessionSegmenter,
    now: i64,
    current: &Settings,
    historical_throttled: bool,
    events: &broadcast::Sender<KernelEvent>,
    dirty: &DirtySessions,
) {
    let mut changed = false;
    match run_incremental_pass(store, segmenter, now, current, dirty).await {
        Ok(pass_changed) => changed |= pass_changed,
        Err(error) => {
            changed = true;
            tracing::warn!(%error, "sessions incremental pass failed; capture remains active")
        }
    }

    let params = params_from_settings(now, current);
    match store
        .freeze_sessions(now.saturating_sub(params.freeze_lookback_ms))
        .await
    {
        Ok(frozen) => changed |= frozen > 0,
        Err(error) => tracing::warn!(%error, "sessions freeze pass failed; capture remains active"),
    }

    if historical_throttled {
        tracing::debug!("sessions historical pass yielded to enrichment throttle");
    } else {
        match run_backfill_chunk(store, segmenter, now, current).await {
            Ok(pass_changed) => changed |= pass_changed,
            Err(error) => {
                changed = true;
                tracing::warn!(%error, "sessions historical pass failed; capture remains active")
            }
        }
    }

    if changed {
        let _ = events.send(KernelEvent::SessionsChanged);
    }
}

fn params_from_settings(now_ms: i64, settings: &Settings) -> SegmentationParams {
    SegmentationParams::shipped(
        now_ms,
        i64::from(settings.sessions_gap_close_secs).saturating_mul(1000),
        i64::from(settings.sessions_min_len_secs).saturating_mul(1000),
    )
}

fn partition(kind: SessionKind, context_key: &str) -> String {
    if kind == SessionKind::Focus {
        "focus".to_string()
    } else {
        context_key.to_string()
    }
}

/// Drops/trims drafts that reach into an already-frozen span of the same identity track.
/// Cross-track overlap remains untouched. The incremental window meets frozen history at its
/// old edge, so the safe retained portion is the tail strictly after the latest overlapping frozen
/// end (session `ended_at` is the timestamp of its last owned frame, not an exclusive bound).
pub(crate) fn trim_drafts_against_frozen(
    drafts: Vec<SessionDraft>,
    frozen: &[Session],
    frames: &[SegmenterFrame],
) -> Vec<SessionDraft> {
    let frame_times: HashMap<i64, i64> = frames
        .iter()
        .map(|frame| (frame.id, frame.captured_at))
        .collect();
    drafts
        .into_iter()
        .filter_map(|mut draft| {
            let own_partition = partition(draft.kind, &draft.context_key);
            let cut = frozen
                .iter()
                .filter(|session| session.frozen)
                .filter(|session| partition(session.kind, &session.context_key) == own_partition)
                .filter_map(|session| {
                    let end = session.ended_at?;
                    (draft.started_at <= end && draft.ended_at >= session.started_at).then_some(end)
                })
                .max();
            if let Some(cut) = cut {
                draft.frame_ids.retain(|id| {
                    frame_times
                        .get(id)
                        .is_some_and(|captured_at| *captured_at > cut)
                });
                if draft.frame_ids.is_empty() {
                    return None;
                }
                let times: Vec<i64> = draft
                    .frame_ids
                    .iter()
                    .filter_map(|id| frame_times.get(id).copied())
                    .collect();
                draft.started_at = *times.iter().min()?;
                draft.ended_at = *times.iter().max()?;
            }
            Some(draft)
        })
        .collect()
}

fn overlap_ms(draft: &SessionDraft, session: &Session, now_ms: i64) -> Option<i64> {
    let session_end = session.ended_at.unwrap_or(now_ms);
    let overlap = draft.ended_at.min(session_end) - draft.started_at.max(session.started_at);
    (overlap > 0).then_some(overlap)
}

/// One-to-one, deterministic draft→row matching by exact context key + maximum time overlap.
pub(crate) fn match_drafts_to_existing(
    drafts: &[SessionDraft],
    existing: &[Session],
    now_ms: i64,
) -> Vec<Option<i64>> {
    let mut used = HashSet::new();
    drafts
        .iter()
        .map(|draft| {
            let best = existing
                .iter()
                .filter(|session| !session.frozen)
                .filter(|session| session.context_key == draft.context_key)
                .filter(|session| !used.contains(&session.id))
                .filter_map(|session| {
                    overlap_ms(draft, session, now_ms).map(|overlap| (session, overlap))
                })
                .max_by(|(left, left_overlap), (right, right_overlap)| {
                    left_overlap
                        .cmp(right_overlap)
                        .then_with(|| {
                            let left_delta = (draft.started_at - left.started_at).abs();
                            let right_delta = (draft.started_at - right.started_at).abs();
                            right_delta.cmp(&left_delta)
                        })
                        .then_with(|| right.id.cmp(&left.id))
                })
                .map(|(session, _)| session.id);
            if let Some(id) = best {
                used.insert(id);
            }
            best
        })
        .collect()
}

pub(crate) async fn run_incremental_pass(
    store: &dyn Store,
    segmenter: &dyn SessionSegmenter,
    now_ms: i64,
    current: &Settings,
    dirty: &DirtySessions,
) -> anyhow::Result<bool> {
    let params = params_from_settings(now_ms, current);
    let existing = store.unfrozen_sessions().await?;
    let mutable_tail = now_ms.saturating_sub(params.freeze_lookback_ms);
    let from_ms = existing
        .iter()
        .map(|session| session.started_at)
        .min()
        .map_or(mutable_tail, |earliest| earliest.min(mutable_tail));
    let frames = store
        .frames_meta_in_range(from_ms, now_ms.saturating_add(1))
        .await?;
    let drafts = segmenter.segment(&frames, &params);
    let frozen = store
        .sessions_in_range(SessionFilter {
            from_ms: Some(from_ms),
            to_ms: Some(now_ms.saturating_add(1)),
            now_ms,
            ..SessionFilter::default()
        })
        .await?
        .into_iter()
        .filter(|session| session.frozen)
        .collect::<Vec<_>>();
    let drafts = trim_drafts_against_frozen(drafts, &frozen, &frames);
    let matches = match_drafts_to_existing(&drafts, &existing, now_ms);

    let mut changed = false;
    let desired_by_existing: HashMap<i64, &SessionDraft> = drafts
        .iter()
        .zip(&matches)
        .filter_map(|(draft, matched)| matched.map(|id| (id, draft)))
        .collect();
    let mut owned_by_existing = HashMap::new();

    // Remove only stale mutable ownership. Keeping already-correct assignments intact makes an
    // identical reconciliation a semantic and physical no-op.
    for session in &existing {
        let ids: Vec<i64> = store
            .session_frames_meta(session.id)
            .await?
            .into_iter()
            .map(|frame| frame.id)
            .collect();
        if let Some(draft) = desired_by_existing.get(&session.id) {
            let desired: HashSet<i64> = draft.frame_ids.iter().copied().collect();
            let stale: Vec<i64> = ids
                .iter()
                .filter(|id| !desired.contains(id))
                .copied()
                .collect();
            if store.assign_frames_session(&stale, None).await? > 0 {
                changed = true;
                // A frame left this session, so its link evidence must be recomputed
                // exactly; a stale link would otherwise keep citing the wrong session.
                mark_dirty(dirty, [session.id]);
            }
        }
        owned_by_existing.insert(session.id, ids.into_iter().collect::<HashSet<_>>());
    }

    let matched_ids: HashSet<i64> = matches.iter().flatten().copied().collect();
    for session in &existing {
        if !matched_ids.contains(&session.id) {
            changed |= store.delete_unfrozen_session(session.id).await?;
        }
    }

    for (draft, matched) in drafts.iter().zip(matches) {
        let session_id = match matched {
            Some(id) => {
                if existing
                    .iter()
                    .find(|session| session.id == id)
                    .is_some_and(|session| !session_matches_draft(session, draft))
                {
                    if !store
                        .update_unfrozen_session(id, draft.as_new_session(false))
                        .await?
                    {
                        continue;
                    }
                    changed = true;
                    // The session's interval or context changed: links derived from it may
                    // have moved.
                    mark_dirty(dirty, [id]);
                }
                id
            }
            None => {
                changed = true;
                let inserted = store.insert_session(draft.as_new_session(false)).await?;
                // A brand new session can own frames that were extracted before it existed
                // (the P5 review finding), so it always needs an initial link rebuild.
                mark_dirty(dirty, [inserted]);
                inserted
            }
        };
        let already_owned = owned_by_existing.get(&session_id);
        let to_assign: Vec<i64> = draft
            .frame_ids
            .iter()
            .filter(|id| !already_owned.is_some_and(|owned| owned.contains(id)))
            .copied()
            .collect();
        if store
            .assign_frames_session(&to_assign, Some(session_id))
            .await?
            > 0
        {
            changed = true;
            // Newly owned frames may have been extracted while assigned elsewhere (or
            // unassigned), so this session's links need a rebuild.
            mark_dirty(dirty, [session_id]);
        }
        // Best-effort exchange extraction for AI sessions: the graph dirty mark above must
        // never replace this artifact refresh (historical pass calls the same helper).
        if draft.kind == SessionKind::Ai {
            changed |= refresh_exchanges(store, segmenter, session_id, draft).await?;
        }
    }
    Ok(changed)
}

fn session_matches_draft(session: &Session, draft: &SessionDraft) -> bool {
    let desired = draft.as_new_session(false);
    session.started_at == desired.started_at
        && session.ended_at == desired.ended_at
        && session.kind == desired.kind
        && session.tool == desired.tool
        && session.host == desired.host
        && session.context_key == desired.context_key
        && session.confidence == desired.confidence
        && !session.frozen
}

async fn refresh_exchanges(
    store: &dyn Store,
    segmenter: &dyn SessionSegmenter,
    session_id: i64,
    draft: &SessionDraft,
) -> anyhow::Result<bool> {
    let contents = store.content_texts_for_frames(&draft.frame_ids).await?;
    let extracted = segmenter.extract_exchanges(draft.tool.as_deref().unwrap_or(""), &contents);
    let artifacts: Vec<NewSessionArtifact> = extracted
        .into_iter()
        .map(|exchange| NewSessionArtifact {
            kind: SessionArtifactKind::Exchange,
            role: Some(exchange.role),
            frame_id: Some(exchange.frame_id),
            content: exchange.content,
        })
        .collect();
    let current: Vec<NewSessionArtifact> = store
        .list_session_artifacts(session_id)
        .await?
        .into_iter()
        .filter(|artifact| artifact.kind == SessionArtifactKind::Exchange)
        .map(|artifact| NewSessionArtifact {
            kind: artifact.kind,
            role: artifact.role,
            frame_id: artifact.frame_id,
            content: artifact.content,
        })
        .collect();
    if current == artifacts {
        return Ok(false);
    }
    store
        .delete_session_artifacts_by_kind(session_id, SessionArtifactKind::Exchange)
        .await?;
    store
        .insert_session_artifacts(session_id, &artifacts)
        .await?;
    Ok(true)
}

/// Returns the timestamp at which a chunk can safely stop: the first frame after a global
/// inactivity gap >= merge_gap, at/after the desired six-hour boundary. If the fixed target
/// itself is safely beyond the final frame, the desired boundary is the cut. `scan_limit_ms` may
/// advance beyond the checkpoint's fixed historical target on later ticks so a closing gap after a
/// target-cut continuous track can eventually be observed.
pub(crate) fn safe_backfill_cut(
    frames: &[SegmenterFrame],
    desired_end_ms: i64,
    merge_gap_ms: i64,
    scan_limit_ms: i64,
) -> Option<i64> {
    if frames.is_empty() {
        // Only the range through `desired_end_ms` is known empty. Advancing all the way to the
        // final target would silently skip frames that have not been scanned yet.
        return Some(desired_end_ms.min(scan_limit_ms));
    }
    for pair in frames.windows(2) {
        if pair[1].captured_at >= desired_end_ms
            && pair[1].captured_at - pair[0].captured_at >= merge_gap_ms
        {
            return Some(pair[1].captured_at.min(scan_limit_ms));
        }
    }
    let last = frames.last()?.captured_at;
    (desired_end_ms.saturating_sub(last) >= merge_gap_ms)
        .then_some(desired_end_ms.min(scan_limit_ms))
}

/// Historical backfill approaches frozen incremental history from the left. When a delayed retry
/// meets an already-frozen row on the same identity track, retain only the prefix strictly before
/// that row. This preserves frozen immutability, avoids duplicate overlapping rows, and keeps every
/// retained draft id assignable.
fn trim_backfill_draft_against_frozen(
    mut draft: SessionDraft,
    frozen: &[Session],
    frame_times: &HashMap<i64, i64>,
) -> Option<SessionDraft> {
    let own_partition = partition(draft.kind, &draft.context_key);
    let cut = frozen
        .iter()
        .filter(|session| session.frozen)
        .filter(|session| partition(session.kind, &session.context_key) == own_partition)
        .filter(|session| {
            let end = session.ended_at.unwrap_or(session.started_at);
            draft.started_at <= end && draft.ended_at >= session.started_at
        })
        .map(|session| session.started_at)
        .min()?;
    draft.frame_ids.retain(|id| {
        frame_times
            .get(id)
            .is_some_and(|captured_at| *captured_at < cut)
    });
    if draft.frame_ids.is_empty() {
        return None;
    }
    let times: Vec<i64> = draft
        .frame_ids
        .iter()
        .filter_map(|id| frame_times.get(id).copied())
        .collect();
    draft.started_at = *times.iter().min()?;
    draft.ended_at = *times.iter().max()?;
    Some(draft)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct BackfillCheckpoint {
    cursor_ms: i64,
    target_ms: i64,
}

async fn load_or_initialize_checkpoint(
    store: &dyn Store,
    target_ms: i64,
) -> anyhow::Result<BackfillCheckpoint> {
    if let Some(value) = store.get_setting(BACKFILL_MARKER).await? {
        return serde_json::from_str(&value).map_err(Into::into);
    }
    let earliest = store
        .sample_frames_in_range(i64::MIN, target_ms, 1)
        .await?
        .first()
        .map(|frame| frame.captured_at)
        .unwrap_or(target_ms);
    let checkpoint = BackfillCheckpoint {
        cursor_ms: earliest,
        target_ms,
    };
    save_checkpoint(store, checkpoint).await?;
    Ok(checkpoint)
}

async fn save_checkpoint(store: &dyn Store, checkpoint: BackfillCheckpoint) -> anyhow::Result<()> {
    store
        .set_setting(BACKFILL_MARKER, &serde_json::to_string(&checkpoint)?)
        .await
}

pub(crate) async fn run_backfill_chunk(
    store: &dyn Store,
    segmenter: &dyn SessionSegmenter,
    now_ms: i64,
    current: &Settings,
) -> anyhow::Result<bool> {
    let params = params_from_settings(now_ms, current);
    let current_frozen_horizon = now_ms.saturating_sub(params.freeze_lookback_ms);
    let mut checkpoint = load_or_initialize_checkpoint(store, current_frozen_horizon).await?;
    if checkpoint.cursor_ms >= checkpoint.target_ms
        || current_frozen_horizon <= checkpoint.cursor_ms
    {
        return Ok(false);
    }

    let desired = checkpoint
        .cursor_ms
        .saturating_add(BACKFILL_CHUNK_MS)
        .min(checkpoint.target_ms);
    let mut scan_end = desired
        .saturating_add(params.merge_gap_ms)
        .min(current_frozen_horizon);
    let frames = loop {
        let frames = store
            .frames_meta_in_range(checkpoint.cursor_ms, scan_end)
            .await?;
        if safe_backfill_cut(
            &frames,
            desired,
            params.merge_gap_ms,
            current_frozen_horizon,
        )
        .is_some()
            || scan_end >= current_frozen_horizon
        {
            break frames;
        }
        scan_end = scan_end
            .saturating_add(BACKFILL_CHUNK_MS)
            .min(current_frozen_horizon);
    };
    let Some(cut) = safe_backfill_cut(
        &frames,
        desired,
        params.merge_gap_ms,
        current_frozen_horizon,
    ) else {
        // The fixed historical target cuts through a still-continuous track. Leave the cursor
        // untouched; the incremental tail owns current work and a later pass can close safely.
        return Ok(false);
    };
    let chunk: Vec<SegmenterFrame> = frames
        .into_iter()
        .filter(|frame| frame.captured_at < cut)
        .collect();
    let mut changed = false;
    if !chunk.is_empty() {
        let frame_times: HashMap<i64, i64> = chunk
            .iter()
            .map(|frame| (frame.id, frame.captured_at))
            .collect();
        let closed_params = SegmentationParams {
            now_ms: cut.saturating_add(params.merge_gap_ms),
            ..params
        };
        let drafts = segmenter.segment(&chunk, &closed_params);
        let existing = store
            .sessions_in_range(SessionFilter {
                from_ms: Some(checkpoint.cursor_ms),
                to_ms: Some(cut),
                now_ms,
                ..SessionFilter::default()
            })
            .await?;
        for draft in drafts {
            let existing_match = existing
                .iter()
                .find(|session| {
                    session.context_key == draft.context_key
                        && session.started_at == draft.started_at
                        && session.ended_at == Some(draft.ended_at)
                })
                .map(|session| (session.id, !session.frozen));
            let (session_id, draft, needs_freeze) = match existing_match {
                Some((id, needs_freeze)) => (id, draft, needs_freeze),
                None => {
                    let overlapping_frozen = existing.iter().any(|session| {
                        session.frozen
                            && partition(session.kind, &session.context_key)
                                == partition(draft.kind, &draft.context_key)
                            && session.ended_at.is_some_and(|end| {
                                draft.started_at <= end && draft.ended_at >= session.started_at
                            })
                    });
                    let draft = if overlapping_frozen {
                        trim_backfill_draft_against_frozen(draft, &existing, &frame_times)
                    } else {
                        Some(draft)
                    };
                    let Some(draft) = draft else {
                        continue;
                    };
                    // Assign before freezing so a partial assignment can be rolled back without
                    // violating the immutable-frozen-row contract.
                    let id = store.insert_session(draft.as_new_session(false)).await?;
                    changed = true;
                    (id, draft, true)
                }
            };
            let already_owned: HashSet<i64> = store
                .session_frames_meta(session_id)
                .await?
                .into_iter()
                .map(|frame| frame.id)
                .collect();
            let unowned_ids: Vec<i64> = draft
                .frame_ids
                .iter()
                .filter(|id| !already_owned.contains(id))
                .copied()
                .collect();
            let expected_changes = unowned_ids.len() as u64;
            let assigned = store
                .assign_frames_session(&unowned_ids, Some(session_id))
                .await?;
            if assigned != expected_changes {
                if needs_freeze {
                    store.delete_unfrozen_session(session_id).await?;
                }
                anyhow::bail!(
                    "historical session {session_id} assigned {assigned}/{expected_changes} new frames"
                );
            }
            changed |= assigned > 0;
            if needs_freeze {
                changed |= store.freeze_sessions(current_frozen_horizon).await? > 0;
                if !store
                    .get_session(session_id)
                    .await?
                    .is_some_and(|session| session.frozen)
                {
                    store.delete_unfrozen_session(session_id).await?;
                    anyhow::bail!(
                        "historical session {session_id} did not freeze after assignment"
                    );
                }
            }
            if draft.kind == SessionKind::Ai {
                changed |= refresh_exchanges(store, segmenter, session_id, &draft).await?;
            }
        }
    }
    checkpoint.cursor_ms = cut;
    save_checkpoint(store, checkpoint).await?;
    tracing::info!(
        cursor_ms = checkpoint.cursor_ms,
        target_ms = checkpoint.target_ms,
        "sessions historical backfill advanced"
    );
    Ok(changed)
}

async fn wait_or_stop(stop: &mut watch::Receiver<bool>, duration: Duration) -> bool {
    tokio::select! {
        biased;
        _ = stop.changed() => true,
        _ = tokio::time::sleep(duration) => *stop.borrow(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use store::SqliteStore;
    use traits::{NewFrame, NewSession, Store};

    use super::*;
    use crate::graph_reconcile::new_dirty_sessions;

    async fn test_store() -> Arc<dyn Store> {
        Arc::new(SqliteStore::open_in_memory().expect("in-memory store"))
    }

    async fn insert_codex_frames(store: &dyn Store, count: i64) {
        for index in 0..count {
            let captured_at = index * 30_000;
            store
                .insert_frame(NewFrame {
                    captured_at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{captured_at}.webp"),
                    content_hash: format!("hash-{captured_at}"),
                    app_hint: Some("codex".to_string()),
                    window_title: Some("Codex".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .expect("insert frame");
        }
    }

    fn dirty_ids(dirty: &DirtySessions) -> Vec<i64> {
        dirty
            .lock()
            .expect("dirty sessions lock")
            .iter()
            .copied()
            .collect()
    }

    #[tokio::test]
    async fn history_backfill_complete_requires_a_reached_target() {
        let store = test_store().await;
        assert!(
            !history_backfill_complete(store.as_ref()).await,
            "a missing marker means the historical pass has not run yet"
        );
        store
            .set_setting(BACKFILL_MARKER, "not a checkpoint")
            .await
            .expect("write bad marker");
        assert!(
            !history_backfill_complete(store.as_ref()).await,
            "an unparsable marker is incomplete, never assumed done"
        );
        store
            .set_setting(BACKFILL_MARKER, r#"{"cursor_ms":5,"target_ms":10}"#)
            .await
            .expect("write partial marker");
        assert!(
            !history_backfill_complete(store.as_ref()).await,
            "a cursor short of the target is incomplete"
        );
        store
            .set_setting(BACKFILL_MARKER, r#"{"cursor_ms":10,"target_ms":10}"#)
            .await
            .expect("write complete marker");
        assert!(
            history_backfill_complete(store.as_ref()).await,
            "a reached target completes the history gate"
        );
    }

    #[tokio::test]
    async fn incremental_pass_marks_inserted_and_assigned_sessions_dirty() {
        let store = test_store().await;
        insert_codex_frames(store.as_ref(), 7).await;
        let engine = sessions::SessionEngine::new().expect("session engine");
        let dirty = new_dirty_sessions();
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &dirty,
        )
        .await
        .expect("incremental pass");
        let sessions = store.unfrozen_sessions().await.expect("sessions");
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            dirty_ids(&dirty),
            vec![sessions[0].id],
            "the inserted session and its assigned frames mark it dirty"
        );
    }

    #[tokio::test]
    async fn an_identical_incremental_pass_marks_nothing() {
        let store = test_store().await;
        insert_codex_frames(store.as_ref(), 7).await;
        let engine = sessions::SessionEngine::new().expect("session engine");
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &new_dirty_sessions(),
        )
        .await
        .expect("first pass");

        let clean = new_dirty_sessions();
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &clean,
        )
        .await
        .expect("second pass");
        assert!(
            dirty_ids(&clean).is_empty(),
            "a pass with no ownership mutation marks nothing dirty"
        );
    }

    #[tokio::test]
    async fn incremental_pass_marks_updated_sessions_dirty() {
        let store = test_store().await;
        insert_codex_frames(store.as_ref(), 7).await;
        let engine = sessions::SessionEngine::new().expect("session engine");
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &new_dirty_sessions(),
        )
        .await
        .expect("first pass");
        let session = store
            .unfrozen_sessions()
            .await
            .expect("sessions")
            .into_iter()
            .next()
            .expect("one session");
        // Mutate the stored row so the next pass has to update it.
        assert!(store
            .update_unfrozen_session(
                session.id,
                NewSession {
                    started_at: session.started_at,
                    ended_at: session.ended_at,
                    kind: session.kind,
                    tool: session.tool.clone(),
                    host: session.host,
                    context_key: session.context_key.clone(),
                    confidence: 0.1,
                    frozen: false,
                },
            )
            .await
            .expect("mutate stored row"));
        let dirty = new_dirty_sessions();
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &dirty,
        )
        .await
        .expect("update pass");
        assert_eq!(
            dirty_ids(&dirty),
            vec![session.id],
            "a session row update marks that session dirty"
        );
    }

    #[tokio::test]
    async fn incremental_pass_marks_stale_unassigns_dirty() {
        let store = test_store().await;
        insert_codex_frames(store.as_ref(), 7).await;
        let engine = sessions::SessionEngine::new().expect("session engine");
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &new_dirty_sessions(),
        )
        .await
        .expect("first pass");
        let session_id = store
            .unfrozen_sessions()
            .await
            .expect("sessions")
            .into_iter()
            .next()
            .expect("one session")
            .id;
        // A frame outside the incremental window owns evidence in this session: the next
        // pass removes that stale ownership and must mark the old session.
        let future_frame = store
            .insert_frame(NewFrame {
                captured_at: 500_000,
                monitor_index: 0,
                width: 1920,
                height: 1080,
                image_path: "frames/500000.webp".to_string(),
                content_hash: "hash-future".to_string(),
                app_hint: Some("codex".to_string()),
                window_title: Some("Codex".to_string()),
                browser_url: None,
                capture_trigger: None,
            })
            .await
            .expect("insert future frame");
        assert_eq!(
            store
                .assign_frames_session(&[future_frame], Some(session_id))
                .await
                .expect("stale assignment"),
            1
        );

        let dirty = new_dirty_sessions();
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &dirty,
        )
        .await
        .expect("stale unassign pass");
        assert_eq!(
            dirty_ids(&dirty),
            vec![session_id],
            "the session that lost stale ownership is marked dirty"
        );
        assert!(
            store
                .session_frames_meta(session_id)
                .await
                .expect("frames")
                .iter()
                .all(|frame| frame.id != future_frame),
            "the stale frame is no longer owned by the session"
        );
    }

    #[tokio::test]
    async fn incremental_ai_pass_still_refreshes_exchange_artifacts() {
        let store = test_store().await;
        for index in 0..7 {
            let captured_at = index * 30_000;
            let frame_id = store
                .insert_frame(NewFrame {
                    captured_at,
                    monitor_index: 0,
                    width: 1920,
                    height: 1080,
                    image_path: format!("frames/{captured_at}.webp"),
                    content_hash: format!("hash-claude-{captured_at}"),
                    app_hint: Some("claude".to_string()),
                    window_title: Some("Claude".to_string()),
                    browser_url: None,
                    capture_trigger: None,
                })
                .await
                .expect("insert ai frame");
            store
                .insert_ocr(
                    frame_id,
                    traits::OcrResult {
                        text: "User: review the diff\nClaude: it looks good".to_string(),
                        mean_confidence: 0.9,
                        engine: "test".to_string(),
                        spans: Vec::new(),
                    },
                )
                .await
                .expect("insert ai ocr");
        }
        let engine = sessions::SessionEngine::new().expect("session engine");
        let dirty = new_dirty_sessions();
        run_incremental_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            &dirty,
        )
        .await
        .expect("incremental pass");
        let session = store
            .unfrozen_sessions()
            .await
            .expect("sessions")
            .into_iter()
            .next()
            .expect("one ai session");
        assert_eq!(session.kind, SessionKind::Ai);
        let artifacts = store
            .list_session_artifacts(session.id)
            .await
            .expect("session artifacts");
        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.kind == SessionArtifactKind::Exchange),
            "the incremental AI pass still refreshes exchange artifacts"
        );
        assert_eq!(
            dirty_ids(&dirty),
            vec![session.id],
            "the same pass still marks the AI session's graph links dirty"
        );
    }

    #[tokio::test]
    async fn scheduler_pass_forwards_the_dirty_marks() {
        let store = test_store().await;
        insert_codex_frames(store.as_ref(), 7).await;
        let engine = sessions::SessionEngine::new().expect("session engine");
        let (events, _receiver) = broadcast::channel(8);
        let dirty = new_dirty_sessions();
        run_scheduler_pass(
            store.as_ref(),
            &engine,
            200_000,
            &Settings::default(),
            true,
            &events,
            &dirty,
        )
        .await;
        let session_id = store
            .unfrozen_sessions()
            .await
            .expect("sessions")
            .into_iter()
            .next()
            .expect("one session")
            .id;
        assert_eq!(
            dirty_ids(&dirty),
            vec![session_id],
            "the scheduler pass threads its incremental dirty marks through"
        );
    }
}
