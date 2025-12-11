# Testing Guide - Screen Memory

This guide covers the testing infrastructure, conventions, and practices for the Screen Memory project.

## Test Overview

Screen Memory has a comprehensive test suite covering all major components:

- **Total Tests**: 59 tests (100% passing)
- **Test Framework**: `tokio::test` for async tests, standard `#[test]` for synchronous
- **Coverage**: Unit tests, integration tests, and end-to-end tests
- **Database Testing**: Temporary databases with `tempfile` crate
- **API Testing**: Mock requests and database integration tests

## Quick Reference

### Running Tests

```powershell
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p screen-db
cargo test -p screen-capture
cargo test -p screen-api
cargo test -p screen-automation

# Run with console output (see println! statements)
cargo test -- --nocapture

# Run a specific test by name
cargo test test_full_pipeline

# Run tests in a specific file
cargo test --test integration_tests

# Run integration tests only
cargo test --test '*'

# Run tests with logging enabled
$env:RUST_LOG="debug"
cargo test -- --nocapture
```

### Test Execution Tips

```powershell
# Run tests in parallel (default)
cargo test

# Run tests sequentially (useful for debugging)
cargo test -- --test-threads=1

# Show test execution time
cargo test -- --nocapture --test-threads=1

# Stop at first failure
cargo test -- --fail-fast
```

## Test Coverage by Crate

### screen-db (16 tests)

Database operations, FTS5 search, filtering, and tag management.

**Tests**:
- `test_database_initialization` - Verify database creates with correct schema
- `test_frame_insertion` - Insert and retrieve frames
- `test_ocr_text_insertion` - Insert OCR text and link to frames
- `test_fts5_search` - Full-text search across OCR content
- `test_frame_filtering_by_time` - Time range queries
- `test_frame_filtering_by_app` - Application-specific filtering
- `test_tag_creation_and_assignment` - Tag CRUD operations
- `test_multiple_tags_per_frame` - Multiple tag associations
- `test_remove_tag_from_frame` - Tag removal
- `test_frame_count_in_range` - Count queries
- `test_database_statistics` - Statistics aggregation
- `test_metadata_storage` - Key-value metadata
- `test_delete_old_frames` - Cleanup operations
- `test_pagination` - Paginated result retrieval

**Location**: `\path\to\app\Screen Memory\screen-db\tests\integration_tests.rs`

**Run**:
```powershell
cargo test -p screen-db
```

### screen-capture (33 tests)

Frame differencing, OCR processing, metrics collection, and window tracking.

**Test Modules** (embedded in source files):
- `capture.rs` - Capture engine tests (4 tests)
- `frame_diff.rs` - Frame differencing algorithms (2 tests)
- `ocr.rs` - OCR engine integration (10 tests)
- `ocr_processor.rs` - OCR processing pipeline (9 tests)
- `monitor.rs` - Monitor enumeration (4 tests)
- `window_context.rs` - Window tracking (2 tests)
- `lib.rs` - Public API tests (2 tests)

**Run**:
```powershell
cargo test -p screen-capture
```

### screen-api (11 tests)

REST API endpoints, request validation, and error handling.

**Tests**:
- `test_server_initialization` - API server startup
- `test_health_endpoint` - Health check endpoint (requires running server)
- `test_search_endpoint` - Search API (requires running server)
- `test_frames_endpoint` - Frame retrieval API
- `test_tags_endpoint` - Tag management API
- `test_automation_click_endpoint` - Automation API
- `test_invalid_search_query` - Error handling for bad queries
- `test_invalid_tag_creation` - Validation testing

**Note**: Tests marked with `#[ignore]` require a running server on port 3131.

**Location**: `\path\to\app\Screen Memory\screen-api\tests\integration_tests.rs`

**Run**:
```powershell
# Run non-ignored tests
cargo test -p screen-api

# Run ignored tests (requires server)
cargo test -p screen-api -- --ignored
```

### screen-automation (13 tests)

UI automation, element selection, and input simulation.

**Tests**:
- `test_engine_creation` - Automation engine initialization
- `test_get_root_element` - Root element access
- `test_get_focused_element` - Focus detection
- `test_enumerate_applications` - Application enumeration
- `test_find_calculator` - Application-specific searches
- `test_window_enumeration` - Window listing
- `test_get_active_window` - Active window detection
- `test_input_simulator` - Input system initialization
- `test_selector_parsing` - Selector syntax validation
- `test_selector_chain` - Chained selectors
- `test_element_attributes` - Element property access
- `test_wait_for_timeout` - Timeout handling
- `test_mouse_button_types` - Mouse button enums
- `test_key_modifier_types` - Keyboard modifier enums

**Location**: `\path\to\app\Screen Memory\screen-automation\tests\integration_tests.rs`

**Run**:
```powershell
cargo test -p screen-automation
```

## Integration Tests

Integration tests verify that multiple components work together correctly.

### End-to-End Tests

**File**: `\path\to\app\Screen Memory\tests\integration\test_end_to_end.rs`

**Tests**:
- `test_full_pipeline` - Complete capture → OCR → database → storage flow
- `test_database_query_pipeline` - Insert frames and query with filters

**Description**: These tests simulate the complete application workflow, from screen capture through OCR processing to database storage and retrieval.

**Run**:
```powershell
cargo test --test test_end_to_end
```

### API Integration Tests

**File**: `\path\to\app\Screen Memory\tests\integration\test_api_integration.rs`

**Tests**:
- `test_health_endpoint` - Basic API health check
- `test_search_endpoint` - Search functionality
- `test_frames_endpoint` - Frame retrieval
- `test_tags_operations` - Complete tag lifecycle
- `test_database_statistics` - Statistics queries

**Description**: Tests API endpoints with real database operations (mocked HTTP layer).

**Run**:
```powershell
cargo test --test test_api_integration
```

## Writing New Tests

### Test Structure Pattern

#### Unit Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_differencing() {
        // Arrange
        let differ = FrameDiffer::new(0.006);
        let image1 = create_test_image(100, 100);
        let image2 = create_test_image(100, 100);

        // Act
        let difference = differ.calculate_difference(&image1, &image2);

        // Assert
        assert!(difference < 0.01, "Images should be identical");
    }
}
```

#### Async Test Example

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ocr_processing() {
        // Arrange
        let engine = OcrEngine::new().await.unwrap();
        let image = load_test_image("test_data/sample.png");

        // Act
        let result = engine.process_image(&image).await.unwrap();

        // Assert
        assert!(!result.text.is_empty());
        assert!(result.confidence > 0.5);
    }
}
```

### Test Database Setup

Always use temporary databases to avoid conflicts and ensure cleanup:

```rust
use tempfile::NamedTempFile;
use screen_db::DatabaseManager;

async fn create_test_db() -> (DatabaseManager, String) {
    let temp_file = NamedTempFile::new().expect("Failed to create temp file");
    let db_path = temp_file.path().to_string_lossy().to_string();
    drop(temp_file); // Release file handle

    let db = DatabaseManager::new(&db_path)
        .await
        .expect("Failed to create test database");

    (db, db_path)
}

#[tokio::test]
async fn test_my_feature() {
    let (db, db_path) = create_test_db().await;

    // Your test code here

    // Cleanup
    db.close().await;
    std::fs::remove_file(&db_path).ok();
}
```

### Helper Functions

Create helper functions for common test data:

```rust
fn create_test_frame(timestamp: DateTime<Utc>, app: &str) -> NewFrame {
    NewFrame {
        timestamp,
        device_name: "test-device".to_string(),
        file_path: "/tmp/test.png".to_string(),
        monitor_index: 0,
        width: 1920,
        height: 1080,
        active_process: Some(app.to_string()),
        // ... other fields
    }
}

fn create_test_ocr(frame_id: i64, text: &str) -> NewOcrText {
    NewOcrText {
        frame_id,
        text: text.to_string(),
        confidence: 0.95,
        // ... other fields
    }
}
```

## Performance Testing

### Benchmarks

Reference benchmarks are located in the `Inspiration/screenpipe-db/benches/` directory:

- `db_benchmarks.rs` - Database operation benchmarks
- `new_db_benchmark.rs` - New database API benchmarks

### Running Benchmarks

```powershell
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench db_benchmarks

# Save baseline for comparison
cargo bench -- --save-baseline my-baseline

# Compare against baseline
cargo bench -- --baseline my-baseline
```

### Performance Test Guidelines

1. **Measure with realistic data volumes**: Test with 100k+ frames
2. **Test concurrent operations**: Multiple readers/writers
3. **Monitor resource usage**: Use Task Manager or Performance Monitor
4. **Profile slow tests**: Use `cargo flamegraph` for bottleneck identification

## Testing Best Practices

### Do's

- Write tests for all public APIs
- Test both success and error paths
- Use descriptive test names: `test_<feature>_<scenario>_<expected_result>`
- Clean up resources (databases, files) after tests
- Use `#[tokio::test]` for async functions
- Test edge cases and boundary conditions
- Mock external dependencies when possible

### Don'ts

- Don't use hardcoded file paths (use tempfile)
- Don't rely on specific timing (use timeouts for async operations)
- Don't ignore test failures
- Don't write tests that depend on external services
- Don't use production databases in tests
- Don't commit test databases to version control

## Continuous Integration

### Pre-commit Checks

Before committing code, run:

```powershell
# Format code
cargo fmt

# Run linter
cargo clippy

# Run all tests
cargo test

# Check for compilation errors
cargo check --all-targets
```

### CI Pipeline

The project uses automated testing on all pull requests:

1. Code formatting check (`cargo fmt --check`)
2. Linter checks (`cargo clippy`)
3. All tests (`cargo test --all`)
4. Build verification (`cargo build --release`)

## Troubleshooting Tests

### Common Issues

**Issue**: Test database already exists
```powershell
# Solution: Clean up test databases
Remove-Item test_*.db
```

**Issue**: Tests fail intermittently
```powershell
# Solution: Run tests sequentially
cargo test -- --test-threads=1
```

**Issue**: OCR tests fail
```
Solution: Ensure Windows OCR language pack is installed
Settings → Language → English → Options → Download
```

**Issue**: Port already in use (API tests)
```
Solution: Change test port or stop running server
```

### Debug Logging

Enable detailed logging during tests:

```powershell
# All logs
$env:RUST_LOG="trace"
cargo test -- --nocapture

# Specific module
$env:RUST_LOG="screen_capture::ocr=debug"
cargo test -- --nocapture
```

## Resources

- [Rust Testing Documentation](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Tokio Testing Guide](https://tokio.rs/tokio/topics/testing)
- [SQLx Testing](https://github.com/launchbadge/sqlx#testing)

---

For questions or issues with tests, consult the [DEVELOPMENT.md](../DEVELOPMENT.md) guide or open an issue on GitHub.
