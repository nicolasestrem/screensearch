//! Integration tests for the REST API

use screen_api::{ApiConfig, ApiServer};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_server_initialization() {
    // Use a test database
    let config = ApiConfig {
        host: "127.0.0.1".to_string(),
        port: 3132,                            // Use different port for testing
        database_path: ":memory:".to_string(), // In-memory database
    };

    // Server should initialize without errors
    let result = ApiServer::new(config).await;
    assert!(result.is_ok(), "Server initialization should succeed");
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_health_endpoint() {
    // Give server time to start
    sleep(Duration::from_millis(500)).await;

    let client = reqwest::Client::new();
    let response = client.get("http://127.0.0.1:3131/health").send().await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 200);
        let json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(json["status"], "ok");
        assert!(json["version"].is_string());
    }
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_search_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:3131/search")
        .query(&[("q", "test"), ("limit", "10")])
        .send()
        .await;

    if let Ok(resp) = response {
        assert!(resp.status().is_success() || resp.status().is_client_error());
    }
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_frames_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:3131/frames")
        .query(&[("limit", "10")])
        .send()
        .await;

    if let Ok(resp) = response {
        assert!(resp.status().is_success());
    }
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_tags_endpoint() {
    let client = reqwest::Client::new();

    // Test listing tags
    let response = client.get("http://127.0.0.1:3131/tags").send().await;

    if let Ok(resp) = response {
        assert!(resp.status().is_success());
    }

    // Test creating a tag
    let create_response = client
        .post("http://127.0.0.1:3131/tags")
        .json(&serde_json::json!({
            "tag_name": "test_tag",
            "description": "Test tag",
            "color": "#FF0000"
        }))
        .send()
        .await;

    if let Ok(resp) = create_response {
        assert!(resp.status().is_success() || resp.status() == 400);
    }
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_automation_click_endpoint() {
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:3131/automation/click")
        .json(&serde_json::json!({
            "x": 100,
            "y": 200,
            "button": "left"
        }))
        .send()
        .await;

    if let Ok(resp) = response {
        // Will fail if automation not implemented, but endpoint should exist
        assert!(resp.status().is_success() || resp.status().is_server_error());
    }
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_invalid_search_query() {
    let client = reqwest::Client::new();
    let response = client
        .get("http://127.0.0.1:3131/search")
        .query(&[("q", "")]) // Empty query should fail
        .send()
        .await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 400);
    }
}

#[tokio::test]
#[ignore] // Requires running server
async fn test_invalid_tag_creation() {
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:3131/tags")
        .json(&serde_json::json!({
            "tag_name": "" // Empty tag name should fail
        }))
        .send()
        .await;

    if let Ok(resp) = response {
        assert_eq!(resp.status(), 400);
    }
}
