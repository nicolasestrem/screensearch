//! ScreenSearch UI Automation Layer
//!
//! Provides a comprehensive Windows UI automation interface using the UIAutomation API.
//! Inspired by Playwright's web automation model for desktop applications.
//!
//! # Architecture
//!
//! - `AutomationEngine`: Core engine wrapping Windows UIAutomation API
//! - `Selector`: Playwright-inspired selector system for locating elements
//! - `UIElement`: Safe wrapper around Windows UI elements
//! - `Input`: Low-level mouse and keyboard simulation
//! - `WindowManager`: Window enumeration and management
//!
//! # Example
//!
//! ```no_run
//! use screen_automation::{AutomationEngine, Selector};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let engine = AutomationEngine::new()?;
//!
//! // Find and click a button
//! let button = engine.find_element(&Selector::role("button").with_name("Submit")).await?;
//! button.click()?;
//!
//! // Type text into an input field
//! let input = engine.find_element(&Selector::role("edit")).await?;
//! input.type_text("Hello, World!")?;
//! # Ok(())
//! # }
//! ```

mod element;
mod engine;
mod errors;
mod input;
mod selector;
mod window;

pub use element::{ClickResult, UIElement, UIElementAttributes};
pub use engine::AutomationEngine;
pub use errors::AutomationError;
pub use input::{InputSimulator, KeyCode, KeyModifier, MouseButton, ScrollDirection};
pub use selector::{Selector, SelectorBuilder};
pub use window::{WindowInfo, WindowManager};

/// Result type for automation operations
pub type Result<T> = std::result::Result<T, AutomationError>;

// Type aliases for API compatibility with screen-api
/// Alias for MouseButton to match screen-api expectations
pub type ClickButton = MouseButton;

/// Alias for Selector to match screen-api expectations
pub type ElementSelector = Selector;
