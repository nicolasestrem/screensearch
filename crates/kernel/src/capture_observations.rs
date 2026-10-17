//! Bounded, process-local observations of capture controller intent and errors.
//!
//! Absence of an interruption is not capture health or evidence of user inactivity.
//! A lost observation (backwards clock, overflow, earlier process) is unknown, never
//! silently complete. Nothing here consumes frame timestamps, content, privacy gates
//! or inference state.

use std::collections::VecDeque;

const TRANSITION_CAP: usize = 1000;
const INTERVAL_CAP: usize = 100;

/// Privacy-safe reasons only; diagnostic strings never enter the observation buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureInterruptionReason {
    CaptureStopped,
    WorkerError,
}

/// Endpoints are actual observations, not clipped query bounds. An unresolved
/// interruption has no end; it does not assert continuous capture failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureInterruption {
    pub start: i64,
    pub end: Option<i64>,
    pub reason: CaptureInterruptionReason,
}

/// Domain snapshot mapped to the coverage wire DTO by the composition root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureObservations {
    pub observation_started_at: i64,
    pub unavailable_before: i64,
    pub intervals: Vec<CaptureInterruption>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CaptureObservation {
    Started,
    Interrupted(CaptureInterruptionReason),
}

#[derive(Debug, Clone, Copy)]
struct Transition {
    at: i64,
    observation: CaptureObservation,
}

pub(crate) struct CaptureObservationBuffer {
    observation_started_at: i64,
    unavailable_before: i64,
    transitions: VecDeque<Transition>,
    /// Set when a backwards clock discards ambiguous history: the current
    /// condition is unknown from `from` until an actual observation
    /// re-establishes it. An unresolved gap reaches forward indefinitely, so
    /// windows touching it stay truncated instead of implying a complete record.
    unknown_condition: Option<UnknownCondition>,
}

/// The condition gap a backwards clock correction leaves behind. Neither
/// endpoint is invented: `from` is the raised availability boundary and
/// `until` is the actual timestamp of the observation that ends the gap.
#[derive(Debug, Clone, Copy)]
struct UnknownCondition {
    from: i64,
    until: Option<i64>,
}

impl CaptureObservationBuffer {
    pub(crate) fn new(at: i64) -> Self {
        // Kernel::new actually initializes the controller disabled. This observes
        // intent from this instant only, never the previous process or health.
        Self {
            observation_started_at: at,
            unavailable_before: at,
            transitions: VecDeque::from([Transition {
                at,
                observation: CaptureObservation::Interrupted(
                    CaptureInterruptionReason::CaptureStopped,
                ),
            }]),
            unknown_condition: None,
        }
    }

    pub(crate) fn record(&mut self, at: i64, observation: CaptureObservation) {
        let latest = self
            .transitions
            .back()
            .map_or(self.unavailable_before, |transition| transition.at);
        if at < latest {
            // A backwards wall clock cannot produce truthful ordered intervals.
            // Lose that ambiguous history explicitly, without inventing timestamps.
            // The discarded observation may have changed the condition and no
            // retained evidence can re-establish it, so the condition is unknown
            // from the boundary until an actual observation actually arrives.
            self.unavailable_before = self.unavailable_before.max(latest);
            self.transitions.clear();
            self.unknown_condition = Some(UnknownCondition {
                from: self.unavailable_before,
                until: None,
            });
            return;
        }
        if self
            .transitions
            .back()
            .is_some_and(|transition| transition.observation == observation)
        {
            // Repeated reports of the same unresolved condition are not a new
            // transition, and must not move its actual observed start.
            return;
        }
        self.transitions.push_back(Transition { at, observation });
        if self.transitions.len() > TRANSITION_CAP {
            self.transitions.pop_front();
            self.unavailable_before = self.transitions.front().expect("retained transition").at;
        }
        if let Some(unknown) = &mut self.unknown_condition {
            // An actual observation re-establishes the condition from its own
            // timestamp; the gap ends there and never moves again.
            unknown.until.get_or_insert(at);
        }
    }

    pub(crate) fn snapshot(&self, from: i64, to: i64) -> CaptureObservations {
        let mut snapshot = CaptureObservations {
            observation_started_at: self.observation_started_at,
            unavailable_before: self.unavailable_before,
            intervals: Vec::new(),
            // Unavailable history and the post-rollback unknown-condition gap
            // both make the window incomplete, never silently empty.
            truncated: from < self.unavailable_before || self.condition_unknown_in(from, to),
        };
        let mut open: Option<CaptureInterruption> = None;
        for transition in &self.transitions {
            if let Some(mut interval) = open.take() {
                interval.end = Some(transition.at);
                snapshot.include(interval, from, to);
            }
            if let CaptureObservation::Interrupted(reason) = transition.observation {
                open = Some(CaptureInterruption {
                    start: transition.at,
                    end: None,
                    reason,
                });
            }
        }
        if let Some(interval) = open {
            snapshot.include(interval, from, to);
        }
        snapshot
    }

    /// Whether the half-open window touches the condition gap left by a
    /// backwards clock. An unresolved gap reaches forward indefinitely: no
    /// later observation has re-established the condition yet.
    fn condition_unknown_in(&self, from: i64, to: i64) -> bool {
        self.unknown_condition
            .is_some_and(|unknown| from < unknown.until.unwrap_or(i64::MAX) && unknown.from < to)
    }
}

impl CaptureObservations {
    fn include(&mut self, interval: CaptureInterruption, from: i64, to: i64) {
        // Strict half-open intersection also excludes zero-duration transitions.
        if interval.start >= to
            || interval
                .end
                .is_some_and(|end| end <= from || end <= interval.start)
        {
            return;
        }
        if self.intervals.len() == INTERVAL_CAP {
            self.truncated = true;
        } else {
            self.intervals.push(interval);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_restart_and_overflow_are_unknown() {
        let mut first_process = CaptureObservationBuffer::new(100);
        first_process.record(110, CaptureObservation::Started);
        let restarted = CaptureObservationBuffer::new(200);
        assert_eq!(restarted.snapshot(100, 300).unavailable_before, 200);
        assert!(restarted.snapshot(100, 200).intervals.is_empty());
        assert!(restarted.snapshot(100, 200).truncated);

        for at in 111..=1110 {
            first_process.record(
                at,
                if at % 2 == 0 {
                    CaptureObservation::Started
                } else {
                    CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped)
                },
            );
        }
        assert_eq!(first_process.transitions.len(), TRANSITION_CAP);
        assert_eq!(first_process.snapshot(100, 1200).unavailable_before, 111);
        assert!(first_process.snapshot(100, 111).truncated);
        assert!(first_process.snapshot(100, 111).intervals.is_empty());
        let recent = first_process.snapshot(1109, 1111);
        assert!(!recent.truncated);
        assert_eq!(recent.intervals[0].start, 1109);
        assert_eq!(recent.intervals[0].end, Some(1110));
        // Evicting an opening endpoint must not invent a clipped interval from
        // a retained closing/start event.
        first_process.record(
            1111,
            CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped),
        );
        let evicted_opening = first_process.snapshot(111, 113);
        assert_eq!(evicted_opening.unavailable_before, 112);
        assert!(evicted_opening.truncated);
        assert!(evicted_opening.intervals.is_empty());
    }

    #[test]
    fn half_open_queries_preserve_observed_endpoints_and_open_pause() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(20, CaptureObservation::Started);
        buffer.record(
            30,
            CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped),
        );
        assert!(buffer.snapshot(20, 30).intervals.is_empty());
        let closed = buffer.snapshot(15, 20);
        assert_eq!(
            (closed.intervals[0].start, closed.intervals[0].end),
            (10, Some(20))
        );
        let open = buffer.snapshot(40, 50);
        assert_eq!((open.intervals[0].start, open.intervals[0].end), (30, None));
        assert!(!open.truncated);
    }

    #[test]
    fn repeated_errors_remain_unresolved_until_a_real_start_transition() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(20, CaptureObservation::Started);
        buffer.record(
            30,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        buffer.record(
            40,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        assert_eq!(buffer.transitions.len(), 3);
        let unresolved = buffer.snapshot(25, 60);
        assert_eq!(unresolved.intervals[0].start, 30);
        assert_eq!(unresolved.intervals[0].end, None);
        assert_eq!(
            unresolved.intervals[0].reason,
            CaptureInterruptionReason::WorkerError
        );
        buffer.record(50, CaptureObservation::Started);
        assert_eq!(buffer.snapshot(25, 60).intervals[0].end, Some(50));
        assert!(buffer.snapshot(50, 60).intervals.is_empty());
    }

    #[test]
    fn response_cap_is_truthful_only_for_intersecting_intervals() {
        let mut buffer = CaptureObservationBuffer::new(0);
        for at in 1..=201 {
            buffer.record(
                at,
                if at % 2 == 1 {
                    CaptureObservation::Started
                } else {
                    CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped)
                },
            );
        }
        assert_eq!(buffer.snapshot(0, 200).intervals.len(), 100);
        assert!(!buffer.snapshot(0, 200).truncated);
        assert_eq!(buffer.snapshot(0, 202).intervals.len(), 100);
        assert!(buffer.snapshot(0, 202).truncated);
        assert_eq!(buffer.snapshot(200, 202).intervals.len(), 1);
        assert!(!buffer.snapshot(200, 202).truncated);
    }

    #[test]
    fn equal_timestamps_do_not_fabricate_positive_duration() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(10, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        buffer.record(20, CaptureObservation::Started);
        assert!(buffer.snapshot(10, 30).intervals.is_empty());
    }

    #[test]
    fn backwards_clock_loses_ambiguous_history_without_rewriting_timestamps() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(30, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        let snapshot = buffer.snapshot(10, 40);
        assert_eq!(snapshot.unavailable_before, 30);
        assert!(snapshot.truncated);
        assert!(snapshot.intervals.is_empty());
        buffer.record(
            40,
            CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped),
        );
        assert_eq!(buffer.snapshot(30, 50).intervals[0].start, 40);
    }

    #[test]
    fn rollback_keeps_condition_unknown_at_and_after_the_boundary_until_reobserved() {
        // Exact counterexample: initialize at 10, Started at 30, WorkerError
        // at 20 after a clock correction. The discarded error may have changed
        // the condition, so windows starting at or after the 30 boundary must
        // stay truncated until an actual observation re-establishes it — they
        // are not complete empty records.
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(30, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        for (from, to) in [(30, 40), (31, 40), (35, 40), (30, 31), (40, 50)] {
            let snapshot = buffer.snapshot(from, to);
            assert_eq!(snapshot.unavailable_before, 30);
            assert!(
                snapshot.truncated,
                "window [{from}, {to}) touches the unknown condition"
            );
            assert!(snapshot.intervals.is_empty());
        }
        // A record while the clock is still behind stays ambiguous: the
        // condition remains unknown, not silently re-established.
        buffer.record(25, CaptureObservation::Started);
        assert!(buffer.snapshot(30, 40).truncated);
        assert!(buffer.snapshot(30, 40).intervals.is_empty());
    }

    #[test]
    fn rollback_gap_ends_at_the_first_actual_recovery_observation() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(30, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        // A real capture start re-establishes the condition from its own
        // timestamp; earlier windows remain honestly truncated.
        buffer.record(45, CaptureObservation::Started);
        let through_gap = buffer.snapshot(30, 50);
        assert!(through_gap.truncated);
        assert!(through_gap.intervals.is_empty());
        assert!(buffer.snapshot(44, 50).truncated);
        let after_recovery = buffer.snapshot(45, 50);
        assert!(!after_recovery.truncated);
        assert!(after_recovery.intervals.is_empty());
    }

    #[test]
    fn rollback_gap_ends_at_the_first_actual_new_transition() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(30, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        buffer.record(
            45,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        // The new error is reported with its own actual timestamp; the gap
        // before it never invents a continuous failure duration.
        let window = buffer.snapshot(30, 50);
        assert!(window.truncated);
        assert_eq!(
            window.intervals,
            vec![CaptureInterruption {
                start: 45,
                end: None,
                reason: CaptureInterruptionReason::WorkerError,
            }]
        );
        let gap_only = buffer.snapshot(30, 45);
        assert!(gap_only.truncated);
        assert!(gap_only.intervals.is_empty());
        let after = buffer.snapshot(45, 50);
        assert!(!after.truncated);
        assert_eq!(after.intervals[0].start, 45);
        assert_eq!(after.intervals[0].end, None);
    }

    #[test]
    fn boundary_timestamp_reestablishes_condition_without_an_invented_gap() {
        let mut buffer = CaptureObservationBuffer::new(10);
        buffer.record(30, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        // An actual observation at exactly the boundary re-establishes the
        // condition from that boundary; only earlier windows stay truncated.
        buffer.record(30, CaptureObservation::Started);
        assert!(buffer.snapshot(29, 40).truncated);
        let snapshot = buffer.snapshot(30, 40);
        assert!(!snapshot.truncated);
        assert!(snapshot.intervals.is_empty());
    }

    #[test]
    fn rollback_after_overflow_keeps_condition_unknown_from_the_boundary() {
        let mut buffer = CaptureObservationBuffer::new(0);
        for at in 1..=1001 {
            buffer.record(
                at,
                if at % 2 == 1 {
                    CaptureObservation::Started
                } else {
                    CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped)
                },
            );
        }
        assert_eq!(buffer.transitions.len(), TRANSITION_CAP);
        assert_eq!(buffer.snapshot(2, 3).unavailable_before, 2);
        buffer.record(
            500,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        // The rollback raises the boundary to the newest observation and the
        // condition is unknown from there until re-established.
        let unknown = buffer.snapshot(1001, 1010);
        assert_eq!(unknown.unavailable_before, 1001);
        assert!(unknown.truncated);
        assert!(unknown.intervals.is_empty());
        assert!(buffer.snapshot(900, 1010).truncated);
        assert!(buffer.snapshot(1001, 1002).truncated);
        buffer.record(1005, CaptureObservation::Started);
        assert!(!buffer.snapshot(1005, 1010).truncated);
        assert!(buffer.snapshot(1004, 1010).truncated);
    }

    #[test]
    fn overflow_after_rollback_keeps_gap_and_eviction_boundaries_exact() {
        let mut buffer = CaptureObservationBuffer::new(0);
        buffer.record(30, CaptureObservation::Started);
        buffer.record(
            20,
            CaptureObservation::Interrupted(CaptureInterruptionReason::WorkerError),
        );
        buffer.record(31, CaptureObservation::Started);
        assert!(buffer.snapshot(30, 40).truncated);
        assert!(!buffer.snapshot(31, 40).truncated);
        for at in 32..=1040 {
            buffer.record(
                at,
                if at % 2 == 0 {
                    CaptureObservation::Interrupted(CaptureInterruptionReason::CaptureStopped)
                } else {
                    CaptureObservation::Started
                },
            );
        }
        assert_eq!(buffer.transitions.len(), TRANSITION_CAP);
        // Eviction advanced the boundary past the resolved gap; windows from
        // the boundary are complete, windows before it stay truncated, and
        // the gap itself remains truncated.
        let from_boundary = buffer.snapshot(41, 42);
        assert_eq!(from_boundary.unavailable_before, 41);
        assert!(!from_boundary.truncated);
        assert!(buffer.snapshot(40, 42).truncated);
        let gap = buffer.snapshot(30, 32);
        assert!(gap.truncated);
        assert!(gap.intervals.is_empty());
    }

    #[test]
    fn initial_disabled_intent_is_observed_without_prior_process_history() {
        let observations = CaptureObservationBuffer::new(100);
        let snapshot = observations.snapshot(50, 200);
        assert_eq!(snapshot.observation_started_at, 100);
        assert_eq!(snapshot.unavailable_before, 100);
        assert!(snapshot.truncated);
        assert_eq!(
            snapshot.intervals,
            vec![CaptureInterruption {
                start: 100,
                end: None,
                reason: CaptureInterruptionReason::CaptureStopped,
            }]
        );
    }
}
