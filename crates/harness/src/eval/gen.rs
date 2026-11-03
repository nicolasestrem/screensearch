//! The five self-generated question families (`docs/kickoffs/P4.md`, 0.4.5 D17).
//!
//! **Nothing here is hand-written ground truth.** Every pair is derived at run time from facts the
//! database already holds; the question text and the expected answer are generated from the same
//! row. There is no question list, no label file and no stored answer key anywhere in the crate.
//!
//! | generator | question | ground truth |
//! |---|---|---|
//! | `last_seen` | When did I last have `<file/document/domain>` open? | newest frames whose title, URL or content text contains the reference |
//! | `involvement` | Which sessions involved `<repo/domain>`? | sessions owning frames whose title or URL contains the reference, ordered by `started_at` |
//! | `sequence` | What was I doing before session `<Z>` (`<local time>`, `<ref>`)? | the session with the greatest `started_at` below `Z`'s, plus its dominant reference |
//! | `window_recall` | What did I work on `<date>` morning? | sessions overlapping local `[06:00, 12:00)`, plus their dominant references |
//! | `seen_again` | Where else did this text appear: `"<text>"`? | frames returned by exact FTS matching of a distinctive line |
//!
//! Sampling is seeded (`[SplitMix64]`); the pair *set* is a deterministic function of
//! `(db, seed, pairs, max_frames, now_ms)`. `now_ms` only matters for still-open sessions.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::facts::{Db, FrameCtx, SessionRow};
use super::prng::{sample, subseed, SplitMix64};
use super::refs::{extract_references, RefKind, Reference};

/// Default total pair count (`docs/kickoffs/P4.md`).
pub const DEFAULT_PAIRS: usize = 300;
/// Local morning window the `window_recall` generator asks about.
pub const MORNING_START_HOUR: u8 = 6;
pub const MORNING_END_HOUR: u8 = 12;
/// Frames inspected for reference discovery, dominant references and seen-again sampling. The
/// *involvement* index pass is exact over the whole table and does not use this slice.
pub const DISCOVERY_FRAMES: usize = 20_000;
/// Reference entities scored by the entity check, independent of `--pairs` so the recovery number
/// does not shrink when a smaller run is requested.
pub const REFERENCE_SAMPLE: usize = 400;
/// A pair keeps at most ten expected ids of each kind: the comparator is `@10`.
pub const MAX_EXPECTED_IDS: usize = 10;
/// Frames read per session when deriving its dominant reference.
const DOMINANT_SESSION_FRAMES: usize = 200;
/// A distinctive string may not appear in more than this many frames.
const MAX_SEEN_AGAIN_MATCHES: usize = 50;
const SEEN_AGAIN_MIN_CHARS: usize = 24;
const SEEN_AGAIN_MAX_CHARS: usize = 160;
const SEEN_AGAIN_PROBE_FRAMES: usize = 400;
const SEEN_AGAIN_LINES_PER_FRAME: usize = 4;

/// The five closed generator names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Generator {
    LastSeen,
    Involvement,
    Sequence,
    WindowRecall,
    SeenAgain,
}

impl Generator {
    /// Fixed order: also the deficit-redistribution order.
    pub const ALL: [Generator; 5] = [
        Generator::LastSeen,
        Generator::Involvement,
        Generator::Sequence,
        Generator::WindowRecall,
        Generator::SeenAgain,
    ];

    /// Stable JSON / file token.
    pub fn tag(self) -> &'static str {
        match self {
            Generator::LastSeen => "last_seen",
            Generator::Involvement => "involvement",
            Generator::Sequence => "sequence",
            Generator::WindowRecall => "window_recall",
            Generator::SeenAgain => "seen_again",
        }
    }

    /// Human label for the report table.
    pub fn label(self) -> &'static str {
        match self {
            Generator::LastSeen => "last-seen",
            Generator::Involvement => "involvement",
            Generator::Sequence => "sequence",
            Generator::WindowRecall => "window recall",
            Generator::SeenAgain => "seen-again",
        }
    }
}

/// A half-open `[start, end)` window in unix ms. A pair without a window means whole history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub start: i64,
    pub end: i64,
}

impl Window {
    pub fn contains(&self, ts: i64) -> bool {
        ts >= self.start && ts < self.end
    }

    pub fn overlaps(&self, start: i64, end: i64) -> bool {
        start < self.end && end > self.start
    }
}

/// One generated question/answer pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pair {
    pub id: String,
    pub generator: Generator,
    pub question: String,
    pub expected_frames: Vec<i64>,
    pub expected_sessions: Vec<i64>,
    /// The reference strings the generator derived; "the answer contains the known fact" means at
    /// least one of these (or `expected_local_date`) appears in the normalized answer.
    pub expected_strings: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<Window>,
    /// `last_seen` only: the local `YYYY-MM-DD` the fact implies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_local_date: Option<String>,
}

impl Pair {
    /// The ordered expected-id list the recall metric reads: frames first, then sessions.
    pub fn expected_order(&self) -> Vec<ExpectedId> {
        self.expected_frames
            .iter()
            .copied()
            .map(ExpectedId::Frame)
            .chain(
                self.expected_sessions
                    .iter()
                    .copied()
                    .map(ExpectedId::Session),
            )
            .collect()
    }
}

/// An expected evidence id, kept typed so the comparator can apply the session-through-frame rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedId {
    Frame(i64),
    Session(i64),
}

/// Per-generator generation accounting, so the report can be honest about the shortfall.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenStat {
    pub generator: Generator,
    /// Pairs the generator could produce from this database (a lower bound when `truncated`).
    pub eligible: usize,
    /// Whether enumeration stopped at its cap, i.e. `eligible` is a lower bound.
    pub truncated: bool,
    /// This generator's even share of the requested total.
    pub target: usize,
    /// Pairs actually emitted after deficit redistribution.
    pub generated: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Everything one generation pass learned, for the report.
#[derive(Debug, Clone)]
pub struct GenOutcome {
    pub pairs: Vec<Pair>,
    pub stats: Vec<GenStat>,
    pub frames_total: i64,
    pub discovery_frames: usize,
    pub reference_pass_frames: usize,
    pub reference_entities: Vec<Reference>,
    pub reference_discovered: usize,
    pub session_count: usize,
    pub history: Option<(i64, i64)>,
    pub now_ms: i64,
    pub seed: u64,
}

/// Generation inputs. `max_frames` caps the exact involvement pass (`None` = whole table).
#[derive(Debug, Clone, Copy)]
pub struct GenOpts {
    pub seed: u64,
    pub pairs: usize,
    pub max_frames: Option<usize>,
    pub now_ms: i64,
}

/// Generate the run's pairs. Deterministic for a given `(db, seed, pairs, max_frames, now_ms)`.
pub fn generate_pairs(db: &Db, opts: &GenOpts) -> Result<GenOutcome> {
    let frames_total = db.count_frames()?;
    let sessions = db.sessions()?;
    let history = db.history_bounds()?;
    let discovery = db.recent_frame_context(DISCOVERY_FRAMES)?;
    let discovered = discover_references(&discovery);
    let requested = opts.pairs.max(1);

    let mut ls_pool: Vec<Reference> = discovered
        .iter()
        .filter(|r| matches!(r.kind, RefKind::File | RefKind::Document | RefKind::Domain))
        .cloned()
        .collect();
    let mut inv_pool: Vec<Reference> = discovered
        .iter()
        .filter(|r| matches!(r.kind, RefKind::Repo | RefKind::Domain))
        .cloned()
        .collect();
    ls_pool.sort();
    ls_pool.dedup();
    inv_pool.sort();
    inv_pool.dedup();

    let ls_candidates = sample(
        &mut SplitMix64::new(subseed(opts.seed, "last_seen")),
        &ls_pool,
        requested,
    );
    let inv_candidates = sample(
        &mut SplitMix64::new(subseed(opts.seed, "involvement")),
        &inv_pool,
        requested,
    );

    // One exact pass over every frame's context columns builds the involvement index. Skipped
    // when the discovery slice contained no repo/domain reference at all.
    let keys: BTreeSet<(RefKind, String)> = inv_candidates
        .iter()
        .map(|r| (r.kind, r.normalized.clone()))
        .collect();
    let (index, reference_pass_frames) = if keys.is_empty() {
        (BTreeMap::new(), 0)
    } else {
        build_ref_index(db, &keys, opts.max_frames)?
    };

    let session_started: HashMap<i64, i64> =
        sessions.iter().map(|s| (s.id, s.started_at)).collect();
    let discovery_frames = discovery.len();
    let reference_discovered = discovered.len();
    let reference_entities = sample(
        &mut SplitMix64::new(subseed(opts.seed, "reference_entities")),
        &discovered,
        REFERENCE_SAMPLE,
    );

    let ctx = GenCtx {
        db,
        opts: *opts,
        sessions,
        session_started,
        discovery,
        ls_candidates,
        inv_candidates,
        ls_pool_len: ls_pool.len(),
        inv_pool_len: inv_pool.len(),
        index,
    };

    let targets = targets_for(requested);
    let mut cap = targets;
    let mut facts: Vec<Facts> = Vec::with_capacity(5);
    for (i, g) in Generator::ALL.iter().enumerate() {
        facts.push(ctx.generate(*g, cap[i])?);
    }
    let mut alloc = allocate(&targets, &available(&facts));
    // A short generator hands its deficit to the others in fixed order; grow their caps only when
    // the requested total is still unreachable.
    let mut guard = 0;
    while alloc.iter().sum::<usize>() < requested && guard < 2 {
        guard += 1;
        let mut grew = false;
        for (i, g) in Generator::ALL.iter().enumerate() {
            if !facts[i].exhausted && cap[i] < requested {
                cap[i] = requested;
                facts[i] = ctx.generate(*g, requested)?;
                grew = true;
            }
        }
        alloc = allocate(&targets, &available(&facts));
        if !grew {
            break;
        }
    }

    let mut pairs = Vec::new();
    let mut stats = Vec::with_capacity(5);
    for (i, g) in Generator::ALL.iter().enumerate() {
        for (n, pair) in facts[i].pairs.iter().take(alloc[i]).enumerate() {
            let mut pair = pair.clone();
            pair.id = format!("{}-{:03}", g.tag(), n + 1);
            pairs.push(pair);
        }
        stats.push(GenStat {
            generator: *g,
            eligible: facts[i].pairs.len(),
            truncated: !facts[i].exhausted,
            target: targets[i],
            generated: alloc[i],
            note: facts[i].note.clone(),
        });
    }

    Ok(GenOutcome {
        pairs,
        stats,
        frames_total,
        discovery_frames,
        reference_pass_frames,
        reference_entities,
        reference_discovered,
        session_count: ctx.sessions.len(),
        history,
        now_ms: opts.now_ms,
        seed: opts.seed,
    })
}

/// Even share of `total`, remainder to the earliest generators in fixed order.
pub fn targets_for(total: usize) -> [usize; 5] {
    let base = total / 5;
    let mut targets = [base; 5];
    for slot in targets.iter_mut().take(total % 5) {
        *slot += 1;
    }
    targets
}

/// `alloc[i] = min(target[i], available[i])`, then the remaining total is handed out one pair at a
/// time in fixed generator order, round-robin, skipping exhausted generators.
pub fn allocate(targets: &[usize; 5], available: &[usize; 5]) -> [usize; 5] {
    let mut alloc = [0usize; 5];
    for i in 0..5 {
        alloc[i] = targets[i].min(available[i]);
    }
    let mut deficit = targets.iter().sum::<usize>() - alloc.iter().sum::<usize>();
    while deficit > 0 {
        let mut progressed = false;
        for i in 0..5 {
            if deficit == 0 {
                break;
            }
            if alloc[i] < available[i] {
                alloc[i] += 1;
                deficit -= 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    alloc
}

fn available(facts: &[Facts]) -> [usize; 5] {
    let mut out = [0usize; 5];
    for (i, f) in facts.iter().enumerate() {
        out[i] = f.pairs.len();
    }
    out
}

/// Enumeration result for one generator.
struct Facts {
    pairs: Vec<Pair>,
    /// True when enumeration reached the end of its candidate pool, i.e. no more units exist.
    exhausted: bool,
    note: Option<String>,
}

impl Facts {
    fn full(pairs: Vec<Pair>, exhausted: bool) -> Self {
        Self {
            pairs,
            exhausted,
            note: None,
        }
    }
}

/// A reference's exact match statistics from the whole-table context pass.
#[derive(Default, Clone)]
struct RefStat {
    newest: Vec<(i64, i64, Option<i64>)>,
    session_ids: BTreeSet<i64>,
}

type RefIndex = BTreeMap<(RefKind, String), RefStat>;

/// Discovery pool: every distinct reference in the newest `DISCOVERY_FRAMES` frames' title + URL.
fn discover_references(frames: &[FrameCtx]) -> Vec<Reference> {
    let mut out = BTreeSet::new();
    for f in frames {
        let text = format!("{}\n{}", f.window_title, f.browser_url);
        for r in extract_references(&text) {
            out.insert(r);
        }
    }
    out.into_iter().collect()
}

/// One exact pass over frame context columns, newest first, accumulating the index entries the
/// involvement generator reads. Returns the number of frames scanned.
fn build_ref_index(
    db: &Db,
    keys: &BTreeSet<(RefKind, String)>,
    max_frames: Option<usize>,
) -> Result<(RefIndex, usize)> {
    let mut index: RefIndex = keys
        .iter()
        .cloned()
        .map(|k| (k, RefStat::default()))
        .collect();
    let scanned = db.for_each_frame_context(max_frames, |f| {
        let text = format!("{}\n{}", f.window_title, f.browser_url);
        for r in extract_references(&text) {
            if let Some(stat) = index.get_mut(&(r.kind, r.normalized)) {
                if stat.newest.len() < MAX_EXPECTED_IDS {
                    stat.newest.push((f.frame_id, f.captured_at, f.session_id));
                }
                if let Some(sid) = f.session_id {
                    stat.session_ids.insert(sid);
                }
            }
        }
    })?;
    Ok((index, scanned))
}

struct GenCtx<'a> {
    db: &'a Db,
    opts: GenOpts,
    sessions: Vec<SessionRow>,
    session_started: HashMap<i64, i64>,
    discovery: Vec<FrameCtx>,
    ls_candidates: Vec<Reference>,
    inv_candidates: Vec<Reference>,
    ls_pool_len: usize,
    inv_pool_len: usize,
    index: RefIndex,
}

impl GenCtx<'_> {
    fn generate(&self, generator: Generator, cap: usize) -> Result<Facts> {
        let cap = cap.max(1);
        match generator {
            Generator::LastSeen => self.last_seen(cap),
            Generator::Involvement => self.involvement(cap),
            Generator::Sequence => self.sequence(cap),
            Generator::WindowRecall => self.window_recall(cap),
            Generator::SeenAgain => self.seen_again(cap),
        }
    }

    /// "When did I last have `<file / document / domain>` open?"
    fn last_seen(&self, cap: usize) -> Result<Facts> {
        let mut pairs = Vec::new();
        let mut hit_cap = false;
        for r in &self.ls_candidates {
            if pairs.len() >= cap {
                hit_cap = true;
                break;
            }
            let rows = self.db.last_seen_frames(&r.text, MAX_EXPECTED_IDS)?;
            let Some(&(_, ts, session_id)) = rows.first() else {
                continue;
            };
            pairs.push(Pair {
                id: String::new(),
                generator: Generator::LastSeen,
                question: format!("When did I last have `{}` open?", r.text),
                expected_frames: rows.iter().map(|(id, _, _)| *id).collect(),
                expected_sessions: session_id.into_iter().collect(),
                expected_strings: vec![r.text.clone()],
                window: None,
                expected_local_date: Some(self.db.local_date(ts)?),
            });
        }
        Ok(Facts::full(
            pairs,
            !hit_cap && self.ls_pool_len <= self.ls_candidates.len(),
        ))
    }

    /// "Which sessions involved `<repo / domain>`?"
    fn involvement(&self, cap: usize) -> Result<Facts> {
        let mut pairs = Vec::new();
        let mut hit_cap = false;
        for r in &self.inv_candidates {
            if pairs.len() >= cap {
                hit_cap = true;
                break;
            }
            let Some(stat) = self.index.get(&(r.kind, r.normalized.clone())) else {
                continue;
            };
            if stat.newest.is_empty() {
                continue;
            }
            let mut sessions: Vec<(i64, i64)> = stat
                .session_ids
                .iter()
                .filter_map(|id| self.session_started.get(id).map(|started| (*id, *started)))
                .collect();
            sessions.sort_by_key(|(id, started)| (*started, *id));
            pairs.push(Pair {
                id: String::new(),
                generator: Generator::Involvement,
                question: format!("Which sessions involved `{}`?", r.text),
                expected_frames: stat.newest.iter().map(|(id, _, _)| *id).collect(),
                expected_sessions: sessions
                    .iter()
                    .take(MAX_EXPECTED_IDS)
                    .map(|(id, _)| *id)
                    .collect(),
                expected_strings: vec![r.text.clone()],
                window: None,
                expected_local_date: None,
            });
        }
        Ok(Facts::full(
            pairs,
            !hit_cap && self.inv_pool_len <= self.inv_candidates.len(),
        ))
    }

    /// "What was I doing before session `<Z>`?"
    fn sequence(&self, cap: usize) -> Result<Facts> {
        let mut pairs = Vec::new();
        let candidates: Vec<usize> = (1..self.sessions.len()).collect();
        let picked = sample(
            &mut SplitMix64::new(subseed(self.opts.seed, "sequence")),
            &candidates,
            cap,
        );
        let history_start = match self.db.history_bounds()? {
            Some((start, _)) => start,
            None => return Ok(Facts::full(Vec::new(), true)),
        };
        for i in picked {
            let z = &self.sessions[i];
            let prev = &self.sessions[i - 1];
            let Some((dominant, _)) = dominant_reference(self.db, prev.id)? else {
                continue;
            };
            let hint = dominant_reference(self.db, z.id)?.map(|(r, _)| r.text);
            let datetime = self.db.local_datetime(z.started_at)?;
            let question = match hint {
                Some(hint) => format!(
                    "What was I doing before session {} ({}, {})?",
                    z.id, datetime, hint
                ),
                None => format!("What was I doing before session {} ({})?", z.id, datetime),
            };
            let frames = self.db.session_frames(prev.id, MAX_EXPECTED_IDS)?;
            pairs.push(Pair {
                id: String::new(),
                generator: Generator::Sequence,
                question,
                expected_frames: frames.iter().rev().map(|f| f.frame_id).collect(),
                expected_sessions: vec![prev.id],
                expected_strings: vec![dominant.text],
                // "before Z": everything up to and including the instant Z began.
                window: Some(Window {
                    start: history_start,
                    end: z.started_at.saturating_add(1),
                }),
                expected_local_date: None,
            });
        }
        Ok(Facts::full(pairs, candidates.len() <= cap))
    }

    /// "What did I work on `<date>` morning?"
    fn window_recall(&self, cap: usize) -> Result<Facts> {
        let days = self.db.local_days()?;
        let picked = sample(
            &mut SplitMix64::new(subseed(self.opts.seed, "window_recall")),
            &days,
            cap,
        );
        let mut pairs = Vec::new();
        for day in &picked {
            let (start, end) =
                self.db
                    .local_hour_window(day, MORNING_START_HOUR, MORNING_END_HOUR)?;
            let sessions = self.db.sessions_overlapping(start, end, self.opts.now_ms)?;
            let mut strings = Vec::new();
            let mut session_ids = Vec::new();
            let mut frames: Vec<(i64, i64)> = Vec::new();
            for s in sessions.iter().take(MAX_EXPECTED_IDS) {
                let Some((dominant, _)) = dominant_reference(self.db, s.id)? else {
                    continue;
                };
                strings.push(dominant.text);
                session_ids.push(s.id);
                for f in self.db.session_frames(s.id, MAX_EXPECTED_IDS)? {
                    frames.push((f.captured_at, f.frame_id));
                }
            }
            if strings.is_empty() {
                continue;
            }
            frames.sort_unstable_by(|a, b| b.cmp(a));
            frames.dedup();
            pairs.push(Pair {
                id: String::new(),
                generator: Generator::WindowRecall,
                question: format!("What did I work on {day} morning?"),
                expected_frames: frames
                    .iter()
                    .take(MAX_EXPECTED_IDS)
                    .map(|(_, id)| *id)
                    .collect(),
                expected_sessions: session_ids,
                expected_strings: strings,
                window: Some(Window { start, end }),
                // No `expected_local_date`: the question already contains the date, so accepting
                // it as the known fact would let an echo pass.
                expected_local_date: None,
            });
        }
        Ok(Facts::full(pairs, days.len() <= cap))
    }

    /// "Where else did this text appear: `<text>`?"
    fn seen_again(&self, cap: usize) -> Result<Facts> {
        if !self.db.has_fts() {
            return Ok(Facts {
                pairs: Vec::new(),
                exhausted: true,
                note: Some("frame_text_fts is missing: seen-again cannot be measured".into()),
            });
        }
        let ids: Vec<i64> = self.discovery.iter().map(|f| f.frame_id).collect();
        let probe = SEEN_AGAIN_PROBE_FRAMES.max(cap * 4).min(ids.len());
        let sampled = sample(
            &mut SplitMix64::new(subseed(self.opts.seed, "seen_again")),
            &ids,
            probe,
        );
        let texts = self.db.content_texts(&sampled)?;
        let mut pairs = Vec::new();
        let mut used: BTreeSet<String> = BTreeSet::new();
        for (_, text) in &texts {
            if pairs.len() >= cap {
                return Ok(Facts::full(pairs, false));
            }
            for line in candidate_lines(text) {
                if pairs.len() >= cap {
                    break;
                }
                // Two frames carrying the same line yield one pair, not two identical questions.
                if !used.insert(line.clone()) {
                    continue;
                }
                let matches = self
                    .db
                    .fts_matches(&quote_phrase(&line), MAX_SEEN_AGAIN_MATCHES + 1)?;
                if matches.len() < 2 || matches.len() > MAX_SEEN_AGAIN_MATCHES {
                    continue;
                }
                let mut session_ids: Vec<i64> = Vec::new();
                for (_, _, sid) in &matches {
                    if let Some(sid) = sid {
                        if !session_ids.contains(sid) {
                            session_ids.push(*sid);
                        }
                    }
                }
                pairs.push(Pair {
                    id: String::new(),
                    generator: Generator::SeenAgain,
                    question: format!("Where else did this text appear: \"{}\"?", line),
                    expected_frames: matches
                        .iter()
                        .take(MAX_EXPECTED_IDS)
                        .map(|(id, _, _)| *id)
                        .collect(),
                    expected_sessions: session_ids.into_iter().take(MAX_EXPECTED_IDS).collect(),
                    expected_strings: vec![line],
                    window: None,
                    expected_local_date: None,
                });
            }
        }
        Ok(Facts::full(pairs, sampled.len() >= ids.len()))
    }
}

/// The session's dominant reference: the reference carried by the most distinct frames, ties broken
/// by `(kind, text)` so the choice is deterministic.
fn dominant_reference(db: &Db, session_id: i64) -> Result<Option<(Reference, usize)>> {
    let frames = db.session_frames(session_id, DOMINANT_SESSION_FRAMES)?;
    let mut counts: BTreeMap<(RefKind, String), (Reference, usize)> = BTreeMap::new();
    for f in &frames {
        let text = format!("{}\n{}", f.window_title, f.browser_url);
        for r in extract_references(&text) {
            counts
                .entry((r.kind, r.normalized.clone()))
                .and_modify(|e| e.1 += 1)
                .or_insert((r, 1));
        }
    }
    let mut ranked: Vec<(Reference, usize)> = counts.into_values().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(ranked.into_iter().next())
}

/// Candidate distinctive lines from one content body: whitespace-collapsed, 24..=160 chars, no
/// double quote (the FTS phrase delimiter), at least one letter, first occurrences only.
fn candidate_lines(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.split_whitespace().collect::<Vec<_>>().join(" ");
        let len = line.chars().count();
        if !(SEEN_AGAIN_MIN_CHARS..=SEEN_AGAIN_MAX_CHARS).contains(&len) {
            continue;
        }
        if line.contains('"') || !line.chars().any(|c| c.is_alphabetic()) {
            continue;
        }
        if out.contains(&line) {
            continue;
        }
        out.push(line);
        if out.len() >= SEEN_AGAIN_LINES_PER_FRAME {
            break;
        }
    }
    out
}

fn quote_phrase(line: &str) -> String {
    format!("\"{}\"", line.replace('"', ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_split_evenly_and_remainder_goes_first() {
        assert_eq!(targets_for(300), [60, 60, 60, 60, 60]);
        assert_eq!(targets_for(10), [2, 2, 2, 2, 2]);
        assert_eq!(targets_for(12), [3, 3, 2, 2, 2]);
        assert_eq!(targets_for(0), [0, 0, 0, 0, 0]);
    }

    #[test]
    fn deficit_moves_in_fixed_order_and_respects_capacity() {
        // last-seen has only 2 units, everything else is rich: its 58 go to the others in order.
        let alloc = allocate(&[60, 60, 60, 60, 60], &[2, 1000, 1000, 1000, 1000]);
        assert_eq!(alloc[0], 2);
        assert_eq!(alloc.iter().sum::<usize>(), 300);
        // 58 pairs over four rich generators: 14 rounds of one each, then the last two go to the
        // earliest generators in fixed order.
        assert_eq!(alloc, [2, 75, 75, 74, 74]);

        // Every generator short: the request cannot be met and nothing is invented.
        let alloc = allocate(&[60, 60, 60, 60, 60], &[3, 4, 5, 6, 7]);
        assert_eq!(alloc, [3, 4, 5, 6, 7]);
    }

    #[test]
    fn candidate_lines_respects_bounds_and_dedupes() {
        let long = "a".repeat(200);
        let text = format!(
            "short line\n\
             the quarterly migration checklist item seven is done\n\
             the quarterly migration checklist item seven is done\n\
             {long}\n\
             a line with a \" quote inside that is long enough\n"
        );
        let lines = candidate_lines(&text);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0],
            "the quarterly migration checklist item seven is done"
        );
    }
}
