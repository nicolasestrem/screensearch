//! Database Performance Benchmarks
//!
//! This module contains Criterion benchmarks for measuring the performance
//! of critical database operations in ScreenSearch.
//!
//! Run with: cargo bench -p screensearch-db
//!
//! Reports are generated in target/criterion/

use chrono::{Duration, Utc};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use screensearch_db::{DatabaseConfig, DatabaseManager, FrameFilter, NewFrame, Pagination};
use tempfile::TempDir;
use tokio::runtime::Runtime;

/// Create a test database with sample data
async fn setup_test_db(frame_count: usize) -> (DatabaseManager, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("bench.db");

    let config = DatabaseConfig {
        path: db_path.to_string_lossy().to_string(),
        max_connections: 10,
        min_connections: 1,
        acquire_timeout_secs: 5,
        enable_wal: true,
        cache_size_kb: -2000,
    };

    let db = DatabaseManager::with_config(config)
        .await
        .expect("Failed to create database");

    // Insert sample frames
    let base_time = Utc::now() - Duration::hours(24);
    for i in 0..frame_count {
        let frame = NewFrame {
            timestamp: base_time + Duration::seconds(i as i64 * 3),
            device_name: "monitor-1".to_string(),
            file_path: format!("/tmp/frame_{}.jpg", i),
            monitor_index: 0,
            width: 1920,
            height: 1080,
            offset_index: 0,
            chunk_id: None,
            active_window: Some(format!("Window {}", i % 10)),
            active_process: Some(format!("app_{}", i % 5)),
            browser_url: if i % 3 == 0 {
                Some(format!("https://example.com/page/{}", i))
            } else {
                None
            },
            focused: Some(i % 2 == 0),
        };

        db.insert_frame(frame)
            .await
            .expect("Failed to insert frame");

        // Add OCR text for some frames
        if i % 2 == 0 {
            let ocr = screensearch_db::NewOcrText {
                frame_id: (i + 1) as i64,
                text: format!(
                    "Sample OCR text for frame {} with searchable content benchmark test query",
                    i
                ),
                x: 0,
                y: 0,
                width: 1920,
                height: 100,
                confidence: 0.95,
                text_json: None,
            };
            let _ = db.insert_ocr_text(ocr).await;
        }
    }

    (db, temp_dir)
}

/// Benchmark: Frame retrieval by ID
fn bench_get_frame(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (db, _temp_dir) = rt.block_on(setup_test_db(1000));

    c.bench_function("get_frame_by_id", |b| {
        b.to_async(&rt).iter(|| async {
            let frame = db.get_frame(black_box(500)).await;
            black_box(frame)
        })
    });
}

/// Benchmark: Frame range queries with different dataset sizes
fn bench_frame_range_query(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("frame_range_queries");
    group.throughput(Throughput::Elements(1));

    for size in [100, 500, 1000].iter() {
        let (db, _temp_dir) = rt.block_on(setup_test_db(*size));
        let now = Utc::now();
        let start = now - Duration::hours(24);

        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, _| {
            b.to_async(&rt).iter(|| async {
                let frames = db
                    .get_frames_in_range(
                        black_box(start),
                        black_box(now),
                        FrameFilter::default(),
                        Pagination {
                            limit: 20,
                            offset: 0,
                        },
                    )
                    .await;
                black_box(frames)
            })
        });
    }
    group.finish();
}

/// Benchmark: FTS5 full-text search
fn bench_fts5_search(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (db, _temp_dir) = rt.block_on(setup_test_db(1000));

    let mut group = c.benchmark_group("fts5_search");

    // Single word search
    group.bench_function("single_word", |b| {
        b.to_async(&rt).iter(|| async {
            let results = db
                .search_ocr_text(
                    black_box("benchmark"),
                    FrameFilter::default(),
                    Pagination {
                        limit: 20,
                        offset: 0,
                    },
                )
                .await;
            black_box(results)
        })
    });

    // Multi-word search
    group.bench_function("multi_word", |b| {
        b.to_async(&rt).iter(|| async {
            let results = db
                .search_ocr_text(
                    black_box("sample searchable content"),
                    FrameFilter::default(),
                    Pagination {
                        limit: 20,
                        offset: 0,
                    },
                )
                .await;
            black_box(results)
        })
    });

    group.finish();
}

/// Benchmark: Frame insertion (write performance)
fn bench_frame_insertion(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("bench_insert.db");

    let config = DatabaseConfig {
        path: db_path.to_string_lossy().to_string(),
        max_connections: 10,
        min_connections: 1,
        acquire_timeout_secs: 5,
        enable_wal: true,
        cache_size_kb: -2000,
    };

    let db = rt
        .block_on(DatabaseManager::with_config(config))
        .expect("Failed to create db");

    let mut counter = 0i64;

    c.bench_function("insert_frame", |b| {
        b.to_async(&rt).iter(|| {
            counter += 1;
            let frame = NewFrame {
                timestamp: Utc::now(),
                device_name: "monitor-1".to_string(),
                file_path: format!("/tmp/bench_frame_{}.jpg", counter),
                monitor_index: 0,
                width: 1920,
                height: 1080,
                offset_index: 0,
                chunk_id: None,
                active_window: Some("Test Window".to_string()),
                active_process: Some("test_app".to_string()),
                browser_url: None,
                focused: Some(true),
            };
            async {
                let id = db.insert_frame(black_box(frame)).await;
                black_box(id)
            }
        })
    });
}

/// Benchmark: Vector search simulation (cosine similarity)
fn bench_cosine_similarity(c: &mut Criterion) {
    // Generate test vectors (768-dimensional like our EmbeddingGemma embeddings)
    let query_vector: Vec<f32> = (0..768).map(|i| (i as f32 * 0.001).sin()).collect();
    let db_vectors: Vec<Vec<f32>> = (0..1000)
        .map(|j| (0..768).map(|i| ((i + j) as f32 * 0.001).cos()).collect())
        .collect();

    c.bench_function("cosine_similarity_1000_vectors", |b| {
        b.iter(|| {
            let mut results: Vec<(usize, f32)> = db_vectors
                .iter()
                .enumerate()
                .map(|(idx, vec)| {
                    let dot: f32 = query_vector
                        .iter()
                        .zip(vec.iter())
                        .map(|(a, b)| a * b)
                        .sum();
                    let norm_q: f32 = query_vector.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let norm_v: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let similarity = if norm_q > 0.0 && norm_v > 0.0 {
                        dot / (norm_q * norm_v)
                    } else {
                        0.0
                    };
                    (idx, similarity)
                })
                .collect();

            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            black_box(results.into_iter().take(10).collect::<Vec<_>>())
        })
    });
}

/// Benchmark: Database statistics collection
fn bench_statistics(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let (db, _temp_dir) = rt.block_on(setup_test_db(1000));

    c.bench_function("get_statistics", |b| {
        b.to_async(&rt).iter(|| async {
            let stats = db.get_statistics().await;
            black_box(stats)
        })
    });
}

criterion_group!(
    benches,
    bench_get_frame,
    bench_frame_range_query,
    bench_fts5_search,
    bench_frame_insertion,
    bench_cosine_similarity,
    bench_statistics,
);

criterion_main!(benches);
