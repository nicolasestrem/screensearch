//! Each sweep becomes a fact sheet of numbered evidence. The model writes the prose;
//! citations are moved below the entry after quotes are checked against their exact frames.
//! The Markdown and companion JSON are written from scratch and never read back.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::stream::{self, StreamExt};

use serde::Serialize;

use super::data::{is_content, History};
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
            "temperature": 0.2,
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
        if !status.is_success() {
            let reason = format!("diary model answered HTTP {}", status.as_u16());
            if status.is_client_error() && status.as_u16() != 429 {
                if let Ok(mut refused) = self.refused.lock() {
                    refused.get_or_insert_with(|| reason.clone());
                }
            }
            anyhow::bail!("{reason}");
        }
        let value: serde_json::Value = resp.json().await.context("diary reply was not JSON")?;
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
    pub model_written: usize,
    pub quiet_shortened: usize,
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

fn calendar_gap(history: &History, earlier: i64, later: i64) -> i64 {
    (history.local(later).date_naive() - history.local(earlier).date_naive()).num_days()
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

const NAME_SYSTEM: &str = "Write one English sentence about the event in the FIRST source. \
Use supporting sources only to add detail to that same event. A plan, offer or price quote is not a completed sale, \
delivery or repair: preserve the status of each action. Never turn a one-time act into a habit or join unrelated stories. \
Name the thing and what happened to it. No quotes, markdown or discussion of memory.";

async fn name_memories(history: &History, outcome: &Outcome, model: &ModelConfig) -> Names {
    let mut names = Names::new();
    for sweep in &outcome.sweeps {
        for p in &sweep.promoted {
            let user = format!(
                "Person: {} ({}).\nSources in priority order:\n{}",
                subject(history),
                history
                    .persona
                    .role
                    .as_deref()
                    .unwrap_or(&history.persona.description),
                content_quotes(p)
                    .map(|(e, line)| format!("{}: {line}", history.day(e.at)))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            if let Ok(text) = model.complete(NAME_SYSTEM, &user).await {
                let text = text.trim().trim_matches('"').trim().to_string();
                let words = text.split_whitespace().count();
                let lower = text.to_lowercase();
                if (6..=28).contains(&words)
                    && !text
                        .contains(['\n', '[', '{', '}', '"', '\u{2014}', '\u{201c}', '\u{201d}'])
                    && ![
                        "consistently",
                        "regularly",
                        "always",
                        "routinely",
                        "habitually",
                    ]
                    .iter()
                    .any(|word| {
                        lower
                            .split(|c: char| !c.is_alphabetic())
                            .any(|w| w == *word)
                    })
                {
                    names.insert(p.key, text);
                }
            }
        }
    }
    names
}

fn build_sheet(history: &History, sweep: &SweepOutcome, names: &Names) -> Sheet {
    let mut sheet = Sheet {
        evidence: Vec::new(),
        facts: Vec::new(),
        draft: Vec::new(),
    };
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
            "CHANGE in {} ({} calendar days apart): on {} it showed {} {past}; on {} {} {revision} {now}.",
            r.now.title,
            calendar_gap(history, r.past.at, r.now.at),
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
            "POSSIBLE CONNECTION: {} {now} and {} {past}, {} calendar days apart.{}{}",
            quote(&echo.now.title),
            quote(&echo.past.title),
            calendar_gap(history, echo.past.at, echo.now.at),
            echo.now
                .fragment
                .as_deref()
                .map(|f| format!(" Now: {}.", quote(f)))
                .unwrap_or_default(),
            echo.past
                .fragment
                .as_deref()
                .map(|f| format!(" Then: {}.", quote(f)))
                .unwrap_or_default(),
        ));
    }
    for echo in &sweep.echoes {
        let (Some(before), Some(after)) = (&echo.past.fragment, &echo.now.fragment) else {
            continue;
        };
        let now = sheet.cite(&echo.now);
        let past = sheet.cite(&echo.past);
        sheet.draft.push(format!(
            "{} days after {} showed {} {past}, {} showed {} {now}.",
            calendar_gap(history, echo.past.at, echo.now.at),
            echo.past.title,
            quote(before),
            echo.now.title,
            quote(after)
        ));
    }
    for g in &sweep.graph {
        if let (Some(before), Some(after)) = (&g.first.fragment, &g.second.fragment) {
            if before == after {
                continue;
            }
            let first = sheet.cite(&g.first);
            let second = sheet.cite(&g.second);
            let gap = calendar_gap(history, g.first.at, g.second.at);
            sheet.facts.push(format!(
                "POSSIBLE CONNECTION ({gap} calendar days apart). Earlier {first}: {}. Later {second}: {}.",
                quote(before),
                quote(after)
            ));
            if sheet.draft.is_empty() {
                sheet.draft.push(format!(
                    "After {gap} days, {} and {} still appeared connected. The earlier capture showed {} {first}. The later capture showed {} {second}.",
                    g.first.title,
                    g.second.title,
                    quote(before),
                    quote(after)
                ));
            }
        }
    }
    for e in sweep.fresh.iter().chain(&sweep.light.lines) {
        if let Some(line) = &e.fragment {
            if sheet.evidence.contains(e) {
                continue;
            }
            let m = sheet.cite(e);
            sheet.facts.push(format!("STAGED {m}: {}", quote(line)));
            if sheet.draft.is_empty() {
                sheet
                    .draft
                    .push(format!("{}: {} {m}.", e.title, quote(line)));
            }
        }
    }

    // Deep
    for p in &sweep.promoted {
        let marks: String = p.evidence.iter().take(3).map(|e| sheet.cite(e)).collect();
        if sheet.draft.is_empty() {
            sheet.draft.push(format!("{} {marks}", name_of(names, p)));
        }
    }
    // The ranked line finds the window, but can leave out the reason or outcome on the
    // very same frame. Give the writer a bounded view of that frame, not new evidence.
    for (i, e) in sheet.evidence.iter().enumerate() {
        let frame = &history.frames[e.frame];
        let staged = frame.at >= sweep.absence.staged_from && frame.at <= sweep.absence.start;
        let mut lines: Vec<&str> = e.fragment.as_deref().into_iter().collect();
        for line in &frame.lines {
            if lines.len() >= 5 {
                break;
            }
            if is_content(line)
                && !history.common_lines.contains(line)
                && !lines.contains(&line.as_str())
            {
                lines.push(line);
            }
        }
        sheet.facts.push(format!(
            "SOURCE [E{}] {} {}\n  {}",
            i + 1,
            if staged { "CURRENT" } else { "EARLIER" },
            history.day(e.at),
            lines.join("\n  ")
        ));
    }
    sheet
}

const SYSTEM: &str = "Write a small English diary entry about a person's life, not their computer. \
Begin with Event: for a real development or useful setup in a continuing story, or Quiet: for routine material. \
An event gets one or two compact sentences; routine forecasts, inventories or errands get ONE short line. \
Start with a fact from a CURRENT source: what moved, the reply, the deadline, the concrete setback or result. \
EARLIER sources are context only. When they clarify the change, say how many calendar days apart they were. \
Don't repeat details from a recycled email unless they matter to the new reply. Don't combine unrelated material. \
Preserve exactly the stage the source documents: advice is not work done; a listing is not an order; \
an offer to hold a part is not a reservation; a notice is not an application or an award; \
a forecast is not observed weather; a plan is not a sale. Don't assign market totals to this person. \
Don't invent or imply a cause, feeling, habit or action. Use plain English, not filler, and no file names or audit language. \
Paraphrase without quotation marks. End with the exact [E1]-style markers for the sources you used, including a CURRENT source. \
Return only the prefix, prose and markers. No markdown or em dashes.";

const EDIT_SYSTEM: &str = "You are the final fact-checker and line editor of a short diary. Translate the sources accurately. \
The draft can contain false statements: never use the draft as evidence. Use only facts stated in the sources. \
Start with the most consequential NEW fact from a CURRENT source, not a person's name, date, or description of reading. \
If there is a relevant older source, briefly state how many calendar days apart and why the old detail matters. Otherwise cut it. \
If an email thread repeats a prior message, use the message new to this date first. Never imply the person acted if it is merely a guide, \
forecast, listing, deadline, note to self, future appointment or offer. Auction volumes are not the person's sales. \
Be specific about status: 'the guide points to a 22 mm seal', 'the supplier offered to hold one', 'the forecast called for rain'. \
Never turn a request, plan, estimate or forecast into an event that already happened. Never infer weather or a response to it. \
Output exactly one line: start with Event: for a real development or meaningful setup; start with Quiet: for weather, \
inventory, or a routine side note. A Quiet line is one short fact, no comparisons, at most 20 words. An Event is at most 60 words. \
Use concise English, no quotes, no titles, no source labels, no repeated text. At the end include only the relevant [E#] markers, at least one CURRENT. \
Example shape: Event: The spare part arrived. Twelve days earlier, the technician had identified the same part in the service manual. [E2][E1] \
Example shape: Quiet: The forecast called for rain before noon. [E2]";

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

/// Drop each quoted claim unless all its quotes occur verbatim in at least one
/// of the specific frames it cites. A phrase may be a substring of one screen line.
fn grounded_claims(text: &str, matches: impl Fn(usize, &str) -> bool) -> String {
    sentences(text)
        .into_iter()
        .filter(|sentence| {
            let groups = citation_groups(sentence);
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

/// Turns any square bracket that is not an evidence marker into a round one, so a reply
/// that glosses a phrase in brackets is kept instead of falling back to the draft.
fn round_stray_brackets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('[') {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 1..];
        let digits = tail
            .strip_prefix('E')
            .map(|t| t.chars().take_while(|c| c.is_ascii_digit()).count())
            .unwrap_or(0);
        if digits > 0 && tail[1 + digits..].starts_with(']') {
            out.push_str(&rest[i..i + digits + 3]);
            rest = &rest[i + digits + 3..];
        } else if let Some(j) = tail.find(']') {
            out.push('(');
            out.push_str(&tail[..j]);
            out.push(')');
            rest = &tail[j + 1..];
        } else {
            out.push('(');
            rest = tail;
        }
    }
    out.push_str(rest);
    out
}

#[derive(Serialize)]
struct Source {
    date: String,
    time: String,
    app: String,
    title: String,
    frame: i64,
}

#[derive(Serialize)]
struct Entry {
    text: String,
    quiet: bool,
    evidence: Vec<Source>,
    model_written: bool,
    #[serde(skip)]
    degraded: Option<String>,
}

fn make_entry(
    history: &History,
    text: &str,
    evidence: &[Evidence],
    quiet: bool,
    model_written: bool,
    degraded: Option<String>,
) -> Entry {
    let mut prose = String::new();
    let mut sources = Vec::<Source>::new();
    let mut previous_end = 0;
    for (range, marks) in citation_groups(text) {
        prose.push_str(text[previous_end..range.start].trim_end());
        for n in marks {
            if let Some(e) = evidence.get(n.wrapping_sub(1)) {
                let frame = history.frames[e.frame].id;
                if !sources.iter().any(|s| s.frame == frame) {
                    sources.push(Source {
                        date: history.day(e.at),
                        time: hm(history, e.at),
                        app: e.app.clone(),
                        title: e.title.replace('\u{2014}', "-"),
                        frame,
                    });
                }
            }
        }
        previous_end = range.end;
    }
    prose.push_str(&text[previous_end..]);
    Entry {
        text: prose.trim().to_string(),
        quiet,
        evidence: sources,
        model_written,
        degraded,
    }
}

async fn write_entry(
    history: &History,
    sweep: &SweepOutcome,
    model: Option<&ModelConfig>,
    names: &Names,
) -> Entry {
    let sheet = build_sheet(history, sweep, names);
    let quiet = !sheet
        .facts
        .iter()
        .any(|f| f.starts_with("CHANGE") || f.starts_with("POSSIBLE CONNECTION"));
    if sheet.draft.is_empty() {
        return make_entry(history, "A quiet stretch.", &[], true, false, None);
    }
    let draft = |reason| {
        make_entry(
            history,
            &sheet.draft[0],
            &sheet.evidence,
            quiet,
            false,
            Some(reason),
        )
    };
    let Some(model) = model else {
        return draft("no diary model was configured".to_string());
    };
    let mut user = format!(
        "Person: {} ({}).\nScene date: {}.\nSource notes follow.\n\n",
        subject(history),
        history
            .persona
            .role
            .as_deref()
            .unwrap_or(&history.persona.description),
        history.day(sweep.absence.start),
    );
    for f in &sheet.facts {
        let _ = writeln!(user, "{f}\n");
    }
    let mut last_error = String::new();
    let first = model.complete(SYSTEM, &user).await;
    for attempt in 0..3 {
        let prompt = match (&first, attempt) {
            (Ok(text), 0) => format!(
                "{user}\nDraft to review (not evidence): {text}\n\nWrite the final diary line now."
            ),
            _ => format!("{user}\nIgnore any earlier draft. Write the final diary line now."),
        };
        match model.complete(EDIT_SYSTEM, &prompt).await {
            Ok(text) => {
                let text = text
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.starts_with("```"))
                    .map(|line| {
                        line.trim_start_matches(['#', '*', ' '])
                            .trim_start_matches("- ")
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .replace(['{', '}', '*'], "")
                    .replace('\u{2014}', "; ");
                let text = round_stray_brackets(&text);
                let (text, quiet) = if let Some(text) = text.strip_prefix("Quiet:") {
                    (text.trim(), true)
                } else if let Some(text) = text.strip_prefix("Event:") {
                    (text.trim(), false)
                } else {
                    (text.as_str(), quiet)
                };
                let grounded = grounded_claims(text, |marker, phrase| {
                    sheet.evidence.get(marker.wrapping_sub(1)).is_some_and(|e| {
                        history.frames[e.frame]
                            .lines
                            .iter()
                            .any(|line| line.contains(phrase))
                    })
                });
                let entry = make_entry(history, &grounded, &sheet.evidence, quiet, true, None);
                let has_current = citation_groups(&grounded)
                    .iter()
                    .flat_map(|(_, marks)| marks)
                    .any(|n| {
                        sheet.evidence.get(n.wrapping_sub(1)).is_some_and(|e| {
                            e.at >= sweep.absence.staged_from && e.at <= sweep.absence.start
                        })
                    });
                let words = entry.text.split_whitespace().count();
                let clean = !entry.text.contains(['|', '[', ']'])
                    && !["CURRENT", "EARLIER", "SOURCE", "Event:", "Quiet:"]
                        .iter()
                        .any(|label| entry.text.contains(label));
                if has_current && clean && words > 0 && (!quiet || words <= 23) {
                    return entry;
                }
                last_error =
                    "no prose with a valid current source after quote checking".to_string();
            }
            Err(e) => last_error = format!("{e:#}"),
        }
    }
    draft(last_error)
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
        .map(|sweep| write_entry(history, sweep, model, names))
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
    let mut days: BTreeMap<chrono::NaiveDate, Vec<&Entry>> = BTreeMap::new();
    if let (Some(first), Some(last)) = (history.frames.first(), history.frames.last()) {
        let last = history.local(last.at).date_naive();
        for day in history
            .local(first.at)
            .date_naive()
            .iter_days()
            .take_while(|&d| d <= last)
        {
            days.insert(day, Vec::new());
        }
    }
    for (sweep, entry) in outcome.sweeps.iter().zip(&entries) {
        days.entry(history.local(sweep.absence.start).date_naive())
            .or_default()
            .push(entry);
    }
    for (day, day_entries) in &days {
        if day_entries.is_empty() {
            continue;
        }
        let _ = writeln!(diary, "## {}\n", day.format("%A %-d %B %Y"));
        for entry in day_entries {
            if let Some(reason) = &entry.degraded {
                let _ = writeln!(
                diary,
                "> **Degraded entry.** No diary model wrote this one ({reason}); it is the rule-based draft built from the same evidence.\n"
            );
            }
            let _ = writeln!(diary, "{}\n", entry.text);
            if !entry.evidence.is_empty() {
                let _ = writeln!(diary, "<details>\n<summary>Sources</summary>\n");
                for source in &entry.evidence {
                    let _ = writeln!(
                        diary,
                        "- {} {} · {} · {}",
                        source.date,
                        source.time,
                        source.app,
                        source.title.replace('|', "\\|")
                    );
                }
                let _ = writeln!(diary, "\n</details>\n");
            }
        }
    }
    let data = serde_json::json!({
        "persona": who,
        "role": history.persona.role,
        "days": days.iter().map(|(day, entries)| serde_json::json!({
            "date": day.to_string(), "entries": entries,
        })).collect::<Vec<_>>(),
    });
    std::fs::write(out.join("DREAMS.md"), diary)?;
    std::fs::write(out.join("MEMORY.md"), memory_md(history, outcome, names))?;
    std::fs::write(
        out.join("DREAMS.json"),
        serde_json::to_string_pretty(&data)? + "\n",
    )?;
    Ok(Written {
        entries: entries.len(),
        degraded: entries
            .iter()
            .filter(|entry| entry.degraded.is_some())
            .count(),
        model_written: entries.iter().filter(|entry| entry.model_written).count(),
        quiet_shortened: entries.iter().filter(|entry| entry.quiet).count(),
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
    use super::{citation_groups, grounded_claims, round_stray_brackets, sentences};

    #[test]
    fn sentences_ignore_file_names_decimals_and_quotes() {
        let text = "She opened \u{201c}diario-campo.docx\u{201d} at dawn [E1]. The score was 0.84 [E2]. \u{201c}Riego. Tomate.\u{201d} returned. [E3] Then nothing else happened.";
        let s = sentences(text);
        assert_eq!(s.len(), 4, "{s:?}");
        assert!(s[2].ends_with("[E3]"), "{s:?}");
    }

    #[test]
    fn formatting_gloss_does_not_hide_valid_citations() {
        let text = round_stray_brackets("a [E1] [E12] b [gloss]");
        assert_eq!(text, "a [E1] [E12] b (gloss)");
        assert_eq!(citation_groups(&text)[0].1, vec![1, 12]);
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
