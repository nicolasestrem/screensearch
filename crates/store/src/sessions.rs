//! Schema-11 session persistence (`03 §4`/`§7e`). Mutating boundaries/classification is
//! guarded by `frozen = 0`; frame assignment additionally refuses to rewrite a frame whose
//! current session is frozen, making the stable-id boundary load-bearing in SQL.

use rusqlite::{params, params_from_iter, types::Value, OptionalExtension, Row};
use traits::{
    FrameMeta, NewSession, NewSessionArtifact, Result, SegmenterFrame, Session, SessionArtifact,
    SessionArtifactKind, SessionArtifactRole, SessionContent, SessionFilter, SessionFrameSample,
    SessionHost, SessionKind, SessionReference,
};

use crate::SqliteStore;

const MAX_SESSION_FRAME_SAMPLE: u32 = 24;

// SQLite's one-argument trim() only removes ASCII space. This is the complete set used by
// Rust's `char::is_whitespace`/`str::trim` for the pinned toolchain, passed as trim()'s explicit
// character set so moving the predicate into SQLite does not narrow the existing contract.
const RUST_TRIM_WHITESPACE: &str = "\u{0009}\u{000a}\u{000b}\u{000c}\u{000d}\u{0020}\u{0085}\u{00a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}";

const SESSION_HAS_USABLE_CONTENT_SQL: &str = "SELECT EXISTS(
       SELECT 1 FROM frames f
       JOIN frame_text ft ON ft.frame_id=f.id
       WHERE f.session_id=?1 AND trim(ft.content_text, ?2)<>''
       LIMIT 1
     )";

fn kind_from_db(value: &str) -> rusqlite::Result<SessionKind> {
    match value {
        "focus" => Ok(SessionKind::Focus),
        "meeting" => Ok(SessionKind::Meeting),
        "ai" => Ok(SessionKind::Ai),
        "other" => Ok(SessionKind::Other),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("invalid session kind {other:?}").into(),
        )),
    }
}

fn host_from_db(value: Option<String>) -> rusqlite::Result<Option<SessionHost>> {
    value
        .map(|value| match value.as_str() {
            "terminal" => Ok(SessionHost::Terminal),
            "desktop" => Ok(SessionHost::Desktop),
            "browser" => Ok(SessionHost::Browser),
            "ide" => Ok(SessionHost::Ide),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid session host {other:?}").into(),
            )),
        })
        .transpose()
}

fn artifact_kind_from_db(value: &str) -> rusqlite::Result<SessionArtifactKind> {
    match value {
        "exchange" => Ok(SessionArtifactKind::Exchange),
        "transcript" => Ok(SessionArtifactKind::Transcript),
        "note" => Ok(SessionArtifactKind::Note),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("invalid session artifact kind {other:?}").into(),
        )),
    }
}

fn role_from_db(value: Option<String>) -> rusqlite::Result<Option<SessionArtifactRole>> {
    value
        .map(|value| match value.as_str() {
            "user" => Ok(SessionArtifactRole::User),
            "agent" => Ok(SessionArtifactRole::Agent),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("invalid session artifact role {other:?}").into(),
            )),
        })
        .transpose()
}

fn row_to_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    let kind: String = row.get(3)?;
    Ok(Session {
        id: row.get(0)?,
        started_at: row.get(1)?,
        ended_at: row.get(2)?,
        kind: kind_from_db(&kind)?,
        tool: row.get(4)?,
        host: host_from_db(row.get(5)?)?,
        context_key: row.get(6)?,
        title: row.get(7)?,
        summary: row.get(8)?,
        summary_model: row.get(9)?,
        confidence: row.get(10)?,
        frozen: row.get::<_, i64>(11)? != 0,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

const SESSION_COLUMNS: &str = "id, started_at, ended_at, kind, tool, host, context_key, title, summary, summary_model, confidence, frozen, created_at, updated_at";

fn row_to_frame(row: &Row<'_>) -> rusqlite::Result<SegmenterFrame> {
    Ok(SegmenterFrame {
        id: row.get(0)?,
        captured_at: row.get(1)?,
        app_hint: row.get(2)?,
        window_title: row.get(3)?,
        browser_url: row.get(4)?,
    })
}

fn row_to_artifact(row: &Row<'_>) -> rusqlite::Result<SessionArtifact> {
    let kind: String = row.get(2)?;
    Ok(SessionArtifact {
        id: row.get(0)?,
        session_id: row.get(1)?,
        kind: artifact_kind_from_db(&kind)?,
        role: role_from_db(row.get(3)?)?,
        frame_id: row.get(4)?,
        content: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn row_to_frame_meta(row: &Row<'_>) -> rusqlite::Result<FrameMeta> {
    Ok(FrameMeta {
        frame_id: row.get(0)?,
        captured_at: row.get(1)?,
        image_path: row.get(2)?,
        image_purged: row.get::<_, i64>(3)? != 0,
        app_hint: row.get(4)?,
    })
}

pub(crate) fn row_to_session_reference(row: &Row<'_>) -> rusqlite::Result<SessionReference> {
    let kind: String = row.get(3)?;
    Ok(SessionReference {
        id: row.get(0)?,
        started_at: row.get(1)?,
        ended_at: row.get(2)?,
        kind: kind_from_db(&kind)?,
        tool: row.get(4)?,
        host: host_from_db(row.get(5)?)?,
        title: row.get(6)?,
        confidence: row.get(7)?,
    })
}

impl SqliteStore {
    pub async fn insert_session(&self, session: NewSession) -> Result<i64> {
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO sessions
                 (started_at, ended_at, kind, tool, host, context_key, confidence, frozen)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    session.started_at,
                    session.ended_at,
                    session.kind.as_str(),
                    session.tool,
                    session.host.map(SessionHost::as_str),
                    session.context_key,
                    session.confidence,
                    i64::from(session.frozen),
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    pub async fn update_unfrozen_session(&self, id: i64, session: NewSession) -> Result<bool> {
        self.with_conn(move |conn| {
            let changed = conn.execute(
                "UPDATE sessions
                 SET started_at=?2, ended_at=?3, kind=?4, tool=?5, host=?6,
                     context_key=?7, confidence=?8, updated_at=unixepoch()*1000
                 WHERE id=?1 AND frozen=0",
                params![
                    id,
                    session.started_at,
                    session.ended_at,
                    session.kind.as_str(),
                    session.tool,
                    session.host.map(SessionHost::as_str),
                    session.context_key,
                    session.confidence,
                ],
            )?;
            Ok(changed > 0)
        })
        .await
    }

    pub async fn delete_unfrozen_session(&self, id: i64) -> Result<bool> {
        self.with_conn(move |conn| {
            Ok(conn.execute("DELETE FROM sessions WHERE id=?1 AND frozen=0", [id])? > 0)
        })
        .await
    }

    pub async fn assign_frames_session(
        &self,
        frame_ids: &[i64],
        session_id: Option<i64>,
    ) -> Result<u64> {
        if frame_ids.is_empty() {
            return Ok(0);
        }
        let frame_ids = frame_ids.to_vec();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let mut changed = 0u64;
            for chunk in frame_ids.chunks(400) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "UPDATE frames SET session_id=?
                     WHERE id IN ({placeholders})
                       AND (session_id IS NULL OR EXISTS (
                           SELECT 1 FROM sessions current
                           WHERE current.id=frames.session_id AND current.frozen=0
                       ))"
                );
                let mut values = Vec::with_capacity(chunk.len() + 1);
                values.push(session_id.map_or(Value::Null, Value::Integer));
                values.extend(chunk.iter().copied().map(Value::Integer));
                changed += tx.execute(&sql, params_from_iter(values))? as u64;
            }
            tx.commit()?;
            Ok(changed)
        })
        .await
    }

    pub async fn freeze_sessions(&self, older_than_ms: i64) -> Result<u64> {
        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE sessions SET frozen=1, updated_at=unixepoch()*1000
                 WHERE frozen=0 AND ended_at IS NOT NULL AND ended_at < ?1",
                [older_than_ms],
            )? as u64)
        })
        .await
    }

    pub async fn unfrozen_sessions(&self) -> Result<Vec<Session>> {
        self.with_conn(move |conn| {
            let sql = format!(
                "SELECT {SESSION_COLUMNS} FROM sessions WHERE frozen=0 ORDER BY started_at, id"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map([], row_to_session)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn sessions_in_range(&self, filter: SessionFilter) -> Result<Vec<Session>> {
        self.with_conn(move |conn| {
            let mut predicates = Vec::new();
            let mut values: Vec<Value> = Vec::new();
            if let Some(from) = filter.from_ms {
                predicates.push("COALESCE(ended_at, ?) > ?");
                values.push(Value::Integer(filter.now_ms));
                values.push(Value::Integer(from));
            }
            if let Some(to) = filter.to_ms {
                predicates.push("started_at < ?");
                values.push(Value::Integer(to));
            }
            if let Some(kind) = filter.kind {
                predicates.push("kind = ?");
                values.push(Value::Text(kind.as_str().to_string()));
            }
            if let Some(tool) = filter.tool {
                predicates.push("tool = ?");
                values.push(Value::Text(tool));
            }
            let mut sql = format!("SELECT {SESSION_COLUMNS} FROM sessions");
            if !predicates.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&predicates.join(" AND "));
            }
            sql.push_str(" ORDER BY started_at DESC, id DESC");
            if let Some(limit) = filter.limit {
                sql.push_str(" LIMIT ?");
                values.push(Value::Integer(i64::from(limit.min(10_000))));
            }
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(params_from_iter(values), row_to_session)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn get_session(&self, id: i64) -> Result<Option<Session>> {
        self.with_conn(move |conn| {
            let sql = format!("SELECT {SESSION_COLUMNS} FROM sessions WHERE id=?1");
            Ok(conn.query_row(&sql, [id], row_to_session).optional()?)
        })
        .await
    }

    pub async fn session_frame_sample(
        &self,
        session_id: i64,
        limit: u32,
    ) -> Result<SessionFrameSample> {
        let limit = limit.min(MAX_SESSION_FRAME_SAMPLE);
        self.with_conn(move |conn| {
            let total: i64 = conn.query_row(
                "SELECT count(*) FROM frames WHERE session_id=?1",
                [session_id],
                |row| row.get(0),
            )?;
            let total_count = u32::try_from(total).unwrap_or(u32::MAX);
            if total == 0 || limit == 0 {
                return Ok(SessionFrameSample {
                    total_count,
                    frames: Vec::new(),
                });
            }

            let frames = if limit == 1 {
                let mut stmt = conn.prepare(
                    "SELECT id, captured_at, image_path, image_purged, app_hint
                     FROM frames WHERE session_id=?1
                     ORDER BY captured_at, id LIMIT 1",
                )?;
                let rows = stmt
                    .query_map([session_id], row_to_frame_meta)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            } else {
                let mut stmt = conn.prepare(
                    "WITH RECURSIVE
                     ranked AS (
                         SELECT id, captured_at, image_path, image_purged, app_hint,
                                row_number() OVER (ORDER BY captured_at, id) - 1 AS rn,
                                count(*) OVER () AS total
                         FROM frames WHERE session_id=?1
                     ),
                     slots(k) AS (
                         SELECT 0
                         UNION ALL SELECT k + 1 FROM slots WHERE k + 1 < ?2
                     )
                     SELECT r.id, r.captured_at, r.image_path, r.image_purged, r.app_hint
                     FROM ranked r JOIN slots s
                       ON r.rn = CASE
                           WHEN r.total <= ?2 THEN s.k
                           ELSE (s.k * (r.total - 1)) / (?2 - 1)
                       END
                     WHERE s.k < min(r.total, ?2)
                     ORDER BY r.captured_at, r.id",
                )?;
                let rows = stmt
                    .query_map(params![session_id, i64::from(limit)], row_to_frame_meta)?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                rows
            };
            Ok(SessionFrameSample {
                total_count,
                frames,
            })
        })
        .await
    }

    pub async fn session_reference_for_frame(
        &self,
        frame_id: i64,
    ) -> Result<Option<SessionReference>> {
        self.with_conn(move |conn| {
            Ok(conn
                .query_row(
                    "SELECT s.id, s.started_at, s.ended_at, s.kind, s.tool, s.host,
                            s.title, s.confidence
                     FROM frames f JOIN sessions s ON s.id=f.session_id
                     WHERE f.id=?1",
                    [frame_id],
                    row_to_session_reference,
                )
                .optional()?)
        })
        .await
    }

    pub async fn session_has_usable_content(&self, session_id: i64) -> Result<bool> {
        self.with_conn(move |conn| {
            Ok(conn.query_row(
                SESSION_HAS_USABLE_CONTENT_SQL,
                params![session_id, RUST_TRIM_WHITESPACE],
                |row| row.get(0),
            )?)
        })
        .await
    }

    pub async fn session_frames_meta(&self, session_id: i64) -> Result<Vec<SegmenterFrame>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, captured_at, app_hint, window_title, browser_url
                 FROM frames WHERE session_id=?1 ORDER BY captured_at, id",
            )?;
            let rows = stmt
                .query_map([session_id], row_to_frame)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn frames_meta_in_range(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<SegmenterFrame>> {
        if to_ms <= from_ms {
            return Ok(Vec::new());
        }
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, captured_at, app_hint, window_title, browser_url
                 FROM frames WHERE captured_at>=?1 AND captured_at<?2
                 ORDER BY captured_at, id",
            )?;
            let rows = stmt
                .query_map(params![from_ms, to_ms], row_to_frame)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn content_texts_for_frames(&self, frame_ids: &[i64]) -> Result<Vec<SessionContent>> {
        if frame_ids.is_empty() {
            return Ok(Vec::new());
        }
        let frame_ids = frame_ids.to_vec();
        self.with_conn(move |conn| {
            let mut out = Vec::new();
            for chunk in frame_ids.chunks(400) {
                let placeholders = std::iter::repeat_n("?", chunk.len())
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT f.id, f.captured_at, ft.content_text
                     FROM frames f JOIN frame_text ft ON ft.frame_id=f.id
                     WHERE f.id IN ({placeholders}) AND trim(ft.content_text)<>''"
                );
                let mut stmt = conn.prepare(&sql)?;
                let rows = stmt.query_map(
                    params_from_iter(chunk.iter().copied().map(Value::Integer)),
                    |row| {
                        Ok(SessionContent {
                            frame_id: row.get(0)?,
                            captured_at: row.get(1)?,
                            content_text: row.get(2)?,
                        })
                    },
                )?;
                out.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            }
            out.sort_by_key(|content| (content.captured_at, content.frame_id));
            Ok(out)
        })
        .await
    }

    pub async fn insert_session_artifacts(
        &self,
        session_id: i64,
        artifacts: &[NewSessionArtifact],
    ) -> Result<Vec<i64>> {
        if artifacts.is_empty() {
            return Ok(Vec::new());
        }
        let artifacts = artifacts.to_vec();
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let mut ids = Vec::with_capacity(artifacts.len());
            for artifact in artifacts {
                tx.execute(
                    "INSERT INTO session_artifacts (session_id, kind, role, frame_id, content)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        session_id,
                        artifact.kind.as_str(),
                        artifact.role.map(SessionArtifactRole::as_str),
                        artifact.frame_id,
                        artifact.content,
                    ],
                )?;
                ids.push(tx.last_insert_rowid());
            }
            tx.commit()?;
            Ok(ids)
        })
        .await
    }

    pub async fn list_session_artifacts(&self, session_id: i64) -> Result<Vec<SessionArtifact>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, kind, role, frame_id, content, created_at
                 FROM session_artifacts WHERE session_id=?1 ORDER BY created_at, id",
            )?;
            let rows = stmt
                .query_map([session_id], row_to_artifact)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(rows)
        })
        .await
    }

    pub async fn delete_session_artifacts_by_kind(
        &self,
        session_id: i64,
        kind: SessionArtifactKind,
    ) -> Result<u64> {
        self.with_conn(move |conn| {
            Ok(conn.execute(
                "DELETE FROM session_artifacts WHERE session_id=?1 AND kind=?2",
                params![session_id, kind.as_str()],
            )? as u64)
        })
        .await
    }

    pub async fn set_session_title_summary(
        &self,
        id: i64,
        title: &str,
        summary: &str,
        model: &str,
    ) -> Result<bool> {
        let (title, summary, model) = (title.to_string(), summary.to_string(), model.to_string());
        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE sessions SET title=?2, summary=?3, summary_model=?4,
                 updated_at=unixepoch()*1000 WHERE id=?1",
                params![id, title, summary, model],
            )? > 0)
        })
        .await
    }

    pub async fn clear_session_title_summary(&self, id: i64) -> Result<bool> {
        self.with_conn(move |conn| {
            Ok(conn.execute(
                "UPDATE sessions SET title=NULL, summary=NULL, summary_model=NULL,
                 updated_at=unixepoch()*1000 WHERE id=?1",
                [id],
            )? > 0)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::{RUST_TRIM_WHITESPACE, SESSION_HAS_USABLE_CONTENT_SQL};

    #[test]
    fn usable_content_query_bounds_results_inside_sqlite() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE sessions (id INTEGER PRIMARY KEY);
             CREATE TABLE frames (
               id INTEGER PRIMARY KEY,
               session_id INTEGER REFERENCES sessions(id)
             );
             CREATE INDEX idx_frames_session ON frames(session_id);
             CREATE TABLE frame_text (
               frame_id INTEGER PRIMARY KEY REFERENCES frames(id),
               content_text TEXT NOT NULL
             );",
        )
        .expect("create query-plan fixture");

        let explain = format!("EXPLAIN {SESSION_HAS_USABLE_CONTENT_SQL}");
        let opcodes = conn
            .prepare(&explain)
            .expect("prepare EXPLAIN")
            .query_map(rusqlite::params![1_i64, RUST_TRIM_WHITESPACE], |row| {
                row.get::<_, String>(1)
            })
            .expect("read EXPLAIN opcodes")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect EXPLAIN opcodes");

        assert!(
            opcodes.iter().any(|opcode| opcode == "DecrJumpZero"),
            "the VM must stop after SQLite finds one usable row; opcodes: {opcodes:?}"
        );

        let query_plan = format!("EXPLAIN QUERY PLAN {SESSION_HAS_USABLE_CONTENT_SQL}");
        let plan = conn
            .prepare(&query_plan)
            .expect("prepare EXPLAIN QUERY PLAN")
            .query_map(rusqlite::params![1_i64, RUST_TRIM_WHITESPACE], |row| {
                row.get::<_, String>(3)
            })
            .expect("read query plan")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("collect query plan");
        assert!(
            plan.iter()
                .any(|detail| detail.contains("idx_frames_session")),
            "session filtering must use idx_frames_session; plan: {plan:?}"
        );
        assert!(
            plan.iter()
                .any(|detail| detail.contains("INTEGER PRIMARY KEY")),
            "frame_text lookup must use its frame_id primary key; plan: {plan:?}"
        );
    }

    #[test]
    fn sqlite_trim_character_set_matches_rust_trim() {
        let rust_whitespace: String = (0..=char::MAX as u32)
            .filter_map(char::from_u32)
            .filter(|character| character.is_whitespace())
            .collect();
        assert_eq!(RUST_TRIM_WHITESPACE, rust_whitespace);
    }
}
