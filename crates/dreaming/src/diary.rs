//! Diary — one short entry per dream in `harness-data/dreams/DREAMS.md`.
//!
//! Plain language, one or two links kept, each with evidence (dates, apps,
//! window titles, a few words) so the person can check it is true. An entry is
//! written even when nothing was kept. Every sentence traces back to captured
//! evidence — no invented content. Output goes to `harness-data/dreams/`
//! (git-ignored). Mirrors OpenClaw's `DREAMS.md` / `memory/dreaming/<phase>/…`
//! contract without touching `MEMORY.md`.

use anyhow::Result;
use chrono::{DateTime, Local, TimeZone};
use std::path::Path;

use crate::deep::Scored;
use crate::light::Staged;
use crate::rem::RemOutput;

pub struct DiaryEntry {
    pub dream_index: usize,
    pub window_start: i64,
    pub window_end: i64,
    pub gap_after_ms: i64,
    pub staged: Staged,
    pub rem: RemOutput,
    pub kept: Vec<Scored>,
    pub degraded: bool,
    pub model_hint: Option<String>,
}

pub fn format_entry(e: &DiaryEntry) -> String {
    let mut out = String::new();
    let ws = ms_to_local(e.window_start);
    let we = ms_to_local(e.window_end);
    let ga = ms_to_local(e.gap_after_ms);
    let same_day = ws.format("%Y-%m-%d").to_string() == we.format("%Y-%m-%d").to_string();
    let range = if same_day { format!("{} → {}", ws.format("%H:%M"), we.format("%H:%M")) } else { format!("{} → {}", ws.format("%Y-%m-%d %H:%M"), we.format("%Y-%m-%d %H:%M")) };
    let date_label = ws.format("%Y-%m-%d").to_string();
    out.push_str(&format!("## Dream {} — {}  (window {}), gap at {}\n\n", e.dream_index + 1, date_label, range, ga.format("%Y-%m-%d %H:%M")));
    if e.degraded {
        out.push_str("> *degraded* — wording from structured evidence");
        if let Some(m) = &e.model_hint {
            out.push_str(&format!(" (model on disk: `{m}` but no running sidecar)"));
        }
        out.push_str("; no inference was used. All lines trace to captured evidence below.\n\n");
    }

    // What the dream noticed
    if e.staged.is_empty() {
        out.push_str("What the dream noticed: nothing captured in this window — the screen was still.\n\n");
    } else {
        let apps = if e.staged.app_counts.is_empty() {
            "no app hint recorded".to_string()
        } else {
            e.staged.app_counts.iter().take(3).map(|(a, n)| format!("{a} ({n})")).collect::<Vec<_>>().join(", ")
        };
        out.push_str(&format!(
            "What the dream noticed: {} frame(s) in {} session(s). Apps: {}.",
            e.staged.frames.len(), e.staged.sessions.len(), apps
        ));
        if !e.rem.themes.is_empty() {
            out.push_str(&format!(" Themes: {}.", e.rem.themes.join("; ")));
        }
        out.push('\n');
        out.push('\n');
    }

    // Links considered / kept
    out.push_str(&format!(
        "Links considered: {}, kept: {} (threshold {:.2}, at most 2).\n\n",
        e.rem.candidates.len(), e.kept.len(), 0.45
    ));

    if e.kept.is_empty() {
        out.push_str("No links were strong enough to keep this time — the dream leaves no new connection.\n\n");
        // Still surface brushed-past as evidence that REM looked.
        let brushed: Vec<_> = e.rem.candidates.iter().filter(|c| matches!(c.kind, crate::rem::CandidateKind::BrushedPast)).collect();
        if !brushed.is_empty() {
            out.push_str("Brushed past without stopping:\n\n");
            for c in brushed.iter().take(3) {
                out.push_str(&format!("- session {}: {}\n", c.a, c.titles.join(" · ")));
            }
            out.push('\n');
        }
    } else {
        out.push_str("Links kept:\n\n");
        for (i, s) in e.kept.iter().enumerate() {
            let c = &s.candidate;
            let pair = match c.b { Some(b) => format!("{} ↔ {}", c.a, b), None => format!("{}", c.a) };
            let kind = match c.kind { crate::rem::CandidateKind::SemanticNeighbour => "close in meaning, far apart in time", crate::rem::CandidateKind::RecurringTheme => "recurring theme", crate::rem::CandidateKind::BrushedPast => "brushed past" };
            out.push_str(&format!("{}. **{}** — *{}* — score {:.2} (relevance {:.2}, recency {:.2}, recurrence {:.2}, richness {:.2})\n", i+1, pair, kind, s.score, s.signals.relevance, s.signals.recency, s.signals.recurrence, s.signals.richness));
            if let Some(sim) = c.similarity {
                out.push_str(&format!("   similarity {:.2}, {:.1} days apart, never same thread.\n", sim, c.distance_days.unwrap_or(0.0)));
            } else if let Some(d) = c.distance_days {
                out.push_str(&format!("   {:.1} days span, {} day(s) with this entity.\n", d, c.recurrence_days));
            }
            // Evidence — dates, apps, titles, a few words — checkable.
            for sid in std::iter::once(c.a).chain(c.b) {
                // find session start for date
                if let Some(sess) = e.staged.sessions.iter().find(|s| s.id == sid).or_else(|| None) {
                    let d = ms_to_local(sess.started_at).format("%Y-%m-%d %H:%M").to_string();
                    let title = sess.title.clone().unwrap_or_else(|| sess.context_key.clone());
                    out.push_str(&format!("   - session {} @ {} — `{}` — app {}\n", sid, d, title, sess.context_key));
                } else {
                    // fallback: id only (still evidence, not invention)
                    out.push_str(&format!("   - session {} (outside staged window — from graph/vec neighbours)\n", sid));
                }
            }
            if !c.apps.is_empty() { out.push_str(&format!("   apps: {}\n", c.apps.join(", "))); }
            if !c.evidence_snippets.is_empty() {
                out.push_str("   words: ");
                for sn in &c.evidence_snippets { out.push_str(&format!("\"{}\" ", sn)); }
                out.push('\n');
            }
            out.push('\n');
        }
    }

    // Frame evidence sample (so the person can spot-check)
    if !e.staged.frames.is_empty() {
        out.push_str("Evidence sample (first 3 frames in window):\n\n");
        for f in e.staged.frames.iter().take(3) {
            let d = ms_to_local(f.captured_at).format("%Y-%m-%d %H:%M:%S").to_string();
            let app = f.app_hint.as_deref().unwrap_or("—");
            let title = f.window_title.as_deref().unwrap_or("—");
            let snippet = f.content_snippet.as_deref().unwrap_or("");
            out.push_str(&format!("- {} — app `{}` — \"{}\" — \"{}\"\n", d, app, title, snippet));
        }
        out.push('\n');
    }

    out.push_str("---\n\n");
    out
}

pub fn write_diary(path: &Path, entries: &[DiaryEntry]) -> Result<()> {
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
    let mut body = String::new();
    body.push_str("# Dreams — ScreenSearch dreaming diary\n\n");
    body.push_str(&format!("> Generated {} (local time). Each dream is one absence gap in the last 7 days.\n", Local::now().format("%Y-%m-%d %H:%M")));
    body.push_str("> Nothing invented: every line traces to captured evidence (dates, apps, titles, a few words) you can check in the app.\n\n");
    body.push_str("---\n\n");
    if entries.is_empty() {
        body.push_str("No dreams this sweep — no absence gaps in the window, or no frames captured.\n\n");
    } else {
        for e in entries { body.push_str(&format_entry(e)); }
    }
    // Summary footer
    let total_considered: usize = entries.iter().map(|e| e.rem.candidates.len()).sum();
    let total_kept: usize = entries.iter().map(|e| e.kept.len()).sum();
    body.push_str(&format!("## Summary\n\nDreams: {}, links considered: {}, links kept: {}.\n", entries.len(), total_considered, total_kept));
    std::fs::write(path, body)?;
    Ok(())
}

fn ms_to_local(ms: i64) -> DateTime<Local> {
    let secs = ms / 1000;
    let nsecs = ((ms % 1000) * 1_000_000) as u32;
    Local.timestamp_opt(secs, nsecs).single().unwrap_or_else(|| Local.timestamp_opt(0, 0).single().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::light::Staged;
    use crate::rem::RemOutput;

    #[test]
    fn diary_always_has_entry_even_when_nothing_kept() {
        let staged = Staged { window_start: 0, window_end: 1000, frames: vec![], sessions: vec![], app_counts: vec![] };
        let e = DiaryEntry {
            dream_index: 0, window_start: 0, window_end: 1000, gap_after_ms: 2000,
            staged, rem: RemOutput { candidates: vec![], themes: vec![] }, kept: vec![],
            degraded: true, model_hint: None,
        };
        let s = format_entry(&e);
        assert!(s.contains("What the dream noticed"));
        assert!(s.contains("No links were strong enough") || s.contains("nothing captured"));
    }
}
