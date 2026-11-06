//! Read-only database access for the P4 eval runner.
//!
//! Every handle comes from [`crate::export::open_readonly`] (`SQLITE_OPEN_READ_ONLY` +
//! `PRAGMA query_only` + a 5 s busy timeout for the live WAL DB). This module issues **SELECTs
//! only**: no temp tables, no pragma writes, no `CREATE`. The `--wal`/`--shm` files are the
//! app's; the runner never needs one of its own.
//!
//! Optional surfaces (`frame_text`, `ocr_text`, the FTS mirror, the 0.4.5 work-graph tables) are
//! detected once at open time, so a baseline DB whose graph tables are still empty reports
//! `n/a (no entity surface)` instead of failing, and a legacy DB without `frame_text` degrades to
//! title/URL matching instead of erroring.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{params, params_from_iter, Connection};

use super::refs::RefKind;
use crate::export::open_readonly;

/// One frame's context columns (no content body: the reference-index pass stays small).
#[derive(Debug, Clone)]
pub struct FrameCtx {
    pub frame_id: i64,
    pub captured_at: i64,
    pub window_title: String,
    pub browser_url: String,
    pub session_id: Option<i64>,
}

/// One session row, the fields the generators and the citation check need.
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub title: Option<String>,
}

/// A cited frame resolved against the DB (existence, window membership, session membership).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitedFrameRow {
    pub id: i64,
    pub exists: bool,
    pub captured_at: Option<i64>,
    pub session_id: Option<i64>,
}

/// A cited session resolved against the DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitedSessionRow {
    pub id: i64,
    pub exists: bool,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
}

/// The read-only eval database plus the optional-table map.
pub struct Db {
    conn: Connection,
    has_frame_text: bool,
    has_ocr_text: bool,
    has_frame_fts: bool,
    has_entities: bool,
    has_aliases: bool,
    has_settings: bool,
}

impl Db {
    /// Open `path` read-only (the `crates/harness/src/export.rs` discipline) and probe the
    /// optional tables. Never creates, migrates or writes anything.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = open_readonly(path)?;
        let has = |name: &str| table_exists(&conn, name);
        let db = Self {
            has_frame_text: has("frame_text"),
            has_ocr_text: has("ocr_text"),
            has_frame_fts: has("frame_text_fts"),
            has_entities: has("entities"),
            has_aliases: has("entity_aliases"),
            has_settings: has("settings"),
            conn,
        };
        Ok(db)
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn has_entity_surface(&self) -> bool {
        self.has_entities
    }

    pub fn has_aliases(&self) -> bool {
        self.has_aliases
    }

    pub fn has_fts(&self) -> bool {
        self.has_frame_fts
    }

    pub fn has_settings(&self) -> bool {
        self.has_settings
    }

    /// `schema_version.version`, when the table exists. The runner reports it; refusing a
    /// non-14 DB is the app's contract (`03 §4`, 0.4.5 D2), not the harness's.
    pub fn schema_version(&self) -> Result<Option<i64>> {
        if !table_exists(&self.conn, "schema_version") {
            return Ok(None);
        }
        let version = self
            .conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get::<_, i64>(0)
            })
            .optional_row()?;
        Ok(version)
    }

    /// `settings.value` for `key`, or `None`.
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        if !self.has_settings {
            return Ok(None);
        }
        let value = self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional_row()?;
        Ok(value)
    }

    pub fn count_frames(&self) -> Result<i64> {
        count(&self.conn, "frames")
    }

    pub fn count_sessions(&self) -> Result<i64> {
        count(&self.conn, "sessions")
    }

    /// `(min(captured_at), max(captured_at))`, or `None` on an empty DB.
    pub fn history_bounds(&self) -> Result<Option<(i64, i64)>> {
        let row = self
            .conn
            .query_row(
                "SELECT MIN(captured_at), MAX(captured_at) FROM frames",
                [],
                |r| Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .optional_row()?;
        Ok(match row {
            Some((Some(a), Some(b))) => Some((a, b)),
            _ => None,
        })
    }

    /// The most recent `limit` frames' context columns (deterministic: `captured_at DESC, id
    /// DESC`). Used for reference discovery, dominant references and seen-again sampling; the
    /// exact reference index below does *not* use this slice.
    pub fn recent_frame_context(&self, limit: usize) -> Result<Vec<FrameCtx>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, captured_at, COALESCE(window_title, ''), COALESCE(browser_url, ''),
                    session_id
             FROM frames
             ORDER BY captured_at DESC, id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit.min(i64::MAX as usize) as i64], map_frame_ctx)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every frame's context columns, newest first, streamed through `f` — one pass, no cap
    /// unless the caller sets `limit`. This is the exact basis of the `involvement` generator.
    pub fn for_each_frame_context(
        &self,
        limit: Option<usize>,
        mut f: impl FnMut(&FrameCtx),
    ) -> Result<usize> {
        let limit_sql = limit.map(|_| " LIMIT ?1").unwrap_or("");
        let sql = format!(
            "SELECT id, captured_at, COALESCE(window_title, ''), COALESCE(browser_url, ''),
                    session_id
             FROM frames
             ORDER BY captured_at DESC, id DESC{limit_sql}"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut count = 0usize;
        match limit {
            Some(n) => {
                let rows = stmt.query_map([n.min(i64::MAX as usize) as i64], map_frame_ctx)?;
                for row in rows {
                    f(&row?);
                    count += 1;
                }
            }
            None => {
                let rows = stmt.query_map([], map_frame_ctx)?;
                for row in rows {
                    f(&row?);
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// All sessions in `started_at` order (id breaks ties), which is also the `sequence`
    /// generator's definition of "the session before Z".
    pub fn sessions(&self) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, title FROM sessions ORDER BY started_at, id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                title: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Sessions whose interval overlaps `[start, end)`; an open session (`ended_at` NULL) is
    /// open-ended up to `now_ms` (`03 §7c` window semantics).
    pub fn sessions_overlapping(
        &self,
        start: i64,
        end: i64,
        now_ms: i64,
    ) -> Result<Vec<SessionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, started_at, ended_at, title FROM sessions
             WHERE started_at < ?2 AND COALESCE(ended_at, ?3) > ?1
             ORDER BY started_at, id",
        )?;
        let rows = stmt.query_map(params![start, end, now_ms], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                started_at: r.get(1)?,
                ended_at: r.get(2)?,
                title: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// A session's frames in time order, capped (used for dominant references and expected ids).
    pub fn session_frames(&self, session_id: i64, limit: usize) -> Result<Vec<FrameCtx>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, captured_at, COALESCE(window_title, ''), COALESCE(browser_url, ''),
                    session_id
             FROM frames
             WHERE session_id = ?1
             ORDER BY captured_at, id
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(
            params![session_id, limit.min(i64::MAX as usize) as i64],
            map_frame_ctx,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Frames matching `needle` (case-insensitive ASCII substring) anywhere the *last-seen*
    /// generator's definition names: `window_title`, `browser_url` or content text, newest first.
    /// Content text is `frame_text.content_text`, falling back to legacy `ocr_text.text`.
    pub fn last_seen_frames(
        &self,
        needle: &str,
        limit: usize,
    ) -> Result<Vec<(i64, i64, Option<i64>)>> {
        let (content, ocr_join) = match (self.has_frame_text, self.has_ocr_text) {
            (true, true) => (
                "COALESCE(NULLIF(ft.content_text, ''), ot.text, '')",
                "LEFT JOIN ocr_text ot ON ot.frame_id = f.id",
            ),
            (true, false) => ("COALESCE(ft.content_text, '')", ""),
            (false, true) => (
                "COALESCE(ot.text, '')",
                "LEFT JOIN ocr_text ot ON ot.frame_id = f.id",
            ),
            (false, false) => ("''", ""),
        };
        let text_join = if self.has_frame_text {
            "LEFT JOIN frame_text ft ON ft.frame_id = f.id"
        } else {
            ""
        };
        let sql = format!(
            "SELECT f.id, f.captured_at, f.session_id
             FROM frames f
             {text_join}
             {ocr_join}
             WHERE instr(lower(COALESCE(f.window_title, '')), ?1) > 0
                OR instr(lower(COALESCE(f.browser_url, '')), ?1) > 0
                OR instr(lower({content}), ?1) > 0
             ORDER BY f.captured_at DESC, f.id DESC
             LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            params![needle.to_lowercase(), limit.min(i64::MAX as usize) as i64],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// `content_text` for the given frame ids (`frame_text` only), ascending by frame id.
    pub fn content_texts(&self, frame_ids: &[i64]) -> Result<Vec<(i64, String)>> {
        if !self.has_frame_text || frame_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=frame_ids.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT frame_id, content_text FROM frame_text
             WHERE frame_id IN ({placeholders}) ORDER BY frame_id"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(frame_ids.iter().copied()), |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Exact FTS5 phrase matches over `frame_text_fts`, newest first, capped at `limit`. The
    /// caller passes an already-quoted phrase; the returned set *is* the seen-again ground truth.
    pub fn fts_matches(&self, phrase: &str, limit: usize) -> Result<Vec<(i64, i64, Option<i64>)>> {
        if !self.has_frame_fts {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT f.id, f.captured_at, f.session_id
             FROM frame_text_fts
             JOIN frames f ON f.id = frame_text_fts.rowid
             WHERE frame_text_fts MATCH ?1
             ORDER BY f.captured_at DESC, f.id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![phrase, limit.min(i64::MAX as usize) as i64], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Distinct local calendar dates that hold at least one frame, newest first.
    pub fn local_days(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT date(captured_at / 1000, 'unixepoch', 'localtime') AS day
             FROM frames
             ORDER BY day DESC",
        )?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Local `[hour_start, hour_end)` bounds of `day` as epoch ms, resolved by SQLite's timezone
    /// math (the `crate::export::day_bounds` rule: the string is local, the `utc` modifier
    /// converts it). DST-transition days resolve to real wall-clock instants.
    pub fn local_hour_window(&self, day: &str, hour_start: u8, hour_end: u8) -> Result<(i64, i64)> {
        let start = format!("{day} {hour_start:02}:00:00");
        let end = format!("{day} {hour_end:02}:00:00");
        let row = self.conn.query_row(
            "SELECT unixepoch(?1, 'utc') * 1000, unixepoch(?2, 'utc') * 1000",
            params![start, end],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok(row)
    }

    /// `YYYY-MM-DD` in the machine's local timezone for `ms`.
    pub fn local_date(&self, ms: i64) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT date(?1 / 1000, 'unixepoch', 'localtime')",
            [ms],
            |r| r.get::<_, String>(0),
        )?)
    }

    /// `YYYY-MM-DD HH:MM` in local time.
    pub fn local_datetime(&self, ms: i64) -> Result<String> {
        Ok(self.conn.query_row(
            "SELECT strftime('%Y-%m-%d %H:%M', ?1 / 1000, 'unixepoch', 'localtime')",
            [ms],
            |r| r.get::<_, String>(0),
        )?)
    }

    /// Resolve cited frame ids: existence, capture time and owning session.
    pub fn cited_frames(&self, ids: &[i64]) -> Result<Vec<CitedFrameRow>> {
        let mut out = Vec::with_capacity(ids.len());
        let mut stmt = self
            .conn
            .prepare_cached("SELECT captured_at, session_id FROM frames WHERE id = ?1")?;
        for id in dedup(ids) {
            let row = stmt
                .query_row([id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
                })
                .optional_row()?;
            out.push(match row {
                Some((captured_at, session_id)) => CitedFrameRow {
                    id,
                    exists: true,
                    captured_at: Some(captured_at),
                    session_id,
                },
                None => CitedFrameRow {
                    id,
                    exists: false,
                    captured_at: None,
                    session_id: None,
                },
            });
        }
        Ok(out)
    }

    /// Resolve cited session ids: existence and interval.
    pub fn cited_sessions(&self, ids: &[i64]) -> Result<Vec<CitedSessionRow>> {
        let mut out = Vec::with_capacity(ids.len());
        let mut stmt = self
            .conn
            .prepare_cached("SELECT started_at, ended_at FROM sessions WHERE id = ?1")?;
        for id in dedup(ids) {
            let row = stmt
                .query_row([id], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
                })
                .optional_row()?;
            out.push(match row {
                Some((started_at, ended_at)) => CitedSessionRow {
                    id,
                    exists: true,
                    started_at: Some(started_at),
                    ended_at,
                },
                None => CitedSessionRow {
                    id,
                    exists: false,
                    started_at: None,
                    ended_at: None,
                },
            });
        }
        Ok(out)
    }

    /// Every `entities` row as `(id, kind, canonical_name, normalized_key)`.
    pub fn entities(&self) -> Result<Vec<(i64, String, String, String)>> {
        if !self.has_entities {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, canonical_name, normalized_key FROM entities ORDER BY id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Every `entity_aliases` row as `(entity_id, alias)`, ascending.
    pub fn aliases(&self) -> Result<Vec<(i64, String)>> {
        if !self.has_aliases {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare("SELECT entity_id, alias FROM entity_aliases ORDER BY entity_id, alias")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Does `needle` appear in any frame's `window_title`, `browser_url` or content text? Used by
    /// the invention check (the question is "is this name in the captured text at all?").
    pub fn text_present(&self, needle: &str) -> Result<bool> {
        let (content, ocr_join) = match (self.has_frame_text, self.has_ocr_text) {
            (true, true) => (
                "COALESCE(NULLIF(ft.content_text, ''), ot.text, '')",
                "LEFT JOIN ocr_text ot ON ot.frame_id = f.id",
            ),
            (true, false) => ("COALESCE(ft.content_text, '')", ""),
            (false, true) => (
                "COALESCE(ot.text, '')",
                "LEFT JOIN ocr_text ot ON ot.frame_id = f.id",
            ),
            (false, false) => ("''", ""),
        };
        let text_join = if self.has_frame_text {
            "LEFT JOIN frame_text ft ON ft.frame_id = f.id"
        } else {
            ""
        };
        let sql = format!(
            "SELECT EXISTS(
                SELECT 1 FROM frames f {text_join} {ocr_join}
                WHERE instr(lower(COALESCE(f.window_title, '')), ?1) > 0
                   OR instr(lower(COALESCE(f.browser_url, '')), ?1) > 0
                   OR instr(lower({content}), ?1) > 0
             )"
        );
        let found: i64 = self
            .conn
            .query_row(&sql, [needle.to_lowercase()], |r| r.get(0))?;
        Ok(found != 0)
    }

    /// Read the API bearer token. Missing is a hard error that names the fix, never a guess.
    pub fn api_token(&self) -> Result<String> {
        match self.setting("api.token")? {
            Some(token) if !token.trim().is_empty() => Ok(token),
            _ => anyhow::bail!(
                "no `api.token` row in the read-only `settings` table \u{2014} enable the API in \
                 ScreenSearch Settings > Integrations (03 \u{a7}7c) so the runner can authenticate; \
                 the token is read from the database, never passed on the command line"
            ),
        }
    }
}

/// The exact key set of a reference index: `(kind, normalized text)`.
pub type RefKey = (RefKind, String);

fn map_frame_ctx(r: &rusqlite::Row<'_>) -> rusqlite::Result<FrameCtx> {
    Ok(FrameCtx {
        frame_id: r.get(0)?,
        captured_at: r.get(1)?,
        window_title: r.get(2)?,
        browser_url: r.get(3)?,
        session_id: r.get(4)?,
    })
}

fn dedup(ids: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

fn count(conn: &Connection, table: &str) -> Result<i64> {
    let n = conn
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
            r.get::<_, i64>(0)
        })
        .with_context(|| format!("counting {table}"))?;
    Ok(n)
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE name = ?1 AND type IN ('table', 'view') LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .optional_row()
    .ok()
    .flatten()
    .is_some()
}

/// `rusqlite`'s `OptionalExtension` without importing the trait at each call site.
trait OptionalRow<T> {
    fn optional_row(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalRow<T> for rusqlite::Result<T> {
    fn optional_row(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
