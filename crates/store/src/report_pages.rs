//! Bounded saved-report reads (`03 §7f.5/.6`, `03 §8c`).
//!
//! The caller holds the Store connection lock. Each call owns one DEFERRED read
//! transaction: the frozen maximum ordering tuple and the candidate summaries
//! share a snapshot. Reads never generate, retry, deliver, repair, write, or
//! overlay current sources; a malformed stored payload is a storage error, never
//! empty success.
//!
//! Ordering is descending `(created_at, run_id)`. The existing
//! `idx_report_runs_created(created_at DESC)` provides the `created_at` prefix
//! but **not** a `run_id` tiebreak, so a continuation is a true two-branch keyset
//! seek: the equal-`created_at` branch probes one bounded tie group and the
//! strictly-lower branch is a single descending index seek to `created_at < ?`.
//! This avoids an optional-guard prefix rescan (which would defeat the index and
//! restart at the top on every page). The honest costs this design does *not*
//! hide: (a) a large tie group is scanned in full for the equal branch, and (b) a
//! selective `phase`/`outcome` filter has no index, so a sparse match scans every
//! candidate row in the window before the limit. Both are measured in the tests.
//!
//! `outcome` has no scalar column, so the mandated outcome filter uses the
//! existing `json_extract(payload, '$.outcome')` in the `WHERE` clause *before*
//! `LIMIT` — never dropped, never post-filtered. The list projection hydrates
//! summary fields only; the full saved Markdown/evidence/private configuration is
//! never deserialized just to discard it. Detail hydrates exactly one stored run.

use rusqlite::{named_params, Connection, OptionalExtension, Row, TransactionBehavior};
use traits::{
    HistoryReadError, MemoryReportRange, ReportDetail, ReportOutcome, ReportPageRequest,
    ReportPosition, ReportRun, ReportRunKind, ReportRunPhase, ReportSnapshot, ReportSummary,
    StoredReportPage, REPORT_ERROR_CODE_ALLOWLIST,
};

use crate::SqliteStore;

// Summary projection shared by both page statements. Columns 0..=5 are real
// columns, 13 is the real `kind` column (`03 §8e`); 6..=12 are `json_extract`
// scalars from `payload` (an object/array extracts as its JSON text; a JSON null
// extracts as SQL NULL). The payload's Markdown, evidence arrays and
// `config_snapshot` are never read here.
//
// The overlap predicate is `window_start < :to AND window_end > :from` — a run's
// `created_at` is deliberately independent of the window, so no created_at bound
// is derived from `from`/`to`.
//
// First page: `continuation = None`. The global maximum tuple is read in this
// same transaction, so every stored row is at or below it and no cap clause is
// needed. `ORDER BY created_at DESC` uses `idx_report_runs_created`; the `run_id`
// term is a bounded per-tie-group sort, so a small LIMIT stops early.
const REPORT_PAGE_FIRST_SQL: &str = r#"
    SELECT run_id, window_start, window_end, phase, created_at, updated_at,
           json_extract(payload,'$.source_id'),
           json_extract(payload,'$.revision_of'),
           json_extract(payload,'$.requested_range'),
           json_extract(payload,'$.timezone'),
           json_extract(payload,'$.local_date'),
           json_extract(payload,'$.outcome'),
           json_extract(payload,'$.error.code'),
           kind
    FROM report_runs
    WHERE window_start < :to AND window_end > :from
      AND (:phase IS NULL OR phase = :phase)
      AND (:outcome IS NULL OR json_extract(payload,'$.outcome') = :outcome)
    ORDER BY created_at DESC, run_id DESC
    LIMIT :row_limit
"#;

// Continuation: a true keyset seek strictly below the last ordering tuple. The
// two branches are disjoint (equal vs strictly lower `created_at`), so UNION ALL
// needs no deduplication. Because `after <= frozen maximum` is validated and the
// frozen maximum was read in the first page's transaction, every row below
// `after` is also below the frozen maximum — the seek alone enforces "never above
// the frozen high tuple" (a row inserted above it after page 1 sorts above
// `after` and is excluded by the seek).
const REPORT_PAGE_AFTER_SQL: &str = r#"
    SELECT run_id, window_start, window_end, phase, created_at, updated_at,
           json_extract(payload,'$.source_id'),
           json_extract(payload,'$.revision_of'),
           json_extract(payload,'$.requested_range'),
           json_extract(payload,'$.timezone'),
           json_extract(payload,'$.local_date'),
           json_extract(payload,'$.outcome'),
           json_extract(payload,'$.error.code'),
           kind
    FROM report_runs
    WHERE created_at = :after_created AND run_id < :after_run
      AND window_start < :to AND window_end > :from
      AND (:phase IS NULL OR phase = :phase)
      AND (:outcome IS NULL OR json_extract(payload,'$.outcome') = :outcome)
    UNION ALL
    SELECT run_id, window_start, window_end, phase, created_at, updated_at,
           json_extract(payload,'$.source_id'),
           json_extract(payload,'$.revision_of'),
           json_extract(payload,'$.requested_range'),
           json_extract(payload,'$.timezone'),
           json_extract(payload,'$.local_date'),
           json_extract(payload,'$.outcome'),
           json_extract(payload,'$.error.code'),
           kind
    FROM report_runs
    WHERE created_at < :after_created
      AND window_start < :to AND window_end > :from
      AND (:phase IS NULL OR phase = :phase)
      AND (:outcome IS NULL OR json_extract(payload,'$.outcome') = :outcome)
    ORDER BY created_at DESC, run_id DESC
    LIMIT :row_limit
"#;

const REPORT_MAX_TUPLE_SQL: &str =
    "SELECT created_at, run_id FROM report_runs ORDER BY created_at DESC, run_id DESC LIMIT 1";

const REPORT_DETAIL_PAYLOAD_SQL: &str = "SELECT payload FROM report_runs WHERE run_id = ?1";

fn storage(error: rusqlite::Error) -> HistoryReadError {
    HistoryReadError::Storage(error.into())
}

/// A stored value that cannot be decoded is corruption, not permission to
/// fabricate a value or drop the row.
fn conversion(column: usize, detail: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            detail.to_owned(),
        )),
    )
}

fn phase_db(value: ReportRunPhase) -> &'static str {
    match value {
        ReportRunPhase::Queued => "queued",
        ReportRunPhase::Running => "running",
        ReportRunPhase::Blocked => "blocked",
        ReportRunPhase::Finished => "finished",
    }
}

fn parse_phase(value: &str) -> Option<ReportRunPhase> {
    match value {
        "queued" => Some(ReportRunPhase::Queued),
        "running" => Some(ReportRunPhase::Running),
        "blocked" => Some(ReportRunPhase::Blocked),
        "finished" => Some(ReportRunPhase::Finished),
        _ => None,
    }
}

fn parse_kind(value: &str) -> Option<ReportRunKind> {
    match value {
        "daily" => Some(ReportRunKind::Daily),
        "weekly_synthesis" => Some(ReportRunKind::WeeklySynthesis),
        _ => None,
    }
}

fn outcome_db(value: ReportOutcome) -> &'static str {
    match value {
        ReportOutcome::Ok => "OK",
        ReportOutcome::Partial => "PARTIAL",
        ReportOutcome::NoData => "NO_DATA",
        ReportOutcome::Blocked => "BLOCKED",
        ReportOutcome::Failed => "FAILED",
    }
}

fn parse_outcome(value: &str) -> Option<ReportOutcome> {
    match value {
        "OK" => Some(ReportOutcome::Ok),
        "PARTIAL" => Some(ReportOutcome::Partial),
        "NO_DATA" => Some(ReportOutcome::NoData),
        "BLOCKED" => Some(ReportOutcome::Blocked),
        "FAILED" => Some(ReportOutcome::Failed),
        _ => None,
    }
}

fn row_to_summary(row: &Row<'_>) -> rusqlite::Result<ReportSummary> {
    let phase_raw: String = row.get(3)?;
    let phase =
        parse_phase(&phase_raw).ok_or_else(|| conversion(3, "invalid stored report phase"))?;
    let outcome_raw: Option<String> = row.get(11)?;
    let outcome = outcome_raw
        .as_deref()
        .map(|value| {
            parse_outcome(value).ok_or_else(|| conversion(11, "invalid stored report outcome"))
        })
        .transpose()?;
    let requested_raw: Option<String> = row.get(8)?;
    let requested_range = requested_raw
        .as_deref()
        .map(|value| {
            serde_json::from_str::<MemoryReportRange>(value)
                .map_err(|_| conversion(8, "invalid stored requested range"))
        })
        .transpose()?;
    let source_id: String = row.get(6)?;
    let timezone: String = row.get(9)?;
    let local_date: String = row.get(10)?;
    let error_code: Option<String> = row.get(12)?;
    let kind_raw: String = row.get(13)?;
    let kind = parse_kind(&kind_raw).ok_or_else(|| conversion(13, "invalid stored report kind"))?;
    Ok(ReportSummary {
        run_id: row.get(0)?,
        source_id,
        kind,
        revision_of: row.get(7)?,
        requested_range,
        window_start: row.get(1)?,
        window_end: row.get(2)?,
        timezone,
        local_date,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        phase,
        outcome,
        error_code: error_code.filter(|code| REPORT_ERROR_CODE_ALLOWLIST.contains(&code.as_str())),
    })
}

fn max_tuple(tx: &rusqlite::Transaction<'_>) -> Result<Option<ReportSnapshot>, HistoryReadError> {
    tx.query_row(REPORT_MAX_TUPLE_SQL, [], |row| {
        Ok(ReportSnapshot {
            max_created_at: row.get(0)?,
            max_run_id: row.get(1)?,
        })
    })
    .optional()
    .map_err(storage)
}

fn query_first(
    tx: &rusqlite::Transaction<'_>,
    request: &ReportPageRequest,
) -> Result<Vec<ReportSummary>, HistoryReadError> {
    let mut statement = tx.prepare(REPORT_PAGE_FIRST_SQL).map_err(storage)?;
    let rows = statement
        .query_map(
            named_params! {
                ":from": request.window.from,
                ":to": request.window.to,
                ":phase": request.filters.phase.map(phase_db),
                ":outcome": request.filters.outcome.map(outcome_db),
                ":row_limit": i64::from(request.limit) + 1,
            },
            row_to_summary,
        )
        .map_err(storage)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(storage)
}

fn query_after(
    tx: &rusqlite::Transaction<'_>,
    request: &ReportPageRequest,
    after: &ReportPosition,
) -> Result<Vec<ReportSummary>, HistoryReadError> {
    let mut statement = tx.prepare(REPORT_PAGE_AFTER_SQL).map_err(storage)?;
    let rows = statement
        .query_map(
            named_params! {
                ":from": request.window.from,
                ":to": request.window.to,
                ":phase": request.filters.phase.map(phase_db),
                ":outcome": request.filters.outcome.map(outcome_db),
                ":after_created": after.created_at,
                ":after_run": after.run_id.as_str(),
                ":row_limit": i64::from(request.limit) + 1,
            },
            row_to_summary,
        )
        .map_err(storage)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(storage)
}

/// One bounded overlap/status page. `snapshot = None` is the honest empty-table
/// case; a `next_position` always implies `Some(snapshot)`.
pub(crate) fn report_page(
    conn: &Connection,
    request: ReportPageRequest,
) -> Result<StoredReportPage, HistoryReadError> {
    request.validate()?;
    // Explicit DEFERRED regardless of the Connection's configured default.
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Deferred)
        .map_err(storage)?;
    // The frozen maximum tuple and the candidate rows share this snapshot. On a
    // continuation the frozen tuple comes from the cursor; a genuinely empty
    // first page yields `None`.
    let snapshot = match &request.continuation {
        Some(continuation) => Some(continuation.snapshot.clone()),
        None => max_tuple(&tx)?,
    };
    let after = request.continuation.as_ref().map(|c| c.after.clone());
    let mut items = match &after {
        Some(position) => query_after(&tx, &request, position)?,
        None => query_first(&tx, &request)?,
    };
    let has_more = items.len() > request.limit as usize;
    items.truncate(request.limit as usize);
    let next_position = if has_more {
        items.last().map(|item| ReportPosition {
            created_at: item.created_at,
            run_id: item.run_id.clone(),
        })
    } else {
        None
    };
    tx.commit().map_err(storage)?;
    debug_assert!(
        next_position.is_none() || snapshot.is_some(),
        "a next_position requires a frozen snapshot"
    );
    Ok(StoredReportPage {
        items,
        snapshot,
        window: request.window,
        filters: request.filters,
        next_position,
    })
}

/// One complete stored run, or `None` (→404). The full saved result is
/// deserialized here only — never for a list page. A malformed payload is a
/// storage error; a valid saved result is exposed regardless of phase.
pub(crate) fn report_detail(
    conn: &Connection,
    run_id: &str,
) -> Result<Option<ReportDetail>, HistoryReadError> {
    let payload: Option<String> = conn
        .query_row(REPORT_DETAIL_PAYLOAD_SQL, [run_id], |row| row.get(0))
        .optional()
        .map_err(storage)?;
    match payload {
        None => Ok(None),
        Some(json) => {
            let run: ReportRun = serde_json::from_str(&json)
                .map_err(|error| HistoryReadError::Storage(error.into()))?;
            Ok(Some(ReportDetail::from_run(run)))
        }
    }
}

impl SqliteStore {
    /// Read one bounded saved-report page without blocking the async runtime.
    pub async fn report_page(
        &self,
        request: ReportPageRequest,
    ) -> Result<StoredReportPage, HistoryReadError> {
        self.with_conn(move |conn| Ok(report_page(conn, request)))
            .await
            .map_err(HistoryReadError::Storage)?
    }

    /// Read one public saved-report detail without blocking the async runtime.
    pub async fn report_detail(
        &self,
        run_id: &str,
    ) -> Result<Option<ReportDetail>, HistoryReadError> {
        let run_id = run_id.to_owned();
        self.with_conn(move |conn| Ok(report_detail(conn, &run_id)))
            .await
            .map_err(HistoryReadError::Storage)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::StatementStatus;
    use traits::{EvidenceWindow, ReportFilters};

    fn base_run(run_id: &str, created_at: i64, window: (i64, i64)) -> ReportRun {
        ReportRun {
            requested_range: Some(MemoryReportRange {
                start_date: "1970-01-01".to_owned(),
                end_date: "1970-01-01".to_owned(),
            }),
            run_id: run_id.to_owned(),
            source_id: "profile".to_owned(),
            kind: ReportRunKind::Daily,
            schedule_id: None,
            revision_of: None,
            window_start: window.0,
            window_end: window.1,
            timezone: "UTC".to_owned(),
            local_date: "1970-01-01".to_owned(),
            phase: ReportRunPhase::Queued,
            outcome: None,
            error: None,
            result: None,
            config_snapshot: "{\"private\":\"secret-config\"}".to_owned(),
            created_at,
            updated_at: created_at,
        }
    }

    fn saved(markdown: &str, frame_ids: Vec<i64>) -> traits::SavedReport {
        traits::SavedReport {
            markdown: markdown.to_owned(),
            model: Some("test-model".to_owned()),
            frames_summarized: frame_ids.len() as u32,
            frame_ids,
            session_ids: vec![7, 8],
            cited_frame_ids: vec![1, 2],
            frames_sampled: 9,
            periods_total: 3,
            periods_covered: 2,
            passes: 2,
            truncated: false,
            daily_run_ids: Vec::new(),
        }
    }

    async fn insert(store: &SqliteStore, run: ReportRun) -> ReportRun {
        store
            .create_report_run(run)
            .await
            .expect("create report run")
    }

    fn window(from: i64, to: i64) -> EvidenceWindow {
        EvidenceWindow { from, to }
    }

    fn request(from: i64, to: i64, limit: u32) -> ReportPageRequest {
        ReportPageRequest {
            window: window(from, to),
            filters: ReportFilters::default(),
            limit,
            continuation: None,
        }
    }

    async fn set_phase(store: &SqliteStore, run_id: &str, phase: ReportRunPhase) {
        let mut run = store.get_report_run(run_id).await.unwrap().unwrap();
        run.phase = phase;
        run.updated_at += 1;
        store
            .update_report_run(run)
            .await
            .expect("update report run");
    }

    #[tokio::test]
    async fn overlap_and_status_filters_apply_in_sql_before_limit() {
        let store = SqliteStore::open_in_memory().unwrap();
        // r_outside has the highest created_at but does not overlap [0,100).
        insert(&store, base_run("r_outside", 60, (200, 300))).await;
        // r_top_nonmatch is the highest overlapping run but is Finished.
        let mut nonmatch = base_run("r_top_nonmatch", 50, (0, 100));
        nonmatch.phase = ReportRunPhase::Finished;
        insert(&store, nonmatch).await;
        let mut mid = base_run("r_mid", 40, (0, 100));
        mid.outcome = Some(ReportOutcome::NoData);
        insert(&store, mid).await;
        insert(&store, base_run("r_low", 10, (0, 100))).await;

        // phase=Queued, limit 1: the filter and overlap run before the limit, so
        // the single row is r_mid, not the filtered-out r_top_nonmatch.
        let mut filtered = request(0, 100, 1);
        filtered.filters.phase = Some(ReportRunPhase::Queued);
        let page = store.report_page(filtered).await.unwrap();
        assert_eq!(ids(&page), vec!["r_mid"]);
        assert_eq!(page.items[0].outcome, Some(ReportOutcome::NoData));
        assert!(page.next_position.is_some());

        // Unfiltered, the same window honours the ordering and the limit.
        let page = store.report_page(request(0, 100, 100)).await.unwrap();
        assert_eq!(ids(&page), vec!["r_top_nonmatch", "r_mid", "r_low"]);
        assert!(page.next_position.is_none());
        assert!(page.snapshot.is_some());
    }

    #[tokio::test]
    async fn keyset_pagination_drains_without_duplicates() {
        let store = SqliteStore::open_in_memory().unwrap();
        for created in [50, 40, 30, 20, 10] {
            insert(&store, base_run(&format!("r{created}"), created, (0, 100))).await;
        }
        let first = store.report_page(request(0, 100, 2)).await.unwrap();
        assert_eq!(ids(&first), vec!["r50", "r40"]);
        let snapshot = first.snapshot.clone().unwrap();

        let mut second_request = request(0, 100, 2);
        second_request.continuation = Some(traits::ReportContinuation {
            snapshot: snapshot.clone(),
            after: first.next_position.clone().unwrap(),
        });
        let second = store.report_page(second_request).await.unwrap();
        assert_eq!(ids(&second), vec!["r30", "r20"]);
        assert_eq!(second.snapshot, Some(snapshot.clone()));

        let mut third_request = request(0, 100, 2);
        third_request.continuation = Some(traits::ReportContinuation {
            snapshot,
            after: second.next_position.clone().unwrap(),
        });
        let third = store.report_page(third_request).await.unwrap();
        assert_eq!(ids(&third), vec!["r10"]);
        assert!(third.next_position.is_none());
    }

    #[tokio::test]
    async fn equal_created_at_orders_by_run_id_descending_and_keeps_revisions() {
        let store = SqliteStore::open_in_memory().unwrap();
        for run_id in ["b", "a", "c"] {
            insert(&store, base_run(run_id, 100, (0, 100))).await;
        }
        let mut revision = base_run("rev", 100, (0, 100));
        revision.revision_of = Some("c".to_owned());
        insert(&store, revision).await;

        let page = store.report_page(request(0, 100, 100)).await.unwrap();
        assert_eq!(ids(&page), vec!["rev", "c", "b", "a"]);
        assert_eq!(
            page.items
                .iter()
                .find(|i| i.run_id == "rev")
                .unwrap()
                .revision_of,
            Some("c".to_owned())
        );
    }

    #[tokio::test]
    async fn empty_table_reports_none_snapshot_and_no_cursor() {
        let store = SqliteStore::open_in_memory().unwrap();
        let page = store.report_page(request(0, 100, 100)).await.unwrap();
        assert!(page.items.is_empty());
        assert!(page.snapshot.is_none());
        assert!(page.next_position.is_none());
    }

    #[tokio::test]
    async fn saved_result_presence_ignores_phase_and_absence_is_null() {
        let store = SqliteStore::open_in_memory().unwrap();
        // No saved result → explicit null, not a fabricated empty report.
        insert(&store, base_run("none", 10, (0, 100))).await;
        // Saved results in non-terminal phases are still exposed immediately.
        let mut queued = base_run("queued", 20, (0, 100));
        queued.result = Some(saved("queued markdown", vec![1, 2]));
        insert(&store, queued).await;
        let mut running = base_run("running", 30, (0, 100));
        running.phase = ReportRunPhase::Running;
        running.result = Some(saved("running markdown", vec![3, 4]));
        insert(&store, running).await;

        assert!(store
            .report_detail("none")
            .await
            .unwrap()
            .unwrap()
            .result
            .is_none());
        let queued = store.report_detail("queued").await.unwrap().unwrap();
        assert_eq!(queued.phase, ReportRunPhase::Queued);
        assert_eq!(queued.result.unwrap().markdown, "queued markdown");
        let running = store.report_detail("running").await.unwrap().unwrap();
        assert_eq!(running.phase, ReportRunPhase::Running);
        assert_eq!(running.result.unwrap().markdown, "running markdown");

        // Unknown run id → None (→404), never a reconstructed report.
        assert!(store.report_detail("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn nodata_saved_result_is_preserved_factually() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut run = base_run("nodata", 10, (0, 100));
        run.phase = ReportRunPhase::Finished;
        run.outcome = Some(ReportOutcome::NoData);
        run.result = Some(saved("No evidence was captured for this window.", vec![]));
        insert(&store, run).await;

        let detail = store.report_detail("nodata").await.unwrap().unwrap();
        assert_eq!(detail.outcome, Some(ReportOutcome::NoData));
        assert_eq!(
            detail.result.unwrap().markdown,
            "No evidence was captured for this window."
        );
    }

    #[tokio::test]
    async fn unknown_error_codes_project_as_null_and_allowlisted_survive() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut unknown = base_run("unknown_err", 10, (0, 100));
        unknown.error = Some(traits::ReportError {
            code: "TOTALLY_MADE_UP".to_owned(),
            message: "private diagnostic text".to_owned(),
        });
        insert(&store, unknown).await;
        let mut allowed = base_run("allowed_err", 20, (0, 100));
        allowed.error = Some(traits::ReportError {
            code: "STORE_ERROR".to_owned(),
            message: "private diagnostic text".to_owned(),
        });
        insert(&store, allowed).await;

        let page = store.report_page(request(0, 100, 100)).await.unwrap();
        let unknown = page
            .items
            .iter()
            .find(|i| i.run_id == "unknown_err")
            .unwrap();
        let allowed = page
            .items
            .iter()
            .find(|i| i.run_id == "allowed_err")
            .unwrap();
        assert_eq!(unknown.error_code, None);
        assert_eq!(allowed.error_code, Some("STORE_ERROR".to_owned()));

        let detail = store.report_detail("allowed_err").await.unwrap().unwrap();
        assert_eq!(detail.error_code, Some("STORE_ERROR".to_owned()));
        // The arbitrary stored message is never projected.
        let json = serde_json::to_string(&detail).unwrap();
        assert!(!json.contains("private diagnostic text"));
    }

    #[tokio::test]
    async fn corrupted_payload_is_a_storage_error_not_empty_success() {
        let store = SqliteStore::open_in_memory().unwrap();
        insert(&store, base_run("broken", 10, (0, 100))).await;
        store
            .with_conn(|conn| {
                conn.execute(
                    "UPDATE report_runs SET payload='{ not valid json' WHERE run_id='broken'",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();

        assert!(matches!(
            store.report_page(request(0, 100, 100)).await,
            Err(HistoryReadError::Storage(_))
        ));
        assert!(matches!(
            store.report_detail("broken").await,
            Err(HistoryReadError::Storage(_))
        ));
    }

    #[tokio::test]
    async fn list_projection_excludes_saved_markdown_evidence_and_config() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut run = base_run("rich", 10, (0, 100));
        run.result = Some(saved("# huge saved markdown body", vec![1, 2, 3]));
        insert(&store, run).await;

        let page = store.report_page(request(0, 100, 100)).await.unwrap();
        let json = serde_json::to_string(&page.items).unwrap();
        assert!(!json.contains("markdown"));
        assert!(!json.contains("huge saved markdown body"));
        assert!(!json.contains("config_snapshot"));
        assert!(!json.contains("secret-config"));
        assert!(!json.contains("schedule_id"));
    }

    /// The persisted `kind` discriminator (`03 §8e`) reaches both wire projections
    /// from the real column, for weekly and daily runs alike.
    #[tokio::test]
    async fn weekly_kind_reaches_summary_and_detail_projections() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut weekly = base_run("weekly", 20, (0, 100));
        weekly.kind = ReportRunKind::WeeklySynthesis;
        weekly.schedule_id = Some("schedule".into());
        weekly.requested_range = None;
        insert(&store, weekly).await;
        insert(&store, base_run("daily", 10, (0, 100))).await;

        let page = store.report_page(request(0, 100, 100)).await.unwrap();
        let items: Vec<(String, ReportRunKind)> = page
            .items
            .iter()
            .map(|item| (item.run_id.clone(), item.kind))
            .collect();
        assert_eq!(
            items,
            vec![
                ("weekly".to_owned(), ReportRunKind::WeeklySynthesis),
                ("daily".to_owned(), ReportRunKind::Daily),
            ]
        );

        let detail = store.report_detail("weekly").await.unwrap().unwrap();
        assert_eq!(detail.kind, ReportRunKind::WeeklySynthesis);
        let detail = store.report_detail("daily").await.unwrap().unwrap();
        assert_eq!(detail.kind, ReportRunKind::Daily);
    }

    #[tokio::test]
    async fn detail_preserves_full_summarized_ids_and_provenance_after_source_deletion() {
        let store = SqliteStore::open_in_memory().unwrap();
        // A real frame that is later deleted: report provenance has no FK to it.
        store
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO frames(id,captured_at,monitor_index,width,height,image_path,content_hash,app_hint,window_title) \
                     VALUES(5,1,0,1920,1080,'5.webp','hash','app','title')",
                    [],
                )?;
                Ok(())
            })
            .await
            .unwrap();
        let ids: Vec<i64> = (1..=250).collect();
        let mut run = base_run("complete", 10, (0, 100));
        run.phase = ReportRunPhase::Finished;
        run.outcome = Some(ReportOutcome::Ok);
        run.result = Some(saved("complete report", ids.clone()));
        insert(&store, run).await;
        store
            .with_conn(|conn| {
                conn.execute("DELETE FROM frames WHERE id=5", [])?;
                Ok(())
            })
            .await
            .unwrap();

        let detail = store.report_detail("complete").await.unwrap().unwrap();
        let result = detail.result.unwrap();
        // >200 summarized ids survive intact (no display cap on provenance).
        assert_eq!(result.frame_ids, ids);
        assert_eq!(result.frames_summarized, 250);
        assert_eq!(result.session_ids, vec![7, 8]);
    }

    #[tokio::test]
    async fn filtered_membership_tracks_current_mutable_status() {
        let store = SqliteStore::open_in_memory().unwrap();
        insert(&store, base_run("a", 20, (0, 100))).await;
        insert(&store, base_run("b", 10, (0, 100))).await;

        let mut queued = request(0, 100, 100);
        queued.filters.phase = Some(ReportRunPhase::Queued);
        assert_eq!(
            ids(&store.report_page(queued.clone()).await.unwrap()),
            vec!["a", "b"]
        );

        // Status is a live observation, not frozen membership.
        set_phase(&store, "b", ReportRunPhase::Finished).await;
        assert_eq!(
            ids(&store.report_page(queued.clone()).await.unwrap()),
            vec!["a"]
        );
        set_phase(&store, "a", ReportRunPhase::Finished).await;
        set_phase(&store, "b", ReportRunPhase::Queued).await;
        assert_eq!(ids(&store.report_page(queued).await.unwrap()), vec!["b"]);
    }

    #[tokio::test]
    async fn invalid_requests_and_continuations_are_rejected() {
        let store = SqliteStore::open_in_memory().unwrap();
        insert(&store, base_run("a", 10, (0, 100))).await;

        assert!(matches!(
            store.report_page(request(0, 100, 0)).await,
            Err(HistoryReadError::InvalidRequest)
        ));
        assert!(matches!(
            store.report_page(request(0, 100, 1001)).await,
            Err(HistoryReadError::InvalidRequest)
        ));
        assert!(matches!(
            store.report_page(request(100, 100, 10)).await,
            Err(HistoryReadError::InvalidRequest)
        ));

        let snapshot = ReportSnapshot {
            max_created_at: 10,
            max_run_id: "a".to_owned(),
        };
        // A seek strictly above the frozen maximum tuple is rejected.
        let mut above = request(0, 100, 10);
        above.continuation = Some(traits::ReportContinuation {
            snapshot: snapshot.clone(),
            after: ReportPosition {
                created_at: 11,
                run_id: "z".to_owned(),
            },
        });
        assert!(matches!(
            store.report_page(above).await,
            Err(HistoryReadError::InvalidRequest)
        ));
        // An empty run id is rejected (no byte cap, but not empty either).
        let mut empty = request(0, 100, 10);
        empty.continuation = Some(traits::ReportContinuation {
            snapshot: snapshot.clone(),
            after: ReportPosition {
                created_at: 10,
                run_id: String::new(),
            },
        });
        assert!(matches!(
            store.report_page(empty).await,
            Err(HistoryReadError::InvalidRequest)
        ));
        // `after == frozen maximum` is valid.
        let mut equal = request(0, 100, 10);
        equal.continuation = Some(traits::ReportContinuation {
            snapshot,
            after: ReportPosition {
                created_at: 10,
                run_id: "a".to_owned(),
            },
        });
        assert!(store.report_page(equal).await.is_ok());
    }

    #[tokio::test]
    async fn continuation_is_never_above_the_frozen_high_tuple() {
        let store = SqliteStore::open_in_memory().unwrap();
        insert(&store, base_run("a", 10, (0, 100))).await;
        insert(&store, base_run("b", 20, (0, 100))).await;

        let first = store.report_page(request(0, 100, 1)).await.unwrap();
        assert_eq!(ids(&first), vec!["b"]);
        let snapshot = first.snapshot.clone().unwrap();
        assert_eq!(snapshot.max_run_id, "b");
        let after = first.next_position.clone().unwrap();

        // Insert a row above the frozen high tuple and a backdated one below it.
        insert(&store, base_run("c", 30, (0, 100))).await;
        insert(&store, base_run("d", 15, (0, 100))).await;

        let mut next = request(0, 100, 100);
        next.continuation = Some(traits::ReportContinuation {
            snapshot: snapshot.clone(),
            after,
        });
        let page = store.report_page(next).await.unwrap();
        // c (above the frozen tuple) is excluded; d (below it) is admitted.
        assert_eq!(ids(&page), vec!["d", "a"]);
        assert_eq!(page.snapshot, Some(snapshot));
    }

    #[tokio::test]
    async fn reads_do_not_write_or_initialize() {
        let store = SqliteStore::open_in_memory().unwrap();
        insert(&store, base_run("a", 10, (0, 100))).await;
        let before: i64 = store
            .with_conn(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM report_runs", [], |r| r.get(0))?)
            })
            .await
            .unwrap();

        store.report_page(request(0, 100, 100)).await.unwrap();
        store.report_detail("a").await.unwrap();
        store.report_page(request(0, 100, 1)).await.unwrap();

        let after: i64 = store
            .with_conn(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM report_runs", [], |r| r.get(0))?)
            })
            .await
            .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn list_report_runs_is_unchanged() {
        let store = SqliteStore::open_in_memory().unwrap();
        let mut run = base_run("a", 10, (0, 100));
        run.result = Some(saved("kept markdown", vec![1, 2]));
        insert(&store, run).await;

        let runs = store.list_report_runs(10, None).await.unwrap();
        assert_eq!(runs.len(), 1);
        // The legacy list still returns the full stored run (result included).
        assert_eq!(runs[0].result.as_ref().unwrap().markdown, "kept markdown");
        assert_eq!(runs[0].config_snapshot, "{\"private\":\"secret-config\"}");
        assert_eq!(runs[0].kind, ReportRunKind::Daily);
    }

    #[tokio::test]
    async fn continuation_seek_cost_does_not_rescan_consumed_prefix() {
        let store = SqliteStore::open_in_memory().unwrap();
        store
            .with_conn(|conn| {
                let tx = conn.unchecked_transaction()?;
                for i in 0..2000i64 {
                    let run = base_run(&format!("r{i:05}"), 1_000_000 - i, (0, 100));
                    tx.execute(
                        "INSERT INTO report_runs(run_id,schedule_id,revision_of,window_start,window_end,phase,created_at,updated_at,payload) \
                         VALUES(?1,NULL,NULL,?2,?3,?4,?5,?5,?6)",
                        rusqlite::params![
                            run.run_id,
                            run.window_start,
                            run.window_end,
                            phase_db(run.phase),
                            run.created_at,
                            serde_json::to_string(&run)?
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .unwrap();

        let steps = |after_created: i64, after_run: &str| {
            let conn = store.conn.lock().expect("store mutex poisoned");
            let mut statement = conn.prepare(REPORT_PAGE_AFTER_SQL).unwrap();
            let rows = statement
                .query_map(
                    named_params! {
                        ":from": 0i64,
                        ":to": 100i64,
                        ":phase": None::<&str>,
                        ":outcome": None::<&str>,
                        ":after_created": after_created,
                        ":after_run": after_run,
                        ":row_limit": 101i64,
                    },
                    row_to_summary,
                )
                .unwrap();
            let count = rows.collect::<rusqlite::Result<Vec<_>>>().unwrap().len();
            assert_eq!(count, 101);
            i64::from(statement.get_status(StatementStatus::VmStep))
        };

        // Shallow seek near the top vs a deep seek with plenty of rows below it.
        let shallow = steps(1_000_000, "r00000");
        let deep = steps(1_000_000 - 1800, "r01800");
        // A true keyset seek must not walk the consumed prefix: the deep page may
        // not cost a whole table more than the shallow one.
        assert!(
            deep <= shallow + 4096,
            "deep continuation rescanned the prefix: shallow={shallow} deep={deep}"
        );
    }

    #[tokio::test]
    async fn sparse_status_filter_examines_all_candidates_before_limit() {
        let store = SqliteStore::open_in_memory().unwrap();
        let rows = 1500i64;
        store
            .with_conn(move |conn| {
                let tx = conn.unchecked_transaction()?;
                for i in 0..rows {
                    let mut run = base_run(&format!("r{i:05}"), 1_000_000 - i, (0, 100));
                    run.phase = ReportRunPhase::Finished;
                    tx.execute(
                        "INSERT INTO report_runs(run_id,schedule_id,revision_of,window_start,window_end,phase,created_at,updated_at,payload) \
                         VALUES(?1,NULL,NULL,?2,?3,?4,?5,?5,?6)",
                        rusqlite::params![
                            run.run_id,
                            run.window_start,
                            run.window_end,
                            phase_db(run.phase),
                            run.created_at,
                            serde_json::to_string(&run)?
                        ],
                    )?;
                }
                tx.commit()?;
                Ok(())
            })
            .await
            .unwrap();

        // No stored run is `queued`, so the filter matches nothing; with no index
        // on phase every candidate must be examined before the empty limit.
        let mut filtered = request(0, 100, 10);
        filtered.filters.phase = Some(ReportRunPhase::Queued);
        let steps = {
            let conn = store.conn.lock().expect("store mutex poisoned");
            let mut statement = conn.prepare(REPORT_PAGE_FIRST_SQL).unwrap();
            let rows = statement
                .query_map(
                    named_params! {
                        ":from": 0i64,
                        ":to": 100i64,
                        ":phase": Some("queued"),
                        ":outcome": None::<&str>,
                        ":row_limit": 11i64,
                    },
                    row_to_summary,
                )
                .unwrap();
            assert_eq!(rows.collect::<rusqlite::Result<Vec<_>>>().unwrap().len(), 0);
            i64::from(statement.get_status(StatementStatus::VmStep))
        };
        assert!(store.report_page(filtered).await.unwrap().items.is_empty());
        assert!(
            steps >= rows,
            "a sparse unindexed filter should touch every candidate row: steps={steps} rows={rows}"
        );
    }

    fn ids(page: &StoredReportPage) -> Vec<&str> {
        page.items.iter().map(|item| item.run_id.as_str()).collect()
    }
}
