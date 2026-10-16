//! Real SQLite regression fixtures for `03 §7f.1/.8` identity and allocation.
//! Direct SQL below is fixture setup/corruption only, never a supported writer.

use rusqlite::{params, Connection};
use store::{SqliteStore, Store};
use traits::{
    HistoryReadError, HistoryUnavailableReason, NewFrame, FRAME_ID_HIGH_WATER_KEY, HISTORY_ID_KEY,
};

fn frame(at: i64) -> NewFrame {
    NewFrame {
        captured_at: at,
        monitor_index: 0,
        width: 1,
        height: 1,
        image_path: format!("frames/{at}.webp"),
        content_hash: format!("hash-{at}"),
        app_hint: None,
        window_title: None,
        browser_url: None,
        capture_trigger: None,
    }
}

fn fixture_setting(conn: &Connection, key: &str, value: &str) {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )
    .unwrap();
}

#[tokio::test]
async fn frame_ids_never_reuse_deleted_tail() {
    let store = SqliteStore::open_in_memory().unwrap();
    let first = store.insert_frame(frame(10)).await.unwrap();
    let tail = store.insert_frame(frame(20)).await.unwrap();
    store.delete_frame(tail).await.unwrap();
    let next = store.insert_frame(frame(5)).await.unwrap();
    assert!(next > tail && tail > first);
    store.delete_frame(first).await.unwrap();
    store.delete_frame(next).await.unwrap();
    assert!(store.insert_frame(frame(1)).await.unwrap() > next);
}

#[tokio::test]
async fn frame_allocator_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("history.db");
    let store = SqliteStore::open_path(&path).unwrap();
    let id = store.insert_frame(frame(10)).await.unwrap();
    let history = Store::history_readiness(&store).await.unwrap();
    store.delete_frame(id).await.unwrap();
    drop(store);

    let reopened = SqliteStore::open_path(&path).unwrap();
    assert_eq!(Store::history_readiness(&reopened).await.unwrap(), history);
    assert!(reopened.insert_frame(frame(1)).await.unwrap() > id);
}

#[tokio::test]
async fn preserves_existing_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.db");
    drop(SqliteStore::open_path(&path).unwrap());
    let conn = Connection::open(&path).unwrap();
    // Simulate a schema-12 database produced before API identity/allocator setup.
    conn.execute(
        "DELETE FROM settings WHERE key IN (?1,?2)",
        params![HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY],
    )
    .unwrap();
    for (id, at) in [(7, 20), (900, 10)] {
        conn.execute(
            "INSERT INTO frames(id,captured_at,monitor_index,width,height,image_path,content_hash)
             VALUES(?1,?2,0,1,1,'fixture.webp',?3)",
            params![id, at, format!("fixture-{id}")],
        )
        .unwrap();
    }
    let schema_before: String = conn
        .query_row(
            "SELECT group_concat(sql, ';') FROM (SELECT sql FROM sqlite_master ORDER BY name)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    fixture_setting(&conn, "memory.source_id", "report-identity-is-not-history");
    drop(conn);

    let store = SqliteStore::open_path(&path).unwrap();
    let metadata = Store::history_readiness(&store).await.unwrap();
    assert!(traits::valid_history_id(&metadata.history_id));
    assert_ne!(metadata.history_id, "report-identity-is-not-history");
    assert_eq!(metadata.frame_id_high_water, 900);
    assert_eq!(store.get_frame(7).await.unwrap().unwrap().captured_at, 20);
    assert_eq!(store.get_frame(900).await.unwrap().unwrap().captured_at, 10);
    assert_eq!(store.insert_frame(frame(1)).await.unwrap(), 901);
    assert_eq!(store.schema_version().unwrap(), 12);
    let conn = Connection::open(&path).unwrap();
    let schema_after: String = conn
        .query_row(
            "SELECT group_concat(sql, ';') FROM (SELECT sql FROM sqlite_master ORDER BY name)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        schema_before, schema_after,
        "setup must not change schema/indexes"
    );
}

#[tokio::test]
async fn allocator_key_survives_settings_level_reset() {
    let store = SqliteStore::open_in_memory().unwrap();
    let id = store.insert_frame(frame(10)).await.unwrap();
    let metadata = Store::history_readiness(&store).await.unwrap();
    store.delete_frame(id).await.unwrap();
    // Settings-level reset means typed/named user values, not unsupported raw SQL
    // DELETE FROM settings. Test the same generic batch/delete seams used by it.
    store
        .set_settings_batch(&[
            ("api.enabled".into(), "false".into()),
            ("capture.enabled".into(), "true".into()),
        ])
        .await
        .unwrap();
    store
        .delete_settings(&["capture.event_on_click"])
        .await
        .unwrap();
    store
        .reset_report_source_id("new-report-identity")
        .await
        .unwrap();
    assert_eq!(Store::history_readiness(&store).await.unwrap(), metadata);
    assert_eq!(
        store
            .get_setting("memory.source_id")
            .await
            .unwrap()
            .as_deref(),
        Some("new-report-identity")
    );
    assert!(store.insert_frame(frame(1)).await.unwrap() > id);
}

#[tokio::test]
async fn generic_settings_cannot_mutate_reserved_metadata() {
    let store = SqliteStore::open_in_memory().unwrap();
    let metadata = Store::history_readiness(&store).await.unwrap();
    for key in [HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY] {
        assert!(store.set_setting(key, "0").await.is_err());
        assert!(store
            .set_settings_batch(&[
                ("ordinary".into(), "changed".into()),
                (key.into(), "0".into())
            ])
            .await
            .is_err());
        assert_eq!(store.get_setting("ordinary").await.unwrap(), None);
        store.set_setting("ordinary", "keep").await.unwrap();
        assert!(store.delete_settings(&["ordinary", key]).await.is_err());
        assert_eq!(
            store.get_setting("ordinary").await.unwrap().as_deref(),
            Some("keep")
        );
        store.delete_settings(&["ordinary"]).await.unwrap();
        assert_eq!(Store::history_readiness(&store).await.unwrap(), metadata);
    }
}

#[tokio::test]
async fn missing_allocator_metadata_blocks_existing_feed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db");
    let store = SqliteStore::open_path(&path).unwrap();
    let id = store.insert_frame(frame(10)).await.unwrap();
    let identity = Store::history_readiness(&store).await.unwrap().history_id;
    store.delete_frame(id).await.unwrap();
    let conn = Connection::open(&path).unwrap();
    conn.execute(
        "DELETE FROM settings WHERE key=?1",
        [FRAME_ID_HIGH_WATER_KEY],
    )
    .unwrap();
    // Readiness must not repair even on an already-open handle.
    assert!(matches!(
        Store::history_readiness(&store).await,
        Err(HistoryReadError::Unavailable(
            HistoryUnavailableReason::MissingAllocator
        ))
    ));
    assert!(store.insert_frame(frame(1)).await.is_err());
    drop(store);
    let reopened = SqliteStore::open_path(&path).unwrap();
    assert!(matches!(
        Store::history_readiness(&reopened).await,
        Err(HistoryReadError::Unavailable(
            HistoryUnavailableReason::MissingAllocator
        ))
    ));
    assert!(reopened.insert_frame(frame(1)).await.is_err());
    assert_eq!(
        reopened.get_setting(FRAME_ID_HIGH_WATER_KEY).await.unwrap(),
        None
    );
    assert_eq!(
        reopened.get_setting(HISTORY_ID_KEY).await.unwrap(),
        Some(identity)
    );
}

#[tokio::test]
async fn partial_or_invalid_metadata_fails_closed_without_repair() {
    for (identity, allocator, expected) in [
        (None, Some("10"), HistoryUnavailableReason::MissingIdentity),
        (
            Some("not-a-uuid"),
            Some("10"),
            HistoryUnavailableReason::InvalidIdentity,
        ),
        (
            Some("550e8400-e29b-41d4-a716-446655440000"),
            Some("-1"),
            HistoryUnavailableReason::InvalidAllocator,
        ),
        (
            Some("550e8400-e29b-41d4-a716-446655440000"),
            Some("9223372036854775808"),
            HistoryUnavailableReason::InvalidAllocator,
        ),
        (
            Some("550e8400-e29b-41d4-a716-446655440000"),
            Some("garbage"),
            HistoryUnavailableReason::InvalidAllocator,
        ),
        (
            Some("550e8400-e29b-41d4-a716-446655440000"),
            Some(""),
            HistoryUnavailableReason::InvalidAllocator,
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("invalid.db");
        drop(SqliteStore::open_path(&path).unwrap());
        let conn = Connection::open(&path).unwrap();
        conn.execute(
            "DELETE FROM settings WHERE key IN (?1,?2)",
            params![HISTORY_ID_KEY, FRAME_ID_HIGH_WATER_KEY],
        )
        .unwrap();
        if let Some(value) = identity {
            fixture_setting(&conn, HISTORY_ID_KEY, value);
        }
        if let Some(value) = allocator {
            fixture_setting(&conn, FRAME_ID_HIGH_WATER_KEY, value);
        }
        let store = SqliteStore::open_path(&path).unwrap();
        assert!(matches!(Store::history_readiness(&store).await,
            Err(HistoryReadError::Unavailable(reason)) if reason == expected));
        assert!(store.insert_frame(frame(1)).await.is_err());
        assert_eq!(
            store.get_setting(HISTORY_ID_KEY).await.unwrap().as_deref(),
            identity
        );
        assert_eq!(
            store
                .get_setting(FRAME_ID_HIGH_WATER_KEY)
                .await
                .unwrap()
                .as_deref(),
            allocator
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_handles_allocate_unique_increasing_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("concurrent.db");
    drop(SqliteStore::open_path(&path).unwrap());
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(4));
    let mut tasks = Vec::new();
    for worker in 0..4 {
        let store = SqliteStore::open_path(&path).unwrap();
        let barrier = barrier.clone();
        tasks.push(tokio::spawn(async move {
            barrier.wait().await;
            let mut ids = Vec::new();
            for n in 0..20 {
                ids.push(store.insert_frame(frame(worker * 100 + n)).await.unwrap());
            }
            assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
            ids
        }));
    }
    let mut all = Vec::new();
    for task in tasks {
        all.extend(task.await.unwrap());
    }
    all.sort_unstable();
    assert_eq!(all, (1..=80).collect::<Vec<i64>>());
    let store = SqliteStore::open_path(&path).unwrap();
    assert_eq!(
        Store::history_readiness(&store)
            .await
            .unwrap()
            .frame_id_high_water,
        80
    );
}

#[tokio::test]
async fn allocator_exhaustion_is_explicit_and_never_falls_back_to_random_rowids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("exhausted.db");
    let store = SqliteStore::open_path(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    fixture_setting(&conn, FRAME_ID_HIGH_WATER_KEY, &(i64::MAX - 1).to_string());
    assert_eq!(store.insert_frame(frame(10)).await.unwrap(), i64::MAX);
    store.delete_frame(i64::MAX).await.unwrap();
    drop(store);
    let store = SqliteStore::open_path(&path).unwrap();
    let error = store.insert_frame(frame(1)).await.unwrap_err();
    assert!(
        error.to_string().contains("frame ID allocator exhausted"),
        "{error:#}"
    );
    assert_eq!(
        Store::history_readiness(&store)
            .await
            .unwrap()
            .frame_id_high_water,
        i64::MAX
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM frames", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn allocation_uses_current_max_and_rolls_back_counter_on_failed_insert() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("atomic.db");
    let store = SqliteStore::open_path(&path).unwrap();
    let conn = Connection::open(&path).unwrap();
    // Unsupported direct writer: allocator still must exceed the current maximum.
    conn.execute("INSERT INTO frames(id,captured_at,monitor_index,width,height,image_path,content_hash) VALUES(500,10,0,1,1,'fixture.webp','fixture')", []).unwrap();
    assert_eq!(store.insert_frame(frame(20)).await.unwrap(), 501);
    conn.execute_batch("CREATE TRIGGER fixture_reject_frame BEFORE INSERT ON frames BEGIN SELECT RAISE(ABORT, 'fixture rejection'); END;").unwrap();
    assert!(store.insert_frame(frame(30)).await.is_err());
    assert_eq!(
        Store::history_readiness(&store)
            .await
            .unwrap()
            .frame_id_high_water,
        501
    );
    conn.execute_batch("DROP TRIGGER fixture_reject_frame;")
        .unwrap();
    assert_eq!(store.insert_frame(frame(30)).await.unwrap(), 502);
}
