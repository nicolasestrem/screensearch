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

mod errors;
mod selector;

#[cfg(windows)]
mod element;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod input;
#[cfg(windows)]
mod window;

#[cfg(not(windows))]
mod stub;

pub use errors::AutomationError;
pub use selector::{Selector, SelectorBuilder};

#[cfg(windows)]
pub use element::{ClickResult, UIElement, UIElementAttributes};
#[cfg(windows)]
pub use engine::AutomationEngine;
#[cfg(windows)]
pub use input::{InputSimulator, KeyCode, KeyModifier, MouseButton, ScrollDirection};
#[cfg(windows)]
pub use window::{WindowInfo, WindowManager};

#[cfg(not(windows))]
pub use stub::*;

/// Result type for automation operations
pub type Result<T> = std::result::Result<T, AutomationError>;
