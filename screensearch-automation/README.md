# Screen Automation

Complete Windows UI automation layer for ScreenSearch. Provides a Playwright-inspired API for automating desktop applications using the Windows UIAutomation API.

## Features

- **Element Location**: Find UI elements using intuitive selectors (role, name, text, ID)
- **Mouse Actions**: Click, double-click, right-click with multiple fallback strategies
- **Keyboard Input**: Type text, send key combinations, common shortcuts
- **Window Management**: Enumerate, find, and focus windows
- **Element Inspection**: Get comprehensive element attributes and properties
- **Async-First**: Built on Tokio with timeout and retry support
- **Thread-Safe**: Safe to use across threads with Arc-based synchronization

## Architecture

### Core Components

1. **AutomationEngine** (`engine.rs`): Main entry point, wraps Windows UIAutomation API
2. **Selector** (`selector.rs`): Playwright-inspired selector system for finding elements
3. **UIElement** (`element.rs`): Safe wrapper around Windows UI elements with interaction methods
4. **InputSimulator** (`input.rs`): Low-level mouse and keyboard simulation
5. **WindowManager** (`window.rs`): Window enumeration and management

### Design Patterns

- **Thread Safety**: All UIAutomation objects wrapped in Arc for thread-safe cloning
- **Error Handling**: Comprehensive error types with context
- **Retry Logic**: Built-in retry and timeout handling for element searches
- **Multiple Strategies**: Click operations try multiple approaches (direct, clickable point, bounds center)

## Usage Examples

### Basic Usage

```rust
use screensearch_automation::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let engine = AutomationEngine::new()?;

    // Find and click a button
    let button = engine.find_element(&Selector::role("button").with_name("OK")).await?;
    button.click()?;

    // Type text into an input field
    let input = engine.find_element(&Selector::role("edit")).await?;
    input.focus()?;
    input.type_text("Hello, World!")?;

    Ok(())
}
```

### Selector Types

```rust
// By role
Selector::role("button").with_name("Submit")

// By automation ID
Selector::id("btnSubmit")

// By text content
Selector::text("Click Here")

// By name
Selector::name("OK Button")

// String parsing
Selector::from("#myButton")  // ID selector
Selector::from("text:Login")  // Text selector
Selector::from("button:OK")   // Role with name
```

### Window Management

```rust
let engine = AutomationEngine::new()?;

// Enumerate all windows
let windows = engine.windows().enumerate()?;

// Find window by title
let notepad_windows = engine.windows().find_by_title("Notepad")?;

// Get active window
if let Some(active) = engine.windows().get_active()? {
    println!("Active: {} - {}", active.title, active.process_name);
}

// Focus a window
engine.windows().focus_window(window.handle)?;
```

### Input Simulation

```rust
let input = engine.input();

// Click at coordinates
input.click_at(100, 200, MouseButton::Left)?;

// Type text
input.type_text("Hello")?;

// Keyboard shortcuts
input.ctrl_key("c")?;  // Ctrl+C
input.alt_key("f4")?;  // Alt+F4
input.send_keys("^+s")?;  // Ctrl+Shift+S

// Common operations
input.copy()?;
input.paste()?;
input.save()?;
```

### Element Inspection

```rust
let element = engine.find_element(&Selector::role("window")).await?;

// Get basic properties
let id = element.id();
let role = element.role();
let name = element.name();

// Get bounds
let (x, y, width, height) = element.bounds()?;

// Get comprehensive attributes
let attrs = element.attributes();
println!("Role: {}", attrs.role);
println!("Label: {:?}", attrs.label);
println!("Value: {:?}", attrs.value);

// Check state
let enabled = element.is_enabled()?;
let visible = element.is_visible()?;

// Navigate tree
let children = element.children()?;
let parent = element.parent()?;
```

### Wait and Retry

```rust
// Wait for element to appear (default 30s timeout)
let button = engine.find_element(&Selector::text("OK")).await?;

// Custom timeout
let button = engine.find_element_with_timeout(
    &Selector::role("button"),
    Duration::from_secs(10)
).await?;

// Wait for custom condition
engine.wait_for(Duration::from_secs(5), || {
    Ok(some_condition())
}).await?;
```

## Running Examples

```bash
# Basic usage
cargo run --example basic_usage

# Notepad automation (will open Notepad)
cargo run --example notepad_automation

# Element search
cargo run --example element_search

# Mouse and keyboard (requires active text editor)
cargo run --example mouse_keyboard
```

## Running Tests

```bash
# Run all tests
cargo test

# Run integration tests
cargo test --test integration_tests

# Run with output
cargo test -- --nocapture
```

## Implementation Details

### Thread Safety

All UIAutomation objects are wrapped in `Arc` for safe sharing across threads:

```rust
pub struct ThreadSafeAutomation(pub Arc<UIAutomation>);
unsafe impl Send for ThreadSafeAutomation {}
unsafe impl Sync for ThreadSafeAutomation {}
```

### Click Strategies

The `UIElement::click()` method tries multiple strategies in order:

1. **Direct UIAutomation click**: Fast but may not work for all elements
2. **Clickable point**: Uses element's designated clickable point
3. **Bounds center**: Falls back to clicking center of bounding rectangle

### Error Handling

Custom error types with context:

```rust
pub enum AutomationError {
    ElementNotFound(String),
    Timeout { operation: String, timeout_ms: u64 },
    PlatformError(String),
    UnsupportedOperation(String),
    InvalidArgument(String),
    InvalidState(String),
    // ...
}
```

### Role Mapping

Generic role names are mapped to Windows ControlType:

- `"button"` → `ControlType::Button`
- `"edit"`, `"input"`, `"textfield"` → `ControlType::Edit`
- `"window"`, `"dialog"` → `ControlType::Window`
- And many more...

## Integration with ScreenSearch

This module is designed to integrate with the ScreenSearch REST API:

- **Element Finding**: `/automation/find-elements` endpoint
- **Click Actions**: `/automation/click` endpoint
- **Keyboard Input**: `/automation/type`, `/automation/press-key` endpoints
- **Window Management**: `/automation/list-elements` endpoint

## Performance Considerations

- **Element Caching**: Elements use cached children when available
- **Timeout Configuration**: Default 30s for element searches, configurable per operation
- **Retry Interval**: 100ms between retries when waiting for elements
- **Matcher Depth**: UIAutomation matcher searches to depth 10 by default

## Windows UIAutomation Patterns

The module supports various UIAutomation patterns:

- **Invoke Pattern**: For clickable elements
- **Value Pattern**: For input fields and editable controls
- **Toggle Pattern**: For checkboxes and toggles
- **Scroll Pattern**: For scrollable containers
- **Expand/Collapse Pattern**: For tree nodes and expandable items

## Requirements

- Windows 7 or later
- Visual Studio Build Tools (for compilation)
- UIAutomation API (built into Windows)

## References

- [Windows UIAutomation Documentation](https://docs.microsoft.com/en-us/windows/win32/winauto/uiauto-uiautomation)
- [uiautomation crate](https://docs.rs/uiautomation)
- [Screenpipe operator module](https://github.com/mediar-ai/screenpipe) (reference implementation)
