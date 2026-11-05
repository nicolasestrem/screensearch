//! Override precedence (`03 §7h.5`): rename, merge and split applied **after** clustering,
//! plus the deletion/retention rule that makes them durable across runs.
//!
//! Rows are read in `(created_at, id)` order and applied in that order, so a later override of
//! the same kind beats an earlier one. For any session, an override-derived assignment beats
//! the clustered one.
//!
//! # Override-managed threads
//!
//! A stored thread is **override-managed** when any override row references its id (as
//! `thread_id` or `target_thread_id`) or when its `cluster_key` starts with
//! [`OVERRIDE_CLUSTER_KEY_PREFIX`]. Managed threads are **not re-clustered**: their sessions
//! are removed from the clustering input by [`crate::clustering::plan_threads`], and their
//! draft comes from the persisted row (membership, `cluster_key` and title from storage; times
//! and member confidences recomputed from the input where the session is present). This is
//! what keeps a merge or a split applied on later runs.
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
//!    sync members, delete named threads, rewrite merge rows).
//!
//! # Kind semantics (`03 §7h.5`)
//!
//! - **rename** sets `threads.title` only. The row's `thread_id` names a persisted thread; it
//!   is resolved to the current draft through `stored`'s id -> `cluster_key` map and skipped
//!   when that draft no longer exists, never applied to a different thread.
//! - **merge** folds the source's members into `target_thread_id` (the survivor) and removes
//!   the emptied source from the model. `payload.session_ids` is recorded evidence and is
//!   never used to decide the fold. The durable state has to survive two constraints at once:
//!   the `thread_overrides` foreign keys are `ON DELETE CASCADE`, so deleting a thread a row
//!   points at would delete that override, and clustering keeps recomputing the source
//!   component from the unchanged `session_entities` links. The rule is:
//!   * when no other override row references the source id, the source row is deleted and
//!     **this** merge row is rewritten to `(thread_id = target, target_thread_id = target)`, a
//!     self-merge marker. The target is then managed, its sessions are excluded from
//!     clustering, and the next run applies the self-merge as a no-op, so the fold is durable
//!     and the deleted source is not recreated;
//!   * when another override row still references the source id, the emptied source row is
//!     retained (and this row is not rewritten), because deleting it would cascade-delete
//!     that other override. The merge re-applies as a no-op each run because the source is
//!     empty and the target is managed.
//!
//!   This is the recorded resolution of the `03 §7h.5` "removes the emptied source thread"
//!   sentence versus the `ON DELETE CASCADE` foreign keys (`specs/06_PATCH_PLAN.md`, P5).
//! - **split** moves the payload's sessions out of wherever they currently are into a new
//!   thread seeded by the override: `cluster_key = override:<override id>`, a stable key that
//!   survives re-clustering. Its title, in order, is the source thread's title as it stands in
//!   the model (so an earlier rename in the same plan is respected), the stored source title,
//!   the title of the thread the first moved session currently belongs to, then the split
//!   `cluster_key`. Its times derive from the moved sessions, and a moved session keeps the
//!   member confidence it carried; a session outside every thread falls back to its best
//!   stored link confidence. The source row is kept in the plan even when it is emptied: the
//!   split row references it and a later run applies the same split again (the "clustering
//!   continues from the override" reading of `03 §7h.5`).
//! - Overrides referencing stored thread ids are resolved through `stored`'s id ->
//!   `cluster_key` map. Threads created by an earlier split have override-derived keys and
//!   are rebuilt by the same override each run, never re-clustered.
//!
//! The plan is deterministic: threads are sorted by `cluster_key`, delete ids are sorted, and
//! override rewrites are sorted by override id.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use traits::{
    GraphClusterInput, GraphClusterLink, GraphSessionInterval, StoredThread, ThreadDraft,
    ThreadMemberDraft, ThreadOverridePayload, ThreadOverrideRow, ThreadOverrideUpdate, ThreadPlan,
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

    fn from_stored(thread: &StoredThread) -> Self {
        Self {
            cluster_key: thread.cluster_key.clone(),
            title: thread.title.clone(),
            first_seen_at: thread.first_seen_at,
            last_seen_at: thread.last_seen_at,
            members: thread
                .sessions
                .iter()
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

/// Every stored thread id any override row points at.
fn referenced_thread_ids(overrides: &[ThreadOverrideRow]) -> BTreeSet<i64> {
    let mut ids = BTreeSet::new();
    for row in overrides {
        if let Some(thread_id) = row.thread_id {
            ids.insert(thread_id);
        }
        if let Some(target_thread_id) = row.target_thread_id {
            ids.insert(target_thread_id);
        }
    }
    ids
}

/// The stored thread ids that are override-managed: referenced by an override row, or carrying
/// an override-derived `cluster_key`.
fn managed_stored_ids(stored: &[StoredThread], overrides: &[ThreadOverrideRow]) -> BTreeSet<i64> {
    let referenced = referenced_thread_ids(overrides);
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
/// removes them from the clustering input so managed threads are never re-clustered.
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
/// disambiguation (`06` #40); sessions are never silently dropped.
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
        let mut thread = ModelThread::from_stored(stored_thread);
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
    let mut override_updates: Vec<ThreadOverrideUpdate> = Vec::new();

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
                    // A self-merge marker left by an earlier run: already applied.
                    continue;
                }
                if !model.contains_key(&source_key) || !model.contains_key(&target_key) {
                    continue;
                }
                let source = model
                    .remove(&source_key)
                    .expect("source thread membership checked above");
                let retained_title = source.title;
                let retained_first_seen_at = source.first_seen_at;
                let retained_last_seen_at = source.last_seen_at;
                let target = model
                    .get_mut(&target_key)
                    .expect("target thread membership checked above");
                for (session_id, confidence) in source.members {
                    target.members.entry(session_id).or_insert(confidence);
                }
                let referenced_elsewhere = overrides.iter().any(|other| {
                    other.id != row.id
                        && (other.thread_id == Some(source_id)
                            || other.target_thread_id == Some(source_id))
                });
                if referenced_elsewhere {
                    // Keep the emptied source row alive so the other override's foreign key
                    // survives, and never rewrite this row: it re-applies as a no-op.
                    model.insert(
                        source_key.clone(),
                        ModelThread {
                            cluster_key: source_key,
                            title: retained_title,
                            first_seen_at: retained_first_seen_at,
                            last_seen_at: retained_last_seen_at,
                            members: BTreeMap::new(),
                        },
                    );
                } else {
                    override_updates.push(ThreadOverrideUpdate {
                        override_id: row.id,
                        thread_id: target_id,
                        target_thread_id: target_id,
                    });
                }
            }
            ThreadOverridePayload::Split { session_ids, .. } => {
                let Some(source_id) = row.thread_id else {
                    continue;
                };
                let source_stored = stored_by_id.get(&source_id).copied();
                let source_key = source_stored.map(|thread| thread.cluster_key.clone());
                let first_moved_title = session_ids.first().and_then(|session_id| {
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
                        .or_insert_with(|| ModelThread::from_stored(thread));
                }
                let mut moved: Vec<(i64, f64)> = Vec::with_capacity(session_ids.len());
                for session_id in session_ids {
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
    override_updates.sort_by_key(|update| update.override_id);

    Ok(ThreadPlan {
        threads,
        delete_thread_ids,
        override_updates,
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
            .unwrap_or_else(|| panic!("session {session_id} is not in {}", thread.cluster_key))
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
    /// existing ids, delete the named ids, and apply the merge rewrites.
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
    /// model with an empty delete list and no repeated merge rewrites.
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
    fn merge_folds_the_source_members_and_names_the_source_for_deletion() {
        let input = three_components();
        let plan = plan_threads(&input, &[], &[]);
        let (stored, _) = apply_plan(&plan, &[], &[]);
        let alpha = stored_by_key(&stored, &cluster_key(&[101]));
        let beta = stored_by_key(&stored, &cluster_key(&[102]));

        let merge = merge_override(1, 10, beta, alpha);
        let plan = plan_threads(&input, &stored, std::slice::from_ref(&merge));
        assert_eq!(plan.threads.len(), 2, "{:?}", plan.threads);

        let merged = thread_by_key(&plan.threads, &alpha.cluster_key);
        let expected_members: Vec<i64> = stored
            .iter()
            .filter(|thread| {
                thread.cluster_key == alpha.cluster_key || thread.cluster_key == beta.cluster_key
            })
            .flat_map(|thread| thread.sessions.iter().map(|member| member.session_id))
            .collect();
        let mut expected_members = expected_members;
        expected_members.sort_unstable();
        assert_eq!(member_ids(merged), expected_members);
        assert_eq!(merged.title, ALPHA);
        assert!(plan
            .threads
            .iter()
            .all(|thread| thread.cluster_key != beta.cluster_key));
        assert_eq!(plan.delete_thread_ids, vec![beta.id]);
        assert_eq!(
            plan.override_updates,
            vec![ThreadOverrideUpdate {
                override_id: merge.id,
                thread_id: alpha.id,
                target_thread_id: alpha.id,
            }]
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
        let plan = plan_threads(&input, &stored, &[second.clone(), first.clone()]);

        let survivor = thread_by_key(&plan.threads, &gamma.cluster_key);
        let mut expected_members: Vec<i64> = [alpha, beta, gamma]
            .iter()
            .flat_map(|thread| thread.sessions.iter().map(|member| member.session_id))
            .collect();
        expected_members.sort_unstable();
        assert_eq!(member_ids(survivor), expected_members);
        assert_eq!(survivor.title, GAMMA);

        // The first merge's source is deleted; the first merge's target is retained empty
        // because the rewritten first merge row still references it.
        assert_eq!(plan.delete_thread_ids, vec![beta.id]);
        let retained = thread_by_key(&plan.threads, &alpha.cluster_key);
        assert!(retained.sessions.is_empty());
        assert_eq!(
            plan.override_updates,
            vec![ThreadOverrideUpdate {
                override_id: first.id,
                thread_id: alpha.id,
                target_thread_id: alpha.id,
            }]
        );

        // Re-planning the applied model is stable.
        let (stored_after, overrides_after) = apply_plan(&plan, &stored, &[second, first]);
        let replanned = plan_threads(&input, &stored_after, &overrides_after);
        assert_eq!(replanned.threads, plan.threads);
        assert!(replanned.delete_thread_ids.is_empty());
        assert!(replanned.override_updates.is_empty());
    }

    #[test]
    fn merge_retains_a_source_that_another_override_references() {
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
        assert_eq!(plan.delete_thread_ids, vec![beta.id]);
        assert_eq!(plan.override_updates.len(), 1);
        assert_eq!(plan.override_updates[0].override_id, 2);

        let split_thread = thread_by_key(&plan.threads, &split_cluster_key(3));
        assert_eq!(member_ids(split_thread), vec![6]);
        assert_eq!(split_thread.title, gamma.title);
        assert!(stored_after
            .iter()
            .any(|thread| thread.cluster_key == split_cluster_key(3)));
        assert_eq!(overrides_after.len(), 3);
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
        let input = hiatus_input();
        let base = cluster_key(&[10]);
        // Sessions 3,4 are held by an override-managed stored thread that persisted the
        // unsuffixed base key; sessions 1,2 re-cluster to the same base key.
        let managed = stored_thread_for(&input, 1, &base, BETA, &[3, 4], 0.95);
        let rename = rename_override(1, 10, managed.id, BETA);
        let overrides = std::slice::from_ref(&rename);
        let plan = plan_threads(&input, std::slice::from_ref(&managed), overrides);

        assert_eq!(plan.threads.len(), 2, "{:?}", plan.threads);
        let managed_thread = thread_by_key(&plan.threads, &base);
        assert_eq!(member_ids(managed_thread), vec![3, 4]);
        let fresh_thread = thread_by_key(&plan.threads, &format!("{base}#1"));
        assert_eq!(member_ids(fresh_thread), vec![1, 2]);
        assert!(plan.delete_thread_ids.is_empty());
        assert!(plan.override_updates.is_empty());

        // The applied model is stable: the managed key survives and the fresh thread keeps
        // its discriminator.
        let (stored_after, overrides_after) =
            apply_plan(&plan, std::slice::from_ref(&managed), overrides);
        let replanned = plan_threads(&input, &stored_after, &overrides_after);
        assert_eq!(replanned.threads, plan.threads);
        assert!(replanned.delete_thread_ids.is_empty());
        assert!(replanned.override_updates.is_empty());
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
        let overrides = vec![
            rename_override(1, 10, managed_base.id, ALPHA),
            rename_override(2, 20, managed_suffixed.id, BETA),
        ];

        let error = run_plan_threads(&input, &[managed_base, managed_suffixed], &overrides)
            .expect_err("a key that is still duplicated must be a loud error");
        let message = error.to_string();
        assert!(
            message.contains(&suffixed),
            "the error did not name the duplicate key: {message}"
        );
    }
}
