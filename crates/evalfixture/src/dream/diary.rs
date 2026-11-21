//! `DREAMS.md` and `MEMORY.md` writers.
//!
//! Each sweep becomes a fact sheet of numbered evidence (`E1`, `E2`, ...) built from the
//! phases' output. A chat completion turns the sheet into a short narrative whose every
//! sentence carries `[E#]` markers; the writer validates the reply (known markers only, a
//! marker in every sentence but the first, a word budget) and replaces each marker group
//! with its `(date time, app, "window title")` tuple. Without a model, or when the reply
//! fails validation twice, the same sheet is composed by rule and the entry is labelled
//! degraded. Both files are created from scratch; neither is ever opened for reading.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::stream::{self, StreamExt};

use super::data::History;
use super::phases::{Evidence, Outcome, Scored, SweepOutcome, SIGNALS, WEIGHTS};

/// An OpenAI-compatible chat endpoint for the diary.
pub struct ModelConfig {
    pub base: String,
    pub model: String,
    key: String,
    max_calls: u32,
    calls: std::sync::atomic::AtomicU32,
    /// Set after a non-retryable refusal (auth, guardrail, unknown model): the rest of the
    /// run is degraded instead of repeating a request that cannot succeed.
    refused: std::sync::Mutex<Option<String>>,
    http: reqwest::Client,
}

impl ModelConfig {
    /// `DREAM_API_BASE`, `DREAM_API_MODEL`, `DREAM_API_KEY` from the process environment,
    /// then from `env_file` (default `~/.config/screensearch-dream/env`). `None` when any is
    /// missing: the run is then degraded, which is reported, never hidden.
    pub fn discover(env_file: Option<&Path>, max_calls: u32) -> Result<Option<Self>> {
        let mut values: BTreeMap<String, String> = BTreeMap::new();
        let default = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(|home| Path::new(&home).join(".config/screensearch-dream/env"));
        if let Some(path) = env_file.map(Path::to_path_buf).or(default) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once('=') {
                        values.insert(k.trim().to_string(), v.trim().to_string());
                    }
                }
            }
        }
        let get = |k: &str| std::env::var(k).ok().or_else(|| values.get(k).cloned());
        let (Some(base), Some(model), Some(key)) = (
            get("DREAM_API_BASE"),
            get("DREAM_API_MODEL"),
            get("DREAM_API_KEY"),
        ) else {
            return Ok(None);
        };
        Ok(Some(Self {
            base: base.trim_end_matches('/').to_string(),
            model,
            key,
            max_calls,
            calls: std::sync::atomic::AtomicU32::new(0),
            refused: std::sync::Mutex::new(None),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(90))
                .build()?,
        }))
    }

    async fn complete(&self, system: &str, user: &str) -> Result<String> {
        if let Some(reason) = self.refused.lock().ok().and_then(|r| r.clone()) {
            anyhow::bail!("{reason}");
        }
        let n = self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if n >= self.max_calls {
            anyhow::bail!("model call budget of {} reached", self.max_calls);
        }
        let body = serde_json::json!({
            "model": self.model,
            "temperature": 0.5,
            "max_tokens": 700,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
        });
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.key)
            .json(&body)
            .send()
            .await
            .context("diary request failed")?;
        let status = resp.status();
        let value: serde_json::Value = resp.json().await.context("diary reply was not JSON")?;
        if !status.is_success() {
            let message = value
                .pointer("/error/message")
                .and_then(|m| m.as_str())
                .unwrap_or("no message");
            let first_line = message.lines().next().unwrap_or("").trim();
            let reason = format!(
                "diary model answered HTTP {}: {first_line}",
                status.as_u16()
            );
            if status.is_client_error() && status.as_u16() != 429 {
                if let Ok(mut refused) = self.refused.lock() {
                    refused.get_or_insert_with(|| reason.clone());
                }
            }
            anyhow::bail!("{reason}");
        }
        value
            .pointer("/choices/0/message/content")
            .and_then(|c| c.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .context("diary reply had no text")
    }
}

pub struct Written {
    pub entries: usize,
    pub degraded: usize,
    pub first_day: String,
    pub last_day: String,
}

/// One sweep's numbered evidence and English fact lines.
struct Sheet {
    evidence: Vec<Evidence>,
    facts: Vec<String>,
    /// Rule-based draft sentences, each already carrying markers.
    draft: Vec<String>,
}

impl Sheet {
    fn cite(&mut self, e: &Evidence) -> String {
        let i = match self.evidence.iter().position(|x| x == e) {
            Some(i) => i,
            None => {
                self.evidence.push(e.clone());
                self.evidence.len() - 1
            }
        };
        format!("[E{}]", i + 1)
    }
}

fn quote(s: &str) -> String {
    format!("\u{201c}{s}\u{201d}")
}

fn subject(history: &History) -> String {
    history
        .persona
        .name
        .clone()
        .unwrap_or_else(|| "the person at this screen".to_string())
}

fn hm(history: &History, ms: i64) -> String {
    history.local(ms).format("%H:%M").to_string()
}

fn duration(ms: i64) -> String {
    let mins = ms / 60_000;
    if mins >= 60 {
        format!("{} h {:02} min", mins / 60, mins % 60)
    } else {
        format!("{mins} min")
    }
}

/// One-sentence names for promoted themes, keyed by theme. Filled by the model when one is
/// configured; a theme without a name falls back to its window-title label.
type Names = BTreeMap<usize, String>;

fn name_of(names: &Names, p: &Scored) -> String {
    names
        .get(&p.key)
        .cloned()
        .unwrap_or_else(|| p.label.clone())
}

fn content_quotes(p: &Scored) -> impl Iterator<Item = (&Evidence, &str)> {
    p.evidence
        .iter()
        .filter_map(|e| e.fragment.as_deref().map(|line| (e, line)))
}

const NAME_SYSTEM: &str = "You name one long-term memory distilled from a person's screen \
history. Reply with ONE English sentence of 8 to 22 words stating what keeps recurring in their \
life or work, using only the facts given (documents, places, on-screen lines, which may be in \
another language). No quotes, no markdown, no mention of software, screens or memory.";

async fn name_memories(history: &History, outcome: &Outcome, model: &ModelConfig) -> Names {
    let mut names = Names::new();
    for sweep in &outcome.sweeps {
        for p in &sweep.promoted {
            let user = format!(
                "Person: {} ({}).\nDocuments and windows in this theme: {}.\nReached from: {}.\nLines that keep returning: {}.\nSeen on {} days between {} and {}.",
                subject(history),
                history.persona.role.as_deref().unwrap_or(&history.persona.description),
                p.label,
                p.probes.join("; "),
                content_quotes(p).map(|(_, line)| line).collect::<Vec<_>>().join(" | "),
                p.days.len(),
                p.days.first().cloned().unwrap_or_default(),
                p.days.last().cloned().unwrap_or_default(),
            );
            if let Ok(text) = model.complete(NAME_SYSTEM, &user).await {
                let text = text.trim().trim_matches('"').trim().to_string();
                let words = text.split_whitespace().count();
                if (6..=28).contains(&words)
                    && !text
                        .contains(['\n', '[', '{', '}', '"', '\u{2014}', '\u{201c}', '\u{201d}'])
                {
                    names.insert(p.key, text);
                }
            }
        }
    }
    names
}

fn build_sheet(history: &History, sweep: &SweepOutcome, outcome: &Outcome, names: &Names) -> Sheet {
    let who = subject(history);
    let mut sheet = Sheet {
        evidence: Vec::new(),
        facts: Vec::new(),
        draft: Vec::new(),
    };
    let a = sweep.absence;
    let l = &sweep.light;
    sheet.facts.push(format!(
        "ABSENCE: the screen went dark at {} and woke at {} ({} away). {} sessions, {} of screen time, were staged.",
        history.local(a.start).format("%a %Y-%m-%d %H:%M"),
        history.local(a.end).format("%a %Y-%m-%d %H:%M"),
        duration(a.end - a.start),
        l.sessions,
        duration(l.minutes * 60_000)
    ));

    // Light
    for (e, frames) in &l.occupied {
        let m = sheet.cite(e);
        let frag = e
            .fragment
            .as_deref()
            .map(|f| format!(", text on screen: {}", quote(f)))
            .unwrap_or_default();
        sheet.facts.push(format!(
            "LIGHT occupied {m}: {} in {} for {frames} captures{frag}",
            quote(&e.title),
            e.app
        ));
    }
    if let Some(e) = &l.last {
        let m = sheet.cite(e);
        sheet.facts.push(format!(
            "LIGHT last window before leaving {m}: {}",
            quote(&e.title)
        ));
    }
    for e in &l.brushed {
        let m = sheet.cite(e);
        sheet.facts.push(format!(
            "LIGHT brushed past (one capture only) {m}: {}",
            quote(&e.title)
        ));
    }
    for e in &l.lines {
        let m = sheet.cite(e);
        if let Some(f) = &e.fragment {
            sheet
                .facts
                .push(format!("LIGHT content line {m}: {}", quote(f)));
        }
    }
    for (label, head) in &l.saved {
        sheet.facts.push(format!(
            "LIGHT the app's saved {label} for this range opens with: {}",
            quote(head)
        ));
    }
    for e in &sweep.fresh {
        let m = sheet.cite(e);
        if let Some(line) = &e.fragment {
            sheet.facts.push(format!(
                "FIRST VISIT {m}: {} showed {}.",
                e.title,
                quote(line)
            ));
        }
    }
    for r in &sweep.revisions {
        let (Some(before), Some(after)) = (&r.past.fragment, &r.now.fragment) else {
            continue;
        };
        let past = sheet.cite(&r.past);
        let now = sheet.cite(&r.now);
        let revision = if r.added {
            "appeared in the later capture"
        } else {
            "was visible instead in the later capture"
        };
        sheet.facts.push(format!(
            "CHANGE in {}: on {} it showed {} {past}; on {} {} {revision} {now}.",
            r.now.title,
            history.day(r.past.at),
            quote(before),
            history.day(r.now.at),
            quote(after)
        ));
        sheet.draft.push(format!(
            "On {}, {} showed {} {past}. On {}, {} {revision} {now}.",
            history.day(r.past.at),
            r.now.title,
            quote(before),
            history.day(r.now.at),
            quote(after)
        ));
    }

    // REM
    for echo in &sweep.echoes {
        let now = sheet.cite(&echo.now);
        let past = sheet.cite(&echo.past);
        sheet.facts.push(format!(
            "REM echo: {} {now} sat close in meaning to a different document, {} {past}, {:.0} days earlier (cosine {:.2} over stored embeddings; shared lines {:.0}%).{}{}",
            quote(&echo.now.title),
            quote(&echo.past.title),
            echo.days_apart,
            echo.similarity,
            echo.line_overlap * 100.0,
            echo.now.fragment.as_deref().map(|f| format!(" Now: {}.", quote(f))).unwrap_or_default(),
            echo.past.fragment.as_deref().map(|f| format!(" Then: {}.", quote(f))).unwrap_or_default(),
        ));
    }
    for echo in &sweep.echoes {
        let (Some(before), Some(after)) = (&echo.past.fragment, &echo.now.fragment) else {
            continue;
        };
        let now = sheet.cite(&echo.now);
        let past = sheet.cite(&echo.past);
        sheet.draft.push(format!(
            "{:.0} days after {} showed {} {past}, {} showed {} {now}. Their wording suggested a connection.",
            echo.days_apart,
            echo.past.title,
            quote(before),
            echo.now.title,
            quote(after)
        ));
    }
    for g in &sweep.graph {
        let first = sheet.cite(&g.first);
        let second = sheet.cite(&g.second);
        let what = if g.kind == "resumption" {
            format!(
                "the work thread {} was picked up again after {:.0} days",
                quote(&g.label),
                g.gap_days
            )
        } else {
            format!("{} came back after {:.0} days", quote(&g.label), g.gap_days)
        };
        sheet
            .facts
            .push(format!("REM work graph {first} {second}: {what}."));
        if let (Some(before), Some(after)) = (&g.first.fragment, &g.second.fragment) {
            sheet.facts.push(format!(
                "Earlier {first}: {}. Later {second}: {}.",
                quote(before),
                quote(after)
            ));
            if sheet.draft.is_empty() {
                sheet.draft.push(format!(
                    "After {:.0} days, {} and {} still appeared connected. The earlier capture showed {} {first}. The later capture showed {} {second}.",
                    g.gap_days,
                    g.first.title,
                    g.second.title,
                    quote(before),
                    quote(after)
                ));
            }
        }
    }
    if sweep.echoes.is_empty() && sweep.graph.is_empty() {
        sheet
            .facts
            .push("REM: nothing from earlier days echoed this stretch.".to_string());
    }

    // Bring in one otherwise unlinked new screen even during a busier stretch.
    for e in sweep
        .fresh
        .iter()
        .filter(|e| {
            !sweep.echoes.iter().any(|link| link.now.title == e.title)
                && !sweep.graph.iter().any(|link| link.second.title == e.title)
        })
        .take(1)
    {
        if let Some(line) = &e.fragment {
            let m = sheet.cite(e);
            sheet.draft.push(format!(
                "{who}'s screen showed {} with the line {} {m}.",
                e.title,
                quote(line)
            ));
        }
    }

    // Deep
    let g = &outcome.gates;
    for p in &sweep.promoted {
        let marks: String = p.evidence.iter().take(2).map(|e| sheet.cite(e)).collect();
        sheet.facts.push(format!(
            "DEEP promoted to MEMORY.md {marks}: the memory {} (built from {}) with score {:.2} (gates: score >= {:.2}, recalls {} >= {}, distinct queries {} >= {}), seen on {} days. It was reached from: {}. What keeps returning: {}.",
            quote(&name_of(names, p)), quote(&p.label), p.score, g.min_score, p.recall, g.min_recall_count, p.queries, g.min_unique_queries, p.days.len(),
            p.probes.iter().map(|q| quote(q)).collect::<Vec<_>>().join(", "),
            content_quotes(p).map(|(_, line)| line).collect::<Vec<_>>().join(" | ")
        ));
        sheet.draft.push(format!(
            "A memory was kept: {}, with a score of {:.2}, recalled {} times from {} different directions {marks}.",
            name_of(names, p), p.score, p.recall, p.queries
        ));
    }
    if sweep.promoted.is_empty() {
        sheet.facts.push("DEEP nothing new was kept.".to_string());
    }
    sheet
}

const SYSTEM: &str = "Write one Dream Diary entry in one English paragraph of 60 to 110 words, \
in third person and past tense. Lead with a concrete change from the sheet: something added, a \
number that moved, an answer that arrived or an earlier idea taking a new form. Say what the \
earlier capture showed and what the later one shows, with both citations. A comparison across \
windows is a possible connection; never present it as a confirmed relationship. If the sheet \
has only first visits, convey one or two specific details. Mention a memory only if the sheet \
says one was newly kept. Use only the sheet. Every sentence ends with evidence markers \
exactly as given, like [E2] or [E1][E4]. Quote at most two short phrases from screen content, \
verbatim within curly quotation marks and cited to the exact marker that shows the words. \
Give the English meaning without quotes. Refer to window titles without quotation marks. \
Never use curly braces, square brackets outside evidence markers, or an em dash. Never name \
the phases, embeddings, the database or the model. Output only the paragraph.";

/// Opening instructions, picked by sweep number so consecutive entries do not all start the
/// same way (the diary itself is never read back to avoid repetition).
const OPENINGS: [&str; 6] = [
    "Open with the hour the screen went dark.",
    "Open with one concrete object or document that was on the screen.",
    "Open with the details that best distinguish the earlier and later captures.",
    "Open with the length of the absence.",
    "Open with the last thing on the screen before it went dark.",
    "Open with the thread that tied the stretch together, without naming the person first.",
];

fn markers_in(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("[E") {
        rest = &rest[i + 2..];
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if rest[digits.len()..].starts_with(']') {
            if let Ok(n) = digits.parse() {
                out.push(n);
            }
        }
    }
    out
}

/// Contiguous citation groups outside quoted content, with byte offsets for rendering.
fn citation_groups(text: &str) -> Vec<(std::ops::Range<usize>, Vec<usize>)> {
    let mut groups = Vec::new();
    let mut quote_end = None;
    let mut end = 0;
    for (i, c) in text.char_indices() {
        if i < end {
            continue;
        }
        match c {
            c if quote_end == Some(c) => quote_end = None,
            '\u{201c}' if quote_end.is_none() => quote_end = Some('\u{201d}'),
            '"' if quote_end.is_none() => quote_end = Some('"'),
            '[' if quote_end.is_none() => {
                let mut cursor = &text[i..];
                let mut marks = Vec::new();
                while let Some(marker) = cursor.trim_start().strip_prefix("[E") {
                    let Some((digits, remaining)) = marker.split_once(']') else {
                        break;
                    };
                    let Ok(number) = digits.parse::<usize>() else {
                        break;
                    };
                    marks.push(number);
                    cursor = remaining;
                }
                if !marks.is_empty() {
                    end = text.len() - cursor.len();
                    groups.push((i..end, marks));
                }
            }
            _ => {}
        }
    }
    groups
}

/// Accepts a reply only if every marker is known, every sentence after the first has one,
/// and the length is in budget.
fn validate(text: &str, known: usize) -> std::result::Result<(), String> {
    let words = text.split_whitespace().count();
    if !(45..=135).contains(&words) {
        return Err(format!("{words} words, expected 60-110"));
    }
    // Any square bracket outside a well-formed marker is a leaked label like "[DEEP]".
    let stripped = (1..=99)
        .rev()
        .fold(text.to_string(), |t, n| t.replace(&format!("[E{n}]"), ""));
    if stripped.contains('[') || stripped.contains(']') {
        return Err("square brackets outside evidence markers".to_string());
    }
    if stripped.contains(['{', '}', '\u{2014}']) {
        return Err("braces or em dash in diary prose".to_string());
    }
    for word in ["REM", "Light", "Deep "] {
        if stripped.contains(word) {
            return Err(format!("names the phase `{}`", word.trim()));
        }
    }
    let marks = markers_in(text);
    if marks.is_empty() {
        return Err("no evidence markers".to_string());
    }
    if let Some(bad) = marks.iter().find(|&&m| m == 0 || m > known) {
        return Err(format!("unknown marker [E{bad}]"));
    }
    let lowered = text.to_lowercase();
    for banned in [
        "embedding",
        "database",
        "fixture",
        "light phase",
        "rem phase",
        "deep phase",
        "synthetic",
        "years",
        "the reader",
    ] {
        if lowered.contains(banned) {
            return Err(format!("mentions `{banned}`"));
        }
    }
    Ok(())
}

/// Drop each quoted claim unless all its quotes occur verbatim in at least one
/// of the specific frames it cites. A phrase may be a substring of one screen line.
fn grounded_claims(text: &str, matches: impl Fn(usize, &str) -> bool) -> String {
    sentences(text)
        .into_iter()
        .filter(|sentence| {
            let groups = citation_groups(sentence);
            if groups.is_empty() {
                return false;
            }
            let mut open = None;
            for (i, c) in sentence.char_indices() {
                match open {
                    Some((start, close)) if c == close => {
                        let phrase: &str = &sentence[start..i];
                        let cited = groups
                            .iter()
                            .find(|(range, _)| range.start > i)
                            .is_some_and(|(_, marks)| marks.iter().any(|&n| matches(n, phrase)));
                        if phrase.trim().is_empty() || !cited {
                            return false;
                        }
                        open = None;
                    }
                    None if c == '\u{201c}' || c == '"' => {
                        let close = if c == '"' { '"' } else { '\u{201d}' };
                        open = Some((i + c.len_utf8(), close));
                    }
                    None if c == '\u{201d}' => return false,
                    _ => {}
                }
            }
            open.is_none()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits prose into sentences: a sentence ends at `.`, `!` or `?` followed by whitespace,
/// outside quotes, so file names, decimals and quoted on-screen text do not split.
/// A marker group written after the full stop stays with its sentence.
fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote_end = None;
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        match c {
            c if quote_end == Some(c) => quote_end = None,
            '\u{201c}' if quote_end.is_none() => quote_end = Some('\u{201d}'),
            '"' if quote_end.is_none() => quote_end = Some('"'),
            '.' | '!' | '?' if quote_end.is_none() => {
                let next = chars.get(i + 1);
                if next.map(|n| n.is_whitespace()).unwrap_or(true) {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => {}
        }
    }
    if !current.trim().is_empty() {
        out.push(current);
    }
    let mut merged: Vec<String> = Vec::new();
    for piece in out {
        let trimmed = piece.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("[E") {
            // "... it. [E2] Next sentence." -> the marker group belongs to the previous one
            let mut rest = trimmed.as_str();
            let mut lead = String::new();
            while let Some(stripped) = rest.strip_prefix("[E") {
                let Some(close) = stripped.find(']') else {
                    break;
                };
                lead.push_str(&rest[..close + 3]);
                rest = stripped[close + 1..].trim_start();
            }
            if let Some(prev) = merged.last_mut() {
                prev.push(' ');
                prev.push_str(&lead);
            }
            if !rest.is_empty() {
                merged.push(rest.to_string());
            }
        } else {
            merged.push(trimmed);
        }
    }
    merged
}

fn tuple(history: &History, e: &Evidence) -> String {
    format!(
        "{}, {}, {}",
        history.local(e.at).format("%Y-%m-%d %H:%M"),
        e.app,
        quote(&e.title)
    )
}

/// Replaces each run of adjacent markers with one parenthesised evidence group.
fn resolve(history: &History, text: &str, evidence: &[Evidence]) -> String {
    let mut out = String::new();
    let mut previous_end = 0;
    for (range, marks) in citation_groups(text) {
        out.push_str(text[previous_end..range.start].trim_end());
        let mut group = Vec::new();
        for n in marks {
            if let Some(e) = evidence.get(n.wrapping_sub(1)) {
                let t = tuple(history, e);
                if !group.contains(&t) {
                    group.push(t);
                }
            }
        }
        if !group.is_empty() {
            let _ = write!(out, " ({})", group.join("; "));
        }
        previous_end = range.end;
    }
    out.push_str(&text[previous_end..]);
    out
}

struct Entry {
    number: usize,
    text: String,
    degraded: Option<String>,
}

async fn write_entry(
    history: &History,
    sweep: &SweepOutcome,
    outcome: &Outcome,
    model: Option<&ModelConfig>,
    names: &Names,
) -> Entry {
    let sheet = build_sheet(history, sweep, outcome, names);
    if sheet.draft.is_empty() {
        return Entry {
            number: sweep.number,
            text: "This stretch brought no new connection or change to keep.".to_string(),
            degraded: None,
        };
    }
    let draft = || resolve(history, &sheet.draft.join(" "), &sheet.evidence);
    let Some(model) = model else {
        return Entry {
            number: sweep.number,
            text: draft(),
            degraded: Some("no diary model was configured".to_string()),
        };
    };
    let mut user = format!(
        "Person: {} ({}).\nOpening: {}\nFact sheet:\n",
        subject(history),
        history
            .persona
            .role
            .as_deref()
            .unwrap_or(&history.persona.description),
        OPENINGS[sweep.number % OPENINGS.len()]
    );
    for f in &sheet.facts {
        let _ = writeln!(user, "- {f}");
    }
    let mut last_error = String::new();
    for attempt in 0..2 {
        let prompt = if attempt == 0 {
            user.clone()
        } else {
            format!("{user}\nYour previous paragraph was rejected: {last_error}. Write it again following every rule.")
        };
        match model.complete(SYSTEM, &prompt).await {
            Ok(text) => {
                let text = text
                    .replace("{\u{201c}", "\u{201c}")
                    .replace("\u{201d}}", "\u{201d}")
                    .replace('\u{2014}', " - ");
                match validate(&text, sheet.evidence.len()) {
                    Ok(()) => {
                        let grounded = grounded_claims(&text, |marker, phrase| {
                            sheet.evidence.get(marker.wrapping_sub(1)).is_some_and(|e| {
                                history.frames[e.frame]
                                    .lines
                                    .iter()
                                    .any(|line| line.contains(phrase))
                            })
                        });
                        if !markers_in(&grounded).is_empty() {
                            return Entry {
                                number: sweep.number,
                                text: resolve(history, &grounded, &sheet.evidence),
                                degraded: None,
                            };
                        }
                        last_error =
                            "reply had no grounded claims after unsupported quotes were dropped"
                                .to_string();
                    }
                    Err(e) => last_error = format!("reply failed validation ({e})"),
                }
            }
            Err(e) => last_error = format!("{e:#}"),
        }
    }
    Entry {
        number: sweep.number,
        text: draft(),
        degraded: Some(last_error),
    }
}

pub async fn write_all(
    history: &History,
    outcome: &Outcome,
    model: Option<&ModelConfig>,
    out: &Path,
) -> Result<Written> {
    std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    let names = match model {
        Some(model) => name_memories(history, outcome, model).await,
        None => Names::new(),
    };
    let names = &names;
    let entries: Vec<Entry> = stream::iter(outcome.sweeps.iter())
        .map(|sweep| write_entry(history, sweep, outcome, model, names))
        .buffered(4)
        .collect()
        .await;

    let who = subject(history);
    let mut diary = String::new();
    let _ = writeln!(diary, "# Dream Diary for {who}\n");
    match &history.persona.role {
        Some(role) => {
            let _ = writeln!(
                diary,
                "{who}, {role}. The person, the places and the documents are invented.\n"
            );
        }
        None => {
            let _ = writeln!(diary, "{}.\n", history.persona.description);
        }
    }
    let _ = writeln!(
        diary,
        "Each entry is one absence from the screen, whenever it happened. While the screen was dark, the day was sorted, compared with older days and weighed for what is worth keeping. Every claim ends with its evidence: (local date and time, app, window title).\n"
    );
    let mut degraded = 0;
    for (sweep, entry) in outcome.sweeps.iter().zip(&entries) {
        let a = sweep.absence;
        let _ = writeln!(diary, "---\n");
        let _ = writeln!(
            diary,
            "## {} \u{00b7} away {} \u{2192} {}{} ({})\n",
            history.local(a.start).format("%A %-d %B %Y"),
            hm(history, a.start),
            if history.day(a.start) == history.day(a.end) {
                String::new()
            } else {
                format!("{} ", history.local(a.end).format("%a"))
            },
            hm(history, a.end),
            duration(a.end - a.start),
        );
        if let Some(reason) = &entry.degraded {
            degraded += 1;
            let _ = writeln!(
                diary,
                "> **Degraded entry.** No diary model wrote this one ({reason}); it is the rule-based draft built from the same evidence.\n"
            );
        }
        let _ = writeln!(diary, "{}\n", entry.text);
        let deep = if !sweep.promoted.is_empty() {
            format!(
                "kept {}",
                sweep
                    .promoted
                    .iter()
                    .map(|p| format!("{} ({:.2})", quote(&name_of(names, p)), p.score))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            "kept nothing".to_string()
        };
        let _ = writeln!(
            diary,
            "<sub>Sweep {} \u{00b7} Light: {} sessions staged \u{00b7} REM: {} echoes, {} graph links \u{00b7} Deep: {deep}</sub>\n",
            entry.number,
            sweep.light.sessions,
            sweep.echoes.len(),
            sweep.graph.len()
        );
    }
    std::fs::write(out.join("DREAMS.md"), diary)?;
    std::fs::write(out.join("MEMORY.md"), memory_md(history, outcome, names))?;
    Ok(Written {
        entries: entries.len(),
        degraded,
        first_day: outcome
            .sweeps
            .first()
            .map(|s| history.day(s.absence.start))
            .unwrap_or_default(),
        last_day: outcome
            .sweeps
            .last()
            .map(|s| history.day(s.absence.start))
            .unwrap_or_default(),
    })
}

fn memory_md(history: &History, outcome: &Outcome, names: &Names) -> String {
    let who = subject(history);
    let g = &outcome.gates;
    let mut md = String::new();
    let _ = writeln!(md, "# Long-term memory for {who}\n");
    let _ = writeln!(
        md,
        "Promoted by Deep sleep. A theme is kept only when its score is at least {:.2}, it was recalled at least {} times and by at least {} distinct queries. Score = {} plus a small Light/REM reinforcement boost (at most 0.10). Recall comes from the stored embeddings of each window ({} episodes in {} themes; echo floor cosine {:.2}, theme floor {:.2}; source: {}) and from the work graph.\n",
        g.min_score,
        g.min_recall_count,
        g.min_unique_queries,
        SIGNALS
            .iter()
            .zip(WEIGHTS)
            .map(|(s, w)| format!("{w:.2} {s}"))
            .collect::<Vec<_>>()
            .join(" + "),
        outcome.episodes,
        outcome.themes,
        outcome.echo_floor,
        outcome.theme_floor,
        outcome.vector_source,
    );
    let mut reinforced: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
    for s in &outcome.sweeps {
        for r in &s.reinforced {
            reinforced
                .entry(r.key)
                .or_default()
                .push((s.number, r.score));
        }
    }
    let mut count = 0;
    for s in &outcome.sweeps {
        for p in &s.promoted {
            count += 1;
            entry(
                &mut md,
                history,
                s,
                p,
                &name_of(names, p),
                reinforced.get(&p.key),
            );
        }
    }
    if count == 0 {
        let _ = writeln!(md, "Nothing has crossed the gates yet.");
    }
    md
}

fn entry(
    md: &mut String,
    history: &History,
    s: &SweepOutcome,
    p: &Scored,
    name: &str,
    again: Option<&Vec<(usize, f64)>>,
) {
    let _ = writeln!(md, "## {name}\n");
    if name != p.label {
        let _ = writeln!(md, "Theme: {}\n", quote(&p.label));
    }
    let _ = writeln!(
        md,
        "Promoted in sweep {} ({} {}) \u{00b7} **score {:.2}** \u{00b7} {} recalls \u{00b7} {} distinct queries \u{00b7} seen on {} days ({} \u{2192} {})\n",
        s.number,
        history.day(s.absence.start),
        hm(history, s.absence.start),
        p.score,
        p.recall,
        p.queries,
        p.days.len(),
        p.days.first().cloned().unwrap_or_default(),
        p.days.last().cloned().unwrap_or_default(),
    );
    for (e, line) in content_quotes(p) {
        let _ = writeln!(md, "- Source: {} ({})", quote(line), tuple(history, e));
    }
    let _ = writeln!(
        md,
        "- Signals: {} \u{00b7} phase boost +{:.2}",
        SIGNALS
            .iter()
            .zip(p.parts)
            .map(|(n, v)| format!("{n} {v:.2}"))
            .collect::<Vec<_>>()
            .join(" \u{00b7} "),
        p.boost
    );
    let _ = writeln!(
        md,
        "- Reached from: {}",
        p.probes
            .iter()
            .map(|q| quote(q))
            .collect::<Vec<_>>()
            .join(", ")
    );
    if let Some(again) = again {
        let (first, last) = (again[0], again[again.len() - 1]);
        let _ = writeln!(
            md,
            "- Reinforced in {} later sweeps ({} to {}, latest score {:.2})",
            again.len(),
            first.0,
            last.0,
            last.1
        );
    }
    let importance = (1.0 + (9.0 * p.score).round()).clamp(1.0, 10.0) as u8;
    let _ = writeln!(
        md,
        "\n<!-- trigger: {} --> <!-- importance: {importance} -->\n",
        p.tags.join(", ")
    );
}

#[cfg(test)]
mod tests {
    use super::{grounded_claims, markers_in, sentences, validate};

    #[test]
    fn sentences_ignore_file_names_decimals_and_quotes() {
        let text = "She opened \u{201c}diario-campo.docx\u{201d} at dawn [E1]. The score was 0.84 [E2]. \u{201c}Riego. Tomate.\u{201d} returned. [E3] Then nothing else happened.";
        let s = sentences(text);
        assert_eq!(s.len(), 4, "{s:?}");
        assert!(s[2].ends_with("[E3]"), "{s:?}");
    }

    #[test]
    fn validation_rejects_unknown_markers_and_braces() {
        let filler = "word ".repeat(60);
        assert!(validate(&format!("{filler} closes here [E9]."), 3).is_err());
        assert!(validate(&format!("{filler} closes {{here}} [E2]."), 3).is_err());
        assert!(validate(&format!("{filler} closes here [E2]."), 3).is_ok());
        assert_eq!(markers_in("a [E1][E12] b [Ex]"), vec![1, 12]);
    }

    #[test]
    fn drops_only_claims_with_quotes_missing_from_the_cited_frames() {
        let text = "Sky notes showed \u{201c}Precio nominal: 3,20 euros\u{201d} [E2]. The price sheet showed \u{201c}3,20 euros\u{201d} [E1]. Bay forecast showed \"lluvia débil\" [E2]. The stall draft showed \u{201c}precio nominal\u{201d} [E1]. She wrote \u{201c}Precio nominal\u{201d} again.";
        let frames = [
            "Precio nominal: 3,20 euros",
            "Nublado por la mañana, lluvia débil.",
        ];
        assert_eq!(
            grounded_claims(text, |marker, phrase| frames[marker - 1].contains(phrase)),
            "The price sheet showed \u{201c}3,20 euros\u{201d} [E1]. Bay forecast showed \"lluvia débil\" [E2]."
        );
    }

    #[test]
    fn each_inline_quote_checks_the_citation_for_its_clause() {
        let earlier = "Earlier the sheet showed \u{201c}3,20 euros\u{201d} [E1]";
        let later = "later it showed \u{201c}2,80 euros\u{201d} [E2]";
        let text = format!(
            "Earlier the sheet showed \u{201c}2,80 euros\u{201d} [E1], later it showed \u{201c}3,20 euros\u{201d} [E2]. {earlier}, {later}."
        );
        let frames = ["Precio anterior: 3,20 euros", "Precio nuevo: 2,80 euros"];
        assert_eq!(
            grounded_claims(&text, |marker, phrase| frames[marker - 1].contains(phrase)),
            format!("{earlier}, {later}.")
        );
    }
}
