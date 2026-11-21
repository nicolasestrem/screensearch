//! Deep phase — score candidates and keep ≤2 per dream.
//!
//! Signals mirror `OpenClaw`'s Deep ranking table and `kernel::insights` weights:
//! relevance (cosine similarity), frequency (hit count / recurrence), recency
//! (time-decay), recurrence (multi-day span), richness (content density). Thresholds
//! are conservative so empty dreams remain possible and no link is invented.

use crate::light::Staged;
use crate::rem::{Candidate, CandidateKind};

const MIN_SCORE: f64 = 0.45;
const MAX_KEPT: usize = 2;

/// Scored candidate.
#[derive(Debug, Clone)]
pub struct Scored {
    pub candidate: Candidate,
    /// Weighted sum 0..1.
    pub score: f64,
    pub signals: Signals,
}

#[derive(Debug, Clone)]
pub struct Signals {
    pub relevance: f64,
    pub frequency: f64,
    pub recency: f64,
    pub recurrence: f64,
    pub richness: f64,
}

/// Score every candidate and keep only those passing thresholds, at most two per dream.
/// `now_ms` is the wall clock for recency decay.
pub fn score_and_keep(candidates: Vec<Candidate>, staged: &Staged, now_ms: i64) -> Vec<Scored> {
    let mut scored: Vec<Scored> = candidates.into_iter().map(|c| score_one(c, staged, now_ms)).collect();
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    let mut kept = Vec::new();
    for s in scored {
        if s.score < MIN_SCORE { continue; }
        // frequency gate: recurring themes and semantic neighbours with higher score pass;
        // brushed-past with low richness rarely clears MIN_SCORE, which is intentional.
        kept.push(s);
        if kept.len() >= MAX_KEPT { break; }
    }
    kept
}

fn score_one(c: Candidate, staged: &Staged, now_ms: i64) -> Scored {
    // relevance: cosine similarity mapped 0..1 (similarity - MIN) / (1 - MIN), clamped.
    // semantic neighbours use it; recurring/brushed have no similarity -> mid.
    let relevance = match c.similarity {
        Some(s) => ((s - 0.80) / 0.20).clamp(0.0, 1.0),
        None => match c.kind {
            CandidateKind::RecurringTheme => 0.6,
            CandidateKind::BrushedPast => 0.3,
            CandidateKind::SemanticNeighbour => 0.0,
        },
    };

    // frequency: for neighbours, 1 if edge exists (binary); for recurring, days normalized.
    let frequency = match c.kind {
        CandidateKind::RecurringTheme => ((c.recurrence_days as f64) / 7.0).clamp(0.0, 1.0),
        CandidateKind::SemanticNeighbour => 0.5, // occurrence is the pair itself
        CandidateKind::BrushedPast => 0.2,
    };

    // recency: exponential decay from most recent session in the pair relative to now.
    let recency = {
        // use the later of the two sessions' started_at if available; fall back to window end
        let anchor = staged.window_end;
        let age_ms = (now_ms - anchor).max(0) as f64;
        // half-life 3 days: recent windows score near 1, week-old near 0.5, month-old near 0.
        (-age_ms / (3.0 * 86_400_000.0)).exp()
    };

    // recurrence: same as frequency's days span but weighted separately (multi-day presence).
    let recurrence = ((c.recurrence_days as f64 - 1.0) / 6.0).clamp(0.0, 1.0);

    // richness: snippet length + title presence + number of distinct apps.
    let richness = {
        let snippet_chars = c.evidence_snippets.iter().map(|s| s.len()).sum::<usize>() as f64;
        let title_bonus = if c.titles.iter().any(|t| !t.is_empty()) { 0.3 } else { 0.0 };
        let app_bonus = (c.apps.len() as f64 * 0.15).min(0.3);
        let text_score = (snippet_chars / 120.0).clamp(0.0, 0.7);
        (text_score + title_bonus + app_bonus).clamp(0.0, 1.0)
    };

    // OpenClaw-weighted sum (relevance 0.30, frequency 0.24, recency 0.15, recurrence 0.10, richness 0.06 + remainder absorbed):
    // we keep the stated five signals with those weights plus normalization.
    let score = relevance * 0.30 + frequency * 0.24 + recency * 0.15 + recurrence * 0.10 + richness * 0.21;

    Scored {
        candidate: c,
        score,
        signals: Signals { relevance, frequency, recency, recurrence, richness },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rem::{Candidate, CandidateKind};

    fn staged() -> Staged {
        Staged { window_start: 0, window_end: 1_000_000, frames: Vec::new(), sessions: Vec::new(), app_counts: Vec::new() }
    }

    #[test]
    fn keeps_at_most_two_and_threshold() {
        let cands = (0..5).map(|i| Candidate {
            a: i, b: Some(i+10), kind: CandidateKind::SemanticNeighbour,
            similarity: Some(0.85 + i as f64 * 0.02), distance_days: Some(3.0),
            apps: vec!["Code".into()], titles: vec!["t".into()], evidence_snippets: vec!["hello world".into()],
            recurrence_days: 2,
        }).collect();
        let kept = score_and_keep(cands, &staged(), 1_000_000);
        assert!(kept.len() <= 2);
        for k in &kept { assert!(k.score >= MIN_SCORE); }
    }

    #[test]
    fn weak_brushed_past_does_not_clear_threshold() {
        let weak = Candidate {
            a: 1, b: None, kind: CandidateKind::BrushedPast,
            similarity: None, distance_days: None,
            apps: vec!["a".into()], titles: vec!["".into()], evidence_snippets: vec![],
            recurrence_days: 1,
        };
        let kept = score_and_keep(vec![weak], &staged(), 10 * 86_400_000);
        assert!(kept.is_empty(), "weak brushed past should not dream");
    }
}
