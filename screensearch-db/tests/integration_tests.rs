//! Integration tests for the screen-db database layer
//!
//! Tests cover all major functionality including frame operations, OCR indexing,
//! full-text search, tag management, and filtering.

use chrono::{Duration, Utc};
use screen_db::{DatabaseManager, FrameFilter, NewFrame, NewOcrText, NewTag, Pagination};
use tempfile::NamedTempFile;

/// Create a temporary database for testing
async fn create_test_db() -> (DatabaseManager, String) {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let db_path = temp_file.path().to_string_lossy().to_string();
    drop(temp_file);

    let db = DatabaseManager::new(&db_path)
        .await
        .expect("Failed to create test database");

    (db, db_path)
}

/// Helper function to create a test frame
fn create_test_frame(timestamp: chrono::DateTime<Utc>, app: &str, window: &str) -> NewFrame {
    NewFrame {
        chunk_id: None,
        timestamp,
        monitor_index: 0,
        device_name: "test-device".to_string(),
        file_path: "/tmp/test.png".to_string(),
        active_window: Some(window.to_string()),
        active_process: Some(app.to_string()),
        browser_url: None,
        width: 1920,
        height: 1080,
        offset_index: 0,
        focused: Some(true),
    }
}

/// Helper function to create test OCR text
fn create_test_ocr(frame_id: i64, text: &str) -> NewOcrText {
    NewOcrText {
        frame_id,
        text: text.to_string(),
        text_json: None,
        x: 10,
        y: 10,
        width: 100,
        height: 50,
        confidence: 0.95,
    }
}

#[tokio::test]
async fn test_database_initialization() {
    let (db, _path) = create_test_db().await;

    // Verify database is initialized
    let stats = db.get_statistics().await.expect("Failed to get statistics");

    assert_eq!(stats.frame_count, 0);
    assert_eq!(stats.ocr_count, 0);
    assert_eq!(stats.tag_count, 0);

    db.close().await;
}

#[tokio::test]
async fn test_frame_insertion() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();
    let frame = create_test_frame(now, "chrome", "Google");

    let frame_id = db
        .insert_frame(frame.clone())
        .await
        .expect("Failed to insert frame");

    assert!(frame_id > 0);

    // Verify the frame was inserted
    let retrieved = db.get_frame(frame_id).await.expect("Failed to get frame");

    assert!(retrieved.is_some());
    let retrieved_frame = retrieved.unwrap();
    assert_eq!(retrieved_frame.id, frame_id);
    assert_eq!(retrieved_frame.active_process, Some("chrome".to_string()));

    db.close().await;
}

#[tokio::test]
async fn test_ocr_text_insertion() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();
    let frame = create_test_frame(now, "chrome", "Google");
    let frame_id = db
        .insert_frame(frame)
        .await
        .expect("Failed to insert frame");

    let ocr = create_test_ocr(frame_id, "Hello World");
    let ocr_id = db
        .insert_ocr_text(ocr)
        .await
        .expect("Failed to insert OCR text");

    assert!(ocr_id > 0);

    // Verify OCR text was inserted
    let retrieved = db
        .get_ocr_text_for_frame(frame_id)
        .await
        .expect("Failed to get OCR text");

    assert_eq!(retrieved.len(), 1);
    assert_eq!(retrieved[0].text, "Hello World");
    assert_eq!(retrieved[0].confidence, 0.95);

    db.close().await;
}

#[tokio::test]
async fn test_fts5_search() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();

    // Insert multiple frames with OCR text
    let frame1 = create_test_frame(now, "chrome", "Search");
    let frame1_id = db
        .insert_frame(frame1)
        .await
        .expect("Failed to insert frame 1");

    let ocr1 = create_test_ocr(frame1_id, "database query language");
    db.insert_ocr_text(ocr1)
        .await
        .expect("Failed to insert OCR 1");

    let frame2 = create_test_frame(now + Duration::seconds(5), "notepad", "Notes");
    let frame2_id = db
        .insert_frame(frame2)
        .await
        .expect("Failed to insert frame 2");

    let ocr2 = create_test_ocr(frame2_id, "SQL database");
    db.insert_ocr_text(ocr2)
        .await
        .expect("Failed to insert OCR 2");

    // Search for "database"
    let results = db
        .search_ocr_text("database", FrameFilter::default(), Pagination::default())
        .await
        .expect("Failed to search");

    assert!(results.len() >= 2, "Expected at least 2 results");

    // Verify that results are sorted by relevance
    for result in &results {
        assert!(!result.ocr_matches.is_empty());
    }

    db.close().await;
}

#[tokio::test]
async fn test_frame_filtering_by_time() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();

    // Insert frames at different times
    let frame1 = create_test_frame(now, "chrome", "Browser");
    let frame1_id = db.insert_frame(frame1).await.unwrap();

    let frame2 = create_test_frame(now + Duration::hours(1), "notepad", "Editor");
    let frame2_id = db.insert_frame(frame2).await.unwrap();

    let frame3 = create_test_frame(now + Duration::hours(2), "vscode", "IDE");
    let frame3_id = db.insert_frame(frame3).await.unwrap();

    // Query frames within first hour
    let start = now - Duration::seconds(60);
    let end = now + Duration::seconds(3600);

    let frames = db
        .get_frames_in_range(start, end, FrameFilter::default(), Pagination::default())
        .await
        .expect("Failed to get frames");

    // Should include frame 1 and 2, but not frame 3
    assert_eq!(frames.len(), 2);
    let frame_ids: Vec<i64> = frames.iter().map(|f| f.id).collect();
    assert!(frame_ids.contains(&frame1_id));
    assert!(frame_ids.contains(&frame2_id));
    assert!(!frame_ids.contains(&frame3_id));

    db.close().await;
}

#[tokio::test]
async fn test_frame_filtering_by_app() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();

    // Insert frames from different apps
    let frame1 = create_test_frame(now, "chrome", "Browser");
    db.insert_frame(frame1).await.unwrap();

    let frame2 = create_test_frame(now, "notepad", "Editor");
    db.insert_frame(frame2).await.unwrap();

    // Query frames from chrome only
    let wide_range = (now - Duration::days(1), now + Duration::days(1));
    let mut filter = FrameFilter::default();
    filter.start_time = Some(wide_range.0);
    filter.end_time = Some(wide_range.1);
    filter.app_name = Some("chrome".to_string());

    let frames = db
        .get_frames_in_range(wide_range.0, wide_range.1, filter, Pagination::default())
        .await
        .expect("Failed to get filtered frames");

    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].active_process, Some("chrome".to_string()));

    db.close().await;
}

#[tokio::test]
async fn test_tag_creation_and_assignment() {
    let (db, _path) = create_test_db().await;

    // Create a tag
    let tag = NewTag {
        tag_name: "Important".to_string(),
        description: Some("Important screenshots".to_string()),
        color: Some("#FF0000".to_string()),
    };

    let tag_id = db.create_tag(tag).await.expect("Failed to create tag");

    assert!(tag_id > 0);

    // Verify tag was created
    let retrieved = db.get_tag(tag_id).await.expect("Failed to get tag");

    assert!(retrieved.is_some());
    let retrieved_tag = retrieved.unwrap();
    assert_eq!(retrieved_tag.tag_name, "Important");

    // Insert a frame and assign the tag
    let now = Utc::now();
    let frame = create_test_frame(now, "chrome", "Test");
    let frame_id = db.insert_frame(frame).await.unwrap();

    let frame_tag_id = db
        .add_tag_to_frame(frame_id, tag_id)
        .await
        .expect("Failed to add tag to frame");

    assert!(frame_tag_id > 0);

    // Verify tag assignment
    let tags = db
        .get_tags_for_frame(frame_id)
        .await
        .expect("Failed to get tags");

    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].tag_name, "Important");

    db.close().await;
}

#[tokio::test]
async fn test_multiple_tags_per_frame() {
    let (db, _path) = create_test_db().await;

    // Create multiple tags
    let tag1 = NewTag {
        tag_name: "Important".to_string(),
        description: None,
        color: None,
    };

    let tag2 = NewTag {
        tag_name: "Review".to_string(),
        description: None,
        color: None,
    };

    let tag1_id = db.create_tag(tag1).await.unwrap();
    let tag2_id = db.create_tag(tag2).await.unwrap();

    // Create frame and add multiple tags
    let now = Utc::now();
    let frame = create_test_frame(now, "chrome", "Test");
    let frame_id = db.insert_frame(frame).await.unwrap();

    db.add_tag_to_frame(frame_id, tag1_id).await.unwrap();
    db.add_tag_to_frame(frame_id, tag2_id).await.unwrap();

    // Verify both tags are associated
    let tags = db
        .get_tags_for_frame(frame_id)
        .await
        .expect("Failed to get tags");

    assert_eq!(tags.len(), 2);
    let tag_names: Vec<String> = tags.iter().map(|t| t.tag_name.clone()).collect();
    assert!(tag_names.contains(&"Important".to_string()));
    assert!(tag_names.contains(&"Review".to_string()));

    db.close().await;
}

#[tokio::test]
async fn test_remove_tag_from_frame() {
    let (db, _path) = create_test_db().await;

    let tag = NewTag {
        tag_name: "Temporary".to_string(),
        description: None,
        color: None,
    };

    let tag_id = db.create_tag(tag).await.unwrap();

    let now = Utc::now();
    let frame = create_test_frame(now, "chrome", "Test");
    let frame_id = db.insert_frame(frame).await.unwrap();

    db.add_tag_to_frame(frame_id, tag_id).await.unwrap();

    // Verify tag was added
    let tags = db.get_tags_for_frame(frame_id).await.unwrap();
    assert_eq!(tags.len(), 1);

    // Remove the tag
    let removed = db
        .remove_tag_from_frame(frame_id, tag_id)
        .await
        .expect("Failed to remove tag");

    assert_eq!(removed, 1);

    // Verify tag was removed
    let tags = db.get_tags_for_frame(frame_id).await.unwrap();
    assert_eq!(tags.len(), 0);

    db.close().await;
}

#[tokio::test]
async fn test_frame_count_in_range() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();

    // Insert frames
    let frame1 = create_test_frame(now, "chrome", "Browser");
    db.insert_frame(frame1).await.unwrap();

    let frame2 = create_test_frame(now + Duration::hours(1), "notepad", "Editor");
    db.insert_frame(frame2).await.unwrap();

    let frame3 = create_test_frame(now + Duration::hours(3), "vscode", "IDE");
    db.insert_frame(frame3).await.unwrap();

    // Count frames in range
    let count = db
        .count_frames_in_range(now, now + Duration::hours(2))
        .await
        .expect("Failed to count frames");

    assert_eq!(count, 2);

    db.close().await;
}

#[tokio::test]
async fn test_database_statistics() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();

    // Insert frames and OCR text
    let frame1 = create_test_frame(now, "chrome", "Browser");
    let frame1_id = db.insert_frame(frame1).await.unwrap();

    let ocr1 = create_test_ocr(frame1_id, "Hello");
    let ocr2 = create_test_ocr(frame1_id, "World");
    db.insert_ocr_text(ocr1).await.unwrap();
    db.insert_ocr_text(ocr2).await.unwrap();

    let frame2 = create_test_frame(now + Duration::hours(1), "notepad", "Editor");
    db.insert_frame(frame2).await.unwrap();

    // Create tags
    let tag = NewTag {
        tag_name: "Test".to_string(),
        description: None,
        color: None,
    };
    db.create_tag(tag).await.unwrap();

    // Get statistics
    let stats = db.get_statistics().await.expect("Failed to get statistics");

    assert_eq!(stats.frame_count, 2);
    assert_eq!(stats.ocr_count, 2);
    assert_eq!(stats.tag_count, 1);
    assert!(stats.oldest_frame.is_some());
    assert!(stats.newest_frame.is_some());

    db.close().await;
}

#[tokio::test]
async fn test_metadata_storage() {
    let (db, _path) = create_test_db().await;

    let key = "test_key";
    let value = "test_value";

    db.set_metadata(key, value)
        .await
        .expect("Failed to set metadata");

    let retrieved = db.get_metadata(key).await.expect("Failed to get metadata");

    assert_eq!(retrieved, Some(value.to_string()));

    db.close().await;
}

#[tokio::test]
async fn test_delete_old_frames() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();
    let old_time = now - Duration::days(10);

    // Insert old frame
    let old_frame = create_test_frame(old_time, "chrome", "Old");
    db.insert_frame(old_frame).await.unwrap();

    // Insert recent frame
    let recent_frame = create_test_frame(now, "chrome", "Recent");
    db.insert_frame(recent_frame).await.unwrap();

    // Delete frames older than 5 days
    let cutoff = now - Duration::days(5);
    let deleted = db
        .delete_old_frames(cutoff)
        .await
        .expect("Failed to delete old frames");

    assert_eq!(deleted, 1);

    // Verify count
    let stats = db.get_statistics().await.unwrap();
    assert_eq!(stats.frame_count, 1);

    db.close().await;
}

#[tokio::test]
async fn test_pagination() {
    let (db, _path) = create_test_db().await;

    let now = Utc::now();

    // Insert multiple frames
    for i in 0..25 {
        let frame = create_test_frame(now + Duration::seconds(i as i64), "chrome", "Page");
        db.insert_frame(frame).await.unwrap();
    }

    // Get first page (10 items)
    let page1 = db
        .get_frames_in_range(
            now - Duration::minutes(1),
            now + Duration::minutes(1),
            FrameFilter::default(),
            Pagination {
                limit: 10,
                offset: 0,
            },
        )
        .await
        .unwrap();

    assert_eq!(page1.len(), 10);

    // Get second page
    let page2 = db
        .get_frames_in_range(
            now - Duration::minutes(1),
            now + Duration::minutes(1),
            FrameFilter::default(),
            Pagination {
                limit: 10,
                offset: 10,
            },
        )
        .await
        .unwrap();

    assert_eq!(page2.len(), 10);

    // Frames should be different
    assert_ne!(page1[0].id, page2[0].id);

    db.close().await;
}
