//! Override precedence (`03 §7h.5`): rename, merge and split applied **after** clustering,
//! plus the retention rule that makes them durable across runs.
//!
//! Rows are read in `(created_at, id)` order and applied in that order, so a later override of
//! the same kind beats an earlier one. For any session, an override-derived assignment beats
//! the clustered one.
//!
//! # Override-managed threads
//!
//! A stored thread is **override-managed** when a **merge or split** row references its id (as
//! `thread_id` or `target_thread_id`) or when its `cluster_key` starts with
//! [`OVERRIDE_CLUSTER_KEY_PREFIX`]. A **rename** row does not manage membership: `03 §7h.5`
//! says a rename sets `threads.title` only, so the renamed thread stays clusterer-managed and
//! can gain newly matching sessions. Its title is re-applied after clustering by resolving the
//! row's stored thread id to that thread's current `cluster_key`.
//!
//! Managed threads are **not re-clustered**: their sessions are removed from the clustering
//! input by [`crate::clustering::plan_threads`], and their draft comes from the persisted row
//! (membership, `cluster_key` and title from storage; times and member confidences recomputed
//! from the input where the session is present). A stored member whose session no longer exists
//! in the input is dropped: `thread_sessions.session_id` is a foreign key and the sessions
//! scheduler deletes sessions, so carrying a deleted id would fail the whole clustering pass
//! (and block unrelated threads) on every retry. A managed thread that loses all its members
//! stays as an empty retained row because an override row references it. This is what keeps a
//! merge or a split applied on later runs.
//!
//! A fresh draft whose `cluster_key` collides with a managed stored thread's key never
//! overwrites it: the managed thread keeps its persisted key and the fresh draft appends the
//! same `#<min member session id>` discriminator `crate::clustering` uses. A key that is still
//! duplicated after that is a hard error naming the key, never a silently dropped component:
//! [`apply`] and [`crate::clustering::plan_threads`] return `anyhow::Result<ThreadPlan>`
//! (`specs/06_PATCH_PLAN.md` #40).
//!
//! # Shape of one cluster run (`crate::clustering::plan_threads`)
//!
//! 1. `clustered` holds the fresh components of the non-managed sessions, keyed by
//!    `cluster_key` (`seed:<ids>`);
//! 2. `stored` holds the currently persisted threads (id, cluster_key, title, members);
//! 3. the working model starts as the fresh drafts plus one draft per managed stored thread;
//! 4. overrides are applied to that model in order;
//! 5. the result is one [`ThreadPlan`] the store persists atomically (upsert by cluster_key,
//!    sync members, delete named threads). An emptied merge or split source row is retained in
//!    the plan with zero members instead of being deleted; no override row is rewritten.
//!
//! # Kind semantics (`03 §7h.5`)
//!
//! - **rename** sets `threads.title` only. The row's `thread_id` names a persisted thread; it
//!   is resolved to the current draft through `stored`'s id -> `cluster_key` map and skipped
//!   when that draft no longer exists, never applied to a different thread. Rename does not
//!   make the thread membership-managed, so the clusterer can still add sessions and the title
//!   is re-applied on every run.
//! - **merge** folds the source's members into `target_thread_id` (the survivor) and **retains
//!   the emptied source thread row in the plan** with an empty member set; the merge override
//!   row itself is never rewritten. `payload.session_ids` is recorded evidence and is never
//!   used to decide the fold. Two constraints hold at once:
//!   * `thread_overrides.thread_id` and `target_thread_id` are `ON DELETE CASCADE`, so deleting
//!     a thread an override row points at would delete that override and lose the user's edit;
//!   * overrides replay in `(created_at, id)` order on every run, and clustering keeps
//!     recomputing the source component from the unchanged `session_entities` links.
//!
//!   Retaining the emptied source row and never rewriting the merge row satisfies both: the
//!   source is already empty when the merge re-applies, so the fold is a no-op, and a split
//!   that created the source replays before the later merge does (its `created_at` is earlier)
//!   and is folded again. This supersedes the earlier "delete the source and rewrite the merge
//!   row to a `(target, target)` self-merge marker" resolution of the `03 §7h.5` "removes the
//!   emptied source thread" sentence versus the `ON DELETE CASCADE` foreign keys
//!   (`specs/06_PATCH_PLAN.md` #37): the self-merge marker was undone whenever an earlier split
//!   recreated the thread it had removed. Emptied rows stay in the plan because they are part
//!   of the deterministic model; P9 should filter `session_count = 0` threads from the Work
//!   screen.
//! - **split** moves the payload's sessions out of wherever they currently are into a new
//!   thread seeded by the override: `cluster_key = override:<override id>`, a stable key that
//!   survives re-clustering. Payload session ids that are absent from the current
//!   [`GraphClusterInput::sessions`] are skipped: the sessions scheduler deletes unfrozen
//!   sessions (the `thread_sessions` row cascades away, but the id stays in the payload JSON),
//!   and planning one would fail `graph_apply_thread_plan`'s foreign-key check and roll back
//!   the whole clustering pass. The split thread row is always kept (its override row
//!   references it) and holds the surviving sessions. Its title, in order, is the source
//!   thread's title as it stands in the model (so an earlier rename in the same plan is
//!   respected), the stored source title, the title of the thread the first surviving moved
//!   session currently belongs to, then the split `cluster_key`. Its times derive from the
//!   moved sessions, and a moved session keeps the member confidence it carried; a session
//!   outside every thread falls back to its best stored link confidence. The source row is
//!   kept in the plan even when it is emptied: the split row references it and a later run
//!   applies the same split again (the "clustering continues from the override" reading of
//!   `03 §7h.5`).
//! - Overrides referencing stored thread ids are resolved through `stored`'s id ->
//!   `cluster_key` map. Threads created by an earlier split have override-derived keys and
//!   are rebuilt by the same override each run, never re-clustered.
//!
//! The plan is deterministic: threads are sorted by `cluster_key` and delete ids are sorted.
//! [`ThreadPlan::override_updates`] stays empty: merge durability no longer rewrites an
//! override row (the field remains part of the `traits` contract).

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use traits::{
    GraphClusterInput, GraphClusterLink, GraphSessionInterval, StoredThread, ThreadDraft,
    ThreadMemberDraft, ThreadOverridePayload, ThreadOverrideRow, ThreadPlan,
};

use crate::clustering::{CLUSTER_KEY_PREFIX, SEED_CONFIDENCE_MIN};

/// Prefix of an override-derived `threads.cluster_key` (`override:<override id>`).
pub const OVERRIDE_CLUSTER_KEY_PREFIX: &str = "override:";

/// The deterministic `cluster_key` of the thread a split override seeds.
pub fn split_cluster_key(override_id: i64) -> String {
    format!("{OVERRIDE_CLUSTER_KEY_PREFIX}{override_id}")
}

/// One thread of the working model while the overrides are applied.
#[derive(Debug, Clone)]
struct ModelThread {
    cluster_key: String,
    title: String,
    first_seen_at: i64,
    last_seen_at: i64,
    members: BTreeMap<i64, f64>,
}

impl ModelThread {
    fn from_draft(draft: ThreadDraft) -> Self {
        Self {
            cluster_key: draft.cluster_key,
            title: draft.title,
            first_seen_at: draft.first_seen_at,
            last_seen_at: draft.last_seen_at,
            members: draft
                .sessions
                .into_iter()
                .map(|member| (member.session_id, member.confidence))
                .collect(),
        }
    }

    /// Builds a managed draft from a persisted row. Members whose session is no longer in the
    /// input are dropped: the sessions scheduler deletes sessions freely (a merge target, a
    /// split-created thread and a retained emptied source can each lose a member), and planning
    /// one would fail the store's `thread_sessions.session_id` foreign-key check and roll back
    /// the whole clustering pass. A managed thread that loses every member stays in the plan as
    /// an empty retained row, because an override row references it.
    fn from_stored(thread: &StoredThread, sessions: &BTreeMap<i64, &GraphSessionInterval>) -> Self {
        Self {
            cluster_key: thread.cluster_key.clone(),
            title: thread.title.clone(),
            first_seen_at: thread.first_seen_at,
            last_seen_at: thread.last_seen_at,
            members: thread
                .sessions
                .iter()
                .filter(|member| sessions.contains_key(&member.session_id))
                .map(|member| (member.session_id, member.confidence))
                .collect(),
        }
    }

    fn into_draft(self) -> ThreadDraft {
        ThreadDraft {
            cluster_key: self.cluster_key,
            title: self.title,
            first_seen_at: self.first_seen_at,
            last_seen_at: self.last_seen_at,
            sessions: self
                .members
                .into_iter()
                .map(|(session_id, confidence)| ThreadMemberDraft {
                    session_id,
                    confidence,
                })
                .collect(),
        }
    }
}

/// Every stored thread id a **merge or split** override row points at. These rows manage
/// membership; a rename row only sets a title (`03 §7h.5`), so it is deliberately excluded and
/// the renamed thread stays clusterer-managed.
fn membership_referenced_thread_ids(overrides: &[ThreadOverrideRow]) -> BTreeSet<i64> {
    let mut ids = BTreeSet::new();
    for row in overrides {
        if !matches!(
            &row.payload,
            ThreadOverridePayload::Merge { .. } | ThreadOverridePayload::Split { .. }
        ) {
            continue;
        }
        if let Some(thread_id) = row.thread_id {
            ids.insert(thread_id);
        }
        if let Some(target_thread_id) = row.target_thread_id {
            ids.insert(target_thread_id);
        }
    }
    ids
}

/// The stored thread ids that are override-managed: referenced by a merge or split row, or
/// carrying an override-derived `cluster_key`.
fn managed_stored_ids(stored: &[StoredThread], overrides: &[ThreadOverrideRow]) -> BTreeSet<i64> {
    let referenced = membership_referenced_thread_ids(overrides);
    stored
        .iter()
        .filter(|thread| {
            referenced.contains(&thread.id)
                || thread.cluster_key.starts_with(OVERRIDE_CLUSTER_KEY_PREFIX)
        })
        .map(|thread| thread.id)
        .collect()
}

/// Sessions that belong to an override-managed stored thread; `crate::clustering::plan_threads`
/// removes them from the clustering input so managed threads are never re-clustered. A
/// rename-only thread is not managed, so its sessions keep participating in clustering.
pub(crate) fn managed_session_ids(
    stored: &[StoredThread],
    overrides: &[ThreadOverrideRow],
) -> BTreeSet<i64> {
    let managed = managed_stored_ids(stored, overrides);
    stored
        .iter()
        .filter(|thread| managed.contains(&thread.id))
        .flat_map(|thread| thread.sessions.iter().map(|member| member.session_id))
        .collect()
}

/// The seed entity ids encoded in a clusterer `cluster_key` (`seed:<id>,<id>`); `None` for an
/// override-derived key or a malformed one.
fn seed_entity_ids(cluster_key: &str) -> Option<Vec<i64>> {
    let raw = cluster_key.strip_prefix(CLUSTER_KEY_PREFIX)?;
    if raw.is_empty() {
        return None;
    }
    raw.split(',')
        .map(|part| part.parse::<i64>().ok())
        .collect()
}

/// Recomputes a managed thread's member times from the sessions present in the input.
///
/// Members whose session is no longer in the input keep the thread's stored bounds (nothing in
/// the input can place them); a thread with no member present at all keeps its stored bounds.
fn recompute_times(
    thread: &mut ModelThread,
    sessions: &BTreeMap<i64, &GraphSessionInterval>,
    now_ms: i64,
) {
    let mut first_seen_at = i64::MAX;
    let mut last_seen_at = i64::MIN;
    let mut found = false;
    for session_id in thread.members.keys() {
        let Some(session) = sessions.get(session_id) else {
            continue;
        };
        found = true;
        first_seen_at = first_seen_at.min(session.started_at);
        last_seen_at = last_seen_at.max(session.ended_at.unwrap_or(now_ms));
    }
    if found {
        thread.first_seen_at = first_seen_at;
        thread.last_seen_at = last_seen_at;
    }
}

/// Recomputes a managed seed-keyed thread's member confidences from the input links, using the
/// same rule as the clusterer (the maximum confidence for any of the thread's seed entities).
/// Members without a strong link to a seed entity keep their stored confidence, and an
/// override-derived thread has no seed entities so it always keeps the stored values.
fn recompute_member_confidences(
    thread: &mut ModelThread,
    links_by_session: &BTreeMap<i64, Vec<&GraphClusterLink>>,
) {
    let Some(seed_ids) = seed_entity_ids(&thread.cluster_key) else {
        return;
    };
    for (session_id, confidence) in thread.members.iter_mut() {
        let mut best: Option<f64> = None;
        if let Some(links) = links_by_session.get(session_id) {
            for link in links {
                if link.confidence < SEED_CONFIDENCE_MIN || !seed_ids.contains(&link.entity_id) {
                    continue;
                }
                best =
                    Some(best.map_or(link.confidence, |current: f64| current.max(link.confidence)));
            }
        }
        if let Some(best) = best {
            *confidence = best;
        }
    }
}

/// The best stored link confidence for a session, the split fallback when the session belongs to
/// no thread at all. `0.0` means the input knows nothing about the session.
fn best_link_confidence(
    session_id: i64,
    links_by_session: &BTreeMap<i64, Vec<&GraphClusterLink>>,
) -> f64 {
    links_by_session
        .get(&session_id)
        .and_then(|links| links.iter().map(|link| link.confidence).reduce(f64::max))
        .unwrap_or(0.0)
}

/// Removes one session from whichever model thread holds it and returns its member confidence.
fn take_session(model: &mut BTreeMap<String, ModelThread>, session_id: i64) -> Option<f64> {
    for thread in model.values_mut() {
        if let Some(confidence) = thread.members.remove(&session_id) {
            return Some(confidence);
        }
    }
    None
}

/// Inserts one draft into the working model. A key that is still duplicated after collision
/// disambiguation is a hard error, never a silently dropped component (`06` #40).
fn insert_model_thread(
    model: &mut BTreeMap<String, ModelThread>,
    thread: ModelThread,
) -> anyhow::Result<()> {
    match model.entry(thread.cluster_key.clone()) {
        Entry::Vacant(entry) => {
            entry.insert(thread);
            Ok(())
        }
        Entry::Occupied(entry) => anyhow::bail!(
            "duplicate thread cluster_key {} after collision disambiguation",
            entry.key()
        ),
    }
}

/// Applies the ordered overrides to the clustered drafts and diffs the result against the
/// persisted threads, producing the plan the store persists (`03 §7h.5`).
///
/// The result is an error when two threads would share a `cluster_key` after collision
/// disambiguation (`06` #40); a clustered session is never silently dropped. The one exception
/// is a split payload id for a session the input no longer contains: that session does not
/// exist, so it is skipped instead of failing the plan on the store's foreign-key check.
pub fn apply(
    clustered: Vec<ThreadDraft>,
    stored: &[StoredThread],
    overrides: &[ThreadOverrideRow],
    input: &GraphClusterInput,
) -> anyhow::Result<ThreadPlan> {
    let sessions: BTreeMap<i64, &GraphSessionInterval> = input
        .sessions
        .iter()
        .map(|session| (session.session_id, session))
        .collect();
    let mut links_by_session: BTreeMap<i64, Vec<&GraphClusterLink>> = BTreeMap::new();
    for link in &input.links {
        links_by_session
            .entry(link.session_id)
            .or_default()
            .push(link);
    }
    let stored_by_id: BTreeMap<i64, &StoredThread> =
        stored.iter().map(|thread| (thread.id, thread)).collect();
    let managed_ids = managed_stored_ids(stored, overrides);

    // Override-managed stored threads are inserted first: they keep their persisted keys, and
    // a fresh draft may never silently overwrite one.
    let mut model: BTreeMap<String, ModelThread> = BTreeMap::new();
    for stored_thread in stored {
        if !managed_ids.contains(&stored_thread.id) {
            continue;
        }
        let mut thread = ModelThread::from_stored(stored_thread, &sessions);
        recompute_times(&mut thread, &sessions, input.now_ms);
        recompute_member_confidences(&mut thread, &links_by_session);
        insert_model_thread(&mut model, thread)?;
    }
    for draft in clustered {
        let mut thread = ModelThread::from_draft(draft);
        if model.contains_key(&thread.cluster_key) {
            // The fresh component keeps its sessions and gets the collision discriminator;
            // the managed thread keeps its persisted key.
            let min_member = thread.members.keys().next().copied().unwrap_or(0);
            thread.cluster_key = format!("{}#{min_member}", thread.cluster_key);
        }
        insert_model_thread(&mut model, thread)?;
    }

    let mut ordered_overrides: Vec<&ThreadOverrideRow> = overrides.iter().collect();
    ordered_overrides.sort_by_key(|row| (row.created_at, row.id));

    for row in ordered_overrides {
        match &row.payload {
            ThreadOverridePayload::Rename { title } => {
                let Some(thread_id) = row.thread_id else {
                    continue;
                };
                let Some(stored_thread) = stored_by_id.get(&thread_id) else {
                    continue;
                };
                if let Some(thread) = model.get_mut(&stored_thread.cluster_key) {
                    thread.title = title.clone();
                }
            }
            ThreadOverridePayload::Merge { .. } => {
                let (Some(source_id), Some(target_id)) = (row.thread_id, row.target_thread_id)
                else {
                    continue;
                };
                let (Some(source_stored), Some(target_stored)) =
                    (stored_by_id.get(&source_id), stored_by_id.get(&target_id))
                else {
                    continue;
                };
                let source_key = source_stored.cluster_key.clone();
                let target_key = target_stored.cluster_key.clone();
                if source_key == target_key {
                    // Defensive only: an earlier build persisted a rewritten `(target,target)`
                    // self-merge row. A self-merge is already applied, and the source row it
                    // names is the survivor itself, which is retained as usual.
                    continue;
                }
                if !model.contains_key(&source_key) || !model.contains_key(&target_key) {
                    continue;
                }
                // Fold the source into the survivor and keep the emptied source row in the
                // model: `thread_overrides` references it and the fold has to replay on every
                // run, so it is never deleted and the override row is never rewritten.
                let session = model
                    .get_mut(&source_key)
                    .expect("source thread membership checked above");
                let folded = std::mem::take(&mut session.members);
                let target = model
                    .get_mut(&target_key)
                    .expect("target thread membership checked above");
                for (session_id, confidence) in folded {
                    target.members.entry(session_id).or_insert(confidence);
                }
            }
            ThreadOverridePayload::Split { session_ids, .. } => {
                let Some(source_id) = row.thread_id else {
                    continue;
                };
                let source_stored = stored_by_id.get(&source_id).copied();
                let source_key = source_stored.map(|thread| thread.cluster_key.clone());
                // Sessions the sessions scheduler deleted are still listed in the payload but
                // absent from the input: their `thread_sessions` row cascaded away with them.
                // Planning one would fail the store's foreign-key check and roll back the whole
                // clustering pass, so only sessions that still exist are moved.
                let surviving_session_ids: Vec<i64> = session_ids
                    .iter()
                    .copied()
                    .filter(|session_id| sessions.contains_key(session_id))
                    .collect();
                let first_moved_title = surviving_session_ids.first().and_then(|session_id| {
                    model
                        .values()
                        .find(|thread| thread.members.contains_key(session_id))
                        .map(|thread| thread.title.clone())
                });
                let title = source_key
                    .as_ref()
                    .and_then(|key| model.get(key))
                    .map(|thread| thread.title.clone())
                    .or_else(|| source_stored.map(|thread| thread.title.clone()))
                    .or(first_moved_title)
                    .unwrap_or_else(|| split_cluster_key(row.id));
                // The split row references the source thread; keep its row in the plan even
                // when the split empties it.
                if let (Some(key), Some(thread)) = (source_key.as_ref(), source_stored) {
                    model
                        .entry(key.clone())
                        .or_insert_with(|| ModelThread::from_stored(thread, &sessions));
                }
                let mut moved: Vec<(i64, f64)> = Vec::with_capacity(surviving_session_ids.len());
                for session_id in &surviving_session_ids {
                    let confidence = take_session(&mut model, *session_id)
                        .unwrap_or_else(|| best_link_confidence(*session_id, &links_by_session));
                    moved.push((*session_id, confidence));
                }
                let key = split_cluster_key(row.id);
                let (fallback_first_seen_at, fallback_last_seen_at) = source_stored
                    .map(|thread| (thread.first_seen_at, thread.last_seen_at))
                    .unwrap_or((input.now_ms, input.now_ms));
                let thread = model.entry(key.clone()).or_insert_with(|| ModelThread {
                    cluster_key: key,
                    title: title.clone(),
                    first_seen_at: fallback_first_seen_at,
                    last_seen_at: fallback_last_seen_at,
                    members: BTreeMap::new(),
                });
                thread.title = title;
                thread.members = moved.into_iter().collect();
            }
        }
    }

    let mut threads: Vec<ThreadDraft> = Vec::with_capacity(model.len());
    for mut thread in model.into_values() {
        recompute_times(&mut thread, &sessions, input.now_ms);
        threads.push(thread.into_draft());
    }
    threads.sort_by(|first, second| first.cluster_key.cmp(&second.cluster_key));

    let planned_keys: BTreeSet<&str> = threads
        .iter()
        .map(|thread| thread.cluster_key.as_str())
        .collect();
    let mut delete_thread_ids: Vec<i64> = stored
        .iter()
        .filter(|thread| !planned_keys.contains(thread.cluster_key.as_str()))
        .map(|thread| thread.id)
        .collect();
    delete_thread_ids.sort_unstable();

    Ok(ThreadPlan {
        threads,
        delete_thread_ids,
        // Merge durability is retention plus replay order, never a row rewrite (module docs);
        // the field stays empty for every plan this planner produces.
        override_updates: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::{EntityKind, ThreadOverrideKind};

    use crate::clustering::{cluster_key, plan_threads as run_plan_threads, DAY_MS};

    /// The existing tests expect a successful plan; the duplicate-key failure has its own test.
    fn plan_threads(
        input: &GraphClusterInput,
        stored: &[StoredThread],
        overrides: &[ThreadOverrideRow],
    ) -> ThreadPlan {
        run_plan_threads(input, stored, overrides).expect("thread planning should succeed")
    }

    const HOUR_MS: i64 = 3_600_000;
    const BASE_MS: i64 = 1_000 * DAY_MS;
    const NOW_MS: i64 = BASE_MS + 100 * DAY_MS;

    const ALPHA: &str = "acme/alpha";
    const BETA: &str = "acme/beta";
    const GAMMA: &str = "acme/gamma";

    fn session_interval(session_id: i64) -> GraphSessionInterval {
        GraphSessionInterval {
            session_id,
            started_at: BASE_MS + session_id * DAY_MS,
            ended_at: Some(BASE_MS + session_id * DAY_MS + HOUR_MS),
        }
    }

    fn link_row(session_id: i64, entity_id: i64, canonical_name: &str) -> GraphClusterLink {
        GraphClusterLink {
            session_id,
            entity_id,
            entity_kind: EntityKind::Repo,
            canonical_name: canonical_name.to_string(),
            confidence: 0.9,
            last_seen_at: BASE_MS + session_id * DAY_MS + HOUR_MS,
        }
    }

    /// One `(entity id, canonical name, session ids)` per component.
    fn cluster_input(components: &[(i64, &str, &[i64])]) -> GraphClusterInput {
        let mut sessions = Vec::new();
        let mut links = Vec::new();
        for (entity_id, canonical_name, session_ids) in components {
            for session_id in *session_ids {
                sessions.push(session_interval(*session_id));
                links.push(link_row(*session_id, *entity_id, canonical_name));
            }
        }
        sessions.sort_by_key(|session| session.session_id);
        links.sort_by_key(|link| (link.session_id, link.entity_id));
        GraphClusterInput {
            now_ms: NOW_MS,
            sessions,
            links,
        }
    }

    fn three_components() -> GraphClusterInput {
        cluster_input(&[
            (101, ALPHA, &[1, 2]),
            (102, BETA, &[3, 4]),
            (103, GAMMA, &[5, 6]),
        ])
    }

    fn session_at(session_id: i64, day: i64) -> GraphSessionInterval {
        GraphSessionInterval {
            session_id,
            started_at: BASE_MS + day * DAY_MS,
            ended_at: Some(BASE_MS + day * DAY_MS + HOUR_MS),
        }
    }

    fn link_at(
        session_id: i64,
        entity_id: i64,
        canonical_name: &str,
        day: i64,
    ) -> GraphClusterLink {
        GraphClusterLink {
            session_id,
            entity_id,
            entity_kind: EntityKind::Repo,
            canonical_name: canonical_name.to_string(),
            confidence: 0.95,
            last_seen_at: BASE_MS + day * DAY_MS + HOUR_MS,
        }
    }

    /// A stored thread whose time bounds are derived from its sessions in the fixture input.
    fn stored_thread_for(
        input: &GraphClusterInput,
        id: i64,
        cluster_key: &str,
        title: &str,
        session_ids: &[i64],
        confidence: f64,
    ) -> StoredThread {
        let intervals: Vec<&GraphSessionInterval> = session_ids
            .iter()
            .filter_map(|session_id| {
                input
                    .sessions
                    .iter()
                    .find(|session| session.session_id == *session_id)
            })
            .collect();
        let first_seen_at = intervals
            .iter()
            .map(|session| session.started_at)
            .min()
            .unwrap_or(input.now_ms);
        let last_seen_at = intervals
            .iter()
            .map(|session| session.ended_at.unwrap_or(input.now_ms))
            .max()
            .unwrap_or(input.now_ms);
        StoredThread {
            id,
            cluster_key: cluster_key.to_string(),
            title: title.to_string(),
            first_seen_at,
            last_seen_at,
            sessions: session_ids
                .iter()
                .map(|session_id| ThreadMemberDraft {
                    session_id: *session_id,
                    confidence,
                })
                .collect(),
        }
    }

    /// Entity 10 seeds sessions 1,2 at days 0/2 and sessions 3,4 at days 45/47: every
    /// cross-part pair is beyond 30 days, so both components carry the base key `seed:10`,
    /// the "project resumed after a long hiatus" shape (`06` #40).
    fn hiatus_input() -> GraphClusterInput {
        GraphClusterInput {
            now_ms: NOW_MS,
            sessions: vec![
                session_at(1, 0),
                session_at(2, 2),
                session_at(3, 45),
                session_at(4, 47),
            ],
            links: vec![
                link_at(1, 10, ALPHA, 0),
                link_at(2, 10, ALPHA, 2),
                link_at(3, 10, ALPHA, 45),
                link_at(4, 10, ALPHA, 47),
            ],
        }
    }

    fn member_ids(thread: &ThreadDraft) -> Vec<i64> {
        thread
            .sessions
            .iter()
            .map(|member| member.session_id)
            .collect()
    }

    fn member_confidence(thread: &ThreadDraft, session_id: i64) -> f64 {
        thread
            .sessions
            .iter()
            .find(|member| member.session_id == session_id)
            // Do not interpolate a session id into the panic message: CodeQL classifies
            // session ids as sensitive and flags cleartext logging of them, even in tests.
            .unwrap_or_else(|| panic!("the expected member is not in the planned thread"))
            .confidence
    }

    fn thread_by_key<'a>(threads: &'a [ThreadDraft], key: &str) -> &'a ThreadDraft {
        threads
            .iter()
            .find(|thread| thread.cluster_key == key)
            .unwrap_or_else(|| panic!("planned thread {key} is missing: {threads:?}"))
    }

    fn stored_by_key<'a>(threads: &'a [StoredThread], key: &str) -> &'a StoredThread {
        threads
            .iter()
            .find(|thread| thread.cluster_key == key)
            .unwrap_or_else(|| panic!("stored thread {key} is missing: {threads:?}"))
    }

    fn stored_member_ids(thread: &StoredThread) -> Vec<i64> {
        thread
            .sessions
            .iter()
            .map(|member| member.session_id)
            .collect()
    }

    /// The plan-level invariant: a `thread_sessions` row is inserted for every planned member,
    /// so every planned member must still exist in the clustering input or the store rolls the
    /// whole pass back on its foreign-key check.
    fn assert_plan_members_exist(input: &GraphClusterInput, plan: &ThreadPlan) {
        let input_sessions: BTreeSet<i64> = input
            .sessions
            .iter()
            .map(|session| session.session_id)
            .collect();
        for thread in &plan.threads {
            for member in &thread.sessions {
                assert!(
                    input_sessions.contains(&member.session_id),
                    "planned thread {} holds a member session that is not in the input",
                    thread.cluster_key
                );
            }
        }
    }

    /// A stored model with three managed threads, two of whose sessions have since been deleted
    /// by the sessions scheduler: the merge target lost session 3 and the split-created thread
    /// lost session 8. Sessions 9,10 are unmanaged and re-cluster normally.
    fn managed_threads_with_deleted_members(
    ) -> (GraphClusterInput, Vec<StoredThread>, ThreadOverrideRow) {
        let input = cluster_input(&[
            (101, ALPHA, &[1, 2]),
            (102, BETA, &[4, 5]),
            (104, GAMMA, &[6, 7]),
            (105, ALPHA, &[9, 10]),
        ]);
        let target = stored_thread_for(&input, 1, &cluster_key(&[101]), ALPHA, &[1, 2, 3], 0.9);
        let source = stored_thread_for(&input, 2, &cluster_key(&[102]), BETA, &[4, 5], 0.9);
        let split_created =
            stored_thread_for(&input, 3, &split_cluster_key(7), GAMMA, &[6, 7, 8], 0.9);
        let merge = merge_override(1, 10, &source, &target);
        (input, vec![target, source, split_created], merge)
    }

    fn rename_override(id: i64, created_at: i64, thread_id: i64, title: &str) -> ThreadOverrideRow {
        ThreadOverrideRow {
            id,
            kind: ThreadOverrideKind::Rename,
            thread_id: Some(thread_id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: title.to_string(),
            },
            created_at,
        }
    }

    fn merge_override(
        id: i64,
        created_at: i64,
        source: &StoredThread,
        target: &StoredThread,
    ) -> ThreadOverrideRow {
        ThreadOverrideRow {
            id,
            kind: ThreadOverrideKind::Merge,
            thread_id: Some(source.id),
            target_thread_id: Some(target.id),
            payload: ThreadOverridePayload::Merge {
                session_ids: source
                    .sessions
                    .iter()
                    .map(|member| member.session_id)
                    .collect(),
            },
            created_at,
        }
    }

    fn split_override(
        id: i64,
        created_at: i64,
        source: &StoredThread,
        session_ids: &[i64],
    ) -> ThreadOverrideRow {
        ThreadOverrideRow {
            id,
            kind: ThreadOverrideKind::Split,
            thread_id: Some(source.id),
            target_thread_id: None,
            payload: ThreadOverridePayload::Split {
                thread_id: source.id,
                session_ids: session_ids.to_vec(),
            },
            created_at,
        }
    }

    /// Simulates the store's atomic application of one plan: upsert by `cluster_key` keeping
    /// existing ids, delete the named ids, and apply planned override rewrites (the graph
    /// planner no longer emits any; the store contract still supports them).
    fn apply_plan(
        plan: &ThreadPlan,
        stored: &[StoredThread],
        overrides: &[ThreadOverrideRow],
    ) -> (Vec<StoredThread>, Vec<ThreadOverrideRow>) {
        for thread in stored {
            let planned = plan
                .threads
                .iter()
                .any(|draft| draft.cluster_key == thread.cluster_key);
            assert!(
                planned || plan.delete_thread_ids.contains(&thread.id),
                "stored thread {} is neither planned nor deleted",
                thread.id
            );
        }
        let mut next_id = stored.iter().map(|thread| thread.id).max().unwrap_or(0) + 1;
        let mut ids_by_key: BTreeMap<String, i64> = stored
            .iter()
            .map(|thread| (thread.cluster_key.clone(), thread.id))
            .collect();
        let mut threads = Vec::with_capacity(plan.threads.len());
        for draft in &plan.threads {
            let id = match ids_by_key.get(&draft.cluster_key) {
                Some(id) => *id,
                None => {
                    let id = next_id;
                    next_id += 1;
                    ids_by_key.insert(draft.cluster_key.clone(), id);
                    id
                }
            };
            threads.push(StoredThread {
                id,
                cluster_key: draft.cluster_key.clone(),
                title: draft.title.clone(),
                first_seen_at: draft.first_seen_at,
                last_seen_at: draft.last_seen_at,
                sessions: draft.sessions.clone(),
            });
        }
        let mut overrides = overrides.to_vec();
        for update in &plan.override_updates {
            let row = overrides
                .iter_mut()
                .find(|row| row.id == update.override_id)
                .unwrap_or_else(|| panic!("override {} is missing", update.override_id));
            row.thread_id = Some(update.thread_id);
            row.target_thread_id = Some(update.target_thread_id);
        }
        (threads, overrides)
    }

    /// Plans once, applies the plan, plans again from its own output, and requires a stable
    /// model with an empty delete list and no repeated override rewrites.
    fn assert_stable_after_apply(
        input: &GraphClusterInput,
        stored: &[StoredThread],
        overrides: &[ThreadOverrideRow],
    ) -> (ThreadPlan, Vec<StoredThread>, Vec<ThreadOverrideRow>) {
        let plan = plan_threads(input, stored, overrides);
        let (stored_after, overrides_after) = apply_plan(&plan, stored, overrides);
        let replanned = plan_threads(input, &stored_after, &overrides_after);
        assert_eq!(
            replanned.threads, plan.threads,
            "re-planning the applied model changed the threads"
        );
        assert!(
            replanned.delete_thread_ids.is_empty(),
            "re-planning deleted rows: {:?}",
            replanned.delete_thread_ids
        );
        assert!(
            replanned.override_updates.is_empty(),
            "re-planning rewrote overrides again: {:?}",
            replanned.override_updates
        );
        (plan, stored_after, overrides_after)
    }

    #[test]
    fn clustering_plan_is_idempotent_from_its_own_output() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        assert_eq!(plan.threads.len(), 3, "{:?}", plan.threads);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());

        let (stored, _) = apply_plan(&plan, &[], &[]);
        let replanned = plan_threads(&input, &stored, &[]);
        assert_eq!(replanned.threads, plan.threads);
        assert!(replanned.delete_thread_ids.is_empty());
        assert!(replanned.override_updates.is_empty());
    }

    #[test]
    fn rename_beats_the_clusterer_title() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha_key = cluster_key(&[101]);
        let alpha = stored_by_key(&stored, &alpha_key);
        assert_eq!(alpha.title, ALPHA);

        let renamed = "Release train";
        let overrides = vec![rename_override(1, 10, alpha.id, renamed)];
        let plan = plan_threads(&input, &stored, &overrides);
        assert_eq!(thread_by_key(&plan.threads, &alpha_key).title, renamed);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
    }

    #[test]
    fn later_rename_beats_an_earlier_rename() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha_key = cluster_key(&[101]);
        let alpha = stored_by_key(&stored, &alpha_key);

        // Supplied in reverse order on purpose: precedence comes from (created_at, id).
        let overrides = vec![
            rename_override(2, 20, alpha.id, "Second"),
            rename_override(1, 10, alpha.id, "First"),
        ];
        let plan = plan_threads(&input, &stored, &overrides);
        assert_eq!(thread_by_key(&plan.threads, &alpha_key).title, "Second");
    }

    #[test]
    fn rename_does_not_freeze_thread_membership() {
        let first_input = cluster_input(&[(101, ALPHA, &[1, 2])]);
        let first_plan = plan_threads(&first_input, &[], &[]);
        let (stored, _) = apply_plan(&first_plan, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));

        // A rename sets the title only (`03 §7h.5`), so a later session that matches the same
        // strong seed must still join the thread instead of being frozen out of clustering.
        let rename = rename_override(1, 10, alpha.id, "Release train");
        let grown_input = cluster_input(&[(101, ALPHA, &[1, 2, 3, 4])]);
        let (plan, stored_after, overrides_after) =
            assert_stable_after_apply(&grown_input, &stored, std::slice::from_ref(&rename));

        assert_eq!(plan.threads.len(), 1, "{:?}", plan.threads);
        let thread = thread_by_key(&plan.threads, &alpha.cluster_key);
        assert_eq!(member_ids(thread), vec![1, 2, 3, 4]);
        assert_eq!(thread.title, "Release train");
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
        assert_eq!(stored_after.len(), 1);
        assert_eq!(overrides_after, vec![rename]);
    }

    #[test]
    fn merge_folds_the_source_members_and_retains_the_emptied_source_row() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let beta = stored_by_key(&stored, &cluster_key(&[102]));

        let merge = merge_override(1, 10, beta, alpha);
        let (plan, stored_after, overrides_after) =
            assert_stable_after_apply(&input, &stored, std::slice::from_ref(&merge));
        // Alpha holds the folded members, beta is retained empty, gamma is untouched.
        assert_eq!(plan.threads.len(), 3, "{:?}", plan.threads);

        let merged = thread_by_key(&plan.threads, &alpha.cluster_key);
        let expected_members: Vec<i64> = [alpha, beta]
            .iter()
            .flat_map(|thread| thread.sessions.iter().map(|member| member.session_id))
            .collect();
        let mut expected_members = expected_members;
        expected_members.sort_unstable();
        assert_eq!(member_ids(merged), expected_members);
        assert_eq!(merged.title, ALPHA);

        // The emptied source row is retained in the plan, neither deleted nor rewritten into
        // a self-merge marker; the merge override row keeps naming the original source.
        let retained = thread_by_key(&plan.threads, &beta.cluster_key);
        assert!(retained.sessions.is_empty());
        assert_eq!(retained.title, BETA);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
        assert_eq!(overrides_after, vec![merge]);
        assert!(stored_member_ids(stored_by_key(&stored_after, &beta.cluster_key)).is_empty());
        assert_eq!(
            stored_member_ids(stored_by_key(&stored_after, &alpha.cluster_key)),
            expected_members
        );
    }

    #[test]
    fn later_merge_absorbs_the_earlier_merge() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let beta = stored_by_key(&stored, &cluster_key(&[102]));
        let gamma = stored_by_key(&stored, &cluster_key(&[103]));

        let first = merge_override(1, 10, beta, alpha);
        let second = merge_override(2, 20, alpha, gamma);
        // Reverse input order: the planner sorts by (created_at, id).
        let (plan, _, overrides_after) =
            assert_stable_after_apply(&input, &stored, &[second.clone(), first.clone()]);

        let survivor = thread_by_key(&plan.threads, &gamma.cluster_key);
        let mut expected_members: Vec<i64> = [alpha, beta, gamma]
            .iter()
            .flat_map(|thread| thread.sessions.iter().map(|member| member.session_id))
            .collect();
        expected_members.sort_unstable();
        assert_eq!(member_ids(survivor), expected_members);
        assert_eq!(survivor.title, GAMMA);

        // Both emptied sources stay in the plan; replaying the two merges in (created_at, id)
        // order restores the same fold every run without rewriting either row.
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
        assert!(thread_by_key(&plan.threads, &alpha.cluster_key)
            .sessions
            .is_empty());
        assert!(thread_by_key(&plan.threads, &beta.cluster_key)
            .sessions
            .is_empty());
        assert_eq!(overrides_after, vec![second, first]);
    }

    #[test]
    fn split_then_later_merge_is_restored_on_every_replan() {
        let input = three_components();
        let clustered = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&clustered, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));

        // Earlier split: session 2 moves out of alpha into the override-seeded thread.
        let split = split_override(1, 10, alpha, &[2]);
        let split_plan = plan_threads(&input, &stored, std::slice::from_ref(&split));
        let (split_stored, split_overrides) =
            apply_plan(&split_plan, &stored, std::slice::from_ref(&split));
        assert_eq!(split_overrides, vec![split.clone()]);
        let split_thread = stored_by_key(&split_stored, &split_cluster_key(1));
        assert_eq!(stored_member_ids(split_thread), vec![2]);
        let beta = stored_by_key(&split_stored, &cluster_key(&[102]));

        // A later merge of the split-created thread into beta. The earlier split replays first
        // and recreates the thread, and this merge folds it again on every run; the merge row
        // is never rewritten into the self-merge marker that used to undo it.
        let merge = merge_override(2, 20, split_thread, beta);
        let (plan, stored_after, overrides_after) =
            assert_stable_after_apply(&input, &split_stored, &[split.clone(), merge.clone()]);

        let survivor = thread_by_key(&plan.threads, &beta.cluster_key);
        assert_eq!(member_ids(survivor), vec![2, 3, 4]);
        assert_eq!(
            stored_member_ids(stored_by_key(&stored_after, &beta.cluster_key)),
            vec![2, 3, 4]
        );
        assert!(thread_by_key(&plan.threads, &split_cluster_key(1))
            .sessions
            .is_empty());
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
        assert_eq!(overrides_after, vec![split, merge]);
    }

    #[test]
    fn merge_keeps_the_title_of_a_renamed_source_it_retains() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let beta = stored_by_key(&stored, &cluster_key(&[102]));

        let rename = rename_override(1, 10, beta.id, "Pinned beta");
        let merge = merge_override(2, 20, beta, alpha);
        let plan = plan_threads(&input, &stored, &[rename, merge]);

        let retained = thread_by_key(&plan.threads, &beta.cluster_key);
        assert!(retained.sessions.is_empty());
        assert_eq!(retained.title, "Pinned beta");
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
        let merged = thread_by_key(&plan.threads, &alpha.cluster_key);
        assert_eq!(member_ids(merged), vec![1, 2, 3, 4]);
    }

    #[test]
    fn split_moves_the_listed_sessions_and_keeps_the_source_title() {
        let input = cluster_input(&[(101, ALPHA, &[1, 2, 3])]);
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let source = stored_by_key(&stored, &cluster_key(&[101]));

        let split = split_override(1, 10, source, &[3]);
        let plan = plan_threads(&input, &stored, &[split]);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());

        let moved = thread_by_key(&plan.threads, &split_cluster_key(1));
        assert_eq!(member_ids(moved), vec![3]);
        assert_eq!(moved.title, source.title);
        let remaining = thread_by_key(&plan.threads, &source.cluster_key);
        assert_eq!(member_ids(remaining), vec![1, 2]);
    }

    #[test]
    fn later_split_beats_an_earlier_split() {
        let input = cluster_input(&[(101, ALPHA, &[1, 2, 3])]);
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let source = stored_by_key(&stored, &cluster_key(&[101]));

        let first = split_override(1, 10, source, &[2, 3]);
        let second = split_override(2, 20, source, &[3]);
        let plan = plan_threads(&input, &stored, &[first, second]);

        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &split_cluster_key(1))),
            vec![2]
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &split_cluster_key(2))),
            vec![3]
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &source.cluster_key)),
            vec![1]
        );
    }

    #[test]
    fn split_retains_an_emptied_source_thread_row() {
        let input = cluster_input(&[(101, ALPHA, &[1, 2])]);
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let source = stored_by_key(&stored, &cluster_key(&[101]));

        let split = split_override(1, 10, source, &[1, 2]);
        let plan = plan_threads(&input, &stored, &[split]);
        let retained = thread_by_key(&plan.threads, &source.cluster_key);
        assert!(retained.sessions.is_empty());
        assert!(plan.delete_thread_ids.is_empty());
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &split_cluster_key(1))),
            vec![1, 2]
        );
    }

    #[test]
    fn split_skips_session_ids_that_no_longer_exist() {
        let input = cluster_input(&[(101, ALPHA, &[1, 2]), (103, GAMMA, &[5, 6])]);
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let gamma_key = cluster_key(&[103]);

        // Session 3 was deleted by the sessions scheduler: its foreign-key cascade removed the
        // thread_sessions row, but the older split payload still lists the id.
        let stale_split = split_override(1, 10, alpha, &[2, 3]);
        // Session 7 now matches gamma, so an unrelated thread updates in the same pass.
        let changed = cluster_input(&[(101, ALPHA, &[1, 2]), (103, GAMMA, &[5, 6, 7])]);
        let plan = plan_threads(&changed, &stored, std::slice::from_ref(&stale_split));

        assert!(
            plan.threads
                .iter()
                .all(|thread| !member_ids(thread).contains(&3)),
            "a deleted session id must never reach the plan"
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &alpha.cluster_key)),
            vec![1]
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &split_cluster_key(1))),
            vec![2]
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &gamma_key)),
            vec![5, 6, 7]
        );
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());

        // The whole plan equals the plan a payload naming only the surviving sessions produces:
        // the stale id is filtered, never an error and never a rolled-back pass.
        let filtered = split_override(1, 10, alpha, &[2]);
        assert_eq!(
            plan,
            plan_threads(&changed, &stored, std::slice::from_ref(&filtered))
        );

        // Re-planning the applied model is stable.
        let (stored_after, overrides_after) =
            apply_plan(&plan, &stored, std::slice::from_ref(&stale_split));
        let replanned = plan_threads(&changed, &stored_after, &overrides_after);
        assert_eq!(replanned.threads, plan.threads);
        assert!(replanned.delete_thread_ids.is_empty());
        assert!(replanned.override_updates.is_empty());
    }

    #[test]
    fn merge_target_drops_a_member_session_that_no_longer_exists() {
        // Session 3 was deleted after the stored merge target was written.
        let input = cluster_input(&[
            (101, ALPHA, &[1, 2]),
            (102, BETA, &[4, 5]),
            (103, GAMMA, &[6, 7]),
        ]);
        let target = stored_thread_for(&input, 1, &cluster_key(&[101]), ALPHA, &[1, 2, 3], 0.9);
        let source = stored_thread_for(&input, 2, &cluster_key(&[102]), BETA, &[4, 5], 0.9);
        let merge = merge_override(1, 10, &source, &target);
        let (plan, _, overrides_after) =
            assert_stable_after_apply(&input, &[target, source], std::slice::from_ref(&merge));

        // The target keeps its surviving members and the folded source; the deleted session is
        // dropped instead of reaching the store's foreign-key check.
        let merged = thread_by_key(&plan.threads, &cluster_key(&[101]));
        assert_eq!(member_ids(merged), vec![1, 2, 4, 5]);
        assert_plan_members_exist(&input, &plan);
        assert!(plan
            .threads
            .iter()
            .all(|thread| !member_ids(thread).contains(&3)));
        assert!(thread_by_key(&plan.threads, &cluster_key(&[102]))
            .sessions
            .is_empty());
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
        assert_eq!(overrides_after, vec![merge]);
    }

    #[test]
    fn split_created_thread_drops_a_member_session_that_no_longer_exists() {
        // Session 6 was deleted after the split-created thread was written.
        let input = cluster_input(&[(101, ALPHA, &[1, 2]), (102, BETA, &[4, 5])]);
        let split_created =
            stored_thread_for(&input, 3, &split_cluster_key(7), GAMMA, &[4, 5, 6], 0.9);
        let plan = plan_threads(&input, std::slice::from_ref(&split_created), &[]);

        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &split_cluster_key(7))),
            vec![4, 5]
        );
        assert_plan_members_exist(&input, &plan);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());

        let (stored_after, overrides_after) =
            apply_plan(&plan, std::slice::from_ref(&split_created), &[]);
        let replanned = plan_threads(&input, &stored_after, &overrides_after);
        assert_eq!(replanned, plan);
        assert_plan_members_exist(&input, &replanned);
    }

    #[test]
    fn every_planned_member_exists_in_the_clustering_input() {
        let (input, stored, merge) = managed_threads_with_deleted_members();
        let plan = plan_threads(&input, &stored, std::slice::from_ref(&merge));

        assert_plan_members_exist(&input, &plan);
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &cluster_key(&[101]))),
            vec![1, 2, 4, 5]
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &split_cluster_key(7))),
            vec![6, 7]
        );
        assert_eq!(
            member_ids(thread_by_key(&plan.threads, &cluster_key(&[105]))),
            vec![9, 10]
        );
        assert!(thread_by_key(&plan.threads, &cluster_key(&[102]))
            .sessions
            .is_empty());
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
    }

    #[test]
    fn plan_with_deleted_managed_members_is_stable_on_replan() {
        let (input, stored, merge) = managed_threads_with_deleted_members();
        let plan = plan_threads(&input, &stored, std::slice::from_ref(&merge));
        let (stored_after, overrides_after) =
            apply_plan(&plan, &stored, std::slice::from_ref(&merge));
        let replanned = plan_threads(&input, &stored_after, &overrides_after);

        // Full plan equality: the same threads, no deletions and no override rewrites.
        assert_eq!(replanned, plan);
        assert_plan_members_exist(&input, &replanned);

        let (stored_again, overrides_again) =
            apply_plan(&replanned, &stored_after, &overrides_after);
        let replanned_again = plan_threads(&input, &stored_again, &overrides_again);
        assert_eq!(replanned_again, replanned);
    }

    #[test]
    fn split_falls_back_to_the_moved_sessions_thread_title_when_the_source_is_gone() {
        let input = cluster_input(&[(101, ALPHA, &[1, 2])]);
        // No stored source thread: the row points at a thread id that does not exist.
        let split = ThreadOverrideRow {
            id: 7,
            kind: ThreadOverrideKind::Split,
            thread_id: Some(999),
            target_thread_id: None,
            payload: ThreadOverridePayload::Split {
                thread_id: 999,
                session_ids: vec![1],
            },
            created_at: 10,
        };
        let plan = plan_threads(&input, &[], &[split]);
        let moved = thread_by_key(&plan.threads, &split_cluster_key(7));
        assert_eq!(member_ids(moved), vec![1]);
        assert_eq!(moved.title, ALPHA);
    }

    #[test]
    fn stale_stored_threads_are_deleted() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);

        // Session 3 is now alone on entity 102: a single-session component is not a thread.
        let changed = cluster_input(&[
            (101, ALPHA, &[1, 2]),
            (102, BETA, &[3]),
            (103, GAMMA, &[5, 6]),
        ]);
        let plan = plan_threads(&changed, &stored, &[]);
        let beta_key = cluster_key(&[102]);
        let beta = stored_by_key(&stored, &beta_key);
        assert_eq!(plan.delete_thread_ids, vec![beta.id]);
        assert!(plan
            .threads
            .iter()
            .all(|thread| thread.cluster_key != beta_key));
    }

    #[test]
    fn plan_with_merge_rename_and_split_is_idempotent() {
        let input = three_components();
        let clustered = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&clustered, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let beta = stored_by_key(&stored, &cluster_key(&[102]));
        let gamma = stored_by_key(&stored, &cluster_key(&[103]));

        let overrides = vec![
            rename_override(1, 10, alpha.id, "Pinned"),
            merge_override(2, 20, beta, alpha),
            split_override(3, 30, gamma, &[6]),
        ];
        let (plan, stored_after, overrides_after) =
            assert_stable_after_apply(&input, &stored, &overrides);

        let renamed = thread_by_key(&plan.threads, &alpha.cluster_key);
        assert_eq!(renamed.title, "Pinned");
        assert_eq!(member_ids(renamed), vec![1, 2, 3, 4]);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());

        // The merge source is retained empty, and the split keeps its source row while moving
        // only its listed session out.
        let retained_source = thread_by_key(&plan.threads, &beta.cluster_key);
        assert!(retained_source.sessions.is_empty());
        let remaining = thread_by_key(&plan.threads, &gamma.cluster_key);
        assert_eq!(member_ids(remaining), vec![5]);

        let split_thread = thread_by_key(&plan.threads, &split_cluster_key(3));
        assert_eq!(member_ids(split_thread), vec![6]);
        assert_eq!(split_thread.title, gamma.title);
        assert!(stored_after
            .iter()
            .any(|thread| thread.cluster_key == split_cluster_key(3)));
        assert_eq!(overrides_after, overrides);
    }

    #[test]
    fn split_session_keeps_the_member_confidence_it_carried() {
        let input = cluster_input(&[(101, ALPHA, &[1, 2])]);
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let source = stored_by_key(&stored, &cluster_key(&[101]));
        let carried = member_confidence(
            &plan
                .threads
                .iter()
                .find(|thread| thread.cluster_key == source.cluster_key)
                .unwrap()
                .clone(),
            2,
        );

        let split = split_override(1, 10, source, &[2]);
        let plan = plan_threads(&input, &stored, &[split]);
        let moved = thread_by_key(&plan.threads, &split_cluster_key(1));
        assert_eq!(member_confidence(moved, 2), carried);
    }

    #[test]
    fn overrides_that_reference_missing_threads_are_skipped() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let baseline = plan_threads(&input, &stored, &[]);

        let missing_rename = rename_override(1, 10, 999, "Ghost");
        let missing_merge = ThreadOverrideRow {
            id: 2,
            kind: ThreadOverrideKind::Merge,
            thread_id: Some(998),
            target_thread_id: Some(999),
            payload: ThreadOverridePayload::Merge {
                session_ids: vec![1],
            },
            created_at: 20,
        };
        let plan = plan_threads(&input, &stored, &[missing_rename, missing_merge]);
        assert_eq!(plan.threads, baseline.threads);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
    }

    #[test]
    fn plan_is_deterministic_for_the_same_input() {
        let input = three_components();
        let clustered = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&clustered, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let beta = stored_by_key(&stored, &cluster_key(&[102]));
        let overrides = vec![
            rename_override(1, 10, alpha.id, "Pinned"),
            merge_override(2, 20, beta, alpha),
        ];
        let first = plan_threads(&input, &stored, &overrides);
        let second = plan_threads(&input, &stored, &overrides);
        assert_eq!(first, second);

        // Stored-thread order is not part of the input identity either.
        let mut reversed_stored = stored.clone();
        reversed_stored.reverse();
        let third = plan_threads(&input, &reversed_stored, &overrides);
        assert_eq!(first, third);
    }

    #[test]
    fn a_hiatus_plan_keeps_both_threads_and_all_sessions() {
        let input = hiatus_input();
        let base = cluster_key(&[10]);
        let plan = plan_threads(&input, &[], &[]);
        assert_eq!(plan.threads.len(), 2, "{:?}", plan.threads);
        let keys: Vec<String> = plan
            .threads
            .iter()
            .map(|thread| thread.cluster_key.clone())
            .collect();
        assert_eq!(keys, vec![format!("{base}#1"), format!("{base}#3")]);

        let mut members: Vec<i64> = plan.threads.iter().flat_map(member_ids).collect();
        members.sort_unstable();
        assert_eq!(members, vec![1, 2, 3, 4]);

        // Re-planning the applied model is stable: the suffixed keys are produced again.
        let (stored_after, overrides_after) = apply_plan(&plan, &[], &[]);
        let replanned = plan_threads(&input, &stored_after, &overrides_after);
        assert_eq!(replanned.threads, plan.threads);
        assert!(replanned.delete_thread_ids.is_empty());
        assert!(replanned.override_updates.is_empty());
    }

    #[test]
    fn a_fresh_component_colliding_with_a_managed_key_keeps_the_managed_thread() {
        let input = GraphClusterInput {
            now_ms: NOW_MS,
            sessions: vec![
                session_at(1, 0),
                session_at(2, 2),
                session_at(3, 45),
                session_at(4, 47),
                session_at(5, 60),
                session_at(6, 62),
            ],
            links: vec![
                link_at(1, 10, ALPHA, 0),
                link_at(2, 10, ALPHA, 2),
                link_at(3, 10, ALPHA, 45),
                link_at(4, 10, ALPHA, 47),
                link_at(5, 20, BETA, 60),
                link_at(6, 20, BETA, 62),
            ],
        };
        let base = cluster_key(&[10]);
        // A merge management edge holds sessions 3,4 in a stored thread that persisted the
        // unsuffixed base key, and sessions 5,6 in its (retained, emptied) source. Sessions
        // 1,2 are unmanaged and re-cluster to the same base key.
        let managed = stored_thread_for(&input, 1, &base, BETA, &[3, 4], 0.95);
        let source = stored_thread_for(&input, 2, &cluster_key(&[20]), ALPHA, &[5, 6], 0.95);
        let merge = merge_override(1, 10, &source, &managed);
        let (plan, _, _) =
            assert_stable_after_apply(&input, &[managed, source], std::slice::from_ref(&merge));

        assert_eq!(plan.threads.len(), 3, "{:?}", plan.threads);
        let managed_thread = thread_by_key(&plan.threads, &base);
        assert_eq!(member_ids(managed_thread), vec![3, 4, 5, 6]);
        assert_eq!(managed_thread.title, BETA);
        let fresh_thread = thread_by_key(&plan.threads, &format!("{base}#1"));
        assert_eq!(member_ids(fresh_thread), vec![1, 2]);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());
    }

    #[test]
    fn a_duplicate_key_after_disambiguation_is_an_error() {
        // Managed threads hold seed:10 and seed:10#1; the fresh component re-clusters to
        // seed:10 with minimum member session id 1, so its discriminator collides again.
        let input = GraphClusterInput {
            now_ms: NOW_MS,
            sessions: vec![
                session_at(1, 0),
                session_at(2, 2),
                session_at(3, 45),
                session_at(4, 47),
                session_at(5, 60),
                session_at(6, 62),
            ],
            links: vec![
                link_at(1, 10, ALPHA, 0),
                link_at(2, 10, ALPHA, 2),
                link_at(3, 10, ALPHA, 45),
                link_at(4, 10, ALPHA, 47),
                link_at(5, 20, BETA, 60),
                link_at(6, 20, BETA, 62),
            ],
        };
        let base = cluster_key(&[10]);
        let suffixed = format!("{base}#1");
        let managed_base = stored_thread_for(&input, 1, &base, ALPHA, &[3, 4], 0.95);
        let managed_suffixed = stored_thread_for(&input, 2, &suffixed, BETA, &[5, 6], 0.95);
        // The merge row is what makes both stored threads membership-managed; a rename row no
        // longer does. Sessions 1,2 then cluster to seed:10, take the `#1` discriminator, and
        // collide with the second managed key.
        let merge = merge_override(1, 10, &managed_suffixed, &managed_base);

        let error = run_plan_threads(
            &input,
            &[managed_base, managed_suffixed],
            std::slice::from_ref(&merge),
        )
        .expect_err("a key that is still duplicated must be a loud error");
        let message = error.to_string();
        assert!(
            message.contains(&suffixed),
            "the error did not name the duplicate key: {message}"
        );
    }
}
