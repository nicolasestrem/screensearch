//! The durable job queue — the heart of *enrich-deferred* (`03 §5`).
//!
//! State machine: `pending → running → done`, or `running →` (`fail`) `→ pending`
//! (retry with backoff) `→ … → dead` (dead-letter at `max_attempts`, never
//! silently dropped). Claims are a single atomic `UPDATE … RETURNING` so no job is
//! handed to two workers.

use anyhow::bail;
use rusqlite::{named_params, params_from_iter, types::Value};
use traits::{Job, JobKind, JobState, JobStats, NewJob, Result};

use crate::SqliteStore;

/// DB `kind` token (`03 §4`). The four `graph_*` values are the 0.4.5 P5 work-graph job
/// kinds (`03 §7h.7`, 0.4.5 D11) and `session_embed` is the P6 Ask v2 session-vector kind
/// (`03 §7i.2`, 0.4.5 D11); every pre-existing kind keeps its persisted string.
fn kind_token(kind: JobKind) -> &'static str {
    match kind {
        JobKind::EmbedText => "embed_text",
        JobKind::VisionTag => "vision_tag",
        JobKind::GraphExtract => "graph_extract",
        JobKind::GraphNormalize => "graph_normalize",
        JobKind::GraphCluster => "graph_cluster",
        JobKind::GraphBackfill => "graph_backfill",
        JobKind::SessionEmbed => "session_embed",
    }
}

fn kind_from_token(s: &str) -> rusqlite::Result<JobKind> {
    match s {
        "embed_text" => Ok(JobKind::EmbedText),
        "vision_tag" => Ok(JobKind::VisionTag),
        "graph_extract" => Ok(JobKind::GraphExtract),
        "graph_normalize" => Ok(JobKind::GraphNormalize),
        "graph_cluster" => Ok(JobKind::GraphCluster),
        "graph_backfill" => Ok(JobKind::GraphBackfill),
        "session_embed" => Ok(JobKind::SessionEmbed),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown job kind {other:?}").into(),
        )),
    }
}

fn state_from_token(s: &str) -> rusqlite::Result<JobState> {
    match s {
        "pending" => Ok(JobState::Pending),
        "running" => Ok(JobState::Running),
        "done" => Ok(JobState::Done),
        "failed" => Ok(JobState::Failed),
        "dead" => Ok(JobState::Dead),
        other => Err(rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            format!("unknown job state {other:?}").into(),
        )),
    }
}

/// Maps a `jobs` row (in the column order used by claim) to a [`Job`].
fn row_to_job(r: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get("id")?,
        kind: kind_from_token(&r.get::<_, String>("kind")?)?,
        frame_id: r.get("frame_id")?,
        state: state_from_token(&r.get::<_, String>("state")?)?,
        priority: r.get("priority")?,
        attempts: r.get("attempts")?,
        max_attempts: r.get("max_attempts")?,
        not_before: r.get("not_before")?,
        last_error: r.get("last_error")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
    })
}

impl SqliteStore {
    /// Enqueues a deferred job, returning its id (`03 §5`).
    pub async fn enqueue_job(&self, job: NewJob) -> Result<i64> {
        let kind = kind_token(job.kind);
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT INTO jobs (kind, frame_id, priority, max_attempts, not_before)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    kind,
                    job.frame_id,
                    job.priority,
                    job.max_attempts,
                    job.not_before
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
    }

    /// Atomically claims up to `limit` runnable jobs of the given `kinds`
    /// (`state='pending'`, `not_before <= now`), marking them `running` and
    /// returning them highest-priority-first (`03 §5`). A single
    /// `UPDATE … RETURNING` guarantees no job is claimed twice.
    pub async fn claim_jobs(&self, kinds: &[JobKind], limit: u32, now: i64) -> Result<Vec<Job>> {
        if kinds.is_empty() {
            return Ok(Vec::new());
        }
        let tokens: Vec<&'static str> = kinds.iter().map(|k| kind_token(*k)).collect();
        let placeholders = vec!["?"; tokens.len()].join(",");
        // `now` (caller-supplied) drives only the `not_before` runnability filter;
        // `updated_at` is stamped with the DB clock (`unixepoch()*1000`) — the same
        // clock the stale-job sweep compares against — so the two never mix clocks.
        // params, in order: now (not_before), kinds…, limit.
        let mut binds: Vec<Value> = vec![Value::Integer(now)];
        binds.extend(tokens.iter().map(|t| Value::Text((*t).to_string())));
        binds.push(Value::Integer(i64::from(limit)));

        let sql = format!(
            "UPDATE jobs SET state = 'running', updated_at = (unixepoch()*1000)
             WHERE id IN (
                 SELECT id FROM jobs
                 WHERE state = 'pending' AND not_before <= ? AND kind IN ({placeholders})
                 ORDER BY priority DESC, id
                 LIMIT ?
             )
             RETURNING id, kind, frame_id, state, priority, attempts, max_attempts,
                       not_before, last_error, created_at, updated_at"
        );

        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(&sql)?;
            let jobs = stmt
                .query_map(params_from_iter(binds), row_to_job)?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            // RETURNING is unordered; restore the claim order (priority desc, id).
            let mut jobs = jobs;
            jobs.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
            Ok(jobs)
        })
        .await
    }

    /// Marks a claimed job `done` (`03 §5`). Errors if no such running job exists
    /// (a zero-row update means an unknown/stale/wrong-state id — surfaced rather
    /// than silently dropped).
    pub async fn complete_job(&self, id: i64) -> Result<()> {
        self.with_conn(move |conn| {
            let changed = conn.execute(
                "UPDATE jobs SET state = 'done', updated_at = (unixepoch()*1000)
                 WHERE id = ?1 AND state = 'running'",
                rusqlite::params![id],
            )?;
            if changed == 0 {
                bail!("complete_job: job {id} is missing or not running");
            }
            Ok(())
        })
        .await
    }

    /// Records a failed attempt (`03 §5`). Always increments `attempts` and stores
    /// `err`. If `retry_at` is `Some` *and* attempts remain (`< max_attempts`), the
    /// job returns to `pending` with `not_before = retry_at` (backoff); otherwise
    /// it is dead-lettered (`state='dead'`) — surfaced in diagnostics, never lost.
    /// Only a claimed/running job may be failed.
    pub async fn fail_job(&self, id: i64, err: &str, retry_at: Option<i64>) -> Result<()> {
        let err = err.to_string();
        self.with_conn(move |conn| {
            let changed = conn.execute(
                "UPDATE jobs SET
                   attempts = attempts + 1,
                   last_error = :err,
                   state = CASE
                     WHEN :retry IS NOT NULL AND attempts + 1 < max_attempts THEN 'pending'
                     ELSE 'dead' END,
                   not_before = CASE
                     WHEN :retry IS NOT NULL AND attempts + 1 < max_attempts THEN :retry
                     ELSE not_before END,
                   updated_at = (unixepoch()*1000)
                 WHERE id = :id AND state = 'running'",
                named_params! { ":err": err, ":retry": retry_at, ":id": id },
            )?;
            if changed == 0 {
                bail!("fail_job: job {id} is missing or not running");
            }
            Ok(())
        })
        .await
    }

    /// Requeues jobs stuck in `running` that a worker abandoned mid-job — there is
    /// no lease (`03 §6` "restart + requeue"; `07` gap #6). Resets them to `pending`
    /// so they are reclaimable; returns the count requeued. Does **not** touch
    /// `attempts` (a crash is not a logical failure).
    ///
    /// - `older_than_ms <= 0` — the **startup sweep**: with no worker live, requeue
    ///   *every* `running` job **unconditionally** (no `updated_at` comparison, so it
    ///   is immune to any sub-second clock skew — a job marked running in the last
    ///   fraction of a second before a crash is never missed).
    /// - `older_than_ms > 0` — the **periodic visibility sweep**: requeue jobs whose
    ///   `updated_at` (stamped by `claim` with the `unixepoch()*1000` DB clock) is at
    ///   least that far in the past. Same clock on both sides, so no mismatch.
    pub async fn reset_stale_running_jobs(&self, older_than_ms: i64) -> Result<u64> {
        self.with_conn(move |conn| {
            let changed = if older_than_ms <= 0 {
                conn.execute(
                    "UPDATE jobs SET state = 'pending', updated_at = (unixepoch()*1000)
                     WHERE state = 'running'",
                    [],
                )?
            } else {
                conn.execute(
                    "UPDATE jobs SET state = 'pending', updated_at = (unixepoch()*1000)
                     WHERE state = 'running' AND updated_at <= (unixepoch()*1000 - ?1)",
                    rusqlite::params![older_than_ms],
                )?
            };
            Ok(changed as u64)
        })
        .await
    }

    /// Aggregate queue counts by state for the diagnostics surface (`03 §7`), plus the
    /// per-kind `vision_tag` pending/running split that drives the tray + quick-menu
    /// "Start/Stop vision tagging" label (0.3.2 PR3, `03 §7d`). One `GROUP BY state, kind`
    /// pass fills both — the vision fields are subsets of the aggregate `pending`/`running`.
    pub async fn job_stats(&self) -> Result<JobStats> {
        self.with_conn(move |conn| {
            let mut stats = JobStats::default();
            let mut stmt =
                conn.prepare("SELECT state, kind, COUNT(*) FROM jobs GROUP BY state, kind")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (state, kind, count) = row?;
                let count = count as u64;
                match state.as_str() {
                    "pending" => stats.pending += count,
                    "running" => stats.running += count,
                    "done" => stats.done += count,
                    "failed" => stats.failed += count,
                    "dead" => stats.dead += count,
                    _ => {}
                }
                if kind == "vision_tag" {
                    match state.as_str() {
                        "pending" => stats.vision_pending += count,
                        "running" => stats.vision_running += count,
                        _ => {}
                    }
                }
            }
            Ok(stats)
        })
        .await
    }

    /// Count of `vision_tag` jobs currently `pending` or `running` — the in-flight
    /// vision backlog the idle backfill watches as a low-watermark (index-backed by
    /// `idx_jobs_frame_kind_state`).
    pub async fn pending_vision_job_count(&self) -> Result<u64> {
        self.with_conn(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM jobs
                 WHERE kind = 'vision_tag' AND state IN ('pending', 'running')",
                [],
                |r| r.get(0),
            )?;
            Ok(count as u64)
        })
        .await
    }

    /// Count of jobs of one kind in `pending` or `running` — lets an idle producer tell
    /// whether a run of that kind is already queued or in flight before it enqueues another
    /// (`03 §5`; the `session_embed` producer uses it, `03 §7i.2`).
    pub async fn pending_jobs_of_kind(&self, kind: JobKind) -> Result<u64> {
        self.with_conn(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM jobs
                 WHERE kind = ?1 AND state IN ('pending', 'running')",
                [kind_token(kind)],
                |r| r.get(0),
            )?;
            Ok(count as u64)
        })
        .await
    }

    /// Deletes all `pending` `vision_tag` jobs (the tray/quick-menu "Stop vision tagging"
    /// action, 0.3.2 PR3; `03 §7d`), returning how many were removed. Only `pending` rows
    /// are dropped: a `running` job has no lease to revoke, so vision work already in
    /// flight completes and is never lost. No schema change — a plain `DELETE` on the
    /// existing `jobs` table (D10).
    pub async fn cancel_pending_vision_jobs(&self) -> Result<u64> {
        self.with_conn(move |conn| {
            let changed = conn.execute(
                "DELETE FROM jobs WHERE kind = 'vision_tag' AND state = 'pending'",
                [],
            )?;
            Ok(changed as u64)
        })
        .await
    }
}
