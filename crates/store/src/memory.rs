//! Transactional report persistence; payload snapshots retain historical provenance.
use crate::SqliteStore;
use anyhow::ensure;
use rusqlite::{params, Connection, OptionalExtension};
use traits::*;

fn run_at(conn: &Connection, id: &str) -> Result<Option<ReportRun>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT payload FROM report_runs WHERE run_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    json.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}
fn delivery_at(conn: &Connection, id: &str) -> Result<Option<ReportDelivery>> {
    let json: Option<String> = conn
        .query_row(
            "SELECT payload FROM report_deliveries WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    json.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}
fn phase(value: ReportRunPhase) -> &'static str {
    match value {
        ReportRunPhase::Queued => "queued",
        ReportRunPhase::Running => "running",
        ReportRunPhase::Blocked => "blocked",
        ReportRunPhase::Finished => "finished",
    }
}
/// The frozen `report_runs.kind` wire tokens (`03 §8e`).
fn kind_db(value: ReportRunKind) -> &'static str {
    match value {
        ReportRunKind::Daily => "daily",
        ReportRunKind::WeeklySynthesis => "weekly_synthesis",
    }
}
fn state(value: ReportDeliveryState) -> &'static str {
    match value {
        ReportDeliveryState::Pending => "pending",
        ReportDeliveryState::Sending => "sending",
        ReportDeliveryState::Succeeded => "succeeded",
        ReportDeliveryState::Conflict => "conflict",
        ReportDeliveryState::Failed => "failed",
        ReportDeliveryState::Skipped => "skipped",
        ReportDeliveryState::UnknownDelivery => "unknown_delivery",
        ReportDeliveryState::Reviewed => "reviewed",
    }
}
fn validate_run(run: &ReportRun) -> Result<()> {
    ensure!(
        !run.run_id.is_empty() && !run.source_id.is_empty(),
        "missing report identity"
    );
    ensure!(run.window_start < run.window_end, "invalid report window");
    ensure!(
        run.schedule_id.is_none() || run.requested_range.is_none(),
        "scheduled reports cannot carry a manual requested range"
    );
    if let Some(result) = &run.result {
        let unique: std::collections::BTreeSet<_> = result.frame_ids.iter().collect();
        ensure!(
            unique.len() == result.frame_ids.len()
                && unique.len() == result.frames_summarized as usize,
            "report provenance count mismatch"
        );
    }
    Ok(())
}
fn write_run(conn: &Connection, run: &ReportRun) -> Result<()> {
    validate_run(run)?;
    conn.execute(
        "UPDATE report_runs SET phase=?2,updated_at=?3,payload=?4,kind=?5 WHERE run_id=?1",
        params![
            run.run_id,
            phase(run.phase),
            run.updated_at,
            serde_json::to_string(run)?,
            kind_db(run.kind)
        ],
    )?;
    Ok(())
}
fn insert_delivery(conn: &Connection, d: &ReportDelivery) -> Result<()> {
    ensure!(!d.id.is_empty(), "missing delivery identity");
    conn.execute("INSERT INTO report_deliveries(id,run_id,state,next_attempt_at,payload) VALUES(?1,?2,?3,?4,?5)",params![d.id,d.run_id,state(d.state),d.next_attempt_at,serde_json::to_string(d)?])?;
    Ok(())
}
fn check_delivery_identity(old: &ReportDelivery, new: &ReportDelivery) -> Result<()> {
    ensure!(
        old.id == new.id
            && old.run_id == new.run_id
            && old.destination == new.destination
            && old.destination_snapshot == new.destination_snapshot
            && old.revision_of == new.revision_of
            && old.created_at == new.created_at
            && old.chunk_index == new.chunk_index
            && old.chunk_count == new.chunk_count,
        "delivery identity is immutable"
    );
    Ok(())
}
fn write_delivery(conn: &Connection, d: &ReportDelivery) -> Result<()> {
    conn.execute(
        "UPDATE report_deliveries SET state=?2,next_attempt_at=?3,payload=?4 WHERE id=?1",
        params![
            d.id,
            state(d.state),
            d.next_attempt_at,
            serde_json::to_string(d)?
        ],
    )?;
    Ok(())
}
impl SqliteStore {
    pub async fn list_report_family_deliveries(&self, run_id: &str) -> Result<Vec<ReportDelivery>> {
        let run_id = run_id.to_owned();
        self.with_conn(move |conn| {
            let mut root = run_at(conn, &run_id)?.ok_or_else(|| anyhow::anyhow!("report not found"))?;
            let mut visited = std::collections::HashSet::new();
            while let Some(parent) = &root.revision_of {
                ensure!(visited.insert(parent.clone()), "report revision cycle");
                root = run_at(conn, parent)?.ok_or_else(|| anyhow::anyhow!("report ancestor unavailable; delivery history cannot be verified"))?;
            }
            let mut statement = conn.prepare(
                "WITH RECURSIVE family(run_id) AS (SELECT ?1 UNION SELECT r.run_id FROM report_runs r JOIN family f ON r.revision_of=f.run_id)
                 SELECT d.payload FROM report_deliveries d JOIN family f ON d.run_id=f.run_id ORDER BY d.id"
            )?;
            let rows = statement.query_map([root.run_id], |row| row.get::<_,String>(0))?;
            rows.map(|json| Ok(serde_json::from_str(&json?)?)).collect()
        }).await
    }

    pub async fn create_report_deliveries(
        &self,
        deliveries: Vec<ReportDelivery>,
    ) -> Result<Vec<ReportDelivery>> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let mut result = Vec::with_capacity(deliveries.len());
            for delivery in deliveries {
                if let Some(old) = delivery_at(&tx, &delivery.id)? {
                    check_delivery_identity(&old, &delivery)?;
                    result.push(old);
                } else {
                    insert_delivery(&tx, &delivery)?;
                    result.push(delivery);
                }
            }
            tx.commit()?;
            Ok(result)
        })
        .await
    }
    pub async fn get_report_delivery(&self, id: &str) -> Result<Option<ReportDelivery>> {
        let id = id.to_owned();
        self.with_conn(move |conn| delivery_at(conn, &id)).await
    }

    pub async fn report_has_usable_content(&self, start: i64, end: i64) -> Result<bool> {
        ensure!(start < end, "invalid report window");
        self.with_conn(move |conn| {
            Ok(conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM frames f JOIN frame_text t ON t.frame_id=f.id
                 WHERE f.captured_at>=?1 AND f.captured_at<?2
                   AND length(trim(t.content_text, char(9,10,11,12,13,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288)))>0)",
                params![start, end],
                |row| row.get(0),
            )?)
        })
        .await
    }

    pub async fn list_pending_report_runs(&self) -> Result<Vec<ReportRun>> {
        self.with_conn(move |conn| {
            let mut statement = conn.prepare(
                "SELECT payload FROM report_runs r WHERE phase IN ('queued','running','blocked')
                 OR EXISTS(SELECT 1 FROM report_deliveries d WHERE d.run_id=r.run_id
                    AND (d.state IN ('pending','sending','unknown_delivery','conflict')
                      OR (d.state='failed' AND d.next_attempt_at IS NOT NULL)))
                 OR (phase='finished' AND json_extract(payload,'$.result') IS NOT NULL
                     AND (NOT EXISTS(SELECT 1 FROM report_deliveries d WHERE d.run_id=r.run_id AND json_extract(d.payload,'$.destination')='vault') OR NOT EXISTS(SELECT 1 FROM report_deliveries d WHERE d.run_id=r.run_id AND json_extract(d.payload,'$.destination')='index') OR NOT EXISTS(SELECT 1 FROM report_deliveries d WHERE d.run_id=r.run_id AND json_extract(d.payload,'$.destination')='telegram')))
                 ORDER BY created_at,run_id",
            )?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            rows.map(|json| Ok(serde_json::from_str(&json?)?)).collect()
        })
        .await
    }

    pub async fn list_report_files(&self) -> Result<Vec<ReportFileOwnership>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare("SELECT payload FROM report_files ORDER BY path")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|s| {
                let mut ownership: ReportFileOwnership = serde_json::from_str(&s?)?;
                ownership.path = ownership.path.replace('\\', "/");
                Ok(ownership)
            })
            .collect()
        })
        .await
    }

    pub async fn ensure_report_source_id(&self, candidate: &str) -> Result<String> {
        let candidate = candidate.to_owned();
        ensure!(!candidate.is_empty(), "empty source identity");
        self.with_conn(move |conn| {
            conn.execute(
                "INSERT OR IGNORE INTO settings(key,value) VALUES('memory.source_id',?1)",
                [&candidate],
            )?;
            Ok(conn.query_row(
                "SELECT value FROM settings WHERE key='memory.source_id'",
                [],
                |r| r.get(0),
            )?)
        })
        .await
    }
    pub async fn reset_report_source_id(&self, new_id: &str) -> Result<()> {
        ensure!(!new_id.is_empty(), "empty source identity");
        self.set_setting("memory.source_id", new_id).await
    }
    pub async fn create_report_run(&self, run: ReportRun) -> Result<ReportRun> {
        validate_run(&run)?;
        self.with_conn(move |conn| {
            let tx=conn.unchecked_transaction()?;
            if let Some(existing)=run_at(&tx,&run.run_id)? { return Ok(existing); }
            if run.revision_of.is_none() {
                let existing: Option<String> = if let Some(schedule)=&run.schedule_id {
                    tx.query_row("SELECT payload FROM report_runs WHERE schedule_id=?1 AND window_start=?2 AND window_end=?3 AND revision_of IS NULL",params![schedule,run.window_start,run.window_end],|r|r.get(0)).optional()?
                } else if run.requested_range.is_none() {
                    tx.query_row("SELECT payload FROM report_runs WHERE schedule_id IS NULL AND revision_of IS NULL AND window_start=?1 AND window_end=?2 AND phase IN ('queued','running') AND json_extract(payload,'$.source_id')=?3 AND json_extract(payload,'$.requested_range') IS NULL ORDER BY created_at DESC",params![run.window_start,run.window_end,run.source_id],|r|r.get(0)).optional()?
                } else { None };
                if let Some(json)=existing {return Ok(serde_json::from_str(&json)?);}
            }
            tx.execute("INSERT INTO report_runs(run_id,schedule_id,revision_of,window_start,window_end,phase,created_at,updated_at,payload,kind) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![run.run_id,run.schedule_id,run.revision_of,run.window_start,run.window_end,phase(run.phase),run.created_at,run.updated_at,serde_json::to_string(&run)?,kind_db(run.kind)])?;
            tx.commit()?;
            Ok(run)
        }).await
    }
    pub async fn get_report_run(&self, id: &str) -> Result<Option<ReportRun>> {
        let id = id.to_owned();
        self.with_conn(move |conn| run_at(conn, &id)).await
    }
    pub async fn list_report_runs(
        &self,
        limit: u32,
        kind: Option<ReportRunKind>,
    ) -> Result<Vec<ReportRun>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT payload FROM report_runs WHERE (?2 IS NULL OR kind=?2) ORDER BY created_at DESC,run_id DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(
                params![limit.clamp(1, 1000), kind.map(kind_db)],
                |r| r.get::<_, String>(0),
            )?;
            rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
        })
        .await
    }
    /// Finished saved daily runs whose window lies inside `[start, end)`, oldest
    /// first by `window_start` (`03 §8e` weekly inputs). Only `kind='daily'` runs
    /// that are `finished` and actually carry a saved result qualify: a
    /// queued/running/blocked/failed run and a finished run without a stored
    /// result are not saved dailies. Containment (not overlap) is the predicate,
    /// so a range containing none is an empty vec, never an error. `run_id`
    /// breaks `window_start` ties deterministically (the spec fixes only the
    /// primary order); the `kind` column and its index keep the scan to daily
    /// rows, and no new index or migration is involved.
    pub async fn saved_daily_runs(&self, start: i64, end: i64) -> Result<Vec<ReportRun>> {
        self.with_conn(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT payload FROM report_runs
                 WHERE kind='daily' AND phase='finished'
                   AND window_start>=?1 AND window_end<=?2
                   AND json_extract(payload,'$.result') IS NOT NULL
                 ORDER BY window_start,run_id",
            )?;
            let rows = stmt.query_map(params![start, end], |r| r.get::<_, String>(0))?;
            rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
        })
        .await
    }
    pub async fn update_report_run(&self, run: ReportRun) -> Result<()> {
        self.with_conn(move |conn| {
            let old =
                run_at(conn, &run.run_id)?.ok_or_else(|| anyhow::anyhow!("report not found"))?;
            ensure!(
                old.source_id == run.source_id
                    && old.kind == run.kind
                    && old.schedule_id == run.schedule_id
                    && old.revision_of == run.revision_of
                    && old.requested_range == run.requested_range
                    && old.window_start == run.window_start
                    && old.window_end == run.window_end
                    && old.timezone == run.timezone
                    && old.local_date == run.local_date
                    && old.config_snapshot == run.config_snapshot
                    && old.created_at == run.created_at,
                "report identity and configuration are immutable"
            );
            if old.result.is_some() {
                ensure!(old.result == run.result, "saved report is immutable");
            }
            write_run(conn, &run)
        })
        .await
    }
    pub async fn claim_report_run(&self, id: &str, now: i64) -> Result<bool> {
        let id = id.to_owned();
        self.with_conn(move |conn| {
            let Some(mut run) = run_at(conn, &id)? else {
                return Ok(false);
            };
            if run.phase != ReportRunPhase::Queued {
                return Ok(false);
            };
            run.phase = ReportRunPhase::Running;
            run.updated_at = now;
            Ok(conn.execute(
                "UPDATE report_runs SET phase='running',updated_at=?2,payload=?3,kind=?4 WHERE run_id=?1 AND phase='queued'",
                params![
                    run.run_id,
                    run.updated_at,
                    serde_json::to_string(&run)?,
                    kind_db(run.kind)
                ],
            )? == 1)
        })
        .await
    }
    pub async fn recover_report_work(&self, now: i64) -> Result<()> {
        self.with_conn(move|conn| {
            let tx=conn.unchecked_transaction()?;
            let runs:Vec<String>=tx.prepare("SELECT run_id FROM report_runs WHERE phase='running'")?.query_map([],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for id in runs {let mut run=run_at(&tx,&id)?.ok_or_else(||anyhow::anyhow!("report disappeared"))?;run.phase=ReportRunPhase::Queued;run.updated_at=now;write_run(&tx,&run)?;}
            let ids:Vec<String>=tx.prepare("SELECT id FROM report_deliveries WHERE state='sending'")?.query_map([],|r|r.get(0))?.collect::<rusqlite::Result<_>>()?;
            for id in ids {
                let mut d=delivery_at(&tx,&id)?.ok_or_else(||anyhow::anyhow!("delivery disappeared"))?;
                if d.destination==ReportDestination::Telegram {d.state=ReportDeliveryState::UnknownDelivery;d.next_attempt_at=None;d.error=Some(ReportError{code:"DELIVERY_FAILED".into(),message:"Delivery was interrupted; service acceptance is unknown. Review before resending.".into()});}
                else {d.state=ReportDeliveryState::Pending;}
                d.updated_at=now;write_delivery(&tx,&d)?;
            }
            tx.commit()?;Ok(())
        }).await
    }
    pub async fn create_report_delivery(&self, delivery: ReportDelivery) -> Result<ReportDelivery> {
        self.with_conn(move |conn| {
            if let Some(old) = delivery_at(conn, &delivery.id)? {
                check_delivery_identity(&old, &delivery)?;
                return Ok(old);
            }
            insert_delivery(conn, &delivery)?;
            Ok(delivery)
        })
        .await
    }
    pub async fn update_report_delivery(
        &self,
        delivery: ReportDelivery,
        expected_state: ReportDeliveryState,
    ) -> Result<bool> {
        self.with_conn(move |conn| {
            let Some(old) = delivery_at(conn, &delivery.id)? else {
                return Ok(false);
            };
            check_delivery_identity(&old, &delivery)?;
            if old.state != expected_state {
                return Ok(false);
            }
            ensure!(
                !matches!(
                    old.state,
                    ReportDeliveryState::Succeeded
                        | ReportDeliveryState::Skipped
                        | ReportDeliveryState::Reviewed
                ),
                "terminal delivery is immutable"
            );
            ensure!(
                old.state != ReportDeliveryState::UnknownDelivery,
                "unknown delivery requires explicit resolution"
            );
            Ok(conn.execute(
                "UPDATE report_deliveries SET state=?2,next_attempt_at=?3,payload=?4 WHERE id=?1 AND state=?5",
                params![delivery.id, state(delivery.state), delivery.next_attempt_at, serde_json::to_string(&delivery)?, state(expected_state)],
            )? == 1)
        })
        .await
    }
    pub async fn resolve_report_delivery(
        &self,
        previous: ReportDelivery,
        replacement: Option<ReportDelivery>,
    ) -> Result<bool> {
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            let Some(old) = delivery_at(&tx, &previous.id)? else {
                return Ok(false);
            };
            if old.state != ReportDeliveryState::UnknownDelivery {
                return Ok(false);
            }
            check_delivery_identity(&old, &previous)?;
            ensure!(
                previous.state == ReportDeliveryState::Reviewed
                    && previous.message_id == old.message_id
                    && previous.error == old.error,
                "resolution must preserve uncertainty and receipts"
            );
            if let Some(new) = replacement {
                ensure!(
                    new.revision_of.as_deref() == Some(old.id.as_str())
                        && new.run_id == old.run_id
                        && new.state == ReportDeliveryState::Pending,
                    "invalid linked resend"
                );
                insert_delivery(&tx, &new)?;
            }
            write_delivery(&tx, &previous)?;
            tx.commit()?;
            Ok(true)
        })
        .await
    }
    pub async fn list_report_deliveries(&self, run_id: &str) -> Result<Vec<ReportDelivery>> {
        let id = run_id.to_owned();
        self.with_conn(move |conn| {
            let mut stmt =
                conn.prepare("SELECT payload FROM report_deliveries WHERE run_id=?1 ORDER BY id")?;
            let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
            rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
        })
        .await
    }
    pub async fn get_report_file(&self, path: &str) -> Result<Option<ReportFileOwnership>> {
        let path = path.replace('\\', "/");
        self.with_conn(move |conn| {
            let json: Option<String> = conn
                .query_row(
                    "SELECT payload FROM report_files WHERE path=?1 OR replace(path,char(92),'/')=?1 ORDER BY path=?1 DESC LIMIT 1",
                    [path],
                    |r| r.get(0),
                )
                .optional()?;
            json.map(|s| {
                let mut ownership: ReportFileOwnership = serde_json::from_str(&s)?;
                ownership.path = ownership.path.replace('\\', "/");
                Ok(ownership)
            }).transpose()
        })
        .await
    }
    pub async fn save_report_file(&self, mut ownership: ReportFileOwnership) -> Result<()> {
        ownership.path = ownership.path.replace('\\', "/");
        self.with_conn(move |conn| {
            let tx = conn.unchecked_transaction()?;
            // Preserve ownership across the separator change without duplicate index entries.
            tx.execute("DELETE FROM report_files WHERE path<>?1 AND replace(path,char(92),'/')=?1", [&ownership.path])?;
            tx.execute("INSERT INTO report_files(path,payload) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET payload=excluded.payload",params![ownership.path,serde_json::to_string(&ownership)?])?;
            tx.commit()?;
            Ok(())
        }).await
    }
    pub async fn report_evidence(&self, frame_ids: Vec<i64>) -> Result<ReportEvidence> {
        self.with_conn(move |conn| {
            let mut result = ReportEvidence::default();
            let mut sessions = std::collections::BTreeSet::new();
            let mut stmt = conn.prepare("SELECT session_id FROM frames WHERE id=?1")?;
            for id in frame_ids {
                match stmt
                    .query_row([id], |r| r.get::<_, Option<i64>>(0))
                    .optional()?
                {
                    None => result.missing_frame_ids.push(id),
                    Some(Some(session)) => {
                        sessions.insert(session);
                    }
                    Some(None) => {}
                }
            }
            result.session_ids = sessions.into_iter().collect();
            Ok(result)
        })
        .await
    }
    pub async fn prune_report_runs(&self, before: i64) -> Result<u64> {
        self.with_conn(move|conn|Ok(conn.execute("DELETE FROM report_runs WHERE phase='finished' AND updated_at < ?1 AND NOT EXISTS(SELECT 1 FROM report_runs child WHERE child.revision_of=report_runs.run_id) AND (json_extract(payload,'$.result') IS NULL OR (EXISTS(SELECT 1 FROM report_deliveries receipts WHERE receipts.run_id=report_runs.run_id AND json_extract(receipts.payload,'$.destination')='vault') AND EXISTS(SELECT 1 FROM report_deliveries receipts WHERE receipts.run_id=report_runs.run_id AND json_extract(receipts.payload,'$.destination')='index') AND EXISTS(SELECT 1 FROM report_deliveries receipts WHERE receipts.run_id=report_runs.run_id AND json_extract(receipts.payload,'$.destination')='telegram'))) AND NOT EXISTS (SELECT 1 FROM report_deliveries d WHERE d.run_id=report_runs.run_id AND (d.state IN ('pending','sending','unknown_delivery','conflict') OR (d.state='failed' AND d.next_attempt_at IS NOT NULL)))",[before])? as u64)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn normalized_paths_preserve_existing_file_ownership_without_duplicate_entries() {
        let store = SqliteStore::open_in_memory().unwrap();
        let legacy = ReportFileOwnership {
            path: "C:/Notes/ScreenSearch/Daily/2026-10-15.md".replace('/', "\\"),
            source_id: "profile".into(),
            run_id: "original".into(),
            hash: "original-hash".into(),
            updated_at: 1,
        };
        store
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO report_files(path,payload) VALUES(?1,?2)",
                    params![legacy.path, serde_json::to_string(&legacy)?],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let normalized = "C:/Notes/ScreenSearch/Daily/2026-10-15.md";
        let mut owner = store.get_report_file(normalized).await.unwrap().unwrap();
        assert_eq!(owner.path, normalized);
        assert_eq!(owner.run_id, "original");
        assert_eq!(owner.hash, "original-hash");
        assert_eq!(
            store.list_report_files().await.unwrap(),
            vec![owner.clone()]
        );
        owner.run_id = "replacement".into();
        owner.hash = "replacement-hash".into();
        store.save_report_file(owner.clone()).await.unwrap();
        assert_eq!(
            store.get_report_file(normalized).await.unwrap(),
            Some(owner)
        );
        store
            .with_conn(|conn| {
                let count: i64 =
                    conn.query_row("SELECT count(*) FROM report_files", [], |r| r.get(0))?;
                assert_eq!(count, 1);
                Ok(())
            })
            .await
            .unwrap();
    }

    fn run(id: &str) -> ReportRun {
        ReportRun {
            run_id: id.into(),
            source_id: "profile".into(),
            kind: ReportRunKind::Daily,
            schedule_id: Some("schedule".into()),
            revision_of: None,
            requested_range: None,
            window_start: 0,
            window_end: 86400000,
            timezone: "UTC".into(),
            local_date: "1970-01-01".into(),
            phase: ReportRunPhase::Queued,
            outcome: None,
            error: None,
            result: None,
            config_snapshot: "{}".into(),
            created_at: 1,
            updated_at: 1,
        }
    }
    fn delivery(id: &str) -> ReportDelivery {
        ReportDelivery {
            id: id.into(),
            run_id: "run".into(),
            destination: ReportDestination::Telegram,
            state: ReportDeliveryState::Pending,
            revision_of: None,
            destination_snapshot: "{\"chat_id\":\"fixture\",\"text\":\"fixture digest\"}".into(),
            path: None,
            hash: None,
            recovery_path: None,
            chunk_index: Some(0),
            chunk_count: Some(1),
            message_id: None,
            error: None,
            next_attempt_at: None,
            attempt: 0,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[tokio::test]
    async fn retention_preserves_ancestor_until_retained_children_are_pruned() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut root = run("run");
        root.phase = ReportRunPhase::Finished;
        root.outcome = Some(ReportOutcome::Failed);
        store.create_report_run(root).await.unwrap();
        let mut child = run("child");
        child.revision_of = Some("run".into());
        store.create_report_run(child.clone()).await.unwrap();
        assert_eq!(store.prune_report_runs(100).await.unwrap(), 0);
        assert!(store.get_report_run("run").await.unwrap().is_some());
        child.phase = ReportRunPhase::Finished;
        child.outcome = Some(ReportOutcome::Failed);
        store.update_report_run(child).await.unwrap();
        // SQLite may prune both in one statement if it visits the leaf first;
        // otherwise the next retention pass removes the now-childless root.
        let first = store.prune_report_runs(100).await.unwrap();
        let second = store.prune_report_runs(100).await.unwrap();
        assert_eq!(first + second, 2);
        assert!(store.list_report_runs(10, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn revision_family_includes_descendant_confirmation_and_missing_ancestor_fails_closed() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.create_report_run(run("run")).await.unwrap();
        let mut revision = run("revision");
        revision.revision_of = Some("run".into());
        store.create_report_run(revision).await.unwrap();
        let mut receipt = delivery("confirmed");
        receipt.run_id = "revision".into();
        receipt.state = ReportDeliveryState::Succeeded;
        receipt.message_id = Some(42);
        store.create_report_delivery(receipt).await.unwrap();
        let family = store.list_report_family_deliveries("run").await.unwrap();
        assert_eq!(family.len(), 1);
        assert_eq!(family[0].message_id, Some(42));
        store
            .with_conn(|conn| {
                conn.execute("DELETE FROM report_runs WHERE run_id='run'", [])?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store
            .list_report_family_deliveries("revision")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn chunk_intents_commit_atomically_and_lookup_ignores_history_limit() {
        let store = SqliteStore::open_in_memory().unwrap();
        store.create_report_run(run("run")).await.unwrap();
        let first = delivery("first");
        let mut invalid = delivery("second");
        invalid.run_id = "missing".into();
        assert!(store
            .create_report_deliveries(vec![first.clone(), invalid])
            .await
            .is_err());
        assert!(store.get_report_delivery("first").await.unwrap().is_none());
        let rows = store
            .create_report_deliveries(vec![first, delivery("second")])
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            store
                .get_report_delivery("first")
                .await
                .unwrap()
                .unwrap()
                .destination_snapshot,
            rows[0].destination_snapshot
        );
    }

    #[tokio::test]
    async fn evidence_probe_distinguishes_empty_content_and_database_error() {
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(!store.report_has_usable_content(0, 20).await.unwrap());
        store.with_conn(|conn| {
            conn.execute("INSERT INTO frames(id,captured_at,monitor_index,width,height,image_path,content_hash) VALUES(1,10,0,1,1,'fixture.webp','hash')",[])?;
            conn.execute("INSERT INTO frame_text(frame_id,raw_text,content_text,primary_source,filter_version,suppressed_count) VALUES(1,'raw','   ','ocr',2,0)",[])?;
            Ok(())
        }).await.unwrap();
        assert!(!store.report_has_usable_content(0, 20).await.unwrap());
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE frame_text SET content_text='Evidence' WHERE frame_id=1",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store.report_has_usable_content(0, 20).await.unwrap());
        assert!(!store.report_has_usable_content(0, 10).await.unwrap());
        store
            .with_conn(|conn| {
                conn.execute_batch("DROP TABLE frame_text;")?;
                Ok(())
            })
            .await
            .unwrap();
        assert!(store.report_has_usable_content(0, 20).await.is_err());
    }

    #[tokio::test]
    async fn pending_work_is_not_limited_by_history_page_and_recovers_saved_output() {
        let store = SqliteStore::open_in_memory().unwrap();
        for index in 0..25 {
            let mut item = run(&format!("run-{index}"));
            item.window_start += index * 86400000;
            item.window_end += index * 86400000;
            item.created_at += index;
            store.create_report_run(item).await.unwrap();
        }
        assert_eq!(store.list_report_runs(20, None).await.unwrap().len(), 20);
        assert_eq!(store.list_pending_report_runs().await.unwrap().len(), 25);
        let mut item = store.get_report_run("run-0").await.unwrap().unwrap();
        item.phase = ReportRunPhase::Finished;
        item.outcome = Some(ReportOutcome::NoData);
        item.result = Some(SavedReport {
            markdown: "No evidence in the inspected window.".into(),
            model: None,
            frame_ids: vec![],
            session_ids: vec![],
            cited_frame_ids: vec![],
            frames_sampled: 0,
            frames_summarized: 0,
            periods_total: 1,
            periods_covered: 0,
            passes: 0,
            truncated: false,
            daily_run_ids: Vec::new(),
        });
        store.update_report_run(item).await.unwrap();
        assert!(store
            .list_pending_report_runs()
            .await
            .unwrap()
            .iter()
            .any(|item| item.run_id == "run-0"));
        assert_eq!(store.prune_report_runs(100).await.unwrap(), 0);
        let mut vault = delivery("vault-success");
        vault.run_id = "run-0".into();
        vault.destination = ReportDestination::Vault;
        vault.state = ReportDeliveryState::Succeeded;
        store.create_report_delivery(vault).await.unwrap();
        assert!(store
            .list_pending_report_runs()
            .await
            .unwrap()
            .iter()
            .any(|item| item.run_id == "run-0"));
        assert_eq!(store.prune_report_runs(100).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn explicit_ranges_are_independent_and_immutable_with_legacy_compatibility() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut explicit = run("explicit-first");
        explicit.schedule_id = None;
        explicit.requested_range = Some(traits::MemoryReportRange {
            start_date: "1970-01-01".into(),
            end_date: "1970-01-01".into(),
        });
        let mut invalid_scheduled = explicit.clone();
        invalid_scheduled.schedule_id = Some("schedule".into());
        assert!(store.create_report_run(invalid_scheduled).await.is_err());
        store.create_report_run(explicit.clone()).await.unwrap();
        let mut second = explicit.clone();
        second.run_id = "explicit-second".into();
        assert_eq!(
            store.create_report_run(second).await.unwrap().run_id,
            "explicit-second"
        );

        let mut legacy = run("legacy");
        legacy.schedule_id = None;
        assert_eq!(
            store
                .create_report_run(legacy.clone())
                .await
                .unwrap()
                .run_id,
            "legacy"
        );
        // Exercise genuine old persisted JSON, rather than only serializing the new None field.
        store.with_conn(|conn| {
            conn.execute("UPDATE report_runs SET payload=json_remove(payload,'$.requested_range') WHERE run_id='legacy'", [])?;
            Ok(())
        }).await.unwrap();
        assert!(store
            .get_report_run("legacy")
            .await
            .unwrap()
            .unwrap()
            .requested_range
            .is_none());
        legacy.phase = ReportRunPhase::Blocked;
        store.update_report_run(legacy.clone()).await.unwrap();
        legacy.run_id = "legacy-duplicate".into();
        // Blocked is not pending: a new request in the same window becomes its own run.
        assert_eq!(
            store.create_report_run(legacy).await.unwrap().run_id,
            "legacy-duplicate"
        );
        let mut third = explicit.clone();
        third.run_id = "explicit-after-legacy".into();
        assert_eq!(
            store.create_report_run(third).await.unwrap().run_id,
            "explicit-after-legacy"
        );
        assert_eq!(store.list_report_runs(10, None).await.unwrap().len(), 5);

        let mut changed = explicit.clone();
        changed.requested_range.as_mut().unwrap().end_date = "1970-01-02".into();
        assert!(store.update_report_run(changed).await.is_err());
        explicit.requested_range = None;
        assert!(store.update_report_run(explicit).await.is_err());
        assert_eq!(
            store
                .get_report_run("explicit-first")
                .await
                .unwrap()
                .unwrap()
                .requested_range
                .unwrap()
                .end_date,
            "1970-01-01"
        );
    }

    #[tokio::test]
    async fn manual_duplicate_does_not_cross_source_identity() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut first = run("first");
        first.schedule_id = None;
        store.create_report_run(first.clone()).await.unwrap();
        let mut duplicate = first.clone();
        duplicate.run_id = "duplicate".into();
        assert_eq!(
            store.create_report_run(duplicate).await.unwrap().run_id,
            "first"
        );
        first.run_id = "new-source".into();
        first.source_id = "other-profile".into();
        assert_eq!(
            store.create_report_run(first).await.unwrap().run_id,
            "new-source"
        );
    }

    /// A blocked previous-day run is not pending work: a new manual request in the same
    /// window must enqueue as its own run instead of silently resolving back to the
    /// blocked one (which would suppress the request). While the matching run IS pending
    /// (queued/running), identical requests still collapse onto it.
    #[tokio::test]
    async fn blocked_previous_day_run_does_not_suppress_new_queued_request() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut previous_day = run("previous-day");
        previous_day.schedule_id = None;
        store.create_report_run(previous_day.clone()).await.unwrap();
        previous_day.phase = ReportRunPhase::Blocked;
        previous_day.outcome = Some(ReportOutcome::Blocked);
        store.update_report_run(previous_day.clone()).await.unwrap();

        let mut retried = previous_day.clone();
        retried.run_id = "previous-day-requeued".into();
        retried.phase = ReportRunPhase::Queued;
        retried.outcome = None;
        let created = store.create_report_run(retried).await.unwrap();
        assert_eq!(created.run_id, "previous-day-requeued");
        assert_eq!(created.phase, ReportRunPhase::Queued);
        assert_eq!(store.list_report_runs(10, None).await.unwrap().len(), 2);

        let mut concurrent = created.clone();
        concurrent.run_id = "previous-day-concurrent".into();
        assert_eq!(
            store.create_report_run(concurrent).await.unwrap().run_id,
            "previous-day-requeued"
        );

        let mut during_running = created.clone();
        during_running.run_id = "previous-day-running".into();
        assert!(store
            .claim_report_run("previous-day-requeued", 2)
            .await
            .unwrap());
        assert_eq!(
            store
                .create_report_run(during_running)
                .await
                .unwrap()
                .run_id,
            "previous-day-requeued"
        );
    }

    #[tokio::test]
    async fn duplicate_report_request_reuses_run_and_claim_is_exclusive() {
        let store = SqliteStore::open_in_memory().unwrap();
        let first = store.create_report_run(run("run")).await.unwrap();
        let duplicate = store.create_report_run(run("other")).await.unwrap();
        assert_eq!(duplicate.run_id, first.run_id);
        let (a, b) = tokio::join!(
            store.claim_report_run("run", 2),
            store.claim_report_run("run", 2)
        );
        assert_ne!(a.unwrap(), b.unwrap());
        let mut changed = first;
        changed.window_end += 1;
        assert!(store.update_report_run(changed).await.is_err());
    }
    /// A weekly synthesis run is identified by `(schedule_id, window)` exactly like a daily
    /// run (`03 §8e`): the same schedule + window reuses the stored run, `kind` is not a
    /// deduplication dimension, and runs of either kind with different windows coexist and
    /// stay separable by the listing filter.
    #[tokio::test]
    async fn weekly_run_identity_reuses_schedule_window() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut weekly = run("weekly-run");
        weekly.kind = ReportRunKind::WeeklySynthesis;
        let first = store.create_report_run(weekly).await.unwrap();
        assert_eq!(first.run_id, "weekly-run");
        assert_eq!(first.kind, ReportRunKind::WeeklySynthesis);

        // Same (schedule_id, window): the existing unique key reuses the stored run.
        let mut duplicate = run("weekly-duplicate");
        duplicate.kind = ReportRunKind::WeeklySynthesis;
        let duplicate = store.create_report_run(duplicate).await.unwrap();
        assert_eq!(duplicate.run_id, first.run_id);
        assert_eq!(duplicate.kind, ReportRunKind::WeeklySynthesis);

        // `kind` is not a deduplication dimension: the unchanged partial unique key still
        // rejects a raw insert for the same schedule + window, whatever kind it carries.
        store
            .with_conn(|conn| {
                assert!(
                    conn.execute(
                        "INSERT INTO report_runs(run_id,schedule_id,revision_of,window_start,window_end,phase,created_at,updated_at,payload,kind) \
                         VALUES('raw-daily','schedule',NULL,0,86400000,'queued',1,1,'{}','daily')",
                        [],
                    )
                    .is_err(),
                    "the unchanged (schedule_id, window) unique key must still conflict"
                );
                Ok(())
            })
            .await
            .unwrap();

        // A daily run and a weekly run for different windows coexist, each persisting its
        // own kind through the real writer.
        let mut daily = run("daily-run");
        daily.window_start = 86400000;
        daily.window_end = 2 * 86400000;
        assert_eq!(
            store.create_report_run(daily).await.unwrap().kind,
            ReportRunKind::Daily
        );
        let mut next_week = run("weekly-next");
        next_week.kind = ReportRunKind::WeeklySynthesis;
        next_week.window_start = 7 * 86400000;
        next_week.window_end = 8 * 86400000;
        assert_eq!(
            store.create_report_run(next_week).await.unwrap().kind,
            ReportRunKind::WeeklySynthesis
        );

        // Read back through the store: kinds survive the payload round-trip...
        assert_eq!(
            store
                .get_report_run("weekly-run")
                .await
                .unwrap()
                .unwrap()
                .kind,
            ReportRunKind::WeeklySynthesis
        );
        assert_eq!(
            store
                .get_report_run("daily-run")
                .await
                .unwrap()
                .unwrap()
                .kind,
            ReportRunKind::Daily
        );
        // ...and the listing filter separates them.
        let mut weekly_ids: Vec<String> = store
            .list_report_runs(10, Some(ReportRunKind::WeeklySynthesis))
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.run_id)
            .collect();
        weekly_ids.sort();
        assert_eq!(weekly_ids, vec!["weekly-next", "weekly-run"]);
        let daily_ids: Vec<String> = store
            .list_report_runs(10, Some(ReportRunKind::Daily))
            .await
            .unwrap()
            .into_iter()
            .map(|r| r.run_id)
            .collect();
        assert_eq!(daily_ids, vec!["daily-run"]);
        assert_eq!(store.list_report_runs(10, None).await.unwrap().len(), 3);
    }

    /// `saved_daily_runs` (`03 §8e` weekly inputs) feeds the weekly synthesis from
    /// finished, saved daily runs whose `[window_start, window_end)` lies inside the
    /// requested range, oldest first. A saved weekly synthesis, a non-finished run
    /// (even one already carrying a saved result), a finished run with no stored
    /// result and runs whose windows only overlap or lie outside the range are not
    /// saved dailies.
    #[tokio::test]
    async fn saved_daily_runs_returns_only_finished_saved_dailies_inside_the_window_oldest_first() {
        const DAY: i64 = 86_400_000;
        let store = SqliteStore::open_in_memory().unwrap();
        let saved_result = || SavedReport {
            markdown: "evidenced daily output".into(),
            model: Some("local".into()),
            frame_ids: vec![1],
            session_ids: vec![7],
            cited_frame_ids: vec![1],
            frames_sampled: 1,
            frames_summarized: 1,
            periods_total: 1,
            periods_covered: 1,
            passes: 1,
            truncated: false,
            daily_run_ids: Vec::new(),
        };
        let saved_daily = |id: &str, start: i64, end: i64| {
            let mut item = run(id);
            item.window_start = start;
            item.window_end = end;
            item.phase = ReportRunPhase::Finished;
            item.outcome = Some(ReportOutcome::Ok);
            item.result = Some(saved_result());
            item
        };

        // Created newest-window-first: ordering must come from `window_start`, not
        // from insertion order.
        store
            .create_report_run(saved_daily("day-newest", 2 * DAY, 3 * DAY))
            .await
            .unwrap();
        store
            .create_report_run(saved_daily("day-oldest", DAY, 2 * DAY))
            .await
            .unwrap();
        // Inside the requested window but not saved dailies:
        let mut running = run("day-running");
        running.window_start = 3 * DAY;
        running.window_end = 4 * DAY;
        running.phase = ReportRunPhase::Running;
        running.result = Some(saved_result());
        store.create_report_run(running).await.unwrap();
        let mut no_result = run("day-finished-no-result");
        no_result.window_start = 4 * DAY;
        no_result.window_end = 5 * DAY;
        no_result.phase = ReportRunPhase::Finished;
        no_result.outcome = Some(ReportOutcome::Failed);
        store.create_report_run(no_result).await.unwrap();
        let mut weekly = saved_daily("weekly-saved", 5 * DAY, 6 * DAY);
        weekly.kind = ReportRunKind::WeeklySynthesis;
        store.create_report_run(weekly).await.unwrap();
        // Windows touching the requested [DAY, 6*DAY) without lying inside it:
        store
            .create_report_run(saved_daily("before", 0, DAY))
            .await
            .unwrap();
        store
            .create_report_run(saved_daily("crosses-start", 0, 2 * DAY))
            .await
            .unwrap();
        store
            .create_report_run(saved_daily("crosses-end", 2 * DAY, 7 * DAY))
            .await
            .unwrap();

        let runs = store.saved_daily_runs(DAY, 6 * DAY).await.unwrap();
        let ids: Vec<&str> = runs.iter().map(|item| item.run_id.as_str()).collect();
        assert_eq!(ids, vec!["day-oldest", "day-newest"]);
        assert_eq!(runs[0].window_start, DAY);
        assert_eq!(runs[1].window_start, 2 * DAY);
        assert!(runs.iter().all(|item| item.kind == ReportRunKind::Daily));
        assert_eq!(
            runs[0].result.as_ref().unwrap().markdown,
            "evidenced daily output"
        );
        // The composition root consumes the store as `dyn Store`; prove the
        // trait forwarding reaches the same implementation.
        let dynamic: &dyn traits::Store = &store;
        assert_eq!(
            dynamic.saved_daily_runs(DAY, 6 * DAY).await.unwrap().len(),
            2
        );
    }

    /// An empty result is honest data, never an error: a range matching no saved
    /// daily returns an empty vec — on an empty store and when runs exist but lie
    /// outside the requested window.
    #[tokio::test]
    async fn saved_daily_runs_empty_window_is_an_empty_vec_not_an_error() {
        const DAY: i64 = 86_400_000;
        let store = SqliteStore::open_in_memory().unwrap();
        assert!(store.saved_daily_runs(0, DAY).await.unwrap().is_empty());

        let mut item = run("outside");
        item.window_start = 2 * DAY;
        item.window_end = 3 * DAY;
        item.phase = ReportRunPhase::Finished;
        item.outcome = Some(ReportOutcome::NoData);
        item.result = Some(SavedReport {
            markdown: "No evidence in the inspected window.".into(),
            model: None,
            frame_ids: vec![],
            session_ids: vec![],
            cited_frame_ids: vec![],
            frames_sampled: 0,
            frames_summarized: 0,
            periods_total: 1,
            periods_covered: 0,
            passes: 0,
            truncated: false,
            daily_run_ids: Vec::new(),
        });
        store.create_report_run(item).await.unwrap();

        assert!(store.saved_daily_runs(0, DAY).await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn interrupted_run_is_recoverable_and_unknown_send_is_not_retried() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.sqlite");
        {
            let store = SqliteStore::open_path(&path).unwrap();
            store.create_report_run(run("run")).await.unwrap();
            assert!(store.claim_report_run("run", 2).await.unwrap());
            let mut d = store
                .create_report_delivery(delivery("send"))
                .await
                .unwrap();
            d.state = ReportDeliveryState::Sending;
            assert!(store
                .update_report_delivery(d, ReportDeliveryState::Pending)
                .await
                .unwrap());
        }
        let store = SqliteStore::open_path(&path).unwrap();
        store.recover_report_work(3).await.unwrap();
        assert_eq!(
            store.get_report_run("run").await.unwrap().unwrap().phase,
            ReportRunPhase::Queued
        );
        let mut old = store.list_report_deliveries("run").await.unwrap().remove(0);
        assert_eq!(old.state, ReportDeliveryState::UnknownDelivery);
        assert!(old.error.is_some());
        let mut invalid = old.clone();
        invalid.state = ReportDeliveryState::Pending;
        assert!(store
            .update_report_delivery(invalid, ReportDeliveryState::UnknownDelivery)
            .await
            .is_err());
        let mut resend = delivery("resend");
        resend.revision_of = Some(old.id.clone());
        old.state = ReportDeliveryState::Reviewed;
        let (a, b) = tokio::join!(
            store.resolve_report_delivery(old.clone(), Some(resend.clone())),
            store.resolve_report_delivery(old, Some(resend))
        );
        assert_ne!(a.unwrap(), b.unwrap());
        assert_eq!(store.list_report_deliveries("run").await.unwrap().len(), 2);
    }
    #[tokio::test]
    async fn run_retention_preserves_pending_delivery_and_file_ownership() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut r = run("run");
        r.phase = ReportRunPhase::Finished;
        r.outcome = Some(ReportOutcome::Failed);
        store.create_report_run(r).await.unwrap();
        let mut d = store
            .create_report_delivery(delivery("send"))
            .await
            .unwrap();
        store
            .save_report_file(ReportFileOwnership {
                path: "fixture.md".into(),
                source_id: "profile".into(),
                run_id: "run".into(),
                hash: "digest".into(),
                updated_at: 1,
            })
            .await
            .unwrap();
        assert_eq!(store.prune_report_runs(100).await.unwrap(), 0);
        d.state = ReportDeliveryState::Failed;
        d.next_attempt_at = Some(200);
        store
            .update_report_delivery(d.clone(), ReportDeliveryState::Pending)
            .await
            .unwrap();
        assert_eq!(store.prune_report_runs(100).await.unwrap(), 0);
        d.next_attempt_at = None;
        store
            .update_report_delivery(d, ReportDeliveryState::Failed)
            .await
            .unwrap();
        assert_eq!(store.prune_report_runs(100).await.unwrap(), 1);
        assert!(store.get_report_file("fixture.md").await.unwrap().is_some());
    }
    #[tokio::test]
    async fn source_identity_is_atomic_and_saved_provenance_is_immutable() {
        let store = SqliteStore::open_in_memory().unwrap();
        let (a, b) = tokio::join!(
            store.ensure_report_source_id("a"),
            store.ensure_report_source_id("b")
        );
        assert_eq!(a.unwrap(), b.unwrap());
        let mut r = run("run");
        r.phase = ReportRunPhase::Finished;
        r.outcome = Some(ReportOutcome::Ok);
        r.result = Some(SavedReport {
            markdown: "evidenced output".into(),
            model: Some("local".into()),
            frame_ids: vec![999],
            session_ids: vec![888],
            cited_frame_ids: vec![999],
            frames_sampled: 1,
            frames_summarized: 1,
            periods_total: 1,
            periods_covered: 1,
            passes: 1,
            truncated: false,
            daily_run_ids: Vec::new(),
        });
        store.create_report_run(r.clone()).await.unwrap();
        r.result.as_mut().unwrap().markdown = "changed".into();
        assert!(store.update_report_run(r).await.is_err());
        assert_eq!(
            store
                .report_evidence(vec![999])
                .await
                .unwrap()
                .missing_frame_ids,
            vec![999]
        );
    }
}
