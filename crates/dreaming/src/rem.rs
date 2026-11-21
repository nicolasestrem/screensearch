//! REM phase — recurring themes and links between sessions that were never put
//! together: sessions **close in meaning** (cosine on `session_vectors`) but **far
//! apart in time** and **never in the same thread**. Also: what was brushed past
//! without stopping.
//!
//! Uses stored embeddings + graph, not shared words. All evidence comes from SELECTs.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

use crate::light::Staged;

/// One candidate link REM surfaced; Deep will score it and keep ≤2.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Pair of session ids (a, b) — for cross-time links; singletons use b = None.
    pub a: i64,
    pub b: Option<i64>,
    pub kind: CandidateKind,
    pub similarity: Option<f64>, // cosine similarity for semantic neighbours
    pub distance_days: Option<f64>,
    pub apps: Vec<String>,
    pub titles: Vec<String>,
    pub evidence_snippets: Vec<String>,
    /// days on which this entity/theme recurred (for recurrence signal).
    pub recurrence_days: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CandidateKind {
    SemanticNeighbour,
    RecurringTheme,
    BrushedPast,
}

/// Thresholds reused from `kernel::insights` (03 §7j.1 / §7h.4):
const NEIGHBOUR_MIN_SIMILARITY: f64 = 0.80;
const RECURRENCE_GAP_MS: i64 = 7 * 24 * 60 * 60 * 1000; // same as kernel::insights::RECURRENCE_GAP_MS
const FAR_APART_MS: i64 = 24 * 60 * 60 * 1000; // "far apart in time" — task ask: distinct calendar days; 24 h is the floor

pub struct RemOutput {
    pub candidates: Vec<Candidate>,
    /// Human-readable theme summaries (for diary when no link is kept).
    pub themes: Vec<String>,
}

/// Run REM over `staged`. `conn` is read-only. Never writes.
pub fn run(conn: &Connection, staged: &Staged) -> Result<RemOutput> {
    let mut candidates = Vec::new();
    let mut themes = Vec::new();

    // 1) Semantic neighbours: close in embedding space, far in time, never same thread.
    let semantic = semantic_neighbours(conn, staged)?;
    let mut semantic_kept = 0;
    for c in semantic {
        if c.similarity.unwrap_or(0.0) >= NEIGHBOUR_MIN_SIMILARITY {
            candidates.push(c);
            semantic_kept += 1;
        }
    }
    if semantic_kept > 0 {
        themes.push(format!("{semantic_kept} close-meaning sessions far apart in time"));
    }

    // 2) Recurring themes: entities that appeared on ≥2 days in the last 30 d window.
    let recurring = recurring_themes(conn, staged)?;
    for c in &recurring {
        themes.push(format!(
            "recurring: {} ({} days)",
            c.apps.join(", "),
            c.recurrence_days
        ));
    }
    candidates.extend(recurring);

    // 3) Brushed past: sessions touched briefly (≤2 frames or <90 s) with no thread
    //    companion in this window and not revisited via the same entity later.
    let brushed = brushed_past(staged);
    for c in &brushed {
        themes.push(format!("brushed past: {}", c.titles.join(", ")));
    }
    candidates.extend(brushed);

    Ok(RemOutput { candidates, themes })
}

fn semantic_neighbours(conn: &Connection, staged: &Staged) -> Result<Vec<Candidate>> {
    // Graceful if no session_vectors yet — the vec extension or table may be absent.
    // Probe rather than hard-fail.
    let has_vec = conn
        .prepare("SELECT 1 FROM sqlite_master WHERE type='table' AND name='session_vectors'")
        .and_then(|mut s| s.query_row([], |_| Ok(())))
        .is_ok();
    // sqlite-vec tables are virtual; sqlite_master lists them too but a missing extension
    // causes a different failure on the actual KNN query. So just attempt and map errors.
    let mut out = Vec::new();
    if !has_vec {
        return Ok(out);
    }

    // Build a fast thread lookup: session_id -> thread_id
    let thread_of = thread_map(conn)?;

    // For each session in window that has a vector, KNN k=5 against all.
    for sess in &staged.sessions {
        let blob: Option<Vec<u8>> = match conn
            .prepare("SELECT embedding FROM session_vectors WHERE session_id = ?1")
        {
            Ok(mut s) => match s.query_row([sess.id], |r| r.get::<_, Vec<u8>>(0)) {
                Ok(b) => Some(b),
                Err(rusqlite::Error::QueryReturnedNoRows) => None,
                Err(_) => None,
            },
            Err(_) => None,
        };
        let Some(blob) = blob else { continue };
        // KNN via sqlite-vec
        let knn_rows: Vec<(i64, f64)> = match (|| {
            let mut stmt = conn.prepare(
                "SELECT session_id, distance FROM session_vectors
                 WHERE embedding MATCH ?1 AND k = ?2 ORDER BY distance",
            )?;
            let rows = stmt.query_map([rusqlite::types::Value::Blob(blob), rusqlite::types::Value::Integer(6)], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
            })?;
            let mut v = Vec::new();
            for r in rows { v.push(r?); }
            Ok::<_, anyhow::Error>(v)
        })() {
            Ok(v) => v,
            Err(_) => continue, // vec extension unavailable or query failed — graceful empty
        };

        for (nid, distance) in knn_rows {
            if nid == sess.id {
                continue;
            }
            let similarity = (1.0 - distance).clamp(-1.0, 1.0);
            // far apart?
            let other_started = session_started(conn, nid).unwrap_or(0);
            let gap = (sess.started_at - other_started).abs();
            if gap < FAR_APART_MS {
                continue;
            }
            // never same thread
            let ta = thread_of.get(&sess.id).copied();
            let tb = thread_of.get(&nid).copied();
            if ta.is_some() && ta == tb {
                continue;
            }

            let (apps, titles, snippets) = session_evidence(conn, sess.id, nid)?;
            out.push(Candidate {
                a: sess.id,
                b: Some(nid),
                kind: CandidateKind::SemanticNeighbour,
                similarity: Some(similarity),
                distance_days: Some(gap as f64 / 86_400_000.0),
                apps,
                titles,
                evidence_snippets: snippets,
                recurrence_days: 1,
            });
        }
    }
    // Dedup unordered pairs
    let mut seen = HashSet::new();
    let mut dedup = Vec::new();
    for c in out {
        let b = c.b.unwrap();
        let key = if c.a < b { (c.a, b) } else { (b, c.a) };
        if seen.insert(key) {
            dedup.push(c);
        }
    }
    // Most similar first
    dedup.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap());
    dedup.truncate(10);
    Ok(dedup)
}

fn recurring_themes(conn: &Connection, staged: &Staged) -> Result<Vec<Candidate>> {
    // Entities whose sessions span >= RECURRENCE_GAP_MS or at least 2 UTC days.
    // Keep it simple: session_entities grouped by entity.
    let mut out = Vec::new();
    let entity_days: Vec<(i64, String, i64 /*first*/, i64 /*last*/, i64 /*sessions*/)> = {
        let sql = "SELECT se.entity_id, e.canonical_name, MIN(s.started_at), MAX(s.started_at), COUNT(DISTINCT s.id)
                   FROM session_entities se
                   JOIN entities e ON e.id = se.entity_id
                   JOIN sessions s ON s.id = se.session_id
                   WHERE s.started_at >= ?1 AND s.started_at < ?2
                   GROUP BY se.entity_id
                   HAVING COUNT(DISTINCT s.id) >= 2";
        let ws = staged.window_start - 30 * 86_400_000; // 30 d lookback for recurrence context
        let we = staged.window_end;
        match conn.prepare(sql) {
            Ok(mut s) => {
                let rows = s.query_map([ws, we], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
                })?;
                let mut v = Vec::new();
                for r in rows { v.push(r?); }
                v
            }
            Err(_) => Vec::new(),
        }
    };

    for (eid, name, first, last, _cnt) in entity_days {
        let gap = last - first;
        let days = ((gap as f64) / 86_400_000.0).ceil() as usize + 1;
        if gap < RECURRENCE_GAP_MS && days < 2 {
            continue;
        }
        // Collect the session sample for evidence (2 sessions touching this entity)
        let sessions: Vec<i64> = {
            let mut s = conn.prepare("SELECT session_id FROM session_entities WHERE entity_id = ?1 ORDER BY last_seen_at DESC LIMIT 2")?;
            let rows = s.query_map([eid], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if sessions.len() < 2 { continue; }
        let (apps, titles, snippets) = session_evidence(conn, sessions[0], sessions[1])?;
        out.push(Candidate {
            a: sessions[0],
            b: Some(sessions[1]),
            kind: CandidateKind::RecurringTheme,
            similarity: None,
            distance_days: Some(gap as f64 / 86_400_000.0),
            apps: {
                let mut a = apps;
                if !name.is_empty() && !a.contains(&name) { a.insert(0, name.clone()); }
                a
            },
            titles,
            evidence_snippets: snippets,
            recurrence_days: days,
        });
    }
    out.truncate(5);
    Ok(out)
}

fn brushed_past(staged: &Staged) -> Vec<Candidate> {
    let mut out = Vec::new();
    for s in &staged.sessions {
        let dur = s.ended_at.map(|e| e - s.started_at).unwrap_or(0);
        let brushed = s.frames_in_window <= 2 || (s.frames_in_window > 0 && dur > 0 && dur < 90_000);
        if !brushed { continue; }
        // Only sessions with some content; skip empty phantoms
        if s.frames_in_window == 0 { continue; }
        let title = s.title.clone().unwrap_or_else(|| s.context_key.clone());
        out.push(Candidate {
            a: s.id,
            b: None,
            kind: CandidateKind::BrushedPast,
            similarity: None,
            distance_days: None,
            apps: vec![s.context_key.clone()],
            titles: vec![title],
            evidence_snippets: Vec::new(),
            recurrence_days: 1,
        });
    }
    out.truncate(5);
    out
}

fn thread_map(conn: &Connection) -> Result<HashMap<i64, i64>> {
    let mut map = HashMap::new();
    if let Ok(mut s) = conn.prepare("SELECT session_id, thread_id FROM thread_sessions") {
        let rows = s.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
        for r in rows { let (sid, tid) = r?; map.insert(sid, tid); }
    }
    Ok(map)
}

fn session_started(conn: &Connection, id: i64) -> Result<i64> {
    let v: i64 = conn.query_row("SELECT started_at FROM sessions WHERE id = ?1", [id], |r| r.get(0))?;
    Ok(v)
}

fn session_evidence(conn: &Connection, a: i64, b: i64) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let mut apps = Vec::new();
    let mut titles = Vec::new();
    let mut snippets = Vec::new();
    for sid in [a, b] {
        // title
        if let Ok(t) = conn.query_row("SELECT title FROM sessions WHERE id = ?1", [sid], |r| r.get::<_, Option<String>>(0)) {
            if let Some(tt) = t { if !tt.is_empty() { titles.push(tt); } }
        }
        // representative frame: earliest frame in session with text
        if let Ok(mut s) = conn.prepare(
            "SELECT f.app_hint, f.window_title, ft.content_text
             FROM frames f LEFT JOIN frame_text ft ON ft.frame_id = f.id
             WHERE f.session_id = ?1 ORDER BY f.captured_at ASC LIMIT 1",
        ) {
            if let Ok(row) = s.query_row([sid], |r| Ok((r.get::<_, Option<String>>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?))) {
                if let Some(app) = row.0 { if !apps.contains(&app) { apps.push(app); } }
                if let Some(ct) = row.2 {
                    let snippet = ct.replace('\n', " ").chars().take(60).collect::<String>();
                    if !snippet.trim().is_empty() { snippets.push(snippet); }
                }
            }
        }
    }
    Ok((apps, titles, snippets))
}
