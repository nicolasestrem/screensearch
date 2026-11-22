//! Read-only loading of a history database for `dream`.
//!
//! Only the normal v15 tables are read (`frames`, `frame_text`, `sessions`, `embeddings`,
//! `embedding_vectors`, `session_vectors`, `entities`, `session_entities`, `threads`,
//! `thread_sessions`, `report_runs`, `session_artifacts`, `ask_turns`, `settings`), so the same
//! loader works on a real profile. The connection is opened `SQLITE_OPEN_READ_ONLY` with
//! `query_only`; nothing in this module can write.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, FixedOffset, Local, Offset, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::gen::personas;

/// One captured frame, reduced to what the phases read.
#[derive(Debug, Clone)]
pub struct Frame {
    pub id: i64,
    pub at: i64,
    pub app: String,
    pub title: String,
    pub session: Option<i64>,
    /// Filtered (`content_text`) lines, chrome already removed by the product's filter.
    pub lines: Vec<String>,
    /// The frame's stored OCR embedding (first chunk), L2-normalised.
    pub vector: Option<Vec<f32>>,
}

/// One window inside one session (same app and title): the unit REM compares, because a
/// session usually mixes several unrelated documents.
#[derive(Debug, Clone)]
pub struct Episode {
    pub session: i64,
    pub app: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub frames: Vec<usize>,
    pub vector: Vec<f32>,
}

/// One session row plus its member frames (indices into `History::frames`).
#[derive(Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub frames: Vec<usize>,
    /// L2-normalised centroid of the stored frame embeddings (or the stored session vector).
    pub vector: Option<Vec<f32>>,
    pub thread: Option<(i64, String)>,
    pub entities: Vec<(i64, String, String)>,
}

/// Who the history belongs to, as far as the database says.
#[derive(Debug, Clone)]
pub struct Persona {
    pub id: String,
    /// A display name ("Lucía Ferrer") when the catalogue knows the persona.
    pub name: Option<String>,
    pub description: String,
    /// "market gardener at Hort dels Tarongers", when the catalogue knows the persona.
    pub role: Option<String>,
    pub offset: FixedOffset,
}

/// Text the app itself already produced for a time range (saved reports, project notes).
#[derive(Debug, Clone)]
pub struct SavedText {
    pub at: i64,
    pub label: String,
    pub text: String,
}

/// Everything `dream` reads, loaded once.
pub struct History {
    pub persona: Persona,
    pub frames: Vec<Frame>,
    pub sessions: Vec<Session>,
    pub session_index: HashMap<i64, usize>,
    /// Chronological episodes with an embedding.
    pub episodes: Vec<Episode>,
    /// Lines that appear in a large share of sessions: residual chrome, never content.
    pub common_lines: HashSet<String>,
    pub saved: Vec<SavedText>,
    /// `ask_turns` questions with the session ids they cited (recall queries).
    pub asks: Vec<(i64, String, Vec<i64>)>,
    pub entity_rows: Vec<traits::EntitySessionInput>,
    pub thread_rows: Vec<traits::ThreadSessionInput>,
    pub vector_source: &'static str,
}

/// An absence in screen activity: one sweep.
#[derive(Debug, Clone, Copy)]
pub struct Absence {
    /// Last frame before the gap.
    pub start: i64,
    /// First frame after the gap.
    pub end: i64,
    /// Where the material this sweep stages begins (end of the previous absence).
    pub staged_from: i64,
}

impl History {
    pub fn load(db: &Path, min_gap_ms: i64) -> Result<Self> {
        if !db.is_file() {
            bail!("{} does not exist", db.display());
        }
        // `vec0` lives in the sqlite-vec extension the store registers process-wide
        // (`sqlite3_auto_extension`) when it opens any database. Opening an in-memory store
        // first therefore makes the extension available to the read-only connection below,
        // without duplicating the store's registration code.
        drop(store::SqliteStore::open_in_memory()?);
        let conn = Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .with_context(|| format!("opening {} read-only", db.display()))?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "query_only", true)?;
        let version: i64 =
            conn.query_row("SELECT version FROM schema_version", [], |r| r.get(0))?;
        if version != 15 {
            bail!("schema_version {version}: dream reads the v15 schema only");
        }

        let persona = read_persona(&conn)?;
        let mut frames = read_frames(&conn)?;
        let frame_index: HashMap<i64, usize> =
            frames.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
        let mut sessions = read_sessions(&conn)?;
        let session_index: HashMap<i64, usize> = sessions
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id, i))
            .collect();
        for (i, frame) in frames.iter().enumerate() {
            if let Some(sid) = frame.session {
                if let Some(&si) = session_index.get(&sid) {
                    sessions[si].frames.push(i);
                }
            }
        }
        let vector_source = attach_vectors(&conn, &frame_index, &mut frames, &mut sessions)?;
        let episodes = build_episodes(&frames, &sessions, min_gap_ms);
        attach_graph(&conn, &session_index, &mut sessions)?;
        let common_lines = common_lines(&frames, &sessions);
        let saved = read_saved(&conn)?;
        let asks = read_asks(&conn)?;
        let entity_rows = read_entity_rows(&conn)?;
        let thread_rows = read_thread_rows(&conn)?;
        Ok(Self {
            persona,
            frames,
            sessions,
            session_index,
            episodes,
            common_lines,
            saved,
            asks,
            entity_rows,
            thread_rows,
            vector_source,
        })
    }

    pub fn local(&self, ms: i64) -> DateTime<FixedOffset> {
        Utc.timestamp_millis_opt(ms)
            .single()
            .unwrap_or_default()
            .with_timezone(&self.persona.offset)
    }

    pub fn day(&self, ms: i64) -> String {
        self.local(ms).format("%Y-%m-%d").to_string()
    }

    pub fn session(&self, id: i64) -> Option<&Session> {
        self.session_index.get(&id).map(|&i| &self.sessions[i])
    }

    pub fn content_line(&self, frame: &Frame, line: &str) -> bool {
        !matches!(
            frame.app.to_ascii_lowercase().as_str(),
            "explorer" | "explorer.exe"
        ) && is_content(line)
            && !self.common_lines.contains(line)
    }

    /// Content lines of an episode, most frequent first, residual chrome removed.
    pub fn episode_lines(&self, episode: &Episode, limit: usize) -> Vec<String> {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for &fi in &episode.frames {
            let frame = &self.frames[fi];
            for line in &frame.lines {
                if self.content_line(frame, line) {
                    *counts.entry(line.as_str()).or_default() += 1;
                }
            }
        }
        let mut lines: Vec<(&str, usize)> = counts.into_iter().collect();
        lines.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(b.0.len().cmp(&a.0.len()))
                .then(a.0.cmp(b.0))
        });
        lines
            .into_iter()
            .take(limit)
            .map(|(l, _)| l.to_string())
            .collect()
    }
}

/// A content line worth quoting: has letters, is not a bare number/file listing.
pub fn is_content(line: &str) -> bool {
    let line = line.trim();
    let path = line.starts_with(r"\\")
        || line.as_bytes().get(..3).is_some_and(|start| {
            start[0].is_ascii_alphabetic() && start[1] == b':' && matches!(start[2], b'\\' | b'/')
        });
    let filename = |word: &str| {
        word.rsplit_once('.').is_some_and(|(name, extension)| {
            !name.is_empty()
                && matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "csv"
                        | "doc"
                        | "docx"
                        | "eml"
                        | "html"
                        | "jpeg"
                        | "jpg"
                        | "json"
                        | "md"
                        | "msg"
                        | "ods"
                        | "odt"
                        | "pdf"
                        | "png"
                        | "ppt"
                        | "pptx"
                        | "rs"
                        | "txt"
                        | "xls"
                        | "xlsx"
                        | "zip"
                )
        })
    };
    let mut words = line.split_whitespace();
    let last = words.next_back();
    let name_only = last.is_some_and(filename);
    let listing_row = matches!(last, Some("KB" | "MB" | "GB" | "bytes")) && words.any(filename);
    let letters = line.chars().filter(|c| c.is_alphabetic()).count();
    !path && !name_only && !listing_row && letters >= 12 && line.split_whitespace().count() >= 3
}

/// Absences of at least `min_gap_ms` between consecutive frames, oldest first.
pub fn absences(history: &History, min_gap_ms: i64) -> Vec<Absence> {
    let mut out = Vec::new();
    let mut staged_from = history.frames.first().map(|f| f.at).unwrap_or(0);
    for pair in history.frames.windows(2) {
        let (a, b) = (pair[0].at, pair[1].at);
        if b - a >= min_gap_ms {
            out.push(Absence {
                start: a,
                end: b,
                staged_from,
            });
            staged_from = b;
        }
    }
    out
}

fn read_persona(conn: &Connection) -> Result<Persona> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [personas::PERSONA_SETTING],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(raw) = raw {
        // The setting holds the persona's public identity; the catalogue adds a display name.
        let meta = personas::metadata_for_setting(Some(&raw))?;
        let offset = personas::offset_of(&meta.timezone)?;
        let known = personas::find(&meta.id);
        let name = known.map(|p| p.name.to_string());
        let role = known.map(|p| match p.employer {
            Some(employer) => format!("{} at {employer}", p.occupation.to_lowercase()),
            None => p.occupation.to_lowercase(),
        });
        return Ok(Persona {
            id: meta.id,
            name,
            description: meta.description,
            role,
            offset,
        });
    }
    // A real profile carries no persona: the diary speaks of "the person at this screen".
    Ok(Persona {
        id: "profile".to_string(),
        name: None,
        description: "the person at this screen".to_string(),
        role: None,
        offset: Local::now().offset().fix(),
    })
}

fn read_frames(conn: &Connection) -> Result<Vec<Frame>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.captured_at, COALESCE(f.app_hint, ''), COALESCE(f.window_title, ''),
                f.session_id, COALESCE(t.content_text, '')
         FROM frames f LEFT JOIN frame_text t ON t.frame_id = f.id
         ORDER BY f.captured_at, f.id",
    )?;
    let rows = stmt.query_map([], |r| {
        let text: String = r.get(5)?;
        Ok(Frame {
            id: r.get(0)?,
            at: r.get(1)?,
            app: r.get(2)?,
            title: r.get(3)?,
            session: r.get(4)?,
            lines: text
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect(),
            vector: None,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn read_sessions(conn: &Connection) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare("SELECT id FROM sessions ORDER BY started_at, id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Session {
            id: r.get(0)?,

            frames: Vec::new(),
            vector: None,
            thread: None,
            entities: Vec::new(),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn blob_to_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn normalise(mut v: Vec<f32>) -> Option<Vec<f32>> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return None;
    }
    v.iter_mut().for_each(|x| *x /= norm);
    Some(v)
}

/// Session centroids from the stored frame embeddings; falls back to `session_vectors`.
fn attach_vectors(
    conn: &Connection,
    frame_index: &HashMap<i64, usize>,
    frames: &mut [Frame],
    sessions: &mut [Session],
) -> Result<&'static str> {
    let mut sums: HashMap<i64, (Vec<f32>, usize)> = HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT e.frame_id, v.embedding FROM embeddings e
         JOIN embedding_vectors v ON v.embedding_id = e.id
         WHERE e.source = 'ocr' ORDER BY e.frame_id, e.chunk_index",
    )?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let frame_id: i64 = row.get(0)?;
        let blob: Vec<u8> = row.get(1)?;
        let Some(&fi) = frame_index.get(&frame_id) else {
            continue;
        };
        let Some(sid) = frames[fi].session else {
            continue;
        };
        let Some(v) = normalise(blob_to_f32(&blob)) else {
            continue;
        };
        if frames[fi].vector.is_none() {
            frames[fi].vector = Some(v.clone());
        }
        let entry = sums.entry(sid).or_insert_with(|| (vec![0.0; v.len()], 0));
        if entry.0.len() == v.len() {
            entry.0.iter_mut().zip(&v).for_each(|(a, b)| *a += b);
            entry.1 += 1;
        }
    }
    let mut filled = 0;
    for session in sessions.iter_mut() {
        if let Some((sum, n)) = sums.remove(&session.id) {
            if n > 0 {
                session.vector = normalise(sum);
                filled += usize::from(session.vector.is_some());
            }
        }
    }
    if filled > 0 {
        return Ok("frame embeddings");
    }
    let mut stmt = conn.prepare("SELECT session_id, embedding FROM session_vectors")?;
    let stored: HashMap<i64, Vec<u8>> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for session in sessions.iter_mut() {
        if let Some(blob) = stored.get(&session.id) {
            session.vector = normalise(blob_to_f32(blob));
            filled += usize::from(session.vector.is_some());
        }
    }
    Ok(if filled > 0 {
        "session vectors"
    } else {
        "none"
    })
}

fn attach_graph(
    conn: &Connection,
    index: &HashMap<i64, usize>,
    sessions: &mut [Session],
) -> Result<()> {
    let mut stmt = conn.prepare(
        "SELECT ts.session_id, t.id, t.title FROM thread_sessions ts
         JOIN threads t ON t.id = ts.thread_id ORDER BY ts.session_id, t.id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?)))?;
    for row in rows {
        let (sid, tid, title): (i64, i64, String) = row?;
        if let Some(&i) = index.get(&sid) {
            sessions[i].thread.get_or_insert((tid, title));
        }
    }
    let mut stmt = conn.prepare(
        "SELECT se.session_id, e.id, e.kind, e.canonical_name FROM session_entities se
         JOIN entities e ON e.id = se.entity_id ORDER BY se.session_id, e.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
    })?;
    for row in rows {
        let (sid, eid, kind, name): (i64, i64, String, String) = row?;
        if let Some(&i) = index.get(&sid) {
            sessions[i].entities.push((eid, kind, name));
        }
    }
    Ok(())
}

/// Groups frames by session and window inside one continuous stretch. Splitting at the
/// same gaps used by the sweep prevents a previous visit from citing future content.
fn build_episodes(frames: &[Frame], sessions: &[Session], min_gap_ms: i64) -> Vec<Episode> {
    let mut stretch = 0;
    let mut stretches = vec![stretch; frames.len()];
    for (i, pair) in frames.windows(2).enumerate() {
        if pair[1].at - pair[0].at >= min_gap_ms {
            stretch += 1;
        }
        stretches[i + 1] = stretch;
    }
    let mut out = Vec::new();
    for s in sessions {
        let mut by: BTreeMap<(_, _, _), Vec<usize>> = BTreeMap::new();
        for &fi in &s.frames {
            let f = &frames[fi];
            by.entry((stretches[fi], f.app.clone(), f.title.clone()))
                .or_default()
                .push(fi);
        }
        for ((_, app, title), idx) in by {
            let mut sum: Vec<f32> = Vec::new();
            for &fi in &idx {
                if let Some(v) = &frames[fi].vector {
                    if sum.is_empty() {
                        sum = vec![0.0; v.len()];
                    }
                    if sum.len() == v.len() {
                        sum.iter_mut().zip(v).for_each(|(a, b)| *a += b);
                    }
                }
            }
            let Some(vector) = normalise(sum) else {
                continue;
            };
            out.push(Episode {
                session: s.id,
                app,
                title,
                start: frames[idx[0]].at,
                end: frames[*idx.last().unwrap_or(&idx[0])].at,
                frames: idx,
                vector,
            });
        }
    }
    out.sort_by(|a, b| a.start.cmp(&b.start).then(a.title.cmp(&b.title)));
    out
}

/// Lines present in more than a quarter of the sessions (at least 4): leftover chrome.
fn common_lines(frames: &[Frame], sessions: &[Session]) -> HashSet<String> {
    let mut df: HashMap<&str, usize> = HashMap::new();
    for s in sessions {
        let mut seen: HashSet<&str> = HashSet::new();
        for &fi in &s.frames {
            for l in &frames[fi].lines {
                seen.insert(l.as_str());
            }
        }
        for l in seen {
            *df.entry(l).or_default() += 1;
        }
    }
    let limit = (sessions.len() / 4).max(4);
    df.into_iter()
        .filter(|(_, n)| *n > limit)
        .map(|(l, _)| l.to_string())
        .collect()
}

fn read_saved(conn: &Connection) -> Result<Vec<SavedText>> {
    let mut out = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT window_end, kind, payload FROM report_runs WHERE phase = 'finished'
         ORDER BY window_end",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (at, kind, payload) = row?;
        let markdown = serde_json::from_str::<serde_json::Value>(&payload)
            .ok()
            .and_then(|v| {
                v.pointer("/result/markdown")
                    .and_then(|m| m.as_str())
                    .map(str::to_string)
            });
        if let Some(text) = markdown {
            let label = if kind == "weekly_synthesis" {
                "weekly synthesis"
            } else {
                "daily report"
            };
            out.push(SavedText {
                at,
                label: label.to_string(),
                text,
            });
        }
    }
    let mut stmt = conn.prepare(
        "SELECT created_at, content FROM session_artifacts WHERE kind = 'note' ORDER BY created_at",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (at, text) = row?;
        out.push(SavedText {
            at,
            label: "project note".to_string(),
            text,
        });
    }
    Ok(out)
}

fn read_asks(conn: &Connection) -> Result<Vec<(i64, String, Vec<i64>)>> {
    let stmt = conn.prepare("SELECT * FROM ask_turns LIMIT 0")?;
    let cols: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    drop(stmt);
    if !cols.iter().any(|c| c == "question") {
        return Ok(Vec::new());
    }
    let created = if cols.iter().any(|c| c == "created_at") {
        "created_at"
    } else {
        "0"
    };
    let citations = if cols.iter().any(|c| c == "citations") {
        "citations"
    } else {
        "'[]'"
    };
    let sql = format!("SELECT {created}, question, {citations} FROM ask_turns ORDER BY 1");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (at, q, cites) = row?;
        let mut ids = Vec::new();
        if let Some(Ok(v)) = cites.map(|c| serde_json::from_str::<serde_json::Value>(&c)) {
            collect_session_ids(&v, &mut ids);
        }
        out.push((at, q, ids));
    }
    Ok(out)
}

fn collect_session_ids(v: &serde_json::Value, out: &mut Vec<i64>) {
    match v {
        serde_json::Value::Object(map) => {
            for (k, val) in map {
                if k == "session_id" {
                    if let Some(id) = val.as_i64() {
                        out.push(id);
                    }
                } else if k == "sessions" && val.is_array() {
                    // `{"sessions":[12, 40], ...}` or `[{"session_id":12}, ...]`
                    for item in val.as_array().into_iter().flatten() {
                        match item.as_i64() {
                            Some(id) => out.push(id),
                            None => collect_session_ids(item, out),
                        }
                    }
                } else {
                    collect_session_ids(val, out);
                }
            }
        }
        serde_json::Value::Array(items) => items.iter().for_each(|i| collect_session_ids(i, out)),
        _ => {}
    }
}

/// The same rows `Store::insight_entity_sessions` feeds the kernel's recurrence derivation.
fn read_entity_rows(conn: &Connection) -> Result<Vec<traits::EntitySessionInput>> {
    let mut stmt = conn.prepare(
        "SELECT e.id, e.kind, e.canonical_name, s.id, s.started_at, e.last_seen_at
         FROM session_entities se
         JOIN sessions s ON s.id = se.session_id
         JOIN entities e ON e.id = se.entity_id
         ORDER BY e.id, s.started_at, s.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(traits::EntitySessionInput {
            entity_id: r.get(0)?,
            entity_kind: r.get(1)?,
            canonical_name: r.get(2)?,
            session_id: r.get(3)?,
            session_started_at: r.get(4)?,
            last_seen_at: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// The same rows `Store::insight_thread_sessions` feeds the kernel's resumption derivation.
fn read_thread_rows(conn: &Connection) -> Result<Vec<traits::ThreadSessionInput>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.title, s.id, s.started_at, s.ended_at, s.title, s.summary
         FROM thread_sessions ts
         JOIN threads t ON t.id = ts.thread_id
         JOIN sessions s ON s.id = ts.session_id
         ORDER BY t.id, s.started_at, s.id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(traits::ThreadSessionInput {
            thread_id: r.get(0)?,
            title: r.get(1)?,
            session_id: r.get(2)?,
            started_at: r.get(3)?,
            ended_at: r.get(4)?,
            session_title: r.get(5)?,
            summary: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::{build_episodes, is_content, Frame, Session};

    #[test]
    fn clips_a_single_window_in_one_session_at_the_sweep_gap() {
        let frames = [0, 30_000, 7_230_000].map(|at| Frame {
            id: at,
            at,
            app: "Excel".to_string(),
            title: "Market prices - Excel".to_string(),
            session: Some(1),
            lines: Vec::new(),
            vector: Some(vec![1.0]),
        });
        let session = Session {
            id: 1,
            frames: vec![0, 1, 2],
            vector: None,
            thread: None,
            entities: Vec::new(),
        };
        let episodes = build_episodes(&frames, &[session], 7_200_000);
        assert_eq!(episodes[0].frames, [0, 1]);
        assert_eq!(episodes[1].frames, [2]);
    }

    #[test]
    fn rejects_file_metadata_keeps_document_content() {
        assert!(!is_content(r"C:\Users\Lucia\Latest market prices.xlsx"));
        assert!(!is_content("Latest market prices.xlsx"));
        assert!(!is_content(
            "Latest market prices.xlsx 14/09/2026 10:20 Microsoft Excel Worksheet 18 KB"
        ));
        assert!(is_content(
            "The price sheet recorded 1,45 EUR/kg this week."
        ));
    }
}
