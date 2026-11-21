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
                p.lines.join(" | "),
                p.days.len(),
                p.days.first().cloned().unwrap_or_default(),
                p.days.last().cloned().unwrap_or_default(),
            );
            if let Ok(text) = model.complete(NAME_SYSTEM, &user).await {
                let text = text.trim().trim_matches('"').trim().to_string();
                let words = text.split_whitespace().count();
                if (6..=28).contains(&words) && !text.contains('\n') && !text.contains('[') {
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
    let mut occupied = Vec::new();
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
        occupied.push((m, e.clone()));
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
    if let Some((m, e)) = occupied.first() {
        let mut s = format!(
            "Before stepping away, {who} spent most of the stretch on {} {m}",
            quote(&e.title)
        );
        if let Some((m2, e2)) = occupied.get(1) {
            let _ = write!(s, ", with {} close behind {m2}", quote(&e2.title));
        }
        s.push('.');
        sheet.draft.push(s);
    }
    if let Some(e) = l.brushed.first() {
        let m = sheet.cite(e);
        sheet.draft.push(format!(
            "{} crossed the screen only once {m}.",
            quote(&e.title)
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
    if let Some(echo) = sweep.echoes.first() {
        let now = sheet.cite(&echo.now);
        let past = sheet.cite(&echo.past);
        sheet.draft.push(format!(
            "The dream tied {} {now} to {} {past}, {:.0} days apart: close in meaning (cosine {:.2}) while sharing almost nothing on the surface.",
            quote(&echo.now.title),
            quote(&echo.past.title),
            echo.days_apart,
            echo.similarity
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
    }
    if sweep.echoes.is_empty() && sweep.graph.is_empty() {
        sheet
            .facts
            .push("REM: nothing from earlier days echoed this stretch.".to_string());
        sheet
            .draft
            .push("Nothing from earlier days echoed this stretch.".to_string());
    }

    // Deep
    let g = &outcome.gates;
    for p in &sweep.promoted {
        let marks: String = p.evidence.iter().take(2).map(|e| sheet.cite(e)).collect();
        sheet.facts.push(format!(
            "DEEP promoted to MEMORY.md {marks}: the memory {} (built from {}) with score {:.2} (gates: score >= {:.2}, recalls {} >= {}, distinct queries {} >= {}), seen on {} days. It was reached from: {}. What keeps returning: {}.",
            quote(&name_of(names, p)), quote(&p.label), p.score, g.min_score, p.recall, g.min_recall_count, p.queries, g.min_unique_queries, p.days.len(),
            p.probes.iter().map(|q| quote(q)).collect::<Vec<_>>().join(", "),
            p.lines.iter().map(|l| quote(l)).collect::<Vec<_>>().join("; ")
        ));
        sheet.draft.push(format!(
            "A memory was kept: {}, with a score of {:.2}, recalled {} times from {} different directions {marks}.",
            quote(&name_of(names, p)), p.score, p.recall, p.queries
        ));
    }
    for r in &sweep.reinforced {
        sheet.facts.push(format!(
            "DEEP already in memory and reinforced: {} (score now {:.2}).",
            quote(&r.label),
            r.score
        ));
    }
    if sweep.promoted.is_empty() {
        if let Some(best) = sweep.considered.iter().find(|c| c.failed_gate.is_some()) {
            let marks: String = best
                .evidence
                .iter()
                .take(1)
                .map(|e| sheet.cite(e))
                .collect();
            let gate = best.failed_gate.clone().unwrap_or_default();
            sheet.facts.push(format!(
                "DEEP nothing promoted {marks}. Strongest candidate {} scored {:.2} and stopped at the gate: {gate}.",
                quote(&best.label),
                best.score
            ));
            sheet.draft.push(format!(
                "Nothing new reached long-term memory; {} came closest at {:.2} and stopped at the gate ({gate}) {marks}.",
                quote(&best.label),
                best.score
            ));
        } else if sweep.reinforced.is_empty() {
            sheet
                .facts
                .push("DEEP no candidate reached the gates.".to_string());
        }
    }
    sheet
}

const SYSTEM: &str = "You write one entry of a Dream Diary: what a person's screen history \
'dreamt' while they were away from it. You receive a fact sheet and an opening instruction. Write \
ONE paragraph of 4 or 5 sentences, 60 to 110 words, in English, third person, past tense, about \
the named person only. It must read like a diary a stranger would enjoy: concrete, calm, quietly \
poetic, never cute, never grand. Hard rules: use ONLY facts from the sheet; never invent people, \
places, numbers, durations, years, intentions or feelings; every sentence except the first ends \
with one or two evidence markers exactly as given, like [E2] or [E1][E4], and a marker must point \
to the fact it supports; quote at most two short on-screen phrases, verbatim in their original \
language inside curly quotes, with an English gloss; say what filled the stretch, what came back \
from an earlier day and how far back, and what was kept for the long term or held back and why \
(give the score when the sheet gives one). Never use the words Light, REM, Deep, phase, sweep, \
embedding, database, software or model, never address the reader, never use square brackets \
except for markers. Output only the paragraph.";

/// Opening instructions, picked by sweep number so consecutive entries do not all start the
/// same way (the diary itself is never read back to avoid repetition).
const OPENINGS: [&str; 6] = [
    "Open with the hour the screen went dark.",
    "Open with one concrete object or document that was on the screen.",
    "Open with what came back from an earlier day.",
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
    // A claim that quotes the screen must carry evidence; at most two sentences may go
    // without (a scene-setting opener and a "nothing echoed" kind of negative).
    let sentences = sentences(text);
    let mut bare = 0;
    for s in &sentences {
        if markers_in(s).is_empty() {
            if s.contains('\u{201c}') {
                return Err(format!("quoted claim without evidence: {s}"));
            }
            bare += 1;
        }
    }
    if bare > 2 {
        return Err(format!(
            "{bare} sentences without evidence, at most 2 allowed"
        ));
    }
    Ok(())
}

/// Splits prose into sentences: a sentence ends at `.`, `!` or `?` followed by whitespace,
/// outside curly quotes, so file names, decimals and quoted on-screen text do not split.
/// A marker group written after the full stop stays with its sentence.
fn sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    for (i, &c) in chars.iter().enumerate() {
        current.push(c);
        match c {
            '\u{201c}' => in_quote = true,
            '\u{201d}' => in_quote = false,
            '.' | '!' | '?' if !in_quote => {
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
        .into_iter()
        .filter(|s| s.split_whitespace().count() > 2)
        .collect()
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
    let mut rest = text;
    loop {
        let Some(i) = rest.find("[E") else {
            out.push_str(rest);
            break;
        };
        out.push_str(rest[..i].trim_end());
        let mut group = Vec::new();
        let mut cursor = &rest[i..];
        while let Some(stripped) = cursor.trim_start().strip_prefix("[E") {
            let digits: String = stripped
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            let after = &stripped[digits.len()..];
            let Some(after) = after.strip_prefix(']') else {
                break;
            };
            if let Ok(n) = digits.parse::<usize>() {
                if let Some(e) = evidence.get(n.wrapping_sub(1)) {
                    let t = tuple(history, e);
                    if !group.contains(&t) {
                        group.push(t);
                    }
                }
            }
            cursor = after;
        }
        if !group.is_empty() {
            let _ = write!(out, " ({})", group.join("; "));
        }
        rest = cursor;
        if rest.is_empty() {
            break;
        }
    }
    out.replace(" .", ".").replace(" ,", ",")
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
            Ok(text) => match validate(&text, sheet.evidence.len()) {
                Ok(()) => {
                    return Entry {
                        number: sweep.number,
                        text: resolve(history, &text, &sheet.evidence),
                        degraded: None,
                    }
                }
                Err(e) => last_error = format!("reply failed validation ({e})"),
            },
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
    let _ = writeln!(diary, "# Dream Diary \u{2014} {who}\n");
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
        } else if !sweep.reinforced.is_empty() {
            format!("reinforced {}", quote(&sweep.reinforced[0].label))
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
    let _ = writeln!(md, "# Long-term memory \u{2014} {who}\n");
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
    if !p.lines.is_empty() {
        let _ = writeln!(
            md,
            "- What keeps returning: {}",
            p.lines
                .iter()
                .map(|l| quote(l))
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    for e in &p.evidence {
        let _ = writeln!(md, "- Source: ({})", tuple(history, e));
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
    use super::{markers_in, sentences, validate};

    #[test]
    fn sentences_ignore_file_names_decimals_and_quotes() {
        let text = "She opened \u{201c}diario-campo.docx\u{201d} at dawn [E1]. The score was 0.84 [E2]. \u{201c}Riego. Tomate.\u{201d} returned. [E3] Then nothing else happened.";
        let s = sentences(text);
        assert_eq!(s.len(), 4, "{s:?}");
        assert!(s[2].ends_with("[E3]"), "{s:?}");
    }

    #[test]
    fn validation_rejects_unknown_markers_and_unevidenced_quotes() {
        let filler = "word ".repeat(60);
        assert!(validate(&format!("{filler} closes here [E9]."), 3).is_err());
        assert!(validate(
            &format!("{filler}. She read \u{201c}lonja\u{201d} again."),
            3
        )
        .is_err());
        assert!(validate(&format!("{filler} closes here [E2]."), 3).is_ok());
        assert_eq!(markers_in("a [E1][E12] b [Ex]"), vec![1, 12]);
    }
}
