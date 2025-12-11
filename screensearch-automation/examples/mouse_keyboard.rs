//! Mouse and keyboard input example
//!
//! Demonstrates low-level input simulation

use screen_automation::*;
use std::time::Duration;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("=== Mouse and Keyboard Input Example ===\n");

    let engine = AutomationEngine::new()?;
    let input = engine.input();

    println!("WARNING: This example will simulate keyboard input.");
    println!("You have 3 seconds to focus a text editor or cancel...\n");
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Type some text
    println!("1. Typing text...");
    input.type_text("Hello from Screen Automation!")?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Press Enter
    println!("2. Pressing Enter...");
    input.press_enter()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Type more text
    println!("3. Typing more text...");
    input.type_text("This is line 2.")?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Select all
    println!("4. Selecting all (Ctrl+A)...");
    input.select_all()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Copy
    println!("5. Copying (Ctrl+C)...");
    input.copy()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Press Delete
    println!("6. Deleting selected text...");
    input.press_delete()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Paste
    println!("7. Pasting (Ctrl+V)...");
    input.paste()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Undo
    println!("8. Undoing (Ctrl+Z)...");
    input.undo()?;
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Redo
    println!("9. Redoing (Ctrl+Y)...");
    input.redo()?;

    println!("\n=== Input simulation completed ===");

    Ok(())
}
