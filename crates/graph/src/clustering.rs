//! Deterministic thread clustering (`03 §7h.4`).
//!
//! A thread is a connected component of the session-sharing relation:
//!
//! - two sessions are joined when they share at least one entity whose
//!   `session_entities.confidence` is at least [`SEED_CONFIDENCE_MIN`] on **both** sides
//!   (`03 §7h.4`: "share at least one entity with confidence >= 0.7");
//! - the gap is measured between the sessions' `last_seen_at` values (the link rows carry
//!   them in `traits::GraphClusterLink`);
//! - the edge weight is `1.0` up to [`FULL_WEIGHT_GAP_MS`] (7 days) and decays linearly to
//!   `0.0` at [`MAX_GAP_MS`] (30 days); a gap beyond 30 days is no edge, while a gap of
//!   exactly 30 days is an edge of weight `0.0`.
//!
//! Membership, not weight, decides which sessions are in a thread: the 30-day boundary is
//! the only threshold, so a zero-weight edge at exactly 30 days still joins its sessions.
//! The weight orders titles and is not a second membership rule; this is stated so nobody
//! hunts for a hidden threshold (`docs/kickoffs/P5.md`).
//!
//! Determinism: the same input (with or without shuffled session order) yields the same
//! threads, the same `cluster_key` values and the same titles.
//!
//! Decisions this module implements (recorded in `specs/05_BUILD_REVIEW.md`):
//!
//! - Only sessions with at least one strong link (`>= 0.7`) participate; a component with a
//!   single session is not a thread (threads exist where sessions connect). The override
//!   planner can still create a thread from explicitly split sessions.
//! - `cluster_key` is [`CLUSTER_KEY_PREFIX`] plus the sorted, deduplicated seed entity ids
//!   joined by `,` (`seed:12,40,91`). The seeds are the entities that produced at least one
//!   in-window pair inside that component; an entity whose only other strong session is
//!   outside the 30-day window does not name the thread.
//! - **Collision disambiguation (`06` #40).** The base key alone is not unique: the same strong
//!   entity can seed two components whose every cross-component session pair is beyond the
//!   30-day window, for example the same project resumed after a hiatus longer than 30 days.
//!   When more than one component in one run shares a base key, every component of that
//!   collision group appends `#<min member session id>` to its key (`seed:12#7`). Components
//!   are disjoint, so their minimum member session ids differ and the suffixed keys are
//!   unique and deterministic; components whose base key is unique keep the spec's
//!   `seed:<ids>` form so thread ids stay stable in normal use. A fresh draft whose key
//!   collides with an override-managed stored thread is disambiguated the same way by
//!   `crate::overrides`, and a key that is still duplicated after that is a hard planning
//!   error instead of a silently dropped component.
//! - `title` is the dominant seed entity's `canonical_name`, where dominant is the highest
//!   summed edge weight over the component (ties break to the lowest entity id). The sum for
//!   one entity is the entity's own edge weights over the component's session pairs, so a pair
//!   that shares two seed entities contributes to both sums.
//! - `thread_sessions.confidence` is the maximum link confidence the member session holds for
//!   any of the component's seed entities.
//! - `first_seen_at` is the earliest member `started_at`; `last_seen_at` is the latest member
//!   end, with an open session (`ended_at` NULL) ending at the input's `now_ms`.
//!
//! [`plan_threads`] is the entry point the job layer uses. Before clustering it removes every
//! session that already belongs to an override-managed stored thread (`crate::overrides`): a
//! managed thread keeps its persisted membership and is never re-clustered, so a merge or a
//! split stays applied on later runs. `cluster_sessions` itself is stateless and has no
//! notion of overrides. `plan_threads` returns an error when the overrides layer reports a
//! key that is still duplicated after disambiguation (`06` #40); it never drops sessions.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use traits::{
    GraphClusterInput, GraphSessionInterval, StoredThread, ThreadDraft, ThreadMemberDraft,
    ThreadOverrideRow, ThreadPlan,
};

/// Minimum `session_entities.confidence` on both sides of a shared entity for an edge
/// (`03 §7h.4`). Below this, an entity exists but cannot seed a thread.
pub const SEED_CONFIDENCE_MIN: f64 = 0.7;

/// Milliseconds in one day, for the decay window.
pub const DAY_MS: i64 = 86_400_000;

/// Full edge weight up to this gap (7 days, `03 §7h.4`).
pub const FULL_WEIGHT_GAP_MS: i64 = 7 * DAY_MS;

/// No edge at or beyond this gap (30 days, `03 §7h.4`).
pub const MAX_GAP_MS: i64 = 30 * DAY_MS;

/// Prefix of every clusterer-derived `threads.cluster_key` (`seed:<id>,<id>`).
pub const CLUSTER_KEY_PREFIX: &str = "seed:";

/// The decaying edge weight for a gap between two `last_seen_at` values: 1.0 up to 7 days,
/// linear to 0.0 at 30 days, 0.0 beyond. Negative gaps (overlapping/out-of-order evidence)
/// are treated as 0.
pub fn edge_weight(gap_ms: i64) -> f64 {
    if gap_ms <= FULL_WEIGHT_GAP_MS {
        return 1.0;
    }
    if gap_ms >= MAX_GAP_MS {
        return 0.0;
    }
    let span = (MAX_GAP_MS - FULL_WEIGHT_GAP_MS) as f64;
    let past = (gap_ms - FULL_WEIGHT_GAP_MS) as f64;
    1.0 - past / span
}

/// Serializes a component's seed entity ids into its deterministic `cluster_key`.
pub fn cluster_key(seed_entity_ids: &[i64]) -> String {
    let mut ids = seed_entity_ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let joined = ids
        .iter()
        .map(i64::to_string)
        .collect::<Vec<String>>()
        .join(",");
    format!("{CLUSTER_KEY_PREFIX}{joined}")
}

/// Appends the deterministic `#<min member session id>` discriminator to every component whose
/// base key is shared by another component in the same run (`06` #40).
///
/// Components are disjoint, so their minimum member session ids are distinct and the suffixed
/// keys are unique. A base key that occurs exactly once keeps the spec's `seed:<ids>` form.
fn disambiguate_colliding_keys(threads: &mut [ThreadDraft]) {
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, thread) in threads.iter().enumerate() {
        groups
            .entry(thread.cluster_key.clone())
            .or_default()
            .push(index);
    }
    for indices in groups.values() {
        if indices.len() < 2 {
            continue;
        }
        for index in indices {
            let min_member = threads[*index]
                .sessions
                .iter()
                .map(|member| member.session_id)
                .min()
                .unwrap_or(0);
            threads[*index].cluster_key = format!("{}#{min_member}", threads[*index].cluster_key);
        }
    }
}

/// One strong session-entity link, deduplicated per `(entity, session)` pair.
#[derive(Debug, Clone, Copy)]
struct SeedLink<'a> {
    last_seen_at: i64,
    confidence: f64,
    canonical_name: &'a str,
}

/// Groups the strong (`>= SEED_CONFIDENCE_MIN`) links by entity and session.
///
/// Links to sessions without an interval are ignored (their times are unknown, so they cannot
/// be a thread member); duplicate rows for the same `(entity, session)` keep the highest
/// confidence, ties keeping the earlier `last_seen_at`, so the map is independent of input
/// order.
fn seed_links(input: &GraphClusterInput) -> BTreeMap<i64, BTreeMap<i64, SeedLink<'_>>> {
    let known_sessions: BTreeSet<i64> = input
        .sessions
        .iter()
        .map(|session| session.session_id)
        .collect();
    let mut by_entity: BTreeMap<i64, BTreeMap<i64, SeedLink<'_>>> = BTreeMap::new();
    for link in &input.links {
        if link.confidence < SEED_CONFIDENCE_MIN || !known_sessions.contains(&link.session_id) {
            continue;
        }
        let candidate = SeedLink {
            last_seen_at: link.last_seen_at,
            confidence: link.confidence,
            canonical_name: link.canonical_name.as_str(),
        };
        match by_entity
            .entry(link.entity_id)
            .or_default()
            .entry(link.session_id)
        {
            Entry::Occupied(mut entry) => {
                let current = entry.get_mut();
                let better = candidate.confidence > current.confidence
                    || (candidate.confidence == current.confidence
                        && candidate.last_seen_at < current.last_seen_at);
                if better {
                    *current = candidate;
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(candidate);
            }
        }
    }
    by_entity
}

/// A union-find over session positions, used to build the connected components.
#[derive(Debug)]
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, mut index: usize) -> usize {
        while self.parent[index] != index {
            self.parent[index] = self.parent[self.parent[index]];
            index = self.parent[index];
        }
        index
    }

    fn union(&mut self, first: usize, second: usize) {
        let (mut first, mut second) = (self.find(first), self.find(second));
        if first == second {
            return;
        }
        if self.rank[first] < self.rank[second] {
            std::mem::swap(&mut first, &mut second);
        }
        self.parent[second] = first;
        if self.rank[first] == self.rank[second] {
            self.rank[first] += 1;
        }
    }
}

/// Clusters the input's sessions into threads, before overrides (`03 §7h.4`).
///
/// The returned drafts are sorted by `cluster_key` so the output is stable. Sessions without
/// a strong link are absent from every thread.
pub fn cluster_sessions(input: &GraphClusterInput) -> Vec<ThreadDraft> {
    let by_entity = seed_links(input);

    // Sessions with at least one strong link are the only cluster candidates.
    let mut seed_sessions: BTreeSet<i64> = BTreeSet::new();
    for sessions in by_entity.values() {
        seed_sessions.extend(sessions.keys().copied());
    }
    let session_ids: Vec<i64> = seed_sessions.into_iter().collect();
    let position: BTreeMap<i64, usize> = session_ids
        .iter()
        .enumerate()
        .map(|(index, session_id)| (*session_id, index))
        .collect();

    // Join every pair that shares an entity within the 30-day boundary (inclusive; a gap of
    // exactly MAX_GAP_MS is a zero-weight edge, which still joins).
    let mut union_find = UnionFind::new(session_ids.len());
    for sessions in by_entity.values() {
        let entries: Vec<(i64, SeedLink<'_>)> = sessions
            .iter()
            .map(|(session_id, link)| (*session_id, *link))
            .collect();
        for (index, (first_id, first)) in entries.iter().enumerate() {
            for (second_id, second) in entries.iter().skip(index + 1) {
                if first.last_seen_at.abs_diff(second.last_seen_at) <= MAX_GAP_MS as u64 {
                    union_find.union(position[first_id], position[second_id]);
                }
            }
        }
    }

    let mut components: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for session_id in &session_ids {
        let root = union_find.find(position[session_id]);
        components.entry(root).or_default().push(*session_id);
    }

    let sessions: BTreeMap<i64, &GraphSessionInterval> = input
        .sessions
        .iter()
        .map(|session| (session.session_id, session))
        .collect();
    let mut entities_by_session: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for (entity_id, entity_sessions) in &by_entity {
        for session_id in entity_sessions.keys() {
            entities_by_session
                .entry(*session_id)
                .or_default()
                .insert(*entity_id);
        }
    }

    let mut threads = Vec::new();
    for members in components.values() {
        if members.len() < 2 {
            continue;
        }
        // An entity names the thread only when it produced at least one in-window pair
        // between members; its score is the summed edge weight it contributed.
        let mut candidate_entities: BTreeSet<i64> = BTreeSet::new();
        for session_id in members {
            if let Some(entities) = entities_by_session.get(session_id) {
                candidate_entities.extend(entities.iter().copied());
            }
        }
        let mut seed_ids: Vec<i64> = Vec::new();
        let mut dominant: Option<(i64, f64)> = None;
        for entity_id in candidate_entities {
            let Some(entity_sessions) = by_entity.get(&entity_id) else {
                continue;
            };
            let links: Vec<(i64, SeedLink<'_>)> = members
                .iter()
                .filter_map(|session_id| {
                    entity_sessions
                        .get(session_id)
                        .map(|link| (*session_id, *link))
                })
                .collect();
            let mut score = 0.0_f64;
            let mut formed_edge = false;
            for (index, (_, first)) in links.iter().enumerate() {
                for (_, second) in links.iter().skip(index + 1) {
                    let gap = first.last_seen_at.abs_diff(second.last_seen_at);
                    if gap <= MAX_GAP_MS as u64 {
                        formed_edge = true;
                        score += edge_weight(gap as i64);
                    }
                }
            }
            if !formed_edge {
                continue;
            }
            seed_ids.push(entity_id);
            if dominant.is_none_or(|(_, best_score)| score > best_score) {
                dominant = Some((entity_id, score));
            }
        }
        let Some((dominant_id, _)) = dominant else {
            continue;
        };
        let title = by_entity
            .get(&dominant_id)
            .and_then(|entity_sessions| entity_sessions.values().next())
            .map(|link| link.canonical_name.to_string())
            .unwrap_or_default();

        let mut thread_members = Vec::new();
        let mut first_seen_at = i64::MAX;
        let mut last_seen_at = i64::MIN;
        for session_id in members {
            let mut confidence: Option<f64> = None;
            for entity_id in &seed_ids {
                if let Some(link) = by_entity
                    .get(entity_id)
                    .and_then(|entity_sessions| entity_sessions.get(session_id))
                {
                    confidence = Some(
                        confidence
                            .map_or(link.confidence, |current: f64| current.max(link.confidence)),
                    );
                }
            }
            let (Some(confidence), Some(session)) = (confidence, sessions.get(session_id)) else {
                continue;
            };
            thread_members.push(ThreadMemberDraft {
                session_id: *session_id,
                confidence,
            });
            first_seen_at = first_seen_at.min(session.started_at);
            last_seen_at = last_seen_at.max(session.ended_at.unwrap_or(input.now_ms));
        }
        if thread_members.is_empty() {
            continue;
        }
        threads.push(ThreadDraft {
            cluster_key: cluster_key(&seed_ids),
            title,
            first_seen_at,
            last_seen_at,
            sessions: thread_members,
        });
    }
    disambiguate_colliding_keys(&mut threads);
    threads.sort_by(|first, second| first.cluster_key.cmp(&second.cluster_key));
    threads
}

/// Builds the complete plan for one cluster run: cluster, then apply overrides, then diff
/// against the persisted threads (`03 §7h.4/.5`).
///
/// Sessions that belong to an override-managed stored thread are not clustered: the managed
/// thread keeps its persisted membership (and, after a merge, its folded members) instead of
/// being rebuilt from links every run (`crate::overrides`).
///
/// The result is an error when a thread key is still duplicated after collision
/// disambiguation (`06` #40); the plan never silently drops a component.
pub fn plan_threads(
    input: &GraphClusterInput,
    stored: &[StoredThread],
    overrides: &[ThreadOverrideRow],
) -> anyhow::Result<ThreadPlan> {
    let managed_sessions = crate::overrides::managed_session_ids(stored, overrides);
    let cluster_input = GraphClusterInput {
        now_ms: input.now_ms,
        sessions: input
            .sessions
            .iter()
            .filter(|session| !managed_sessions.contains(&session.session_id))
            .cloned()
            .collect(),
        links: input
            .links
            .iter()
            .filter(|link| !managed_sessions.contains(&link.session_id))
            .cloned()
            .collect(),
    };
    let clustered = cluster_sessions(&cluster_input);
    crate::overrides::apply(clustered, stored, overrides, input)
}

#[cfg(test)]
mod tests {
    use super::*;

    use traits::{EntityKind, GraphClusterLink};

    const HOUR_MS: i64 = 3_600_000;
    const BASE_MS: i64 = 1_000 * DAY_MS;
    const NOW_MS: i64 = BASE_MS + 100 * DAY_MS;
    const ALPHA: &str = "acme/alpha";
    const BETA: &str = "acme/beta";

    fn session(session_id: i64, started_at: i64, ended_at: Option<i64>) -> GraphSessionInterval {
        GraphSessionInterval {
            session_id,
            started_at,
            ended_at,
        }
    }

    fn link(
        session_id: i64,
        entity_id: i64,
        canonical_name: &str,
        confidence: f64,
        last_seen_at: i64,
    ) -> GraphClusterLink {
        GraphClusterLink {
            session_id,
            entity_id,
            entity_kind: EntityKind::Repo,
            canonical_name: canonical_name.to_string(),
            confidence,
            last_seen_at,
        }
    }

    fn input(
        now_ms: i64,
        sessions: Vec<GraphSessionInterval>,
        links: Vec<GraphClusterLink>,
    ) -> GraphClusterInput {
        GraphClusterInput {
            now_ms,
            sessions,
            links,
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

    #[test]
    fn clustering_is_deterministic_for_the_same_input_and_a_shuffled_one() {
        let sessions = vec![
            session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(2, BASE_MS + DAY_MS, Some(BASE_MS + DAY_MS + HOUR_MS)),
            session(
                3,
                BASE_MS + 2 * DAY_MS,
                Some(BASE_MS + 2 * DAY_MS + HOUR_MS),
            ),
        ];
        let links = vec![
            link(1, 10, ALPHA, 0.95, BASE_MS + HOUR_MS),
            link(2, 10, ALPHA, 0.90, BASE_MS + DAY_MS + HOUR_MS),
            link(2, 11, BETA, 0.80, BASE_MS + DAY_MS + HOUR_MS),
            link(3, 11, BETA, 0.85, BASE_MS + 2 * DAY_MS + HOUR_MS),
        ];

        let first = cluster_sessions(&input(NOW_MS, sessions.clone(), links.clone()));
        let second = cluster_sessions(&input(NOW_MS, sessions.clone(), links.clone()));
        assert_eq!(first, second);

        // Reverse both vectors: a different input order, the same clustering problem.
        let mut shuffled_sessions = sessions;
        shuffled_sessions.reverse();
        let mut shuffled_links = links;
        shuffled_links.reverse();
        let shuffled = cluster_sessions(&input(NOW_MS, shuffled_sessions, shuffled_links));
        assert_eq!(first, shuffled, "session order changed the threads");

        assert_eq!(first.len(), 1, "{first:?}");
        let thread = &first[0];
        // Entity 10 is the lowest id among the two seed entities with equal weight.
        assert_eq!(thread.cluster_key, cluster_key(&[10, 11]));
        assert_eq!(thread.title, ALPHA);
        assert_eq!(member_ids(thread), vec![1, 2, 3]);
    }

    #[test]
    fn a_hiatus_keeps_two_components_with_the_same_seed_entity_distinct() {
        // Entity 10 seeds sessions 1,2 at days 0/2 and sessions 3,4 at days 45/47: every
        // cross-part pair is beyond 30 days, so both components carry the base key seed:10.
        let sessions = vec![
            session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(
                2,
                BASE_MS + 2 * DAY_MS,
                Some(BASE_MS + 2 * DAY_MS + HOUR_MS),
            ),
            session(
                3,
                BASE_MS + 45 * DAY_MS,
                Some(BASE_MS + 45 * DAY_MS + HOUR_MS),
            ),
            session(
                4,
                BASE_MS + 47 * DAY_MS,
                Some(BASE_MS + 47 * DAY_MS + HOUR_MS),
            ),
        ];
        let links = vec![
            link(1, 10, ALPHA, 0.95, BASE_MS + HOUR_MS),
            link(2, 10, ALPHA, 0.95, BASE_MS + 2 * DAY_MS + HOUR_MS),
            link(3, 10, ALPHA, 0.95, BASE_MS + 45 * DAY_MS + HOUR_MS),
            link(4, 10, ALPHA, 0.95, BASE_MS + 47 * DAY_MS + HOUR_MS),
        ];
        let now_ms = BASE_MS + 100 * DAY_MS;
        let first = cluster_sessions(&input(now_ms, sessions.clone(), links.clone()));
        assert_eq!(first.len(), 2, "{first:?}");
        let base = cluster_key(&[10]);
        let expected_keys = vec![format!("{base}#1"), format!("{base}#3")];
        let keys: Vec<String> = first
            .iter()
            .map(|thread| thread.cluster_key.clone())
            .collect();
        assert_eq!(keys, expected_keys);

        let mut members: Vec<i64> = first.iter().flat_map(member_ids).collect();
        members.sort_unstable();
        assert_eq!(members, vec![1, 2, 3, 4]);
        assert!(first.iter().all(|thread| thread.title == ALPHA));

        // Shuffled input order yields the same threads and keys.
        let mut shuffled_sessions = sessions;
        shuffled_sessions.reverse();
        let mut shuffled_links = links;
        shuffled_links.reverse();
        let shuffled = cluster_sessions(&input(now_ms, shuffled_sessions, shuffled_links));
        assert_eq!(first, shuffled);
    }

    #[test]
    fn edge_weight_is_full_to_seven_days_and_decays_to_zero_at_thirty() {
        assert_eq!(edge_weight(0), 1.0);
        assert_eq!(edge_weight(FULL_WEIGHT_GAP_MS), 1.0);
        let midpoint = FULL_WEIGHT_GAP_MS + (MAX_GAP_MS - FULL_WEIGHT_GAP_MS) / 2;
        let expected_midpoint =
            1.0 - (midpoint - FULL_WEIGHT_GAP_MS) as f64 / (MAX_GAP_MS - FULL_WEIGHT_GAP_MS) as f64;
        assert_eq!(edge_weight(midpoint), expected_midpoint);
        assert_eq!(edge_weight(MAX_GAP_MS), 0.0);
        assert_eq!(edge_weight(MAX_GAP_MS + DAY_MS), 0.0);
        assert_eq!(edge_weight(-DAY_MS), 1.0);
    }

    #[test]
    fn a_thirty_day_gap_still_joins_sessions_and_beyond_it_does_not() {
        let sessions = vec![
            session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(2, BASE_MS + HOUR_MS, Some(BASE_MS + 2 * HOUR_MS)),
        ];
        let at_boundary = input(
            NOW_MS,
            sessions.clone(),
            vec![
                link(1, 10, ALPHA, 0.9, BASE_MS),
                link(2, 10, ALPHA, 0.9, BASE_MS + MAX_GAP_MS),
            ],
        );
        let threads = cluster_sessions(&at_boundary);
        assert_eq!(threads.len(), 1, "{threads:?}");
        assert_eq!(member_ids(&threads[0]), vec![1, 2]);

        let beyond = input(
            NOW_MS,
            sessions,
            vec![
                link(1, 10, ALPHA, 0.9, BASE_MS),
                link(2, 10, ALPHA, 0.9, BASE_MS + MAX_GAP_MS + 1),
            ],
        );
        assert!(cluster_sessions(&beyond).is_empty());
    }

    #[test]
    fn only_links_at_or_above_the_seed_confidence_join_sessions() {
        let sessions = vec![
            session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(2, BASE_MS, Some(BASE_MS + HOUR_MS)),
        ];
        let below = SEED_CONFIDENCE_MIN - 0.001;
        let strong = input(
            NOW_MS,
            sessions.clone(),
            vec![
                link(1, 10, ALPHA, SEED_CONFIDENCE_MIN, BASE_MS),
                link(2, 10, ALPHA, SEED_CONFIDENCE_MIN, BASE_MS),
            ],
        );
        assert_eq!(cluster_sessions(&strong).len(), 1);

        let weak = input(
            NOW_MS,
            sessions.clone(),
            vec![
                link(1, 10, ALPHA, below, BASE_MS),
                link(2, 10, ALPHA, below, BASE_MS),
            ],
        );
        assert!(cluster_sessions(&weak).is_empty());

        // One side below the minimum is not a seed link for that session.
        let half = input(
            NOW_MS,
            sessions,
            vec![
                link(1, 10, ALPHA, SEED_CONFIDENCE_MIN, BASE_MS),
                link(2, 10, ALPHA, below, BASE_MS),
            ],
        );
        assert!(cluster_sessions(&half).is_empty());
    }

    #[test]
    fn a_single_session_component_is_not_a_thread() {
        let single = input(
            NOW_MS,
            vec![session(1, BASE_MS, Some(BASE_MS + HOUR_MS))],
            vec![link(1, 10, ALPHA, 0.95, BASE_MS)],
        );
        assert!(cluster_sessions(&single).is_empty());
    }

    #[test]
    fn title_is_the_dominant_seed_entity_and_ties_break_to_the_lowest_id() {
        let sessions = vec![
            session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(2, BASE_MS, Some(BASE_MS + HOUR_MS)),
        ];
        // Equal gaps for both entities: the two scores tie, so the lowest entity id wins.
        let tied = cluster_sessions(&input(
            NOW_MS,
            sessions.clone(),
            vec![
                link(1, 10, ALPHA, 0.9, BASE_MS),
                link(2, 10, ALPHA, 0.9, BASE_MS),
                link(1, 20, BETA, 0.9, BASE_MS),
                link(2, 20, BETA, 0.9, BASE_MS),
            ],
        ));
        assert_eq!(tied.len(), 1);
        assert_eq!(tied[0].title, ALPHA);
        assert_eq!(tied[0].cluster_key, cluster_key(&[10, 20]));

        // Entity 20 keeps a full-weight pair while entity 10 has decayed: entity 20 dominates.
        let decayed_gap = 20 * DAY_MS;
        assert!(edge_weight(decayed_gap) < edge_weight(0));
        let dominated = cluster_sessions(&input(
            NOW_MS,
            sessions,
            vec![
                link(1, 10, ALPHA, 0.9, BASE_MS),
                link(2, 10, ALPHA, 0.9, BASE_MS + decayed_gap),
                link(1, 20, BETA, 0.9, BASE_MS),
                link(2, 20, BETA, 0.9, BASE_MS),
            ],
        ));
        assert_eq!(dominated.len(), 1);
        assert_eq!(dominated[0].title, BETA);
    }

    #[test]
    fn thread_member_confidence_is_the_maximum_over_the_seed_links() {
        let first_session_links = [(10, ALPHA, 0.80), (20, BETA, 0.95)];
        let second_session_links = [(10, ALPHA, 0.75), (20, BETA, 0.72)];
        let mut links = Vec::new();
        for (entity_id, canonical_name, confidence) in first_session_links {
            links.push(link(1, entity_id, canonical_name, confidence, BASE_MS));
        }
        for (entity_id, canonical_name, confidence) in second_session_links {
            links.push(link(2, entity_id, canonical_name, confidence, BASE_MS));
        }
        let threads = cluster_sessions(&input(
            NOW_MS,
            vec![
                session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
                session(2, BASE_MS, Some(BASE_MS + HOUR_MS)),
            ],
            links,
        ));
        assert_eq!(threads.len(), 1, "{threads:?}");
        let expected_first = first_session_links
            .iter()
            .map(|(_, _, confidence)| *confidence)
            .fold(f64::MIN, f64::max);
        let expected_second = second_session_links
            .iter()
            .map(|(_, _, confidence)| *confidence)
            .fold(f64::MIN, f64::max);
        assert_eq!(member_confidence(&threads[0], 1), expected_first);
        assert_eq!(member_confidence(&threads[0], 2), expected_second);
    }

    #[test]
    fn title_score_sums_every_edge_an_entity_contributes() {
        // Three sessions share entity 20 (three pairs), only two share entity 10 (one pair),
        // so the higher entity id dominates by summed edge weight.
        let sessions = vec![
            session(1, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(2, BASE_MS, Some(BASE_MS + HOUR_MS)),
            session(3, BASE_MS, Some(BASE_MS + HOUR_MS)),
        ];
        let links = vec![
            link(1, 10, ALPHA, 0.9, BASE_MS),
            link(2, 10, ALPHA, 0.9, BASE_MS),
            link(1, 20, BETA, 0.9, BASE_MS),
            link(2, 20, BETA, 0.9, BASE_MS),
            link(3, 20, BETA, 0.9, BASE_MS),
        ];
        let threads = cluster_sessions(&input(NOW_MS, sessions, links));
        assert_eq!(threads.len(), 1, "{threads:?}");
        let alpha_score = edge_weight(0);
        let beta_score = 3.0 * edge_weight(0);
        assert!(beta_score > alpha_score);
        assert_eq!(threads[0].title, BETA);
        assert_eq!(threads[0].cluster_key, cluster_key(&[10, 20]));
    }

    #[test]
    fn thread_times_span_the_members_and_an_open_session_ends_at_now() {
        let first_start = BASE_MS;
        let first_end = BASE_MS + 2 * HOUR_MS;
        let second_start = BASE_MS + HOUR_MS;
        let now_ms = BASE_MS + 5 * HOUR_MS;
        let threads = cluster_sessions(&input(
            now_ms,
            vec![
                session(1, first_start, Some(first_end)),
                session(2, second_start, None),
            ],
            vec![
                link(1, 10, ALPHA, 0.9, BASE_MS),
                link(2, 10, ALPHA, 0.9, BASE_MS),
            ],
        ));
        assert_eq!(threads.len(), 1, "{threads:?}");
        assert_eq!(threads[0].first_seen_at, first_start.min(second_start));
        assert_eq!(threads[0].last_seen_at, now_ms.max(first_end));
    }
}
