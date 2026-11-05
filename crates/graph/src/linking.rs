//! The session interval rule (`03 §7h.3`), retained as a pure, database-free primitive.
//!
//! A hit described by this rule would link to the session whose `[started_at, ended_at)`
//! contains the hit's timestamp, with an open session (`ended_at` NULL) open-ended. That rule is
//! **not used for production linking**: `03 §7e` makes `frames.session_id` the authoritative
//! owner, and the #157 review-fix pass resolved the conflict ownership-only (a frame links to its
//! owner or to no session; see `crates/traits/src/contracts.rs::graph_frame_input` and `06` #41).
//! The interval logic survives as `Store::graph_session_at` and as these executable examples, so
//! the retained primitive and its tests stay honest; it is not evidence that extraction links by
//! interval.
//!
//! Linking itself is persisted by `Store::graph_apply_hits` (`03 §7h.3`): one
//! `session_entities` row per `(session_id, entity_id)`, `hit_count` counting distinct
//! contributing frames (an exact per-session rebuild can replace it absolutely), and
//! `confidence` the maximum of the contributing hits.

use traits::GraphSessionInput;

/// The session whose interval contains `at_ms`, or `None` when the time falls between
/// sessions. Deterministic: when two sessions' intervals overlap (concurrent tracks may
/// overlap in time), the lowest session id wins, matching the store's `ORDER BY id LIMIT 1`.
pub fn session_containing(
    at_ms: i64,
    sessions: &[GraphSessionInput],
) -> Option<&GraphSessionInput> {
    sessions
        .iter()
        .filter(|session| session.contains(at_ms))
        .min_by_key(|session| session.session_id)
}

/// The maximum confidence of the contributing hits for one link (`03 §7h.3`: a session never
/// renders more certain than its best evidence). An empty contribution set has no
/// confidence and returns `None`.
pub fn link_confidence(contributing: &[f64]) -> Option<f64> {
    contributing.iter().copied().reduce(f64::max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: i64, started_at: i64, ended_at: Option<i64>) -> GraphSessionInput {
        GraphSessionInput {
            session_id: id,
            started_at,
            ended_at,
            tool: None,
            host: None,
        }
    }

    #[test]
    fn closed_interval_is_half_open() {
        let sessions = vec![session(1, 1_000, Some(2_000))];
        assert_eq!(session_containing(999, &sessions), None);
        assert_eq!(session_containing(1_000, &sessions).unwrap().session_id, 1);
        assert_eq!(session_containing(1_999, &sessions).unwrap().session_id, 1);
        assert_eq!(session_containing(2_000, &sessions), None);
    }

    #[test]
    fn open_session_is_open_ended() {
        let sessions = vec![session(1, 1_000, None)];
        assert_eq!(session_containing(1_000, &sessions).unwrap().session_id, 1);
        assert_eq!(
            session_containing(i64::MAX / 2, &sessions)
                .unwrap()
                .session_id,
            1
        );
        assert_eq!(session_containing(999, &sessions), None);
    }

    #[test]
    fn overlapping_sessions_resolve_to_the_lowest_id() {
        let sessions = vec![
            session(7, 1_000, Some(3_000)),
            session(2, 2_000, Some(4_000)),
        ];
        assert_eq!(session_containing(2_500, &sessions).unwrap().session_id, 2);
        assert_eq!(session_containing(1_500, &sessions).unwrap().session_id, 7);
    }

    #[test]
    fn link_confidence_is_the_maximum_of_contributing_hits() {
        assert_eq!(link_confidence(&[0.60, 0.95, 0.80]), Some(0.95));
        assert_eq!(link_confidence(&[]), None);
        assert_eq!(link_confidence(&[0.60]), Some(0.60));
    }
}
