//! Work-graph domain types and cross-crate data contracts (`03 §7h`, 0.4.5 P5).
//!
//! The graph is *derived-but-persisted* (`03 §7h`): every table is recomputable from
//! frames, text and sessions, and deleting it loses no evidence. This module holds the
//! plain data exchanged between the three P5 implementations:
//!
//! - `graph` (pure rules: extraction, dedup identity, clustering, override precedence);
//! - `store` (SQL over the six `03 §4` tables, read and write side);
//! - `kernel` (job bodies, the backfill cursor and the idle scheduler).
//!
//! Nothing here performs I/O, reads a clock, or knows about a database. Times are unix
//! epoch milliseconds supplied by the caller (`03 §7h`).

use serde::{Deserialize, Serialize};

use crate::sessions::SessionHost;

/// The closed entity kinds (`03 §7h.1`, 0.4.5 D8): six values, no `topic` and no people.
///
/// `App` is *never* produced by heuristic extraction (`03 §7h.2` has no app-name source);
/// it appears only when the optional normalization pass promotes a topic-shaped name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Repo,
    File,
    Domain,
    Document,
    Ref,
    App,
}

impl EntityKind {
    /// The `entities.kind` token; the closed CHECK set in `03 §4`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Repo => "repo",
            Self::File => "file",
            Self::Domain => "domain",
            Self::Document => "document",
            Self::Ref => "ref",
            Self::App => "app",
        }
    }

    /// Parses a persisted `entities.kind` token. `None` for a token outside the closed set.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "repo" => Some(Self::Repo),
            "file" => Some(Self::File),
            "domain" => Some(Self::Domain),
            "document" => Some(Self::Document),
            "ref" => Some(Self::Ref),
            "app" => Some(Self::App),
            _ => None,
        }
    }
}

/// `entities.source` provenance (`03 §4` CHECK `('heuristic','llm')`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntitySource {
    /// Deterministic rule-based extraction (`03 §7h.2`, never model-dependent).
    Heuristic,
    /// Proposed by the optional normalization pass; additive only.
    Llm,
}

impl EntitySource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heuristic => "heuristic",
            Self::Llm => "llm",
        }
    }
}

/// `entity_aliases.source` provenance (`03 §4` CHECK `('heuristic','llm','user')`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AliasSource {
    Heuristic,
    Llm,
    User,
}

impl AliasSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Heuristic => "heuristic",
            Self::Llm => "llm",
            Self::User => "user",
        }
    }
}

/// A persisted `entities` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: i64,
    pub kind: EntityKind,
    pub canonical_name: String,
    pub normalized_key: String,
    pub source: EntitySource,
    /// The extraction rule's own confidence, never invented or smoothed (`03 §7h.2`).
    pub confidence: f64,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

/// One heuristic hit for one frame (`03 §7h.2`).
///
/// `canonical_name` is the display form this hit saw; it becomes the row's canonical name
/// only if this hit created the row (`03 §7h.2`: "the display form of the first hit that
/// created the row"). `normalized_key` is the dedup identity (`UNIQUE(kind, normalized_key)`).
/// `aliases` are additive display forms recorded with `source='heuristic'` when the canonical
/// name alone is not literally present in the captured text (for example a `#42` reference
/// derived from a `/pull/42` URL path); they keep the invention metric truthful.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphHit {
    pub kind: EntityKind,
    pub canonical_name: String,
    pub normalized_key: String,
    pub confidence: f64,
    pub aliases: Vec<String>,
}

/// The session context a frame belongs to, used to interpret window titles (`03 §7h.2`
/// reads tool recognition as input). `ended_at` NULL means the session is still open.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphSessionInput {
    pub session_id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub tool: Option<String>,
    pub host: Option<SessionHost>,
}

impl GraphSessionInput {
    /// `03 §7h.3`: a hit links to the session whose `[started_at, ended_at)` contains the
    /// hit's timestamp. An open session (`ended_at` NULL) is open-ended.
    pub fn contains(&self, at_ms: i64) -> bool {
        at_ms >= self.started_at && self.ended_at.is_none_or(|end| at_ms < end)
    }
}

/// Everything heuristic extraction may read for one frame (`03 §7h.2`): its stored context,
/// its filtered content text, and the session it falls into (if any).
#[derive(Debug, Clone, PartialEq)]
pub struct GraphFrameInput {
    pub frame_id: i64,
    pub captured_at: i64,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub browser_url: Option<String>,
    pub content_text: Option<String>,
    /// The session whose interval contains `captured_at`, as resolved by the store.
    pub session: Option<GraphSessionInput>,
}

/// A session-entity link candidate for clustering: the strong link rows plus the entity
/// display data needed for titles (`03 §7h.4`).
#[derive(Debug, Clone, PartialEq)]
pub struct GraphClusterLink {
    pub session_id: i64,
    pub entity_id: i64,
    pub entity_kind: EntityKind,
    pub canonical_name: String,
    /// `session_entities.confidence`; only links at or above 0.7 seed clusters.
    pub confidence: f64,
    /// `session_entities.last_seen_at`, the interval end used for the decay window.
    pub last_seen_at: i64,
}

/// One session's exactly-recomputed link to one entity (`03 §7h.3`).
///
/// Used by the ownership reconciliation pass (`Store::graph_replace_session_links`): the kernel
/// re-extracts every frame of the session, aggregates the hits, and the store replaces the
/// session's link rows with these values in one transaction. Unlike the incremental
/// [`GraphHit`] path, `hit_count`, `confidence` and the seen window are absolute here, so a
/// rebuild is exact and cannot inflate an existing link.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphSessionLinkAggregate {
    pub kind: EntityKind,
    pub canonical_name: String,
    pub normalized_key: String,
    /// Maximum rule confidence among the contributing frames.
    pub confidence: f64,
    /// Number of distinct session frames that produced this entity.
    pub hit_count: u32,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub aliases: Vec<String>,
}

/// A session as clustering sees it (`03 §7h.4`). `ended_at` NULL is an open session and is
/// treated as ending at the input's `now_ms` for time-derived thread fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSessionInterval {
    pub session_id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// The complete deterministic input to one clustering run.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphClusterInput {
    pub now_ms: i64,
    pub sessions: Vec<GraphSessionInterval>,
    pub links: Vec<GraphClusterLink>,
}

/// One session's membership in a planned thread, with the thread-member confidence
/// (`thread_sessions.confidence`).
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadMemberDraft {
    pub session_id: i64,
    pub confidence: f64,
}

/// A planned `threads` row: the clusterer or an override produced it, and the store
/// upserts it by `cluster_key` (`UNIQUE`, `03 §4`).
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadDraft {
    pub cluster_key: String,
    pub title: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub sessions: Vec<ThreadMemberDraft>,
}

/// One persisted thread and its current members, read before planning (`03 §7h.4`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredThread {
    pub id: i64,
    pub cluster_key: String,
    pub title: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub sessions: Vec<ThreadMemberDraft>,
}

/// `thread_overrides.kind` (`03 §4` CHECK `('rename','merge','split')`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadOverrideKind {
    Rename,
    Merge,
    Split,
}

impl ThreadOverrideKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rename => "rename",
            Self::Merge => "merge",
            Self::Split => "split",
        }
    }

    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "rename" => Some(Self::Rename),
            "merge" => Some(Self::Merge),
            "split" => Some(Self::Split),
            _ => None,
        }
    }
}

/// The typed JSON payload of a `thread_overrides` row (`03 §7h.5`). The table CHECK
/// constrains which thread ids each kind may carry; these variants add the payload shape.
///
/// The JSON is **untagged**, matching the spec's documented forms exactly: `{"title":…}`,
/// `{"session_ids":[…]}` and `{"thread_id":…,"session_ids":[…]}`. The variants are declared
/// most-specific-first because serde's untagged deserializer ignores unknown fields: `Split`
/// (requires `thread_id` + `session_ids`) must be tried before `Merge` (requires only
/// `session_ids`). Validation still checks the parsed variant against the row's `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ThreadOverridePayload {
    /// `{"thread_id": ..., "session_ids": [...]}` - `thread_id` is the source thread the
    /// sessions are moved out of; the new thread is seeded by the override itself.
    Split {
        thread_id: i64,
        session_ids: Vec<i64>,
    },
    /// `{"session_ids": [...]}` - the source thread's member set at merge time (audit and
    /// undo); the fold itself follows `thread_sessions`.
    Merge { session_ids: Vec<i64> },
    /// `{"title": ...}` - sets `threads.title` only.
    Rename { title: String },
}

impl ThreadOverridePayload {
    /// Parses a persisted payload and validates it against the row's kind (`03 §7h.5`).
    pub fn from_json(kind: ThreadOverrideKind, payload: &str) -> crate::Result<Self> {
        let parsed: Self = serde_json::from_str(payload)
            .map_err(|e| anyhow::anyhow!("thread override payload is not typed JSON: {e}"))?;
        parsed.validate(kind)?;
        Ok(parsed)
    }

    /// Serializes the typed payload back to its `thread_overrides.payload` JSON form.
    pub fn to_json(&self) -> crate::Result<String> {
        serde_json::to_string(self)
            .map_err(|e| anyhow::anyhow!("serializing thread override payload: {e}"))
    }

    /// Checks the variant matches the row kind and that no session ids repeat.
    pub fn validate(&self, kind: ThreadOverrideKind) -> crate::Result<()> {
        match (kind, self) {
            (ThreadOverrideKind::Rename, Self::Rename { title }) => {
                if title.trim().is_empty() {
                    anyhow::bail!("rename override title must not be empty");
                }
            }
            (ThreadOverrideKind::Merge, Self::Merge { session_ids }) => {
                if session_ids.is_empty() {
                    anyhow::bail!("merge override must list the source thread's sessions");
                }
                validate_unique_ids(session_ids)?;
            }
            (
                ThreadOverrideKind::Split,
                Self::Split {
                    thread_id,
                    session_ids,
                },
            ) => {
                if *thread_id <= 0 {
                    anyhow::bail!("split override payload.thread_id must be a positive thread id");
                }
                if session_ids.is_empty() {
                    anyhow::bail!("split override must list at least one session");
                }
                validate_unique_ids(session_ids)?;
            }
            _ => anyhow::bail!(
                "thread override payload does not match its kind {}",
                kind.as_str()
            ),
        }
        Ok(())
    }
}

fn validate_unique_ids(ids: &[i64]) -> crate::Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for id in ids {
        if *id <= 0 {
            anyhow::bail!("thread override session ids must be positive");
        }
        if !seen.insert(*id) {
            anyhow::bail!("thread override session ids must not repeat");
        }
    }
    Ok(())
}

/// A persisted `thread_overrides` row, decoded (`03 §7h.5`). Rows are read in
/// `(created_at, id)` order and applied after clustering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadOverrideRow {
    pub id: i64,
    pub kind: ThreadOverrideKind,
    pub thread_id: Option<i64>,
    pub target_thread_id: Option<i64>,
    pub payload: ThreadOverridePayload,
    pub created_at: i64,
}

/// A new `thread_overrides` row to persist. P5 itself never writes one in production code;
/// the store method exists for P9's UI and for the override-precedence tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadOverrideDraft {
    pub kind: ThreadOverrideKind,
    pub thread_id: Option<i64>,
    pub target_thread_id: Option<i64>,
    pub payload: ThreadOverridePayload,
}

impl ThreadOverrideDraft {
    /// Validates the kind/thread-id shape against the `thread_overrides` table CHECK
    /// (`03 §4`) and the typed payload (`03 §7h.5`), including the fields a kind may carry.
    pub fn validate(&self) -> crate::Result<()> {
        let shape_ok = match self.kind {
            ThreadOverrideKind::Rename => {
                self.thread_id.is_some() && self.target_thread_id.is_none()
            }
            ThreadOverrideKind::Merge => match (self.thread_id, self.target_thread_id) {
                (Some(source), Some(target)) => source != target,
                _ => false,
            },
            // Split moves sessions out of one source thread; it never carries a target.
            ThreadOverrideKind::Split => {
                self.thread_id.is_some() && self.target_thread_id.is_none()
            }
        };
        if !shape_ok {
            anyhow::bail!(
                "thread override {} carries the wrong thread id shape",
                self.kind.as_str()
            );
        }
        if let (
            ThreadOverrideKind::Split,
            Some(row_thread_id),
            ThreadOverridePayload::Split {
                thread_id: payload_thread_id,
                ..
            },
        ) = (self.kind, self.thread_id, &self.payload)
        {
            if row_thread_id != *payload_thread_id {
                anyhow::bail!("split override payload.thread_id must name the row's source thread");
            }
        }
        self.payload.validate(self.kind)
    }
}

/// A planned rewrite of an existing override row (traits contract; retained for the store path).
///
/// The current P5 planner never emits one: the emptied merge source row is retained in the plan
/// and the merge row is left untouched, because `thread_overrides.thread_id` is
/// `ON DELETE CASCADE` and because replaying overrides in `(created_at, id)` order makes a
/// split-then-merge durable without mutating user rows (`06_PATCH_PLAN` #37, revised by the
/// #157 review fix). The type stays so the store method keeps a stable contract for a future
/// migration that does need a planned rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadOverrideUpdate {
    pub override_id: i64,
    pub thread_id: i64,
    pub target_thread_id: i64,
}

/// The complete deterministic thread model for one cluster run, before persistence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ThreadPlan {
    /// Every thread that must exist after this run; the store upserts by `cluster_key`.
    pub threads: Vec<ThreadDraft>,
    /// Existing thread ids that no longer exist in the model and are safe to delete.
    ///
    /// "Safe" means no override row still references them in the model: the planner retains every
    /// merge/split referenced thread and every split-created thread. A rename row is not part of
    /// membership management, so if clustering no longer produces its thread's `cluster_key`, the
    /// old row is deleted here and the rename row cascades away with it; that residual title loss
    /// is recorded in `07`.
    pub delete_thread_ids: Vec<i64>,
    /// Planned override-row rewrites (traits contract; the store applies any that are present).
    ///
    /// The current P5 planner always leaves this empty: merge durability is retention of the
    /// emptied source row plus `(created_at, id)` replay order, and no user override row is ever
    /// mutated (`06` #37 revised by the #157 review-fix pass).
    pub override_updates: Vec<ThreadOverrideUpdate>,
}

/// The five table counts logged once when the backfill completes and shown as acceptance
/// evidence (`03 §7h.6`, P5 acceptance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphTableCounts {
    pub entities: u64,
    pub entity_aliases: u64,
    pub session_entities: u64,
    pub threads: u64,
    pub thread_sessions: u64,
}

/// The resumable-backfill checkpoint read from the job queue itself (`03 §7h.6`): no new
/// state is stored, the queue is the checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GraphBackfillStatus {
    /// `MAX(jobs.frame_id)` over `graph_extract` jobs, the backfill cursor.
    pub cursor: Option<i64>,
    /// `MAX(frames.id)`, the completion target.
    pub max_frame_id: Option<i64>,
    pub pending_extract: u64,
    pub pending_backfill: u64,
    pub pending_cluster: u64,
    pub pending_normalize: u64,
    /// `done` `graph_extract` jobs; the scheduler uses it to notice that new extraction
    /// work landed since the last clustering pass (kernel-local memory, not persisted state).
    pub done_extract: u64,
}

/// One session summary candidate for the optional normalization pass (`03 §7h.2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSummaryInput {
    pub session_id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub tool: Option<String>,
    pub context_key: String,
    pub title: Option<String>,
    pub summary: Option<String>,
}

/// One existing entity plus its aliases, the catalog the normalization pass proposes
/// changes against (`03 §7h.2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityCatalogRow {
    pub id: i64,
    pub kind: EntityKind,
    pub canonical_name: String,
    pub normalized_key: String,
    pub aliases: Vec<(String, AliasSource)>,
}

/// One validated normalization proposal (`03 §7h.2`): either a new alias for an existing
/// entity (`matched_entity_id`) or a new `llm`-sourced entity promoted from a topic-shaped
/// name. It never rewrites a heuristic entity's `normalized_key`.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizationProposal {
    pub kind: EntityKind,
    pub canonical_name: String,
    pub normalized_key: String,
    pub aliases: Vec<String>,
    /// When set, the proposal attaches aliases to this existing entity instead of creating
    /// a row; the pass never mutates the matched row's identity.
    pub matched_entity_id: Option<i64>,
    pub confidence: f64,
}

/// What one normalization pass wrote. Both counters are additive: the pass never deletes
/// or rewrites a heuristic row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NormalizationOutcome {
    pub entities_created: u64,
    pub aliases_inserted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_kind_round_trips_through_the_closed_set() {
        for kind in [
            EntityKind::Repo,
            EntityKind::File,
            EntityKind::Domain,
            EntityKind::Document,
            EntityKind::Ref,
            EntityKind::App,
        ] {
            assert_eq!(EntityKind::from_token(kind.as_str()), Some(kind));
        }
        assert_eq!(EntityKind::from_token("topic"), None);
    }

    #[test]
    fn open_session_is_open_ended_and_closed_interval_is_half_open() {
        let open = GraphSessionInput {
            session_id: 1,
            started_at: 100,
            ended_at: None,
            tool: None,
            host: None,
        };
        assert!(open.contains(100));
        assert!(open.contains(i64::MAX));

        let closed = GraphSessionInput {
            session_id: 2,
            started_at: 100,
            ended_at: Some(200),
            tool: None,
            host: None,
        };
        assert!(!closed.contains(99));
        assert!(closed.contains(100));
        assert!(closed.contains(199));
        assert!(!closed.contains(200));
    }

    #[test]
    fn override_drafts_reject_the_wrong_shape() {
        let rename_missing_thread = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Rename,
            thread_id: None,
            target_thread_id: None,
            payload: ThreadOverridePayload::Rename {
                title: "title".into(),
            },
        };
        assert!(rename_missing_thread.validate().is_err());

        let merge_missing_target = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Merge,
            thread_id: Some(1),
            target_thread_id: None,
            payload: ThreadOverridePayload::Merge {
                session_ids: vec![1, 2],
            },
        };
        assert!(merge_missing_target.validate().is_err());

        let valid = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Split,
            thread_id: Some(1),
            target_thread_id: None,
            payload: ThreadOverridePayload::Split {
                thread_id: 1,
                session_ids: vec![3, 4],
            },
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn payloads_serialize_in_the_spec_documented_untagged_forms() {
        let rename = ThreadOverridePayload::Rename {
            title: "release work".into(),
        };
        assert_eq!(rename.to_json().unwrap(), r#"{"title":"release work"}"#);

        let merge = ThreadOverridePayload::Merge {
            session_ids: vec![4, 5],
        };
        assert_eq!(merge.to_json().unwrap(), r#"{"session_ids":[4,5]}"#);

        let split = ThreadOverridePayload::Split {
            thread_id: 7,
            session_ids: vec![11, 12],
        };
        assert_eq!(
            split.to_json().unwrap(),
            r#"{"thread_id":7,"session_ids":[11,12]}"#
        );
        let back =
            ThreadOverridePayload::from_json(ThreadOverrideKind::Split, &split.to_json().unwrap())
                .unwrap();
        assert_eq!(back, split);
    }

    #[test]
    fn payload_parser_reads_the_documented_shapes_and_rejects_a_kind_mismatch() {
        assert_eq!(
            ThreadOverridePayload::from_json(ThreadOverrideKind::Rename, r#"{"title":"t"}"#)
                .unwrap(),
            ThreadOverridePayload::Rename { title: "t".into() }
        );
        assert_eq!(
            ThreadOverridePayload::from_json(ThreadOverrideKind::Merge, r#"{"session_ids":[1,2]}"#)
                .unwrap(),
            ThreadOverridePayload::Merge {
                session_ids: vec![1, 2]
            }
        );
        // A merge payload never parses as a split, and a rename is not a merge payload.
        assert!(ThreadOverridePayload::from_json(
            ThreadOverrideKind::Merge,
            r#"{"title":"wrong variant"}"#
        )
        .is_err());
        assert!(ThreadOverridePayload::from_json(
            ThreadOverrideKind::Split,
            r#"{"session_ids":[1]}"#
        )
        .is_err());
    }

    #[test]
    fn split_drafts_must_name_their_own_source_thread_and_carry_no_target() {
        let mismatched = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Split,
            thread_id: Some(3),
            target_thread_id: None,
            payload: ThreadOverridePayload::Split {
                thread_id: 4,
                session_ids: vec![9],
            },
        };
        assert!(mismatched.validate().is_err());

        let with_target = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Split,
            thread_id: Some(3),
            target_thread_id: Some(4),
            payload: ThreadOverridePayload::Split {
                thread_id: 3,
                session_ids: vec![9],
            },
        };
        assert!(with_target.validate().is_err());

        let self_merge = ThreadOverrideDraft {
            kind: ThreadOverrideKind::Merge,
            thread_id: Some(8),
            target_thread_id: Some(8),
            payload: ThreadOverridePayload::Merge {
                session_ids: vec![1],
            },
        };
        assert!(self_merge.validate().is_err());
    }

    #[test]
    fn payloads_round_trip_through_typed_json() {
        let payload = ThreadOverridePayload::Split {
            thread_id: 7,
            session_ids: vec![11, 12],
        };
        let json = payload.to_json().unwrap();
        let back = ThreadOverridePayload::from_json(ThreadOverrideKind::Split, &json).unwrap();
        assert_eq!(back, payload);

        assert!(ThreadOverridePayload::from_json(
            ThreadOverrideKind::Merge,
            r#"{"title":"wrong variant"}"#
        )
        .is_err());
    }
}
