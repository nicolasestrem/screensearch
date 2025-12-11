//! Basic usage example for screen-automation

use screen_automation::*;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    println!("=== Screen Automation Basic Usage Example ===\n");

    // Create automation engine
    println!("1. Creating automation engine...");
    let engine = AutomationEngine::new()?;
    println!("   Engine created successfully\n");

    // Get root element
    println!("2. Getting root element (desktop)...");
    let root = engine.root()?;
    println!("   Root element: {:?}\n", root.role());

    // Get all applications
    println!("3. Enumerating applications...");
    let apps = engine.applications()?;
    println!("   Found {} applications:", apps.len());
    for (i, app) in apps.iter().take(5).enumerate() {
        let name = app.name().unwrap_or_else(|| "Unknown".to_string());
        println!("   {}. {} ({})", i + 1, name, app.role());
    }
    println!();

    // Get focused element
    println!("4. Getting focused element...");
    match engine.focused_element() {
        Ok(elem) => {
            println!("   Focused: {:?}", elem.name());
        }
        Err(e) => {
            println!("   No focused element: {}", e);
        }
    }
    println!();

    // Window enumeration
    println!("5. Enumerating windows...");
    let windows = engine.windows().enumerate()?;
    println!("   Found {} windows:", windows.len());
    for (i, window) in windows.iter().take(5).enumerate() {
        println!(
            "   {}. {} - {} ({}x{} pixels)",
            i + 1,
            window.title,
            window.process_name,
            window.width,
            window.height
        );
    }
    println!();

    // Get active window
    println!("6. Getting active window...");
    if let Some(window) = engine.windows().get_active()? {
        println!("   Active: {} - {}", window.title, window.process_name);
        println!("   Position: ({}, {})", window.x, window.y);
        println!("   Size: {}x{}", window.width, window.height);
        println!("   Minimized: {}", window.is_minimized);
    } else {
        println!("   No active window");
    }

    println!("\n=== Example completed successfully ===");

    Ok(())
}
