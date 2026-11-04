//! The five metrics, defined here and nowhere else (`docs/kickoffs/P4.md`; the numbers this module
//! produces are the ones Gate A freezes as the P5/P6 thresholds).
//!
//! Definitions, exactly as implemented:
//!
//! 1. **Evidence recall@10 (the comparator).** For one pair, the pair's expected ids in generator
//!    order (expected frame ids first, then expected session ids); take the first 10; the score is
//!    `matched / min(10, expected.len())`. A cited id set is the turn's cited frame ids plus its
//!    cited session ids; an expected **session** id also counts as matched when a cited frame's
//!    `session_id` equals it. The aggregate is the arithmetic mean over pairs, overall and per
//!    generator. Citations are the only evidence the product exposes to a client (`03 §7i.3`), so
//!    this is the comparator.
//! 2. **Retrieval recall@10 (diagnostic, never mixed into 1).** Per pair, whether the pair's first
//!    expected frame id appears in the first 10 `frame_id`s of `GET /v1/search` for the question
//!    text with `limit=10&include_chrome=false`. Pairs without expected frames, or whose search
//!    call failed, are not measured.
//! 3. **Citation validity.** `valid / total` over every cited id of every turn (the kickoff states
//!    its complement, `invalid / total`; both counts are reported). A cited frame is valid when it
//!    exists and its `captured_at` is inside the pair's window (no window = whole history). A
//!    cited session is valid when it exists and its interval overlaps the window (an open session
//!    is open-ended). Every invalid id is emitted with its pair.
//! 4. **Known fact present.** The answer, lowercased and whitespace-collapsed, contains at least
//!    one normalized expected string, or - for `last_seen` - the expected local `YYYY-MM-DD`. The
//!    rate is over **all** pairs of the scope (a failed turn has no answer and counts as a miss).
//! 5. **Latency p50 / p95.** Per turn, POST to the terminal `done`/`error` event, wall clock ms,
//!    over successful turns only (nearest-rank percentile). Failures are counted separately by
//!    class.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::client::{FailureClass, TurnFailure};
use super::facts::{CitedFrameRow, CitedSessionRow};
use super::gen::{ExpectedId, Generator, Pair, Window};

/// One turn's evidence, resolved against the DB at collection time so the metric math is pure.
#[derive(Debug, Clone)]
pub struct TurnRun {
    pub pair_id: String,
    pub generator: Generator,
    pub failure: Option<TurnFailure>,
    pub latency_ms: Option<u64>,
    pub cited_frames: Vec<CitedFrameRow>,
    pub cited_sessions: Vec<CitedSessionRow>,
    /// The first `frame_id`s from `GET /v1/search` (diagnostic recall arm).
    pub retrieval_ids: Vec<i64>,
    pub retrieval_error: Option<String>,
    pub answer: String,
    /// The terminal event payload, verbatim (forward-compatible: later fields are ignored here).
    pub terminal: Value,
}

impl TurnRun {
    pub fn success(
        pair: &Pair,
        answer: String,
        cited_frames: Vec<CitedFrameRow>,
        cited_sessions: Vec<CitedSessionRow>,
        latency_ms: u64,
        terminal: Value,
    ) -> Self {
        Self {
            pair_id: pair.id.clone(),
            generator: pair.generator,
            failure: None,
            latency_ms: Some(latency_ms),
            cited_frames,
            cited_sessions,
            retrieval_ids: Vec::new(),
            retrieval_error: None,
            answer,
            terminal,
        }
    }

    pub fn failed(pair: &Pair, failure: TurnFailure) -> Self {
        Self {
            pair_id: pair.id.clone(),
            generator: pair.generator,
            failure: Some(failure),
            latency_ms: None,
            cited_frames: Vec::new(),
            cited_sessions: Vec::new(),
            retrieval_ids: Vec::new(),
            retrieval_error: None,
            answer: String::new(),
            terminal: Value::Null,
        }
    }
}

/// Which cited-id space an invalid citation belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationKind {
    Frame,
    Session,
}

impl CitationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CitationKind::Frame => "frame",
            CitationKind::Session => "session",
        }
    }
}

/// One cited id that failed the validity rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvalidCitation {
    pub pair_id: String,
    pub generator: Generator,
    pub kind: CitationKind,
    pub id: i64,
    pub reason: &'static str,
}

/// Aggregated numbers for one scope (overall, or one generator). Serialized into `report.json`
/// so later phases read the numbers instead of parsing Markdown; rates are `null`, never a fake
/// zero, when the scope had nothing to measure.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Aggregate {
    pub pairs: usize,
    pub evidence_score_sum: f64,
    pub evidence_pairs: usize,
    pub retrieval_hits: usize,
    pub retrieval_pairs: usize,
    pub cited_ids: usize,
    pub invalid_ids: usize,
    pub known_hits: usize,
    pub known_pairs: usize,
    pub turns_ok: usize,
    pub failures: BTreeMap<FailureClass, usize>,
    #[serde(skip)]
    latencies: Vec<u64>,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
}

impl Aggregate {
    /// Mean of the per-pair fractions; `None` when nothing was measurable (never a fake zero).
    pub fn evidence_recall10(&self) -> Option<f64> {
        if self.evidence_pairs == 0 {
            None
        } else {
            Some(self.evidence_score_sum / self.evidence_pairs as f64)
        }
    }

    pub fn retrieval_recall10(&self) -> Option<f64> {
        if self.retrieval_pairs == 0 {
            None
        } else {
            Some(self.retrieval_hits as f64 / self.retrieval_pairs as f64)
        }
    }

    /// `valid / total` cited ids; `None` when the turns cited nothing at all.
    pub fn citation_validity(&self) -> Option<f64> {
        if self.cited_ids == 0 {
            None
        } else {
            Some(1.0 - self.invalid_ids as f64 / self.cited_ids as f64)
        }
    }

    pub fn known_fact_rate(&self) -> Option<f64> {
        if self.known_pairs == 0 {
            None
        } else {
            Some(self.known_hits as f64 / self.known_pairs as f64)
        }
    }

    pub fn failure_count(&self) -> usize {
        self.failures.values().sum()
    }

    fn finish(&mut self) {
        self.latencies.sort_unstable();
        self.p50 = percentile(&self.latencies, 0.50);
        self.p95 = percentile(&self.latencies, 0.95);
    }
}

/// One generator's row.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratorMetrics {
    pub generator: Generator,
    pub aggregate: Aggregate,
}

/// The whole run's metrics.
#[derive(Debug, Clone, Serialize)]
pub struct Metrics {
    pub overall: Aggregate,
    pub per_generator: Vec<GeneratorMetrics>,
    pub invalid: Vec<InvalidCitation>,
}

/// Nearest-rank percentile over an ascending slice (`ceil(p*n)`, clamped).
pub fn percentile(sorted: &[u64], p: f64) -> Option<u64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = (p * sorted.len() as f64).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    Some(sorted[idx])
}

/// Lowercase + collapse every whitespace run to one space.
pub fn normalize(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Does the answer contain the known fact the generator derived?
pub fn known_fact(pair: &Pair, answer: &str) -> bool {
    let answer = normalize(answer);
    if answer.is_empty() {
        return false;
    }
    if let Some(date) = &pair.expected_local_date {
        if answer.contains(&date.to_lowercase()) {
            return true;
        }
    }
    pair.expected_strings
        .iter()
        .any(|value| !value.is_empty() && answer.contains(&normalize(value)))
}

/// Compute the five metrics over the generated pairs and their turns.
pub fn compute(pairs: &[Pair], runs: &[TurnRun]) -> Metrics {
    let mut by_pair: BTreeMap<&str, &TurnRun> = BTreeMap::new();
    for run in runs {
        by_pair.insert(run.pair_id.as_str(), run);
    }

    let mut overall = Aggregate::default();
    let mut per_generator: Vec<GeneratorMetrics> = Generator::ALL
        .iter()
        .map(|g| GeneratorMetrics {
            generator: *g,
            aggregate: Aggregate::default(),
        })
        .collect();
    let mut invalid = Vec::new();

    for pair in pairs {
        let Some(run) = by_pair.get(pair.id.as_str()) else {
            continue; // no turn recorded (the run stopped early); never invent one
        };
        let slot = per_generator
            .iter_mut()
            .find(|gm| gm.generator == pair.generator)
            .expect("every generator has a metrics row");
        invalid.extend(add(&mut overall, pair, run));
        add(&mut slot.aggregate, pair, run);
    }

    overall.finish();
    for gm in &mut per_generator {
        gm.aggregate.finish();
    }
    Metrics {
        overall,
        per_generator,
        invalid,
    }
}

/// Fold one turn into one aggregate, returning its invalid citations.
fn add(agg: &mut Aggregate, pair: &Pair, run: &TurnRun) -> Vec<InvalidCitation> {
    agg.pairs += 1;
    agg.known_pairs += 1;
    if known_fact(pair, &run.answer) {
        agg.known_hits += 1;
    }
    match &run.failure {
        Some(failure) => {
            *agg.failures.entry(failure.class).or_insert(0) += 1;
        }
        None => {
            agg.turns_ok += 1;
            if let Some(latency) = run.latency_ms {
                agg.latencies.push(latency);
            }
        }
    }

    // Diagnostic retrieval arm: only measurable when the pair has a frame ground truth and the
    // search call itself succeeded.
    if let Some(first) = pair.expected_frames.first() {
        if run.retrieval_error.is_none() {
            agg.retrieval_pairs += 1;
            if run.retrieval_ids.iter().take(10).any(|id| id == first) {
                agg.retrieval_hits += 1;
            }
        }
    }

    let expected = pair.expected_order();
    let denominator = expected.len().min(10);
    if denominator > 0 {
        let matched = expected
            .iter()
            .take(10)
            .filter(|id| is_cited(**id, run))
            .count();
        agg.evidence_score_sum += matched as f64 / denominator as f64;
        agg.evidence_pairs += 1;
    }

    let mut invalid = Vec::new();
    for frame in &run.cited_frames {
        agg.cited_ids += 1;
        if !frame_valid(frame, pair.window) {
            agg.invalid_ids += 1;
            invalid.push(InvalidCitation {
                pair_id: pair.id.clone(),
                generator: pair.generator,
                kind: CitationKind::Frame,
                id: frame.id,
                reason: if frame.exists {
                    "outside the pair window"
                } else {
                    "frame does not exist"
                },
            });
        }
    }
    for session in &run.cited_sessions {
        agg.cited_ids += 1;
        if !session_valid(session, pair.window) {
            agg.invalid_ids += 1;
            invalid.push(InvalidCitation {
                pair_id: pair.id.clone(),
                generator: pair.generator,
                kind: CitationKind::Session,
                id: session.id,
                reason: if session.exists {
                    "does not overlap the pair window"
                } else {
                    "session does not exist"
                },
            });
        }
    }
    invalid
}

/// An expected id counts as cited: frames exactly, sessions directly or through a cited frame.
fn is_cited(expected: ExpectedId, run: &TurnRun) -> bool {
    match expected {
        ExpectedId::Frame(id) => run.cited_frames.iter().any(|f| f.id == id),
        ExpectedId::Session(id) => {
            run.cited_sessions.iter().any(|s| s.id == id)
                || run.cited_frames.iter().any(|f| f.session_id == Some(id))
        }
    }
}

fn frame_valid(frame: &CitedFrameRow, window: Option<Window>) -> bool {
    if !frame.exists {
        return false;
    }
    match (window, frame.captured_at) {
        (None, _) => true,
        (Some(w), Some(ts)) => w.contains(ts),
        (Some(_), None) => false,
    }
}

fn session_valid(session: &CitedSessionRow, window: Option<Window>) -> bool {
    if !session.exists {
        return false;
    }
    let Some(window) = window else {
        return true;
    };
    let start = session.started_at.unwrap_or(i64::MIN);
    let end = session.ended_at.unwrap_or(i64::MAX); // still open = open-ended
    window.overlaps(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(id: &str, generator: Generator, window: Option<Window>) -> Pair {
        Pair {
            id: id.to_string(),
            generator,
            question: "q".to_string(),
            expected_frames: vec![1, 2, 3],
            expected_sessions: vec![10],
            expected_strings: vec!["Alpha Notes".to_string()],
            window,
            expected_local_date: None,
        }
    }

    fn frame(
        id: i64,
        exists: bool,
        captured_at: Option<i64>,
        session: Option<i64>,
    ) -> CitedFrameRow {
        CitedFrameRow {
            id,
            exists,
            captured_at,
            session_id: session,
        }
    }

    fn session(id: i64, exists: bool, start: i64, end: Option<i64>) -> CitedSessionRow {
        CitedSessionRow {
            id,
            exists,
            started_at: Some(start),
            ended_at: end,
        }
    }

    #[test]
    fn evidence_recall_counts_the_session_through_its_frame() {
        let p = pair("p", Generator::LastSeen, None);
        let run = TurnRun {
            pair_id: "p".into(),
            generator: Generator::LastSeen,
            failure: None,
            latency_ms: Some(10),
            // frame 1 (session 10) and frame 3 match; frame 2 is missing, session 10 is not cited
            // directly but frame 1 belongs to it, so frame(1) + session(10) + frame(3) = 3 of 4.
            cited_frames: vec![
                frame(1, true, Some(5), Some(10)),
                frame(3, true, Some(9), None),
            ],
            cited_sessions: vec![],
            retrieval_ids: vec![],
            retrieval_error: None,
            answer: "alpha   NOTES was open".into(),
            terminal: Value::Null,
        };
        let metrics = compute(&[p], &[run]);
        let recall = metrics.overall.evidence_recall10().unwrap();
        assert!((recall - 0.75).abs() < 1e-9, "{recall}");
        assert_eq!(
            metrics.overall.known_hits, 1,
            "case/space-insensitive match"
        );
    }

    #[test]
    fn recall_uses_only_the_first_ten_expected_ids() {
        let mut p = pair("p", Generator::SeenAgain, None);
        p.expected_frames = (1..=20).collect();
        p.expected_sessions = vec![];
        let run = TurnRun {
            pair_id: "p".into(),
            generator: Generator::SeenAgain,
            failure: None,
            latency_ms: Some(1),
            cited_frames: (1..=12).map(|i| frame(i, true, Some(i), None)).collect(),
            cited_sessions: vec![],
            retrieval_ids: vec![],
            retrieval_error: None,
            answer: "Alpha Notes".into(),
            terminal: Value::Null,
        };
        let m = compute(&[p], &[run]);
        assert_eq!(m.overall.evidence_recall10().unwrap(), 1.0);
    }

    #[test]
    fn citation_validity_checks_existence_and_the_window() {
        let p = pair(
            "p",
            Generator::WindowRecall,
            Some(Window {
                start: 100,
                end: 200,
            }),
        );
        let run = TurnRun {
            pair_id: "p".into(),
            generator: Generator::WindowRecall,
            failure: None,
            latency_ms: Some(1),
            cited_frames: vec![
                frame(1, true, Some(150), Some(10)), // valid
                frame(2, true, Some(250), None),     // outside the window
                frame(3, false, None, None),         // does not exist
            ],
            cited_sessions: vec![
                session(10, true, 90, Some(180)),  // overlaps
                session(11, true, 300, Some(400)), // after the window
            ],
            retrieval_ids: vec![],
            retrieval_error: None,
            answer: "Alpha Notes".into(),
            terminal: Value::Null,
        };
        let m = compute(&[p], &[run]);
        assert_eq!(m.overall.cited_ids, 5);
        assert_eq!(m.overall.invalid_ids, 3);
        assert_eq!(m.overall.citation_validity().unwrap(), 0.4);
        let reasons: Vec<&str> = m.invalid.iter().map(|i| i.reason).collect();
        assert!(reasons.contains(&"outside the pair window"));
        assert!(reasons.contains(&"frame does not exist"));
        assert!(reasons.contains(&"does not overlap the pair window"));
    }

    #[test]
    fn open_sessions_and_no_window_are_always_valid() {
        let p = pair("p", Generator::LastSeen, None);
        let run = TurnRun {
            pair_id: "p".into(),
            generator: Generator::LastSeen,
            failure: None,
            latency_ms: Some(1),
            cited_frames: vec![frame(1, true, Some(1), None)],
            cited_sessions: vec![session(10, true, 5, None)],
            retrieval_ids: vec![],
            retrieval_error: None,
            answer: "Alpha Notes".into(),
            terminal: Value::Null,
        };
        let m = compute(&[p], &[run]);
        assert_eq!(m.overall.invalid_ids, 0);
        assert_eq!(m.overall.citation_validity().unwrap(), 1.0);
    }

    #[test]
    fn retrieval_is_diagnostic_and_separate_from_evidence_recall() {
        let p = pair("p", Generator::Involvement, None);
        let run = TurnRun {
            pair_id: "p".into(),
            generator: Generator::Involvement,
            failure: None,
            latency_ms: Some(1),
            cited_frames: vec![frame(1, true, Some(1), None)],
            cited_sessions: vec![],
            retrieval_ids: vec![9, 8, 1], // first expected frame id = 1, third hit
            retrieval_error: None,
            answer: "Alpha Notes".into(),
            terminal: Value::Null,
        };
        // A cited frame is not a search result: the two numbers must stay independent.
        let m = compute(&[p], &[run]);
        assert_eq!(m.overall.retrieval_recall10().unwrap(), 1.0);
        assert!((m.overall.evidence_recall10().unwrap() - 0.25).abs() < 1e-9);
    }

    #[test]
    fn failed_turns_count_as_known_fact_misses_and_are_classified() {
        let p = pair("p", Generator::Sequence, None);
        let failure = TurnFailure {
            class: FailureClass::Timeout,
            detail: "slow".into(),
            terminal: None,
        };
        let run = TurnRun::failed(&p, failure);
        let m = compute(&[p], &[run]);
        assert_eq!(m.overall.turns_ok, 0);
        assert_eq!(m.overall.failures.get(&FailureClass::Timeout), Some(&1));
        assert_eq!(m.overall.known_fact_rate().unwrap(), 0.0);
        assert_eq!(m.overall.p50, None);
        assert_eq!(m.overall.evidence_recall10().unwrap(), 0.0);
    }

    #[test]
    fn latency_percentiles_are_nearest_rank_over_successful_turns_only() {
        assert_eq!(percentile(&[], 0.5), None);
        assert_eq!(percentile(&[10, 20, 30, 40], 0.50), Some(20));
        assert_eq!(percentile(&[10, 20, 30, 40], 0.95), Some(40));
        assert_eq!(percentile(&[500], 0.95), Some(500));
    }

    #[test]
    fn per_generator_rows_split_the_pairs() {
        let a = pair("a", Generator::LastSeen, None);
        let b = pair("b", Generator::WindowRecall, None);
        let run_a = TurnRun {
            pair_id: "a".into(),
            generator: Generator::LastSeen,
            failure: None,
            latency_ms: Some(100),
            cited_frames: vec![],
            cited_sessions: vec![],
            retrieval_ids: vec![],
            retrieval_error: None,
            answer: "Alpha Notes".into(),
            terminal: Value::Null,
        };
        let run_b = TurnRun {
            pair_id: "b".into(),
            generator: Generator::WindowRecall,
            failure: None,
            latency_ms: Some(300),
            cited_frames: vec![],
            cited_sessions: vec![],
            retrieval_ids: vec![],
            retrieval_error: None,
            answer: "nothing".into(),
            terminal: Value::Null,
        };
        let m = compute(&[a, b], &[run_a, run_b]);
        assert_eq!(m.overall.pairs, 2);
        assert_eq!(m.overall.p50, Some(100));
        assert_eq!(m.overall.p95, Some(300));
        let last_seen = m
            .per_generator
            .iter()
            .find(|g| g.generator == Generator::LastSeen)
            .unwrap();
        let window = m
            .per_generator
            .iter()
            .find(|g| g.generator == Generator::WindowRecall)
            .unwrap();
        assert_eq!(last_seen.aggregate.known_hits, 1);
        assert_eq!(window.aggregate.known_hits, 0);
        assert_eq!(last_seen.aggregate.p50, Some(100));
        assert_eq!(window.aggregate.p50, Some(300));
    }
}
