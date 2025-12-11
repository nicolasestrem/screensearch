//! Notepad automation example
//!
//! This example demonstrates:
//! - Opening an application
//! - Finding UI elements
//! - Typing text
//! - Using keyboard shortcuts

use screen_automation::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("=== Notepad Automation Example ===\n");

    let engine = AutomationEngine::new()?;

    // Open Notepad
    println!("1. Opening Notepad...");
    let notepad = engine.open_application("notepad").await?;
    println!("   Notepad opened: {:?}", notepad.name());
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Find the edit control
    println!("\n2. Finding text editor...");
    let editor = engine
        .find_element_with_timeout(&Selector::role("edit"), Duration::from_secs(5))
        .await?;
    println!("   Found editor: {:?}", editor.role());

    // Focus the editor
    println!("\n3. Focusing editor...");
    editor.focus()?;
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Type some text
    println!("\n4. Typing text...");
    let text = "Hello from Screen Memory UI Automation!\n\nThis text was typed programmatically using the Windows UIAutomation API.";
    editor.type_text(text)?;
    println!("   Text typed successfully");

    // Wait a bit
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Select all text
    println!("\n5. Selecting all text (Ctrl+A)...");
    engine.input().select_all()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Copy to clipboard
    println!("\n6. Copying to clipboard (Ctrl+C)...");
    engine.input().copy()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Undo
    println!("\n7. Undoing (Ctrl+Z)...");
    engine.input().undo()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Paste
    println!("\n8. Pasting from clipboard (Ctrl+V)...");
    engine.input().paste()?;

    println!("\n=== Automation completed ===");
    println!("Note: Please close Notepad manually (automation will not close it)");

    Ok(())
}
