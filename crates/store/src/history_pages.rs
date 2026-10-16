//! Bounded chronological evidence and coverage reads (`03 §7f.1/.2/.6`).
//! The caller holds the Store connection lock. Each call owns one DEFERRED read
//! transaction; metadata, membership and hydrated content share that snapshot.

use rusqlite::{named_params, params, Connection, OptionalExtension, Row, TransactionBehavior};
use traits::{
    EvidenceWindow, FrameEvidence, FramePageRequest, FramePosition, FrameReference, FrameSnapshot,
    FrameTextState, HistoryReadError, StoredCoverage, StoredFramePage, TextSource,
};

use crate::{history::read_history_metadata, SqliteStore};

// LIMIT is inside the membership CTE: only limit+1 IDs are hydrated with full
// text/vision below. The two optional tables have frame_id PRIMARY KEY, so each
// stored frame produces exactly one row. Target context lives in frame_text,
// not a JSON blob or vision_analysis.app_hint. COALESCE preserves present "".
// lower/instr is literal, ASCII-case-insensitive and NUL-safe: unlike LIKE,
// it neither interprets %/_/backslash nor truncates an accepted NUL-containing
// title filter. The additional timestamp bound keeps continuation index scans
// from restarting at the beginning of the requested window.
const FRAME_PAGE_SQL: &str = r#"
    WITH page AS (
        SELECT f.id, f.captured_at
        FROM frames AS f
        LEFT JOIN frame_text AS t ON t.frame_id = f.id
        WHERE f.captured_at >= :from AND f.captured_at < :to
          AND f.captured_at >= COALESCE(:after_time, :from)
          AND f.id <= :maximum
          AND (:after_time IS NULL OR (f.captured_at, f.id) > (:after_time, :after_id))
          AND (:app IS NULL OR COALESCE(t.target_app_hint, f.app_hint) = :app COLLATE NOCASE)
          AND (:title IS NULL OR instr(lower(COALESCE(t.target_window_title, f.window_title)), lower(:title)) > 0)
        ORDER BY f.captured_at ASC, f.id ASC
        LIMIT :row_limit
    )
    SELECT f.id, f.captured_at, f.monitor_index,
           COALESCE(t.target_app_hint, f.app_hint),
           COALESCE(t.target_window_title, f.window_title), f.browser_url,
           t.raw_text, t.content_text, t.primary_source, t.filter_version, t.frame_id,
           v.description, f.session_id, f.image_purged
    FROM page AS p
    JOIN frames AS f ON f.id = p.id
    LEFT JOIN frame_text AS t ON t.frame_id = f.id
    LEFT JOIN vision_analysis AS v ON v.frame_id = f.id
    ORDER BY p.captured_at ASC, p.id ASC
"#;

const COVERAGE_COUNTS_SQL: &str = "
    SELECT COUNT(*), COUNT(t.frame_id)
    FROM frames AS f
    LEFT JOIN frame_text AS t ON t.frame_id = f.id
    WHERE f.captured_at >= ?1 AND f.captured_at < ?2";

fn storage(error: rusqlite::Error) -> HistoryReadError {
    HistoryReadError::Storage(error.into())
}

fn observed_at() -> Result<i64, HistoryReadError> {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| HistoryReadError::Storage(error.into()))?;
    i64::try_from(elapsed.as_millis()).map_err(|error| HistoryReadError::Storage(error.into()))
}

fn row_to_evidence(row: &Row<'_>) -> rusqlite::Result<FrameEvidence> {
    let text_available = row.get::<_, Option<i64>>(10)?.is_some();
    let source = row.get::<_, Option<String>>(8)?;
    let text_source = source
        .as_deref()
        .map(|source| {
            TextSource::from_db_str(source).ok_or_else(|| {
                // An impossible production CHECK value is corruption, not
                // permission to fabricate OCR provenance or discard text.
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "invalid stored frame text source",
                    )),
                )
            })
        })
        .transpose()?;
    Ok(FrameEvidence {
        frame_id: row.get(0)?,
        captured_at: row.get(1)?,
        monitor_index: row.get(2)?,
        app_hint: row.get(3)?,
        window_title: row.get(4)?,
        browser_url: row.get(5)?,
        raw_text: row.get(6)?,
        content_text: row.get(7)?,
        text_source,
        filter_version: row.get(9)?,
        text_state: if text_available {
            FrameTextState::Available
        } else {
            FrameTextState::NotAvailable
        },
        vision_description: row.get(11)?,
        session_id: row.get(12)?,
        image_purged: row.get(13)?,
    })
}

/// One bounded-membership, live-content page. No initialization or repair.
/// `unchecked_transaction` permits the existing with_conn(&Connection) seam;
/// the caller must hold its exclusive Store lock and not nest transactions.
pub(crate) fn frame_page(
    conn: &Connection,
    request: FramePageRequest,
) -> Result<StoredFramePage, HistoryReadError> {
    request.validate()?;
    // Explicit DEFERRED, regardless of this Connection's configured default.
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Deferred)
        .map_err(storage)?;
    // This first database read establishes the transaction snapshot.
    let history = read_history_metadata(&tx)?;
    let observed_at = observed_at()?;
    let snapshot = if let Some(continuation) = &request.continuation {
        if continuation.snapshot.history_id != history.history_id {
            return Err(HistoryReadError::HistoryChanged);
        }
        // Deletions may lower current MAX(id), never the supported allocator.
        // Do not reject a valid continuation just because its tail was deleted.
        if continuation.snapshot.snapshot_max_frame_id > history.frame_id_high_water {
            return Err(HistoryReadError::InvalidRequest);
        }
        continuation.snapshot.clone()
    } else {
        let maximum: i64 = tx
            .query_row("SELECT COALESCE(MAX(id), 0) FROM frames", [], |row| {
                row.get(0)
            })
            .map_err(storage)?;
        FrameSnapshot {
            history_id: history.history_id,
            snapshot_max_frame_id: maximum.max(0),
        }
    };
    let after = request.continuation.as_ref().map(|c| c.after);

    let mut items = {
        let mut stmt = tx.prepare(FRAME_PAGE_SQL).map_err(storage)?;
        let rows = stmt
            .query_map(
                named_params! {
                    ":from": request.window.from,
                    ":to": request.window.to,
                    ":maximum": snapshot.snapshot_max_frame_id,
                    ":after_time": after.map(|p| p.captured_at),
                    ":after_id": after.map(|p| p.frame_id),
                    ":app": request.filters.app.as_deref(),
                    ":title": request.filters.window_title.as_deref(),
                    ":row_limit": i64::from(request.limit) + 1,
                },
                row_to_evidence,
            )
            .map_err(storage)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(storage)?
    };
    let has_more = items.len() > request.limit as usize;
    items.truncate(request.limit as usize);
    let next_position = if has_more {
        items.last().map(|item| FramePosition {
            captured_at: item.captured_at,
            frame_id: item.frame_id,
        })
    } else {
        None
    };
    tx.commit().map_err(storage)?;
    Ok(StoredFramePage {
        items,
        snapshot,
        window: request.window,
        filters: request.filters,
        next_position,
        observed_at,
    })
}

fn row_to_reference(row: &Row<'_>) -> rusqlite::Result<FrameReference> {
    Ok(FrameReference {
        frame_id: row.get(0)?,
        captured_at: row.get(1)?,
    })
}

impl SqliteStore {
    /// Read one chronological page without blocking the async runtime.
    pub async fn frame_page(
        &self,
        request: FramePageRequest,
    ) -> Result<StoredFramePage, HistoryReadError> {
        self.with_conn(move |conn| Ok(frame_page(conn, request)))
            .await
            .map_err(HistoryReadError::Storage)?
    }

    /// Read current storage facts; capture/runtime state is composed above Store.
    pub async fn stored_coverage(
        &self,
        window: EvidenceWindow,
    ) -> Result<StoredCoverage, HistoryReadError> {
        self.with_conn(move |conn| Ok(stored_coverage(conn, window)))
            .await
            .map_err(HistoryReadError::Storage)?
    }
}

/// SQL facts only: global chronological extrema and half-open-window counts.
/// Empty storage is not inferred inactivity; failed storage is never zero data.
pub(crate) fn stored_coverage(
    conn: &Connection,
    window: EvidenceWindow,
) -> Result<StoredCoverage, HistoryReadError> {
    window.validate()?;
    let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Deferred)
        .map_err(storage)?;
    let history = read_history_metadata(&tx)?;
    let observed_at = observed_at()?;
    let oldest_frame = tx
        .query_row(
            "SELECT id, captured_at FROM frames ORDER BY captured_at ASC, id ASC LIMIT 1",
            [],
            row_to_reference,
        )
        .optional()
        .map_err(storage)?;
    let newest_frame = tx
        .query_row(
            "SELECT id, captured_at FROM frames ORDER BY captured_at DESC, id DESC LIMIT 1",
            [],
            row_to_reference,
        )
        .optional()
        .map_err(storage)?;
    // COUNT() is nonnegative; rusqlite has no u64 FromSql, so decode as i64 and
    // checked-convert (no cast/clamp): a negative count is a typed storage error.
    let (frames_stored, frames_text_available) = tx
        .query_row(
            COVERAGE_COUNTS_SQL,
            params![window.from, window.to],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .map_err(storage)?;
    let frames_stored =
        u64::try_from(frames_stored).map_err(|error| HistoryReadError::Storage(error.into()))?;
    let frames_text_available = u64::try_from(frames_text_available)
        .map_err(|error| HistoryReadError::Storage(error.into()))?;
    tx.commit().map_err(storage)?;
    Ok(StoredCoverage {
        history,
        window,
        observed_at,
        oldest_frame,
        newest_frame,
        newest_captured_at: newest_frame.map(|reference| reference.captured_at),
        frames_stored,
        frames_text_available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use traits::{
        FrameContinuation, FrameFilters, HistoryUnavailableReason, FRAME_ID_HIGH_WATER_KEY,
        HISTORY_ID_KEY,
    };

    fn request(limit: u32) -> FramePageRequest {
        FramePageRequest {
            window: EvidenceWindow {
                from: -100,
                to: 1000,
            },
            filters: FrameFilters::default(),
            limit,
            continuation: None,
        }
    }

    // Production migrations/setup, FTS triggers and foreign keys, not a parallel
    // test schema. Explicit fixture writes below use the production allocator.
    fn fixture() -> SqliteStore {
        SqliteStore::open_in_memory().expect("open production SQLite store")
    }

    fn insert(conn: &Connection, at: i64, app: Option<&str>, title: Option<&str>) -> i64 {
        let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
            .expect("fixture transaction");
        let id = crate::history::allocate_frame_id(&tx).expect("fixture allocation");
        tx.execute(
            "INSERT INTO frames(id,captured_at,monitor_index,width,height,image_path,content_hash,
                                app_hint,window_title,browser_url)
             VALUES(?1,?2,2,1920,1080,'private/frame.jpg','fixture',?3,?4,'https://example.test/')",
            params![id, at, app, title],
        )
        .expect("fixture frame");
        tx.commit().expect("fixture commit");
        id
    }

    fn text(conn: &Connection, id: i64, raw: &str, content: &str) {
        conn.execute(
            "INSERT INTO frame_text(frame_id,raw_text,content_text,primary_source,filter_version,suppressed_count)
             VALUES(?1,?2,?3,'uia',7,0)",
            params![id, raw, content],
        )
        .expect("fixture text");
    }

    fn resume(page: &StoredFramePage, limit: u32) -> FramePageRequest {
        FramePageRequest {
            window: page.window,
            filters: page.filters.clone(),
            limit,
            continuation: Some(FrameContinuation {
                snapshot: page.snapshot.clone(),
                after: page.next_position.expect("nonterminal page"),
            }),
        }
    }

    fn ids(page: &StoredFramePage) -> Vec<i64> {
        page.items.iter().map(|item| item.frame_id).collect()
    }

    #[tokio::test]
    async fn trait_object_dispatch_uses_real_sql_and_preserves_typed_readiness_errors() {
        let concrete = fixture();
        let expected;
        {
            let conn = concrete.conn.lock().expect("fixture connection");
            expected = insert(&conn, 1, None, None);
            text(&conn, expected, "stored raw", "stored content");
        }
        let store: &dyn traits::Store = &concrete;
        let page = store
            .frame_page(request(1))
            .await
            .expect("concrete trait dispatch");
        assert_eq!(ids(&page), [expected]);
        assert_eq!(page.items[0].raw_text.as_deref(), Some("stored raw"));
        let coverage = store
            .stored_coverage(request(1).window)
            .await
            .expect("coverage trait dispatch");
        assert_eq!(
            (coverage.frames_stored, coverage.frames_text_available),
            (1, 1)
        );
        {
            let conn = concrete.conn.lock().expect("fixture corruption only");
            conn.execute("DELETE FROM settings WHERE key=?1", [HISTORY_ID_KEY])
                .expect("remove identity");
        }
        assert!(matches!(
            store.frame_page(request(1)).await,
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingIdentity
            ))
        ));
        assert!(matches!(
            store.stored_coverage(request(1).window).await,
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingIdentity
            ))
        ));
    }

    #[test]
    fn chronological_ties_out_of_order_and_half_open_window() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let a = insert(&conn, 30, None, None);
        let b = insert(&conn, 10, None, None);
        let c = insert(&conn, 10, None, None);
        let lower = insert(&conn, -100, None, None);
        insert(&conn, 1000, None, None);
        insert(&conn, -101, None, None);
        let first = frame_page(&conn, request(2)).expect("first page");
        assert_eq!(ids(&first), [lower, b]);
        assert_eq!(
            first.next_position,
            Some(FramePosition {
                captured_at: 10,
                frame_id: b
            })
        );
        let last = frame_page(&conn, resume(&first, 2)).expect("last page");
        assert_eq!(ids(&last), [c, a]);
        assert_eq!(
            last.next_position, None,
            "exactly limit rows is terminal without lookahead"
        );
    }

    #[test]
    fn frozen_membership_excludes_new_equal_and_backdated_inserts() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let a = insert(&conn, 10, None, None);
        let b = insert(&conn, 30, None, None);
        let c = insert(&conn, 20, None, None);
        let first = frame_page(&conn, request(1)).expect("first");
        assert_eq!(ids(&first), [a]);
        assert_eq!(first.snapshot.snapshot_max_frame_id, c);
        insert(&conn, 10, None, None);
        insert(&conn, 15, None, None);
        insert(&conn, 40, None, None);
        let last = frame_page(&conn, resume(&first, 100)).expect("continuation");
        assert_eq!(ids(&last), [c, b]);
        assert_eq!(last.snapshot, first.snapshot);
        assert!(last.next_position.is_none());
    }

    #[test]
    fn filters_resolve_target_metadata_before_limit_and_preserve_literals() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        insert(&conn, 1, Some("unrelated"), Some("100XdoneY tail"));
        insert(&conn, 2, None, None);
        let target = insert(&conn, 3, Some("wrong"), Some("wrong"));
        text(&conn, target, "raw", "content");
        conn.execute(
            "UPDATE frame_text SET target_app_hint='Editor.EXE', target_window_title=?1 WHERE frame_id=?2",
            params![r"prefix 100%done_\ tail suffix", target],
        ).expect("target metadata");
        let fallback = insert(&conn, 4, Some("EDITOR.exe"), Some(r"100%done_\ TAIL"));
        let overridden = insert(&conn, 5, Some("Editor.exe"), Some(r"100%done_\ tail"));
        text(&conn, overridden, "raw", "content");
        conn.execute(
            "UPDATE frame_text SET target_app_hint='',target_window_title='' WHERE frame_id=?1",
            [overridden],
        )
        .expect("empty target is present, not fallback");
        let mut query = request(1);
        query.filters = FrameFilters {
            app: Some("editor.exe".into()),
            window_title: Some(r"100%done_\ tail".into()),
        };
        let first = frame_page(&conn, query).expect("filtered first");
        assert_eq!(ids(&first), [target]);
        assert_eq!(first.items[0].app_hint.as_deref(), Some("Editor.EXE"));
        assert_eq!(
            first.items[0].window_title.as_deref(),
            Some(r"prefix 100%done_\ tail suffix")
        );
        let last = frame_page(&conn, resume(&first, 1)).expect("filtered continuation");
        assert_eq!(ids(&last), [fallback]);
        assert!(last.next_position.is_none());
        let mut query = request(100);
        query.filters.app = Some("EDITOR".into());
        assert!(frame_page(&conn, query)
            .expect("exact, not app substring")
            .items
            .is_empty());
        let mut query = request(100);
        query.filters.window_title = Some(String::new());
        assert!(
            !ids(&frame_page(&conn, query).expect("empty literal")).contains(&2),
            "NULL does not match even an empty supplied title"
        );
    }

    #[test]
    fn ascii_casefold_does_not_fold_unicode_or_trim_whitespace() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let a = insert(&conn, 1, Some("ÉDITOR"), Some("ÉCRAN Report"));
        insert(&conn, 2, Some("éditor"), Some("écran Report"));
        let mut query = request(100);
        query.filters = FrameFilters {
            app: Some("Éditor".into()),
            window_title: Some("Écran report".into()),
        };
        assert_eq!(
            ids(&frame_page(&conn, query.clone()).expect("ASCII-only case folding")),
            [a]
        );
        query.filters.app = Some(" Éditor".into());
        assert!(frame_page(&conn, query)
            .expect("literal whitespace")
            .items
            .is_empty());
    }

    #[test]
    fn literal_title_filter_keeps_embedded_nul_and_metacharacters() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        insert(&conn, 1, None, Some("foo"));
        let full = insert(&conn, 2, None, Some("prefix FOO\0BAR%_\\ suffix"));
        insert(&conn, 3, None, Some("foo\0other%_\\"));
        let mut query = request(100);
        query.filters.window_title = Some("foo\0bar%_\\".into());
        assert_eq!(
            ids(&frame_page(&conn, query).expect("NUL-safe literal title")),
            [full]
        );
        let mut query = request(100);
        query.filters.window_title = Some("\0BAR%_\\".into());
        assert_eq!(
            ids(&frame_page(&conn, query).expect("literal substring after NUL")),
            [full]
        );
    }

    #[test]
    fn missing_empty_full_text_vision_and_session_are_distinct_and_untruncated() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let missing = insert(&conn, 1, None, None);
        let empty = insert(&conn, 2, None, None);
        text(&conn, empty, "", "");
        let full = insert(&conn, 3, Some("app"), Some("title"));
        let raw = "raw text 界\n".repeat(2000);
        let content = "filtered text\n".repeat(2000);
        let vision = "stored vision\n".repeat(2000);
        text(&conn, full, &raw, &content);
        conn.execute(
            "INSERT INTO vision_analysis(frame_id,description,confidence,model,app_hint) VALUES(?1,?2,0.9,'fixture','not-context')",
            params![full, vision],
        ).expect("vision");
        conn.execute("INSERT INTO sessions(id,started_at,kind,context_key,confidence) VALUES(1,0,'focus','fixture',0.9)", [])
            .expect("session");
        conn.execute(
            "UPDATE frames SET session_id=1,image_purged=1 WHERE id=?1",
            [full],
        )
        .expect("link session");
        let page = frame_page(&conn, request(100)).expect("hydrated page");
        assert_eq!(ids(&page), [missing, empty, full]);
        let no_text = &page.items[0];
        assert_eq!(no_text.text_state, FrameTextState::NotAvailable);
        assert_eq!(
            (
                no_text.raw_text.as_ref(),
                no_text.content_text.as_ref(),
                no_text.text_source,
                no_text.filter_version
            ),
            (None, None, None, None)
        );
        assert!(no_text.vision_description.is_none());
        assert!(no_text.session_id.is_none());
        assert_eq!(page.items[1].text_state, FrameTextState::Available);
        assert_eq!(page.items[1].raw_text.as_deref(), Some(""));
        assert_eq!(page.items[1].content_text.as_deref(), Some(""));
        let item = &page.items[2];
        assert_eq!(item.raw_text.as_deref(), Some(raw.as_str()));
        assert_eq!(item.content_text.as_deref(), Some(content.as_str()));
        assert_eq!(item.text_source, Some(TextSource::Uia));
        assert_eq!(item.filter_version, Some(7));
        assert_eq!(item.vision_description.as_deref(), Some(vision.as_str()));
        assert_eq!(item.session_id, Some(1));
        assert_eq!(item.monitor_index, 2);
        assert_eq!(item.browser_url.as_deref(), Some("https://example.test/"));
        assert!(item.image_purged);
        let serialized = serde_json::to_string(&page.items).expect("serialize DTOs");
        assert!(!serialized.contains("image_path"));
        assert!(!serialized.contains("private/frame.jpg"));
        assert!(!serialized.contains("not-context"));
        // Later enrichment is live content: retrying the same bounded traversal
        // observes it without pretending that the earlier page had complete OCR.
        text(&conn, missing, "late raw", "late content");
        let retry = frame_page(&conn, request(100)).expect("late enrichment re-read");
        assert_eq!(retry.items[0].raw_text.as_deref(), Some("late raw"));
    }

    #[test]
    fn drains_more_than_a_thousand_without_a_200_row_cap_or_duplicates() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let mut expected = Vec::new();
        for index in 0..1107 {
            let at = i64::from((index * 37) % 900);
            let id = insert(&conn, at, None, None);
            if index % 2 == 0 {
                text(&conn, id, "", "");
            }
            expected.push((at, id));
        }
        expected.sort_unstable();
        let mut page = frame_page(&conn, request(1000)).expect("maximum-size page");
        assert_eq!(page.items.len(), 1000);
        let snapshot = page.snapshot.clone();
        let mut actual = ids(&page);
        page = frame_page(&conn, resume(&page, 1000)).expect("tail page");
        assert_eq!(page.items.len(), 107);
        actual.extend(ids(&page));
        assert_eq!(
            actual,
            expected.into_iter().map(|(_, id)| id).collect::<Vec<_>>()
        );
        assert_eq!(page.snapshot, snapshot);
        assert!(page.next_position.is_none());
    }

    #[test]
    fn deletion_of_seek_row_tail_and_all_remaining_rows_still_progresses() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let a = insert(&conn, 1, None, None);
        let b = insert(&conn, 2, None, None);
        let c = insert(&conn, 3, None, None);
        let tail = insert(&conn, 4, None, None);
        let first = frame_page(&conn, request(1)).expect("first");
        conn.execute("DELETE FROM frames WHERE id IN (?1,?2)", params![a, tail])
            .expect("delete seek/tail");
        let second =
            frame_page(&conn, resume(&first, 1)).expect("deleted seek is not required to exist");
        assert_eq!(ids(&second), [b]);
        conn.execute("DELETE FROM frames WHERE id=?1", [c])
            .expect("delete remaining");
        let last = frame_page(&conn, resume(&second, 1)).expect("terminal empty");
        assert!(last.items.is_empty());
        assert!(last.next_position.is_none());
        assert_eq!(last.snapshot.snapshot_max_frame_id, tail);
    }

    #[test]
    fn coverage_counts_text_presence_and_uses_global_chronological_extrema() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let new = insert(&conn, i64::MAX - 1, None, None);
        let old = insert(&conn, i64::MIN, None, None);
        let middle = insert(&conn, 12, None, None);
        let latest_tie = insert(&conn, i64::MAX - 1, None, None);
        let oldest_tie = insert(&conn, i64::MIN, None, None);
        text(&conn, middle, "", "");
        insert(&conn, 13, None, None);
        insert(&conn, 20, None, None);
        let coverage =
            stored_coverage(&conn, EvidenceWindow { from: 12, to: 20 }).expect("coverage");
        assert_eq!(
            coverage.oldest_frame,
            Some(FrameReference {
                frame_id: old,
                captured_at: i64::MIN
            })
        );
        assert_eq!(
            coverage.newest_frame,
            Some(FrameReference {
                frame_id: latest_tie,
                captured_at: i64::MAX - 1
            })
        );
        assert_ne!(coverage.newest_frame.expect("newest").frame_id, new);
        assert_ne!(coverage.oldest_frame.expect("oldest").frame_id, oldest_tie);
        assert_eq!(coverage.newest_captured_at, Some(i64::MAX - 1));
        assert_eq!(
            (coverage.frames_stored, coverage.frames_text_available),
            (2, 1)
        );
        let empty =
            stored_coverage(&conn, EvidenceWindow { from: 100, to: 200 }).expect("empty window");
        assert_eq!((empty.frames_stored, empty.frames_text_available), (0, 0));
        assert_eq!(
            empty.oldest_frame, coverage.oldest_frame,
            "extrema are global, not window-local"
        );
        let high_water = coverage.history.frame_id_high_water;
        conn.execute("DELETE FROM frames", []).expect("delete all");
        let deleted = stored_coverage(&conn, request(1).window).expect("empty store");
        assert_eq!(deleted.history.frame_id_high_water, high_water);
        assert_eq!(
            (
                deleted.oldest_frame,
                deleted.newest_frame,
                deleted.newest_captured_at
            ),
            (None, None, None)
        );
        assert_eq!(
            (deleted.frames_stored, deleted.frames_text_available),
            (0, 0)
        );
        let page = frame_page(&conn, request(100)).expect("empty page");
        assert_eq!(page.snapshot.snapshot_max_frame_id, 0);
        assert!(page.items.is_empty());
        assert!(page.next_position.is_none());
    }

    #[test]
    fn invalid_requests_and_continuation_relationships_are_errors() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        insert(&conn, 1, None, None);
        insert(&conn, 2, None, None);
        for limit in [0, 1001, u32::MAX] {
            assert!(matches!(
                frame_page(&conn, request(limit)),
                Err(HistoryReadError::InvalidRequest)
            ));
        }
        let mut query = request(1);
        query.window.to = query.window.from;
        assert!(matches!(
            frame_page(&conn, query),
            Err(HistoryReadError::InvalidRequest)
        ));
        assert!(matches!(
            stored_coverage(&conn, EvidenceWindow { from: 1, to: 1 }),
            Err(HistoryReadError::InvalidRequest)
        ));
        let mut query = request(1);
        query.filters.app = Some("é".repeat(129));
        assert!(matches!(
            frame_page(&conn, query),
            Err(HistoryReadError::InvalidRequest)
        ));
        let mut query = request(1);
        query.filters.window_title = Some("é".repeat(129));
        assert!(matches!(
            frame_page(&conn, query),
            Err(HistoryReadError::InvalidRequest)
        ));
        let mut query = request(1);
        query.filters.app = Some("é".repeat(128));
        assert!(
            frame_page(&conn, query).is_ok(),
            "byte-boundary is accepted"
        );
        let first = frame_page(&conn, request(1)).expect("first");
        let mut continuation = resume(&first, 1);
        continuation
            .continuation
            .as_mut()
            .expect("cursor")
            .after
            .captured_at = first.window.to;
        assert!(matches!(
            frame_page(&conn, continuation),
            Err(HistoryReadError::InvalidRequest)
        ));
        let mut continuation = resume(&first, 1);
        continuation
            .continuation
            .as_mut()
            .expect("cursor")
            .after
            .frame_id = first.snapshot.snapshot_max_frame_id + 1;
        assert!(matches!(
            frame_page(&conn, continuation),
            Err(HistoryReadError::InvalidRequest)
        ));
        let mut continuation = resume(&first, 1);
        continuation
            .continuation
            .as_mut()
            .expect("cursor")
            .snapshot
            .snapshot_max_frame_id += 1;
        assert!(
            matches!(
                frame_page(&conn, continuation),
                Err(HistoryReadError::InvalidRequest)
            ),
            "future snapshot maximum above persisted high-water"
        );
        let mut continuation = resume(&first, 1);
        continuation
            .continuation
            .as_mut()
            .expect("cursor")
            .snapshot
            .history_id = "00000000-0000-4000-8000-000000000000".into();
        assert!(matches!(
            frame_page(&conn, continuation),
            Err(HistoryReadError::HistoryChanged)
        ));
    }

    #[test]
    fn deferred_read_snapshot_keeps_metadata_maximum_and_hydration_together() {
        let dir = tempfile::tempdir().expect("isolated database directory");
        let path = dir.path().join("history-pages.db");
        let reader = SqliteStore::open_path(&path).expect("production reader store");
        let writer = SqliteStore::open_path(&path).expect("production writer store");
        let read_conn = reader.conn.lock().expect("reader connection");
        let first = insert(&read_conn, 1, Some("before"), None);
        text(&read_conn, first, "before raw", "before content");
        let tx = rusqlite::Transaction::new_unchecked(&read_conn, TransactionBehavior::Deferred)
            .expect("read transaction");
        let history = read_history_metadata(&tx).expect("establish snapshot with metadata read");
        {
            let write_conn = writer.conn.lock().expect("writer connection");
            insert(&write_conn, 2, None, None);
            write_conn.execute(
                "UPDATE frame_text SET raw_text='after raw',content_text='after content',target_app_hint='after' WHERE frame_id=?1",
                [first],
            ).expect("concurrent enrichment commits in WAL");
        }
        let maximum: i64 = tx
            .query_row("SELECT COALESCE(MAX(id),0) FROM frames", [], |r| r.get(0))
            .expect("maximum in same read snapshot");
        assert_eq!(maximum, first);
        assert_eq!(history.frame_id_high_water, first);
        let rows = tx
            .prepare(FRAME_PAGE_SQL)
            .expect("production page SQL")
            .query_map(
                named_params! {
                    ":from": -100_i64, ":to": 1000_i64, ":maximum": maximum,
                    ":after_time": Option::<i64>::None, ":after_id": Option::<i64>::None,
                    ":app": Option::<&str>::None, ":title": Option::<&str>::None,
                    ":row_limit": 101_i64,
                },
                row_to_evidence,
            )
            .expect("query production page SQL")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("hydrate in same snapshot");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].raw_text.as_deref(), Some("before raw"));
        assert_eq!(rows[0].app_hint.as_deref(), Some("before"));
        let counts: (i64, i64) = tx
            .query_row(COVERAGE_COUNTS_SQL, [-100_i64, 1000], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .expect("coverage counts in same read snapshot");
        assert_eq!(counts, (1, 1));
        tx.commit().expect("release read snapshot");
        let fresh = frame_page(&read_conn, request(100)).expect("next call sees live changes");
        assert_eq!(fresh.items.len(), 2);
        assert_eq!(fresh.items[0].raw_text.as_deref(), Some("after raw"));
        assert_eq!(fresh.items[0].app_hint.as_deref(), Some("after"));
    }

    #[test]
    fn query_only_connection_reads_succeed_and_missing_identity_is_never_initialized() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        let id = insert(&conn, 1, None, None);
        text(&conn, id, "raw", "content");
        conn.execute(
            "UPDATE frame_text SET primary_source='ocr' WHERE frame_id=?1",
            [id],
        )
        .expect("stored OCR provenance");
        conn.execute_batch("PRAGMA query_only=ON")
            .expect("enforce no writes");
        let page = frame_page(&conn, request(1)).expect("read-only page");
        assert_eq!(page.items[0].text_source, Some(TextSource::Ocr));
        assert_eq!(
            stored_coverage(&conn, request(1).window)
                .expect("read-only coverage")
                .frames_text_available,
            1
        );
        conn.execute_batch("PRAGMA query_only=OFF")
            .expect("fixture mutation only");
        conn.execute(
            "DELETE FROM settings WHERE key IN (?1,?2)",
            params![HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY],
        )
        .expect("remove both keys only in test");
        conn.execute_batch("PRAGMA query_only=ON")
            .expect("enforce no initialization");
        assert!(matches!(
            frame_page(&conn, request(1)),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingIdentity
            ))
        ));
        assert!(matches!(
            stored_coverage(&conn, request(1).window),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingIdentity
            ))
        ));
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM settings WHERE key IN (?1,?2)",
                params![HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY],
                |r| r.get(0),
            )
            .expect("inspect absent metadata");
        assert_eq!(count, 0);
    }

    #[test]
    fn metadata_failure_is_read_only_and_storage_failure_is_not_empty_success() {
        let store = fixture();
        let conn = store.conn.lock().expect("connection");
        insert(&conn, 1, None, None);
        conn.execute(
            "DELETE FROM settings WHERE key=?1",
            [FRAME_ID_HIGH_WATER_KEY],
        )
        .expect("test-only corruption");
        assert!(matches!(
            frame_page(&conn, request(1)),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingAllocator
            ))
        ));
        assert!(matches!(
            stored_coverage(&conn, request(1).window),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingAllocator
            ))
        ));
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM settings WHERE key=?1",
                [FRAME_ID_HIGH_WATER_KEY],
                |r| r.get::<_, i64>(0)
            )
            .expect("not repaired"),
            0
        );
        conn.execute(
            "INSERT INTO settings(key,value) VALUES(?1,'invalid')",
            [FRAME_ID_HIGH_WATER_KEY],
        )
        .expect("test-only invalid allocator");
        assert!(matches!(
            stored_coverage(&conn, request(1).window),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::InvalidAllocator
            ))
        ));
        conn.execute(
            "UPDATE settings SET value='1' WHERE key=?1",
            [FRAME_ID_HIGH_WATER_KEY],
        )
        .expect("restore test allocator");
        conn.execute(
            "UPDATE settings SET value='invalid' WHERE key=?1",
            [HISTORY_ID_KEY],
        )
        .expect("test-only invalid identity");
        assert!(matches!(
            frame_page(&conn, request(1)),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::InvalidIdentity
            ))
        ));
        conn.execute("DELETE FROM settings WHERE key=?1", [HISTORY_ID_KEY])
            .expect("test-only missing identity");
        assert!(matches!(
            stored_coverage(&conn, request(1).window),
            Err(HistoryReadError::Unavailable(
                HistoryUnavailableReason::MissingIdentity
            ))
        ));
        conn.execute(
            "INSERT INTO settings(key,value) VALUES(?1,'00000000-0000-4000-8000-000000000000')",
            [HISTORY_ID_KEY],
        )
        .expect("restore test identity");
        conn.execute_batch("ALTER TABLE frames RENAME TO broken_frames;")
            .expect("test-only inaccessible table");
        assert!(matches!(
            frame_page(&conn, request(1)),
            Err(HistoryReadError::Storage(_))
        ));
        assert!(matches!(
            stored_coverage(&conn, request(1).window),
            Err(HistoryReadError::Storage(_))
        ));
    }
}
