//! Integration tests for the screen-db database layer
//!
//! Tests cover all major functionality including frame operations, OCR indexing,
//! full-text search, tag management, and filtering.

use chrono::{Duration, Utc};
use screensearch_db::{
    DatabaseManager, FrameFilter, NewEmbedding, NewFrame, NewOcrText, NewTag, Pagination,
};
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
async fn test_sqlite_vec_search_and_delete_sync() {
    let (db, _path) = create_test_db().await;
    let first_frame = db
        .insert_frame(create_test_frame(Utc::now(), "code", "First"))
        .await
        .unwrap();
    let second_frame = db
        .insert_frame(create_test_frame(Utc::now(), "browser", "Second"))
        .await
        .unwrap();

    let mut first_vector = vec![0.0; 768];
    first_vector[0] = 1.0;
    let mut second_vector = vec![0.0; 768];
    second_vector[1] = 1.0;

    for (frame_id, text, embedding) in [
        (first_frame, "first document", first_vector.clone()),
        (second_frame, "second document", second_vector),
    ] {
        db.insert_embedding(NewEmbedding {
            frame_id,
            chunk_text: text.to_string(),
            chunk_index: 0,
            embedding,
            provider: "test".to_string(),
            model: "test-model".to_string(),
            model_version: "1".to_string(),
            content_hash: text.to_string(),
        })
        .await
        .unwrap();
    }

    let results = db.search_embeddings(first_vector, 2, 0.0).await.unwrap();
    assert_eq!(results[0].frame.id, first_frame);
    assert_eq!(results[0].retrieval_source, "vector");

    db.delete_embeddings_for_frame(first_frame).await.unwrap();
    let mut remaining_query = vec![0.0; 768];
    remaining_query[1] = 1.0;
    let remaining = db.search_embeddings(remaining_query, 2, 0.0).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].frame.id, second_frame);

    db.close().await;
}

#[tokio::test]
async fn test_embedding_batch_replacement_is_atomic_and_deduplicated() {
    let (db, _path) = create_test_db().await;
    let frame_id = db
        .insert_frame(create_test_frame(Utc::now(), "code", "Atomic"))
        .await
        .unwrap();
    let base = NewEmbedding {
        frame_id,
        chunk_text: "existing chunk".to_string(),
        chunk_index: 0,
        embedding: vec![0.0; 768],
        provider: "test".to_string(),
        model: "test-model".to_string(),
        model_version: "1".to_string(),
        content_hash: "existing".to_string(),
    };
    db.insert_embeddings(vec![base]).await.unwrap();

    let duplicate_batch = vec![
        NewEmbedding {
            frame_id,
            chunk_text: "replacement one".to_string(),
            chunk_index: 0,
            embedding: vec![0.0; 768],
            provider: "test".to_string(),
            model: "test-model".to_string(),
            model_version: "1".to_string(),
            content_hash: "replacement-one".to_string(),
        },
        NewEmbedding {
            frame_id,
            chunk_text: "replacement duplicate".to_string(),
            chunk_index: 0,
            embedding: vec![0.0; 768],
            provider: "test".to_string(),
            model: "test-model".to_string(),
            model_version: "1".to_string(),
            content_hash: "replacement-duplicate".to_string(),
        },
    ];
    assert!(db.insert_embeddings(duplicate_batch).await.is_err());

    let stored = db.get_embeddings_for_frame(frame_id).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].chunk_text, "existing chunk");

    db.close().await;
}

#[tokio::test]
async fn test_time_filtered_vector_search_expands_past_global_neighbors() {
    let (db, _path) = create_test_db().await;
    let now = Utc::now();

    for index in 0..100 {
        let frame_id = db
            .insert_frame(create_test_frame(
                now - Duration::days(30),
                "archive",
                &format!("Out of range {index}"),
            ))
            .await
            .unwrap();
        let mut vector = vec![0.0; 768];
        vector[0] = 1.0;
        db.insert_embedding(NewEmbedding {
            frame_id,
            chunk_text: format!("global neighbor {index}"),
            chunk_index: 0,
            embedding: vector,
            provider: "test".to_string(),
            model: "test-model".to_string(),
            model_version: "1".to_string(),
            content_hash: format!("global-{index}"),
        })
        .await
        .unwrap();
    }

    let relevant_frame = db
        .insert_frame(create_test_frame(now, "code", "Relevant"))
        .await
        .unwrap();
    let mut relevant_vector = vec![0.0; 768];
    relevant_vector[0] = 0.8;
    relevant_vector[1] = 0.6;
    db.insert_embedding(NewEmbedding {
        frame_id: relevant_frame,
        chunk_text: "relevant in-range result".to_string(),
        chunk_index: 0,
        embedding: relevant_vector,
        provider: "test".to_string(),
        model: "test-model".to_string(),
        model_version: "1".to_string(),
        content_hash: "relevant".to_string(),
    })
    .await
    .unwrap();

    let mut query = vec![0.0; 768];
    query[0] = 1.0;
    let results = db
        .search_embeddings_with_time_range(
            query,
            1,
            0.0,
            Some(now - Duration::minutes(1)),
            Some(now + Duration::minutes(1)),
        )
        .await
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].frame.id, relevant_frame);

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
    let filter = FrameFilter {
        start_time: Some(wide_range.0),
        end_time: Some(wide_range.1),
        app_name: Some("chrome".to_string()),
        ..FrameFilter::default()
    };

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

/// Regression test for the vision worker infinite-loop bug: a failed analysis
/// task must NOT be left immediately re-claimable (which would peg a CPU core
/// and flood the log). After a failure the task is backed off, so the very next
/// claim returns nothing.
#[tokio::test]
async fn test_failed_analysis_task_is_not_immediately_reclaimable() {
    let (db, _path) = create_test_db().await;

    let frame_id = db
        .insert_frame(create_test_frame(Utc::now(), "code", "Will fail"))
        .await
        .unwrap();

    db.enqueue_frame_for_analysis(frame_id, 0).await.unwrap();

    // Claim and fail the task.
    let task = db
        .claim_analysis_task("worker-1")
        .await
        .unwrap()
        .expect("a task should be claimable");
    assert_eq!(task.frame_id, frame_id);

    db.fail_analysis_task(task.id, frame_id, "boom".to_string())
        .await
        .unwrap();

    // The task is backed off, not unlocked: the next claim must return None
    // instead of the same task. This is the core of the infinite-loop fix.
    let next = db.claim_analysis_task("worker-1").await.unwrap();
    assert!(
        next.is_none(),
        "a just-failed task must not be immediately re-claimable"
    );

    // It is still in the queue (one attempt < MAX_ANALYSIS_ATTEMPTS) and the
    // frame is marked failed.
    let status = db.get_vision_status().await.unwrap();
    assert_eq!(status.failed, 1);
    assert_eq!(status.queue_depth, 1);

    db.close().await;
}

/// Verify the consolidated conditional-aggregation `get_vision_status` query
/// reports correct per-status counts.
#[tokio::test]
async fn test_vision_status_counts_by_analysis_status() {
    use screensearch_db::models::FrameAnalysisUpdate;

    let (db, _path) = create_test_db().await;

    // One frame analyzed successfully -> completed.
    let completed_id = db
        .insert_frame(create_test_frame(Utc::now(), "code", "Done"))
        .await
        .unwrap();
    db.enqueue_frame_for_analysis(completed_id, 0)
        .await
        .unwrap();
    let task = db.claim_analysis_task("w").await.unwrap().unwrap();
    db.complete_analysis_task(
        task.id,
        completed_id,
        FrameAnalysisUpdate {
            description: Some("a window".to_string()),
            visible_text_json: Some("[]".to_string()),
            activity_type: Some("coding".to_string()),
            app_hint: None,
            confidence: Some(0.9),
            analysis_time_ms: Some(42),
        },
    )
    .await
    .unwrap();

    // One frame failed.
    let failed_id = db
        .insert_frame(create_test_frame(Utc::now(), "code", "Bad"))
        .await
        .unwrap();
    db.enqueue_frame_for_analysis(failed_id, 0).await.unwrap();
    let task = db.claim_analysis_task("w").await.unwrap().unwrap();
    db.fail_analysis_task(task.id, failed_id, "nope".to_string())
        .await
        .unwrap();

    // One frame still pending (enqueued, never claimed).
    let pending_id = db
        .insert_frame(create_test_frame(Utc::now(), "code", "Later"))
        .await
        .unwrap();
    db.enqueue_frame_for_analysis(pending_id, 0).await.unwrap();

    let status = db.get_vision_status().await.unwrap();
    assert_eq!(status.total_frames, 3);
    assert_eq!(status.completed, 1);
    assert_eq!(status.failed, 1);
    assert_eq!(status.pending, 1);
    assert_eq!(status.processing, 0);

    db.close().await;
}

/// When vision analysis completes with a description, a frame that was already
/// embedded (OCR only, before analysis finished) must have its embeddings cleared
/// so the background worker re-embeds it with the new description included.
#[tokio::test]
async fn test_vision_completion_clears_embeddings_for_reembedding() {
    use screensearch_db::models::FrameAnalysisUpdate;

    let (db, _path) = create_test_db().await;

    let frame_id = db
        .insert_frame(create_test_frame(Utc::now(), "design", "Canvas"))
        .await
        .unwrap();
    // OCR text makes the frame eligible for embedding.
    db.insert_ocr_text(create_test_ocr(frame_id, "some screen text"))
        .await
        .unwrap();

    // Embed it OCR-only, as the worker would before vision completes.
    db.insert_embedding(NewEmbedding {
        frame_id,
        chunk_text: "some screen text".to_string(),
        chunk_index: 0,
        embedding: vec![0.1; 768],
        provider: "fastembed".to_string(),
        model: "EmbeddingGemma-300M".to_string(),
        model_version: "1".to_string(),
        content_hash: "hash".to_string(),
    })
    .await
    .unwrap();

    // Embedded -> no longer pending.
    let pending = db.get_frames_without_embeddings(10).await.unwrap();
    assert!(
        !pending.iter().any(|f| f.id == frame_id),
        "frame should not be pending once embedded"
    );

    // Vision completes with a non-empty description.
    db.enqueue_frame_for_analysis(frame_id, 0).await.unwrap();
    let task = db.claim_analysis_task("w").await.unwrap().unwrap();
    db.complete_analysis_task(
        task.id,
        frame_id,
        FrameAnalysisUpdate {
            description: Some("a bar chart of quarterly revenue".to_string()),
            visible_text_json: Some("[]".to_string()),
            activity_type: Some("design".to_string()),
            app_hint: None,
            confidence: Some(0.8),
            analysis_time_ms: Some(10),
        },
    )
    .await
    .unwrap();

    // Embeddings cleared -> frame is re-queued for embedding.
    let pending = db.get_frames_without_embeddings(10).await.unwrap();
    assert!(
        pending.iter().any(|f| f.id == frame_id),
        "frame should be re-queued for embedding after vision adds a description"
    );

    db.close().await;
}

/// A vision completion with only `visible_text` labels (no description) must
/// still clear embeddings, since those labels are part of the embedding text.
#[tokio::test]
async fn test_vision_completion_with_labels_only_clears_embeddings() {
    use screensearch_db::models::FrameAnalysisUpdate;

    let (db, _path) = create_test_db().await;

    let frame_id = db
        .insert_frame(create_test_frame(Utc::now(), "app", "Toolbar"))
        .await
        .unwrap();
    db.insert_ocr_text(create_test_ocr(frame_id, "screen text"))
        .await
        .unwrap();
    db.insert_embedding(NewEmbedding {
        frame_id,
        chunk_text: "screen text".to_string(),
        chunk_index: 0,
        embedding: vec![0.1; 768],
        provider: "fastembed".to_string(),
        model: "EmbeddingGemma-300M".to_string(),
        model_version: "1".to_string(),
        content_hash: "hash".to_string(),
    })
    .await
    .unwrap();

    db.enqueue_frame_for_analysis(frame_id, 0).await.unwrap();
    let task = db.claim_analysis_task("w").await.unwrap().unwrap();
    db.complete_analysis_task(
        task.id,
        frame_id,
        FrameAnalysisUpdate {
            description: None,
            visible_text_json: Some(r#"["Submit","Cancel","Settings"]"#.to_string()),
            activity_type: Some("productivity".to_string()),
            app_hint: None,
            confidence: Some(0.7),
            analysis_time_ms: Some(8),
        },
    )
    .await
    .unwrap();

    let pending = db.get_frames_without_embeddings(10).await.unwrap();
    assert!(
        pending.iter().any(|f| f.id == frame_id),
        "labels-only vision completion should re-queue the frame for embedding"
    );

    db.close().await;
}

/// A frame with NO OCR text but a vision description must become embeddable
/// (the "pure chart / canvas" case the feature targets).
#[tokio::test]
async fn test_frame_without_ocr_but_with_description_is_embeddable() {
    use screensearch_db::models::FrameAnalysisUpdate;

    let (db, _path) = create_test_db().await;

    let frame_id = db
        .insert_frame(create_test_frame(Utc::now(), "figma", "Canvas"))
        .await
        .unwrap();
    // Deliberately no OCR text inserted.

    // Before vision: nothing embeddable -> not pending.
    let pending = db.get_frames_without_embeddings(10).await.unwrap();
    assert!(
        !pending.iter().any(|f| f.id == frame_id),
        "a frame with neither OCR nor vision text must not be embeddable"
    );

    // Vision adds a description.
    db.enqueue_frame_for_analysis(frame_id, 0).await.unwrap();
    let task = db.claim_analysis_task("w").await.unwrap().unwrap();
    db.complete_analysis_task(
        task.id,
        frame_id,
        FrameAnalysisUpdate {
            description: Some("a dashboard mockup with a blue bar chart".to_string()),
            visible_text_json: Some("[]".to_string()),
            activity_type: Some("design".to_string()),
            app_hint: None,
            confidence: Some(0.8),
            analysis_time_ms: Some(12),
        },
    )
    .await
    .unwrap();

    // Now embeddable purely on the vision description.
    let pending = db.get_frames_without_embeddings(10).await.unwrap();
    assert!(
        pending.iter().any(|f| f.id == frame_id),
        "a frame with a vision description should be embeddable even without OCR"
    );

    db.close().await;
}

/// A vision completion with no description must NOT wipe existing embeddings.
#[tokio::test]
async fn test_vision_completion_without_description_keeps_embeddings() {
    use screensearch_db::models::FrameAnalysisUpdate;

    let (db, _path) = create_test_db().await;

    let frame_id = db
        .insert_frame(create_test_frame(Utc::now(), "code", "Doc"))
        .await
        .unwrap();
    db.insert_ocr_text(create_test_ocr(frame_id, "screen text"))
        .await
        .unwrap();
    db.insert_embedding(NewEmbedding {
        frame_id,
        chunk_text: "screen text".to_string(),
        chunk_index: 0,
        embedding: vec![0.1; 768],
        provider: "fastembed".to_string(),
        model: "EmbeddingGemma-300M".to_string(),
        model_version: "1".to_string(),
        content_hash: "hash".to_string(),
    })
    .await
    .unwrap();

    db.enqueue_frame_for_analysis(frame_id, 0).await.unwrap();
    let task = db.claim_analysis_task("w").await.unwrap().unwrap();
    db.complete_analysis_task(
        task.id,
        frame_id,
        FrameAnalysisUpdate {
            description: None,
            visible_text_json: None,
            activity_type: None,
            app_hint: None,
            confidence: None,
            analysis_time_ms: Some(5),
        },
    )
    .await
    .unwrap();

    // Still embedded -> not pending.
    let pending = db.get_frames_without_embeddings(10).await.unwrap();
    assert!(
        !pending.iter().any(|f| f.id == frame_id),
        "embeddings must be preserved when vision adds no description"
    );

    db.close().await;
}

/// Image-embedding index: insert, KNN search, eligibility, replace, and
/// model-contract invalidation.
#[tokio::test]
async fn test_image_embedding_insert_search_and_eligibility() {
    let (db, _path) = create_test_db().await;

    let f1 = db
        .insert_frame(create_test_frame(Utc::now(), "a", "One"))
        .await
        .unwrap();
    let f2 = db
        .insert_frame(create_test_frame(Utc::now(), "b", "Two"))
        .await
        .unwrap();

    let mut v1 = vec![0.0f32; 768];
    v1[0] = 1.0;
    let mut v2 = vec![0.0f32; 768];
    v2[1] = 1.0;
    db.insert_image_embedding(
        f1,
        v1.clone(),
        "fastembed",
        "nomic-embed-vision-v1.5",
        "1",
        "h1",
    )
    .await
    .unwrap();
    db.insert_image_embedding(f2, v2, "fastembed", "nomic-embed-vision-v1.5", "1", "h2")
        .await
        .unwrap();

    // A query nearest v1 must rank frame 1 first, tagged as an image hit.
    let mut q = vec![0.0f32; 768];
    q[0] = 0.9;
    q[1] = 0.1;
    let results = db
        .search_image_embeddings_with_time_range(q, 5, 0.0, None, None)
        .await
        .unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].frame.id, f1);
    assert_eq!(results[0].retrieval_source, "image");

    // Both embedded -> not pending; a fresh frame is eligible (no OCR required).
    let pending = db.get_frames_without_image_embeddings(10).await.unwrap();
    assert!(!pending.iter().any(|f| f.id == f1 || f.id == f2));
    let f3 = db
        .insert_frame(create_test_frame(Utc::now(), "c", "Three"))
        .await
        .unwrap();
    let pending = db.get_frames_without_image_embeddings(10).await.unwrap();
    assert!(pending.iter().any(|f| f.id == f3));

    // Re-inserting replaces (one image embedding per frame).
    db.insert_image_embedding(f1, v1, "fastembed", "nomic-embed-vision-v1.5", "1", "h1b")
        .await
        .unwrap();
    let status = db.get_image_embedding_status().await.unwrap();
    assert_eq!(status.frames_with_embeddings, 2);

    // A model-contract change clears the index and flags a reindex.
    let changed = db
        .ensure_image_embedding_model("fastembed", "other-model", "1", 768)
        .await
        .unwrap();
    assert!(changed);
    let status = db.get_image_embedding_status().await.unwrap();
    assert_eq!(status.frames_with_embeddings, 0);

    db.close().await;
}

/// hybrid_search fuses the image-embedding index: a textless frame (image
/// embedding only) is retrievable when an image-query vector is supplied, and
/// absent when it is not.
#[tokio::test]
async fn test_hybrid_search_fuses_image_results() {
    let (db, _path) = create_test_db().await;

    let a = db
        .insert_frame(create_test_frame(Utc::now(), "app", "A"))
        .await
        .unwrap();
    let b = db
        .insert_frame(create_test_frame(Utc::now(), "app", "B"))
        .await
        .unwrap();

    // Frame A: OCR text + a text (EmbeddingGemma-space) embedding.
    db.insert_ocr_text(create_test_ocr(a, "alpha"))
        .await
        .unwrap();
    let mut ta = vec![0.0f32; 768];
    ta[0] = 1.0;
    db.insert_embedding(NewEmbedding {
        frame_id: a,
        chunk_text: "alpha".to_string(),
        chunk_index: 0,
        embedding: ta.clone(),
        provider: "fastembed".to_string(),
        model: "EmbeddingGemma-300M".to_string(),
        model_version: "1".to_string(),
        content_hash: "a".to_string(),
    })
    .await
    .unwrap();

    // Frame B: textless — only an image (nomic-space) embedding.
    let mut ib = vec![0.0f32; 768];
    ib[5] = 1.0;
    db.insert_image_embedding(
        b,
        ib.clone(),
        "fastembed",
        "nomic-embed-vision-v1.5",
        "1",
        "b",
    )
    .await
    .unwrap();

    let start = chrono::DateTime::<Utc>::MIN_UTC;
    // Not MAX_UTC: its leading '+' sign breaks SQLite's lexicographic timestamp
    // comparison. Production search uses `now()` for the end bound.
    let end = Utc::now() + chrono::Duration::hours(1);

    // With an image-query vector, the textless frame B is fused in.
    let results = db
        .hybrid_search("alpha", ta.clone(), Some(ib), 10, start, end)
        .await
        .unwrap();
    let ids: Vec<i64> = results.iter().map(|r| r.frame.id).collect();
    assert!(ids.contains(&a), "text-matched frame A should be present");
    assert!(ids.contains(&b), "image-only frame B should be fused in");
    let b_res = results.iter().find(|r| r.frame.id == b).unwrap();
    assert!(
        b_res.retrieval_source == "image" || b_res.retrieval_source == "hybrid",
        "frame B should be tagged as an image/hybrid hit, got {}",
        b_res.retrieval_source
    );

    // Without an image-query vector, the textless frame B does not appear.
    let results_no_image = db
        .hybrid_search("alpha", ta, None, 10, start, end)
        .await
        .unwrap();
    assert!(
        !results_no_image.iter().any(|r| r.frame.id == b),
        "frame B must not appear without an image query"
    );

    db.close().await;
}

/// The image-embedding cursor (`image_embeddings_last_processed_frame_id`) makes
/// get_frames_without_image_embeddings skip frames at or below the cursor, so a
/// frame that failed to embed is not re-fetched forever.
#[tokio::test]
async fn test_image_embedding_cursor_skips_processed_frames() {
    let (db, _path) = create_test_db().await;

    let f1 = db
        .insert_frame(create_test_frame(Utc::now(), "app", "One"))
        .await
        .unwrap();
    let f2 = db
        .insert_frame(create_test_frame(Utc::now(), "app", "Two"))
        .await
        .unwrap();
    let f3 = db
        .insert_frame(create_test_frame(Utc::now(), "app", "Three"))
        .await
        .unwrap();

    // No embeddings yet -> all three eligible.
    let pending = db.get_frames_without_image_embeddings(10).await.unwrap();
    assert_eq!(pending.len(), 3);

    // Simulate the worker advancing the cursor past f2 (e.g. f1/f2 failed to
    // embed). Only f3 should remain eligible — f1/f2 are not retried.
    db.set_metadata("image_embeddings_last_processed_frame_id", &f2.to_string())
        .await
        .unwrap();
    let pending = db.get_frames_without_image_embeddings(10).await.unwrap();
    let ids: Vec<i64> = pending.iter().map(|f| f.id).collect();
    assert_eq!(ids, vec![f3]);
    assert!(!ids.contains(&f1) && !ids.contains(&f2));

    db.close().await;
}
