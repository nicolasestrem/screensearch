//! Light -> REM -> Deep over each absence, after the OpenClaw phase model.
//!
//! The unit of recall is the **episode**: one window inside one session, with the
//! normalised mean of its frames' stored embeddings. Sessions mix unrelated documents, so a
//! whole-session centroid blurs meaning; an episode does not.
//!
//! * **Themes** are the short-term candidates. Episodes join themes in time order (leader
//!   clustering on cosine): an episode only ever sees earlier episodes, so a sweep never uses
//!   the future, and the assignment is deterministic.
//! * **Light** stages what the screen held since the previous absence: dwell-ranked windows,
//!   windows only brushed past, the strongest content lines, and any text the app itself
//!   had saved for the range (daily report, weekly synthesis, project note). Each staged
//!   episode is a Light hit on its theme. No durable write.
//! * **REM** recalls, for every staged episode, the nearest episode at least
//!   [`ECHO_MIN_DAYS`] earlier that is a *different* document (another window title) and
//!   shares few lines, by cosine over the stored embeddings. Those are echoes: close in
//!   meaning, far in time, not a shared keyword. The work graph adds entity recurrences
//!   through the kernel's own `derive_recurrence` / `derive_resumption`.
//! * **Deep** scores every theme the sweep touched with the six OpenClaw weights plus a
//!   recency-decayed Light/REM boost, applies the three gates and promotes at most
//!   [`Gates::max_per_sweep`] new memories; promoted themes are reinforced later.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use traits::{InsightPayload, NewInsight};

use super::data::{is_content, Absence, Episode, History};

const DAY_MS: f64 = 86_400_000.0;
/// Two episodes must be at least this far apart to count as an echo across time.
pub const ECHO_MIN_DAYS: f64 = 3.0;
/// The graph's look-back, the kernel's insight window.
const GRAPH_LOOKBACK_MS: i64 = kernel::insights::LOOKBACK_MS;
/// Two episodes whose content lines overlap more than this are the same text, not an echo.
const MAX_LINE_OVERLAP: f32 = 0.5;
/// Theme index of an episode that carries no content and joins no theme.
const NO_THEME: usize = usize::MAX;

/// The three promotion gates (all must pass) and the per-sweep promotion cap.
#[derive(Debug, Clone)]
pub struct Gates {
    pub min_score: f64,
    pub min_recall_count: u32,
    pub min_unique_queries: u32,
    pub max_per_sweep: usize,
}

impl Default for Gates {
    fn default() -> Self {
        Self {
            min_score: 0.75,
            min_recall_count: 4,
            min_unique_queries: 4,
            max_per_sweep: 1,
        }
    }
}

/// The six OpenClaw weights: relevance, frequency, query diversity, recency,
/// consolidation, conceptual richness.
pub const WEIGHTS: [f64; 6] = [0.30, 0.24, 0.15, 0.15, 0.10, 0.06];
pub const SIGNALS: [&str; 6] = [
    "relevance",
    "frequency",
    "query diversity",
    "recency",
    "consolidation",
    "richness",
];

/// A pointer into the database: the diary's evidence tuple.
#[derive(Debug, Clone, PartialEq)]
pub struct Evidence {
    pub frame: usize,
    pub at: i64,
    pub app: String,
    pub title: String,
    pub fragment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Revision {
    pub past: Evidence,
    pub now: Evidence,
    pub added: bool,
}

#[derive(Debug, Clone)]
pub struct Echo {
    pub now: Evidence,
    pub past: Evidence,
    pub similarity: f32,
    pub line_overlap: f32,
    pub days_apart: f64,
}

#[derive(Debug, Clone)]
pub struct GraphEcho {
    pub kind: &'static str,
    pub label: String,
    pub first: Evidence,
    pub second: Evidence,
    pub gap_days: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Light {
    pub sessions: usize,
    pub minutes: i64,
    pub occupied: Vec<(Evidence, usize)>,
    pub last: Option<Evidence>,
    pub brushed: Vec<Evidence>,
    pub lines: Vec<Evidence>,
    pub saved: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct Scored {
    pub key: usize,
    pub label: String,
    pub score: f64,
    pub parts: [f64; 6],
    pub boost: f64,
    pub recall: u32,
    pub queries: u32,
    pub probes: Vec<String>,
    pub days: Vec<String>,
    pub tags: Vec<String>,
    pub evidence: Vec<Evidence>,
    /// `None` when every gate passed.
    pub failed_gate: Option<String>,
    pub already_promoted: bool,
}

#[derive(Debug, Clone)]
pub struct SweepOutcome {
    pub number: usize,
    pub absence: Absence,
    pub light: Light,
    pub echoes: Vec<Echo>,
    pub graph: Vec<GraphEcho>,
    pub revisions: Vec<Revision>,
    pub fresh: Vec<Evidence>,
    pub promoted: Vec<Scored>,
    pub reinforced: Vec<Scored>,
}

pub struct Outcome {
    pub sweeps: Vec<SweepOutcome>,
    pub gates: Gates,
    pub vector_source: &'static str,
    pub echo_floor: f32,
    pub theme_floor: f32,
    pub themes: usize,
    pub episodes: usize,
}

impl Outcome {
    pub fn promoted_total(&self) -> usize {
        self.sweeps.iter().map(|s| s.promoted.len()).sum()
    }
}

/// A theme's short-term recall state.
#[derive(Debug, Default, Clone)]
struct Theme {
    members: Vec<usize>,
    days: BTreeSet<String>,
    recalls: u32,
    probes: BTreeSet<String>,
    similarities: Vec<f32>,
    light_hits: Vec<i64>,
    rem_hits: Vec<i64>,
    last_touch: i64,
    tags: BTreeSet<String>,
    promoted: bool,
}

pub fn friendly_app(hint: &str) -> String {
    match hint.to_ascii_lowercase().as_str() {
        "excel" => "Excel",
        "winword" => "Word",
        "outlook" => "Outlook",
        "powerpnt" => "PowerPoint",
        "chrome" => "Chrome",
        "msedge" => "Edge",
        "firefox" => "Firefox",
        "explorer" => "File Explorer",
        "acrobat" | "acrord32" => "Acrobat Reader",
        "whatsapp" => "WhatsApp",
        "thunderbird" => "Thunderbird",
        "telegram" => "Telegram",
        "teams" | "ms-teams" => "Teams",
        "notepad" => "Notepad",
        "onenote" => "OneNote",
        _ => return hint.to_string(),
    }
    .to_string()
}

/// A window title without its trailing " - Application" part.
pub fn stem(title: &str) -> String {
    match title.rsplit_once(" - ") {
        Some((head, _)) if !head.trim().is_empty() => head.trim().to_string(),
        _ => title.trim().to_string(),
    }
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    let union = a.union(b).count() as f32;
    if union == 0.0 {
        return 0.0;
    }
    a.intersection(b).count() as f32 / union
}

fn percentile(mut v: Vec<f32>, p: usize) -> f32 {
    if v.is_empty() {
        return 1.1;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) * p) / 100]
}

/// The episode's evidence: the longest content line it showed, and when.
fn evidence_of(history: &History, e: &Episode) -> Evidence {
    select_evidence(history, e, |_| true).unwrap_or_else(|| evidence_at(history, e.frames[0], None))
}

fn select_evidence(
    history: &History,
    e: &Episode,
    eligible: impl Fn(&str) -> bool,
) -> Option<Evidence> {
    let mut best: Option<(usize, &str)> = None;
    for &fi in &e.frames {
        let f = &history.frames[fi];
        for l in f
            .lines
            .iter()
            .filter(|l| eligible(l) && history.content_line(f, l))
        {
            if best.is_none_or(|(_, b)| l.len() >= b.len()) {
                best = Some((fi, l));
            }
        }
    }
    best.map(|(fi, line)| evidence_at(history, fi, Some(line)))
}

fn evidence_at(history: &History, frame: usize, fragment: Option<&str>) -> Evidence {
    let f = &history.frames[frame];
    Evidence {
        frame,
        at: f.at,
        app: friendly_app(&f.app),
        title: f.title.clone(),
        fragment: fragment.map(str::to_string),
    }
}

/// Two episodes can echo only across applications: within one application the shared
/// layout dominates the embedding, which would make every spreadsheet "remember" every
/// other spreadsheet.
fn can_echo(a: &Episode, b: &Episode) -> bool {
    a.app != b.app && stem(&a.title) != stem(&b.title)
}

/// Pairwise similarities between recallable episodes of different applications at least
/// [`ECHO_MIN_DAYS`] apart. The echo floor is their 95th percentile and the theme floor
/// their 90th, so both bars adapt to the embedding model and to the history.
fn floors(episodes: &[Episode], recallable: &[bool]) -> (f32, f32) {
    let mut sims = Vec::new();
    let idx: Vec<usize> = (0..episodes.len()).filter(|&i| recallable[i]).collect();
    let step = (idx.len() / 400).max(1);
    for (k, &i) in idx.iter().enumerate().step_by(step) {
        for &j in idx.iter().skip(k + 1).step_by(step) {
            let (a, b) = (&episodes[i], &episodes[j]);
            if ((b.start - a.end).abs() as f64) >= ECHO_MIN_DAYS * DAY_MS && can_echo(a, b) {
                sims.push(dot(&a.vector, &b.vector));
            }
        }
    }
    (percentile(sims.clone(), 95), percentile(sims, 90))
}

/// Leader clustering in time order: an episode joins the most similar existing theme when
/// the cosine to its centroid reaches `floor`, otherwise starts a new theme.
fn assign_themes(episodes: &[Episode], recallable: &[bool], floor: f32) -> (Vec<usize>, usize) {
    let mut centroids: Vec<Vec<f32>> = Vec::new();
    let mut of = Vec::with_capacity(episodes.len());
    for (k, e) in episodes.iter().enumerate() {
        if !recallable[k] {
            of.push(NO_THEME);
            continue;
        }
        let best = centroids
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let norm = c
                    .iter()
                    .map(|x| x * x)
                    .sum::<f32>()
                    .sqrt()
                    .max(f32::EPSILON);
                (i, dot(c, &e.vector) / norm)
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((i, sim)) if sim >= floor => {
                centroids[i]
                    .iter_mut()
                    .zip(&e.vector)
                    .for_each(|(a, b)| *a += b);
                of.push(i);
            }
            _ => {
                centroids.push(e.vector.clone());
                of.push(centroids.len() - 1);
            }
        }
    }
    (of, centroids.len())
}

fn theme_label(history: &History, theme: &Theme) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for &m in &theme.members {
        *counts.entry(stem(&history.episodes[m].title)).or_default() += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter()
        .take(2)
        .map(|(s, _)| s)
        .collect::<Vec<_>>()
        .join(" / ")
}

fn graph_echoes(
    history: &History,
    absence: &Absence,
    staged_sessions: &BTreeSet<i64>,
    theme_of_episode: &[usize],
    themes: &mut [Theme],
) -> Vec<GraphEcho> {
    let window_end = absence.start + 1;
    let window_start = window_end - GRAPH_LOOKBACK_MS;
    // The episode of a session that names the entity (or its longest one).
    let pick = |session: i64, name: &str, current: bool| -> Option<usize> {
        let lowered = name.to_lowercase();
        let in_session: Vec<usize> = history
            .episodes
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.session == session
                    && e.end <= absence.start
                    && (!current || e.start >= absence.staged_from)
            })
            .map(|(i, _)| i)
            .collect();
        in_session
            .iter()
            .copied()
            .find(|&i| history.episodes[i].title.to_lowercase().contains(&lowered))
            .or_else(|| {
                in_session
                    .iter()
                    .copied()
                    .max_by_key(|&i| history.episodes[i].frames.len())
            })
    };
    let mut out = Vec::new();
    let mut handle = |insight: NewInsight| {
        let (kind, label, first_s, second_s, gap) = match insight.payload {
            InsightPayload::Recurrence(p) if staged_sessions.contains(&p.second_session_id) => (
                "recurrence",
                p.entity_name,
                p.first_session_id,
                p.second_session_id,
                p.gap_ms,
            ),
            InsightPayload::Resumption(p) if staged_sessions.contains(&p.resumed_session_id) => (
                "resumption",
                p.title,
                p.previous_session_id,
                p.resumed_session_id,
                p.gap_ms,
            ),
            _ => return,
        };
        let (Some(a), Some(b)) = (pick(first_s, &label, false), pick(second_s, &label, true))
        else {
            return;
        };
        for t in BTreeSet::from([theme_of_episode[a], theme_of_episode[b]]) {
            if t == NO_THEME {
                continue;
            }
            let theme = &mut themes[t];
            theme.recalls += 1;
            theme.probes.insert(format!("work graph: {label}"));
            theme.rem_hits.push(absence.start);
            theme.tags.insert(label.to_lowercase());
        }
        out.push(GraphEcho {
            kind,
            label,
            first: evidence_of(history, &history.episodes[a]),
            second: evidence_of(history, &history.episodes[b]),
            gap_days: gap as f64 / DAY_MS,
        });
    };
    let entity_rows: Vec<_> = history
        .entity_rows
        .iter()
        .filter(|r| r.session_started_at < window_end)
        .cloned()
        .collect();
    for insight in kernel::insights::derive_recurrence(&entity_rows, window_start, window_end) {
        handle(insight);
    }
    let thread_rows: Vec<_> = history
        .thread_rows
        .iter()
        .filter(|r| r.started_at < window_end)
        .cloned()
        .collect();
    for insight in kernel::insights::derive_resumption(&thread_rows, window_start, window_end) {
        handle(insight);
    }
    out.sort_by(|a, b| b.gap_days.total_cmp(&a.gap_days));
    out.truncate(2);
    out
}

pub fn dream(history: &History, absences: &[Absence], gates: &Gates) -> Outcome {
    let episodes = &history.episodes;
    let lines: Vec<HashSet<String>> = episodes
        .iter()
        .map(|e| history.episode_lines(e, 30).into_iter().collect())
        .collect();
    // An episode with no content line (a bare file listing, an empty window) shows no
    // meaning to recall: it is staged by Light but never compared by REM.
    let recallable: Vec<bool> = lines.iter().map(|l| !l.is_empty()).collect();
    let (echo_floor, theme_floor) = floors(episodes, &recallable);
    let (theme_of, theme_count) = assign_themes(episodes, &recallable, theme_floor);
    let mut themes: Vec<Theme> = vec![Theme::default(); theme_count];
    if std::env::var_os("DREAM_DEBUG").is_some() {
        eprintln!(
            "dream: episodes={} themes={theme_count} echo_floor={echo_floor:.3} theme_floor={theme_floor:.3}",
            episodes.len()
        );
    }
    let mut sweeps = Vec::new();
    let mut latest_windows: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut seen_echoes = HashSet::new();
    let mut seen_graph = HashSet::new();

    for absence in absences {
        let now = absence.start;
        let staged: Vec<usize> = (0..episodes.len())
            .filter(|&i| episodes[i].start >= absence.staged_from && episodes[i].start <= now)
            .collect();
        let staged_sessions: BTreeSet<i64> = staged.iter().map(|&i| episodes[i].session).collect();
        let mut touched: BTreeSet<usize> = BTreeSet::new();

        // ---- Light ----------------------------------------------------------------------
        let mut light = Light {
            sessions: staged_sessions.len(),
            minutes: staged_sessions
                .iter()
                .filter_map(|id| history.session(*id))
                .map(|s| (s.end.min(now) - s.start.max(absence.staged_from)).max(0))
                .sum::<i64>()
                / 60_000,
            ..Light::default()
        };
        for &i in &staged {
            let e = &episodes[i];
            let t = theme_of[i];
            if t == NO_THEME {
                continue;
            }
            let theme = &mut themes[t];
            theme.members.push(i);
            theme.days.insert(history.day(e.start));
            theme.light_hits.push(now);
            theme.last_touch = theme.last_touch.max(e.end);
            if let Some(s) = history.session(e.session) {
                for (_, _, name) in &s.entities {
                    if e.title.to_lowercase().contains(&name.to_lowercase()) {
                        theme.tags.insert(name.to_lowercase());
                    }
                }
            }
            touched.insert(t);
        }
        // Windows by dwell across the staged range: (frames, first, last, longest episode).
        let mut windows: BTreeMap<(String, String), (usize, i64, usize)> = BTreeMap::new();
        for &i in &staged {
            let e = &episodes[i];
            let w = windows
                .entry((e.app.clone(), e.title.clone()))
                .or_insert((0, e.start, i));
            w.0 += e.frames.len();
            if episodes[w.2].frames.len() < e.frames.len() {
                w.2 = i;
            }
        }
        let mut ranked: Vec<_> = windows.values().copied().collect();
        ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (n, _, i) in ranked.iter().take(3) {
            light
                .occupied
                .push((evidence_of(history, &episodes[*i]), *n));
        }
        if let Some(fi) = staged
            .iter()
            .flat_map(|&i| episodes[i].frames.iter().copied())
            .filter(|&fi| history.frames[fi].at <= now)
            .max()
        {
            light.last = Some(evidence_at(history, fi, None));
        }
        for (n, _, i) in ranked.iter().rev() {
            let ev = evidence_of(history, &episodes[*i]);
            if *n == 1
                && light.brushed.len() < 2
                && !light.occupied.iter().any(|o| o.0.title == ev.title)
            {
                light.brushed.push(Evidence {
                    fragment: None,
                    ..ev
                });
            }
        }
        let mut seen = HashSet::new();
        for (_, _, i) in ranked.iter().take(6) {
            let ev = evidence_of(history, &episodes[*i]);
            if let Some(f) = &ev.fragment {
                if light.lines.len() < 4 && seen.insert(f.clone()) {
                    light.lines.push(ev);
                }
            }
        }
        for saved in history
            .saved
            .iter()
            .filter(|t| t.at > absence.staged_from && t.at <= absence.end)
        {
            let head = saved
                .text
                .lines()
                .find(|l| is_content(l))
                .unwrap_or("")
                .trim()
                .to_string();
            light
                .saved
                .push((saved.label.clone(), head.chars().take(120).collect()));
        }

        // Compare against the latest earlier visit to this window, before advancing its state.
        let mut revisions = Vec::new();
        let mut fresh = Vec::new();
        for &i in &staged {
            if !recallable[i] {
                continue;
            }
            let e = &episodes[i];
            let key = (e.app.clone(), e.title.clone());
            if let Some(p) = latest_windows.insert(key, i) {
                let present = |episode: &Episode, line: &str| {
                    episode
                        .frames
                        .iter()
                        .any(|&fi| history.frames[fi].lines.iter().any(|l| l == line))
                };
                let previous = &episodes[p];
                let Some(current) = select_evidence(history, e, |l| !present(previous, l)) else {
                    continue;
                };
                let removed = select_evidence(history, previous, |l| !present(e, l));
                revisions.push(Revision {
                    added: removed.is_none(),
                    past: removed.unwrap_or_else(|| evidence_of(history, previous)),
                    now: current,
                });
            } else {
                fresh.push(evidence_of(history, e));
            }
        }
        revisions.sort_by_key(|r| r.added);
        revisions.truncate(3);
        fresh.truncate(3);

        // ---- REM ------------------------------------------------------------------------
        let mut echoes: Vec<(f32, Echo)> = Vec::new();
        for &i in &staged {
            let e = &episodes[i];
            if !recallable[i] {
                continue;
            }
            let e_stem = stem(&e.title);
            let mut best: Option<(f32, usize, f32)> = None;
            for (j, p) in episodes.iter().enumerate() {
                if ((e.start - p.end) as f64) < ECHO_MIN_DAYS * DAY_MS || !recallable[j] {
                    continue;
                }
                if !can_echo(e, p) {
                    continue; // same document or same application: a revisit, not an echo
                }
                let sim = dot(&e.vector, &p.vector);
                if sim < echo_floor {
                    continue;
                }
                let overlap = jaccard(&lines[i], &lines[j]);
                if overlap > MAX_LINE_OVERLAP {
                    continue;
                }
                if best.map(|b| sim > b.0).unwrap_or(true) {
                    best = Some((sim, j, overlap));
                }
            }
            let Some((sim, j, overlap)) = best else {
                continue;
            };
            let p = &episodes[j];
            for t in BTreeSet::from([theme_of[i], theme_of[j]]) {
                let theme = &mut themes[t];
                theme.recalls += 1;
                theme.probes.insert(e_stem.clone());
                theme.probes.insert(stem(&p.title));
                theme.similarities.push(sim);
                theme.rem_hits.push(now);
                theme.days.insert(history.day(p.start));
                touched.insert(t);
            }
            echoes.push((
                sim,
                Echo {
                    now: evidence_of(history, e),
                    past: evidence_of(history, p),
                    similarity: sim,
                    line_overlap: overlap,
                    days_apart: (e.start - p.end) as f64 / DAY_MS,
                },
            ));
        }
        echoes.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut seen_pairs = HashSet::new();
        let echoes: Vec<Echo> = echoes
            .into_iter()
            .map(|e| e.1)
            .filter(|e| e.now.fragment != e.past.fragment)
            .filter(|e| seen_pairs.insert((stem(&e.now.title), stem(&e.past.title))))
            .filter(|e| {
                seen_echoes.insert((
                    e.now.title.clone(),
                    e.past.title.clone(),
                    e.now.fragment.clone(),
                    e.past.fragment.clone(),
                ))
            })
            .take(3)
            .collect();

        let mut graph = graph_echoes(history, absence, &staged_sessions, &theme_of, &mut themes);
        graph.retain(|g| {
            g.first.fragment.is_some()
                && g.second.fragment.is_some()
                && g.first.fragment != g.second.fragment
                && seen_graph.insert((
                    g.kind,
                    g.label.clone(),
                    g.first.fragment.clone(),
                    g.second.fragment.clone(),
                ))
        });
        for (i, t) in themes.iter().enumerate() {
            if t.rem_hits.last() == Some(&now) {
                touched.insert(i);
            }
        }
        // Interactive recall: Ask turns before this sweep that cited a theme's sessions.
        for (at, question, cited) in &history.asks {
            if *at > now {
                continue;
            }
            for &t in &touched {
                let theme = &mut themes[t];
                if theme
                    .members
                    .iter()
                    .any(|&m| cited.contains(&episodes[m].session))
                {
                    theme.probes.insert(format!("ask: {}", question.trim()));
                }
            }
        }

        // ---- Deep -----------------------------------------------------------------------
        let mut considered: Vec<Scored> = touched
            .iter()
            .map(|&t| score(history, t, &themes[t], now, gates, echo_floor))
            .collect();
        considered.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.key.cmp(&b.key)));
        let mut promoted = Vec::new();
        let mut reinforced = Vec::new();
        for s in &considered {
            if s.failed_gate.is_some() {
                continue;
            }
            if s.already_promoted {
                reinforced.push(s.clone());
            } else if promoted.len() < gates.max_per_sweep {
                promoted.push(s.clone());
            }
        }
        for p in &promoted {
            themes[p.key].promoted = true;
        }
        sweeps.push(SweepOutcome {
            number: sweeps.len() + 1,
            absence: *absence,
            light,
            echoes,
            graph,
            revisions,
            fresh,
            promoted,
            reinforced,
        });
    }
    Outcome {
        sweeps,
        gates: gates.clone(),
        vector_source: history.vector_source,
        echo_floor,
        theme_floor,
        themes: theme_count,
        episodes: episodes.len(),
    }
}

fn decay(now: i64, at: i64) -> f64 {
    (-((now - at).max(0) as f64) / (7.0 * DAY_MS)).exp()
}

fn score(history: &History, key: usize, t: &Theme, now: i64, gates: &Gates, floor: f32) -> Scored {
    // Relevance: how far above the echo floor the theme's recalls sat (0.5 at the floor).
    let relevance = if t.similarities.is_empty() {
        if t.recalls > 0 {
            0.5
        } else {
            0.0
        }
    } else {
        let mean =
            t.similarities.iter().map(|&s| s as f64).sum::<f64>() / t.similarities.len() as f64;
        let span = (1.0 - floor as f64).max(1e-6);
        (0.5 + 0.5 * ((mean - floor as f64) / span)).clamp(0.0, 1.0)
    };
    let signals = (t.light_hits.len() + t.rem_hits.len()) as f64;
    let frequency = ((1.0 + signals).ln() / 41.0f64.ln()).min(1.0);
    let diversity = (t.probes.len() as f64 / 6.0).min(1.0);
    let recency = decay(now, t.last_touch);
    let consolidation = (t.days.len() as f64 / 10.0).min(1.0);
    let stems: BTreeSet<String> = t
        .members
        .iter()
        .map(|&m| stem(&history.episodes[m].title))
        .collect();
    let richness = ((stems.len() + t.tags.len()) as f64 / 6.0).min(1.0);
    let parts = [
        relevance,
        frequency,
        diversity,
        recency,
        consolidation,
        richness,
    ];
    let base: f64 = parts.iter().zip(WEIGHTS).map(|(p, w)| p * w).sum();
    let boost = (t
        .light_hits
        .iter()
        .map(|&h| 0.005 * decay(now, h))
        .sum::<f64>()
        + t.rem_hits
            .iter()
            .map(|&h| 0.015 * decay(now, h))
            .sum::<f64>())
    .min(0.10);
    let total = (base + boost).min(1.0);
    let queries = t.probes.len() as u32;
    let mut evidence: Vec<Evidence> = Vec::new();
    let mut seen_lines = HashSet::new();
    for e in t.members.iter().rev().map(|&m| &history.episodes[m]) {
        if evidence.len() == 3 {
            break;
        }
        if !evidence.iter().any(|x| stem(&x.title) == stem(&e.title)) {
            if let Some(source) = select_evidence(history, e, |l| !seen_lines.contains(l)) {
                seen_lines.extend(source.fragment.iter().cloned());
                evidence.push(source);
            }
        }
    }
    let failed_gate = if evidence.is_empty() {
        Some("no content text".to_string())
    } else if total < gates.min_score {
        Some(format!("score {total:.2} below {:.2}", gates.min_score))
    } else if t.recalls < gates.min_recall_count {
        Some(format!(
            "{} recalls, {} needed",
            t.recalls, gates.min_recall_count
        ))
    } else if queries < gates.min_unique_queries {
        Some(format!(
            "{queries} distinct queries, {} needed",
            gates.min_unique_queries
        ))
    } else {
        None
    };

    let mut tags: Vec<String> = t.tags.iter().take(3).cloned().collect();
    for s in &stems {
        if tags.len() >= 3 {
            break;
        }
        let s = s.to_lowercase();
        if !tags.contains(&s) && s.chars().count() <= 40 {
            tags.push(s);
        }
    }
    Scored {
        key,
        label: theme_label(history, t),
        score: total,
        parts,
        boost,
        recall: t.recalls,
        queries,
        probes: t.probes.iter().take(6).cloned().collect(),
        days: t.days.iter().cloned().collect(),
        tags,
        evidence,
        failed_gate,
        already_promoted: t.promoted,
    }
}
