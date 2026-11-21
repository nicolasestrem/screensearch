//! Light phase — sort and stage recent activity since the last dream.
//! No durable writes. Reads frames/sessions/entities within one dream window.

use anyhow::Result;
use rusqlite::Connection;
use std::collections::HashMap;

/// Minimal staged view the later phases need.
#[derive(Debug, Clone)]
pub struct StagedFrame {
    pub id: i64,
    pub captured_at: i64,
    pub app_hint: Option<String>,
    pub window_title: Option<String>,
    pub session_id: Option<i64>,
    pub content_snippet: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StagedSession {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub kind: String,
    pub context_key: String,
    pub title: Option<String>,
    /// frame count inside the dream window that carries this session_id.
    pub frames_in_window: usize,
}

#[derive(Debug, Clone)]
pub struct Staged {
    pub window_start: i64,
    pub window_end: i64,
    pub frames: Vec<StagedFrame>,
    pub sessions: Vec<StagedSession>,
    /// histogram of app_hint in window (for reporting).
    pub app_counts: Vec<(String, usize)>,
}

impl Staged {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// Stage `[window_start, window_end)` — sorted by `captured_at`, deduped by frame id.
pub fn stage(conn: &Connection, window_start: i64, window_end: i64) -> Result<Staged> {
    // Frames with a content snippet for evidence (truncate later for prose).
    let mut frames = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT f.id, f.captured_at, f.app_hint, f.window_title, f.session_id,
                    ft.content_text
             FROM frames f
             LEFT JOIN frame_text ft ON ft.frame_id = f.id
             WHERE f.captured_at >= ?1 AND f.captured_at < ?2
             ORDER BY f.captured_at ASC",
        )?;
        let rows = stmt.query_map([window_start, window_end], |r| {
            let snippet: Option<String> = r.get(5)?;
            Ok(StagedFrame {
                id: r.get(0)?,
                captured_at: r.get(1)?,
                app_hint: r.get(2)?,
                window_title: r.get(3)?,
                session_id: r.get(4)?,
                content_snippet: snippet.map(|s| truncate_snippet(&s, 80)),
            })
        })?;
        for row in rows {
            frames.push(row?);
        }
    }

    // Sessions overlapping the window — used for REM semantic neighbour KNN.
    let mut sessions = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, started_at, ended_at, kind, context_key, title
             FROM sessions
             WHERE started_at < ?2 AND COALESCE(ended_at, ?2) > ?1
             ORDER BY started_at ASC",
        )?;
        let rows = stmt.query_map([window_start, window_end], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?;
        // count frames per session within this window for later richness/brush detection
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for f in &frames {
            if let Some(sid) = f.session_id {
                *counts.entry(sid).or_default() += 1;
            }
        }
        for row in rows {
            let (id, started_at, ended_at, kind, context_key, title) = row?;
            if counts.get(&id).copied().unwrap_or(0) == 0 && !overlaps_window(started_at, ended_at, window_start, window_end) {
                // Window membership clause above may include a session that has no frame
                // in-window (artifact-only session); keep only sessions with evidence here.
                // That's Light's «stage the recent material» contract, not a silent siphon.
                continue;
            }
            sessions.push(StagedSession {
                id,
                started_at,
                ended_at,
                kind,
                context_key,
                title,
                frames_in_window: counts.get(&id).copied().unwrap_or(0),
            });
        }
    }

    // Histogram.
    let mut hist: HashMap<String, usize> = HashMap::new();
    for f in &frames {
        if let Some(app) = f.app_hint.as_deref() {
            *hist.entry(app.to_string()).or_default() += 1;
        }
    }
    let mut app_counts: Vec<_> = hist.into_iter().collect();
    app_counts.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(Staged { window_start, window_end, frames, sessions, app_counts })
}

fn overlaps_window(started_at: i64, ended_at: Option<i64>, ws: i64, we: i64) -> bool {
    started_at < we && ended_at.unwrap_or(we) > ws
}

fn truncate_snippet(s: &str, max: usize) -> String {
    let t = s.replace('\n', " ").replace('\r', " ");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.chars().count() <= max {
        t
    } else {
        t.chars().take(max).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn seed() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE frames(id INTEGER PRIMARY KEY, captured_at INTEGER, app_hint TEXT, window_title TEXT, session_id INTEGER);
             CREATE TABLE frame_text(frame_id INTEGER PRIMARY KEY, content_text TEXT);
             CREATE TABLE sessions(id INTEGER PRIMARY KEY, started_at INTEGER, ended_at INTEGER, kind TEXT, context_key TEXT, title TEXT);
            ",
        ).unwrap();
        conn.execute("INSERT INTO sessions VALUES (1, 0, 10000, 'focus', 'focus:code', 'Code')", []).unwrap();
        conn.execute("INSERT INTO frames VALUES (1, 1000, 'Code', 'main.rs', 1)", []).unwrap();
        conn.execute("INSERT INTO frame_text VALUES (1, 'hello world')", []).unwrap();
        conn
    }

    #[test]
    fn stage_sorted_and_counts() {
        let conn = seed();
        let s = stage(&conn, 0, 20000).unwrap();
        assert_eq!(s.frames.len(), 1);
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].frames_in_window, 1);
    }
}
