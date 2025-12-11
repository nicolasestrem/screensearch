//! Example client usage of the ScreenSearch API
//!
//! Run with: cargo run --example client_usage

use reqwest::Client;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let base_url = "http://localhost:3131";

    println!("ScreenSearch API Client Example\n");

    // 1. Health Check
    println!("1. Checking server health...");
    let health = client
        .get(format!("{}/health", base_url))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    println!("   Status: {}", health["status"]);
    println!("   Frames: {}", health["frame_count"]);
    println!("   OCR Records: {}\n", health["ocr_count"]);

    // 2. Search for text
    println!("2. Searching for 'hello'...");
    let search_results = client
        .get(format!("{}/search", base_url))
        .query(&[("q", "hello"), ("limit", "5")])
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    println!(
        "   Found {} results\n",
        search_results.as_array().unwrap_or(&vec![]).len()
    );

    // 3. Get recent frames
    println!("3. Getting recent frames...");
    let frames = client
        .get(format!("{}/frames", base_url))
        .query(&[("limit", "10")])
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    println!(
        "   Retrieved {} frames\n",
        frames.as_array().unwrap_or(&vec![]).len()
    );

    // 4. Create a tag
    println!("4. Creating tag 'test-tag'...");
    let create_result = client
        .post(format!("{}/tags", base_url))
        .json(&json!({
            "tag_name": "test-tag",
            "description": "Test tag from example",
            "color": "#0088FF"
        }))
        .send()
        .await;

    match create_result {
        Ok(resp) => {
            if resp.status().is_success() {
                let tag = resp.json::<serde_json::Value>().await?;
                println!("   Created tag with ID: {}\n", tag["id"]);
            } else {
                println!("   Tag creation failed (may already exist)\n");
            }
        }
        Err(e) => println!("   Error: {}\n", e),
    }

    // 5. List all tags
    println!("5. Listing all tags...");
    let tags = client
        .get(format!("{}/tags", base_url))
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    println!(
        "   Total tags: {}\n",
        tags.as_array().unwrap_or(&vec![]).len()
    );

    // 6. Keyword search
    println!("6. Searching keywords 'error,warning'...");
    let keyword_results = client
        .get(format!("{}/search/keywords", base_url))
        .query(&[("keywords", "error,warning"), ("limit", "10")])
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    println!(
        "   Found {} keyword matches\n",
        keyword_results.as_array().unwrap_or(&vec![]).len()
    );

    // 7. Automation example (commented out to avoid unwanted actions)
    println!("7. Automation endpoints available:");
    println!("   - POST /automation/click");
    println!("   - POST /automation/type");
    println!("   - POST /automation/scroll");
    println!("   - POST /automation/press-key");
    println!("   - POST /automation/find-elements");
    println!("   - POST /automation/open-app");
    println!("   - POST /automation/open-url\n");

    /*
    // Example automation call (uncomment to use):
    let click_result = client
        .post(format!("{}/automation/click", base_url))
        .json(&json!({
            "x": 100,
            "y": 200,
            "button": "left"
        }))
        .send()
        .await?;
    println!("Click result: {:?}", click_result.status());
    */

    println!("Example completed successfully!");

    Ok(())
}
