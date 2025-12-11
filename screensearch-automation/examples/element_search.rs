//! Element search example
//!
//! Demonstrates different selector types and element search strategies

use screen_automation::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("=== Element Search Example ===\n");

    let engine = AutomationEngine::new()?;

    // 1. Search by role
    println!("1. Searching for button elements...");
    match engine.find_elements(&Selector::role("button").build()) {
        Ok(buttons) => {
            println!("   Found {} buttons", buttons.len());
            for (i, button) in buttons.iter().take(5).enumerate() {
                println!("   {}. {:?}", i + 1, button.name());
            }
        }
        Err(e) => {
            println!("   No buttons found: {}", e);
        }
    }
    println!();

    // 2. Search by text
    println!("2. Searching for elements containing 'OK'...");
    match engine.find_elements(&Selector::text("OK")) {
        Ok(elements) => {
            println!("   Found {} elements", elements.len());
            for (i, elem) in elements.iter().take(3).enumerate() {
                println!("   {}. {:?} ({})", i + 1, elem.name(), elem.role());
            }
        }
        Err(e) => {
            println!("   No elements found: {}", e);
        }
    }
    println!();

    // 3. Search by name
    println!("3. Searching for window elements...");
    match engine.find_elements(&Selector::role("window").build()) {
        Ok(windows) => {
            println!("   Found {} windows", windows.len());
            for (i, window) in windows.iter().take(5).enumerate() {
                let name = window.name().unwrap_or_else(|| "Unnamed".to_string());
                println!("   {}. {}", i + 1, name);

                // Try to get bounds
                if let Ok((x, y, w, h)) = window.bounds() {
                    println!("      Position: ({}, {}), Size: {}x{}", x, y, w, h);
                }
            }
        }
        Err(e) => {
            println!("   No windows found: {}", e);
        }
    }
    println!();

    // 4. Search with timeout
    println!("4. Searching with timeout (will fail quickly)...");
    match engine
        .find_element_with_timeout(
            &Selector::id("nonexistent_element_12345"),
            std::time::Duration::from_secs(2),
        )
        .await
    {
        Ok(elem) => {
            println!("   Found: {:?}", elem);
        }
        Err(e) => {
            println!("   Expected error: {}", e);
        }
    }
    println!();

    // 5. Element attributes
    println!("5. Getting detailed attributes from root element...");
    let root = engine.root()?;
    let attrs = root.attributes();
    println!("   Role: {}", attrs.role);
    println!("   Label: {:?}", attrs.label);
    println!("   Value: {:?}", attrs.value);
    println!("   Description: {:?}", attrs.description);
    println!("   Properties: {} keys", attrs.properties.len());

    println!("\n=== Search example completed ===");

    Ok(())
}
