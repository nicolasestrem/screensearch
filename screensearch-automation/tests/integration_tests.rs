//! Integration tests for UI automation

use screen_automation::*;
use std::time::Duration;

#[tokio::test]
async fn test_engine_creation() {
    let engine = AutomationEngine::new();
    assert!(engine.is_ok(), "Should be able to create automation engine");
}

#[tokio::test]
async fn test_get_root_element() {
    let engine = AutomationEngine::new().unwrap();
    let root = engine.root();
    assert!(root.is_ok(), "Should be able to get root element");
}

#[tokio::test]
async fn test_get_focused_element() {
    let engine = AutomationEngine::new().unwrap();
    let focused = engine.focused_element();
    // May fail if no element is focused, which is ok
    println!("Focused element result: {:?}", focused);
}

#[tokio::test]
async fn test_enumerate_applications() {
    let engine = AutomationEngine::new().unwrap();
    let apps = engine.applications();
    assert!(apps.is_ok(), "Should be able to enumerate applications");

    let apps = apps.unwrap();
    assert!(!apps.is_empty(), "Should have at least one application");

    // Print first few applications
    for (i, app) in apps.iter().take(5).enumerate() {
        println!(
            "App {}: {} ({})",
            i,
            app.name().unwrap_or_else(|| "Unknown".to_string()),
            app.role()
        );
    }
}

#[tokio::test]
async fn test_find_calculator() {
    let engine = AutomationEngine::new().unwrap();

    // Try to find calculator if it's running
    match engine.application("calc").await {
        Ok(app) => {
            println!("Found calculator: {:?}", app.name());
            assert!(app.name().is_some());
        }
        Err(e) => {
            println!("Calculator not running (this is ok): {}", e);
        }
    }
}

#[tokio::test]
async fn test_window_enumeration() {
    let engine = AutomationEngine::new().unwrap();
    let windows = engine.windows().enumerate();

    assert!(windows.is_ok(), "Should be able to enumerate windows");

    let windows = windows.unwrap();
    println!("Found {} windows", windows.len());

    for (i, window) in windows.iter().take(5).enumerate() {
        println!(
            "Window {}: {} - {} ({}x{} at {}, {})",
            i, window.title, window.process_name, window.width, window.height, window.x, window.y
        );
    }
}

#[tokio::test]
async fn test_get_active_window() {
    let engine = AutomationEngine::new().unwrap();
    let active = engine.windows().get_active();

    assert!(active.is_ok(), "Should be able to get active window");

    if let Ok(Some(window)) = active {
        println!("Active window: {} - {}", window.title, window.process_name);
        assert!(!window.title.is_empty());
    }
}

#[tokio::test]
async fn test_input_simulator() {
    let engine = AutomationEngine::new().unwrap();
    let input = engine.input();

    // Test that we can create the input simulator
    // We won't actually send input in tests to avoid interfering with the system
    assert!(std::mem::size_of_val(input) > 0);
}

#[tokio::test]
async fn test_selector_parsing() {
    // Test various selector formats
    let sel1 = Selector::from("#myButton");
    assert!(matches!(sel1, Selector::Id(_)));

    let sel2 = Selector::from("text:Click Me");
    assert!(matches!(sel2, Selector::Text(_)));

    let sel3 = Selector::from("button:Submit");
    assert!(matches!(sel3, Selector::Role { .. }));

    let sel4 = Selector::role("button").with_name("OK").build();
    assert!(matches!(sel4, Selector::Role { .. }));
}

#[tokio::test]
async fn test_selector_chain() {
    let chain = Selector::role("window")
        .build()
        .then(Selector::role("button").build());

    assert!(matches!(chain, Selector::Chain(_)));
}

#[tokio::test]
async fn test_element_attributes() {
    let engine = AutomationEngine::new().unwrap();
    let root = engine.root().unwrap();

    let attrs = root.attributes();
    println!("Root element role: {}", attrs.role);
    assert!(!attrs.role.is_empty());
}

#[tokio::test]
async fn test_wait_for_timeout() {
    let engine = AutomationEngine::new().unwrap();

    // This should timeout quickly since we're waiting for something that doesn't exist
    let result = engine
        .wait_for(Duration::from_millis(500), || {
            Ok(false) // Always return false to trigger timeout
        })
        .await;

    assert!(result.is_err(), "Should timeout");
    assert!(
        result.unwrap_err().is_timeout(),
        "Should be a timeout error"
    );
}

#[tokio::test]
async fn test_mouse_button_types() {
    assert_eq!(MouseButton::Left, MouseButton::Left);
    assert_ne!(MouseButton::Left, MouseButton::Right);
}

#[tokio::test]
async fn test_key_modifier_types() {
    assert_eq!(KeyModifier::Ctrl, KeyModifier::Ctrl);
    assert_ne!(KeyModifier::Ctrl, KeyModifier::Alt);
}
