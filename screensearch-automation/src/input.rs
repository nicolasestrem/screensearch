//! Input simulation for mouse and keyboard

use crate::errors::AutomationError;
use uiautomation::inputs::{Keyboard, Mouse};
use uiautomation::types::Point;

/// Mouse button types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Key modifier flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

/// Scroll direction for UI automation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Minimal KeyCode enum covering keys used in screen-api handlers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    // Control keys (most commonly used)
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,

    // Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Modifiers
    Shift,
    Control,
    Alt,

    // Special
    Space,
}

impl KeyCode {
    /// Parse KeyCode from string
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "enter" | "return" => Some(KeyCode::Enter),
            "escape" | "esc" => Some(KeyCode::Escape),
            "tab" => Some(KeyCode::Tab),
            "backspace" => Some(KeyCode::Backspace),
            "delete" | "del" => Some(KeyCode::Delete),
            "up" | "arrowup" => Some(KeyCode::ArrowUp),
            "down" | "arrowdown" => Some(KeyCode::ArrowDown),
            "left" | "arrowleft" => Some(KeyCode::ArrowLeft),
            "right" | "arrowright" => Some(KeyCode::ArrowRight),
            "shift" => Some(KeyCode::Shift),
            "control" | "ctrl" => Some(KeyCode::Control),
            "alt" => Some(KeyCode::Alt),
            "space" => Some(KeyCode::Space),
            _ => None,
        }
    }

    /// Convert KeyCode to SendKeys notation
    pub(crate) fn to_sendkeys(self) -> &'static str {
        match self {
            KeyCode::Enter => "{ENTER}",
            KeyCode::Escape => "{ESC}",
            KeyCode::Tab => "{TAB}",
            KeyCode::Backspace => "{BACKSPACE}",
            KeyCode::Delete => "{DELETE}",
            KeyCode::ArrowUp => "{UP}",
            KeyCode::ArrowDown => "{DOWN}",
            KeyCode::ArrowLeft => "{LEFT}",
            KeyCode::ArrowRight => "{RIGHT}",
            KeyCode::Space => " ",
            _ => "",
        }
    }
}

/// Input simulator for direct mouse and keyboard control
///
/// Provides low-level input simulation capabilities independent of UI elements
pub struct InputSimulator {
    mouse: Mouse,
    keyboard: Keyboard,
}

impl InputSimulator {
    /// Create a new input simulator
    pub fn new() -> Self {
        Self {
            mouse: Mouse::default(),
            keyboard: Keyboard::default(),
        }
    }

    /// Click at specific screen coordinates
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use screen_automation::{AutomationEngine, MouseButton};
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = AutomationEngine::new()?;
    /// engine.input().click_at(100, 200, MouseButton::Left)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn click_at(&self, x: i32, y: i32, button: MouseButton) -> Result<(), AutomationError> {
        let point = Point::new(x, y);

        match button {
            MouseButton::Left => self.mouse.click(point),
            MouseButton::Right => self.mouse.right_click(point),
            MouseButton::Middle => {
                return Err(AutomationError::UnsupportedOperation(
                    "Middle mouse button not supported by uiautomation crate".to_string(),
                ))
            }
        }
        .map_err(AutomationError::platform)
    }

    /// Double-click at specific screen coordinates
    pub fn double_click_at(&self, x: i32, y: i32) -> Result<(), AutomationError> {
        let point = Point::new(x, y);
        self.mouse
            .double_click(point)
            .map_err(AutomationError::platform)
    }

    /// Move mouse to specific coordinates
    pub fn move_to(&self, x: i32, y: i32) -> Result<(), AutomationError> {
        let point = Point::new(x, y);
        self.mouse.move_to(point).map_err(AutomationError::platform)
    }

    /// Type text using keyboard
    ///
    /// This sends text character by character
    pub fn type_text(&self, text: &str) -> Result<(), AutomationError> {
        self.keyboard
            .send_text(text)
            .map_err(AutomationError::platform)
    }

    /// Send key combination
    ///
    /// Uses Windows SendKeys notation:
    /// - `^` = Ctrl
    /// - `%` = Alt
    /// - `+` = Shift
    /// - `{ENTER}` = Enter key
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use screen_automation::AutomationEngine;
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = AutomationEngine::new()?;
    ///
    /// // Ctrl+C
    /// engine.input().send_keys("^c")?;
    ///
    /// // Alt+Tab
    /// engine.input().send_keys("%{TAB}")?;
    ///
    /// // Ctrl+Shift+S
    /// engine.input().send_keys("^+s")?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn send_keys(&self, keys: &str) -> Result<(), AutomationError> {
        self.keyboard
            .send_keys(keys)
            .map_err(AutomationError::platform)
    }

    /// Press a single key
    ///
    /// Common keys: "ENTER", "ESC", "TAB", "BACKSPACE", "DELETE", "F1"-"F12", etc.
    pub fn press_key(&self, key: &str) -> Result<(), AutomationError> {
        let key_str = format!("{{{}}}", key);
        self.keyboard
            .send_keys(&key_str)
            .map_err(AutomationError::platform)
    }

    /// Press Enter key
    pub fn press_enter(&self) -> Result<(), AutomationError> {
        self.press_key("ENTER")
    }

    /// Press Escape key
    pub fn press_escape(&self) -> Result<(), AutomationError> {
        self.press_key("ESC")
    }

    /// Press Tab key
    pub fn press_tab(&self) -> Result<(), AutomationError> {
        self.press_key("TAB")
    }

    /// Press Backspace key
    pub fn press_backspace(&self) -> Result<(), AutomationError> {
        self.press_key("BACKSPACE")
    }

    /// Press Delete key
    pub fn press_delete(&self) -> Result<(), AutomationError> {
        self.press_key("DELETE")
    }

    /// Send Ctrl+key combination
    pub fn ctrl_key(&self, key: &str) -> Result<(), AutomationError> {
        self.send_keys(&format!("^{}", key))
    }

    /// Send Alt+key combination
    pub fn alt_key(&self, key: &str) -> Result<(), AutomationError> {
        self.send_keys(&format!("%{}", key))
    }

    /// Send Shift+key combination
    pub fn shift_key(&self, key: &str) -> Result<(), AutomationError> {
        self.send_keys(&format!("+{}", key))
    }

    /// Copy to clipboard (Ctrl+C)
    pub fn copy(&self) -> Result<(), AutomationError> {
        self.ctrl_key("c")
    }

    /// Paste from clipboard (Ctrl+V)
    pub fn paste(&self) -> Result<(), AutomationError> {
        self.ctrl_key("v")
    }

    /// Cut to clipboard (Ctrl+X)
    pub fn cut(&self) -> Result<(), AutomationError> {
        self.ctrl_key("x")
    }

    /// Select all (Ctrl+A)
    pub fn select_all(&self) -> Result<(), AutomationError> {
        self.ctrl_key("a")
    }

    /// Undo (Ctrl+Z)
    pub fn undo(&self) -> Result<(), AutomationError> {
        self.ctrl_key("z")
    }

    /// Redo (Ctrl+Y)
    pub fn redo(&self) -> Result<(), AutomationError> {
        self.ctrl_key("y")
    }

    /// Save (Ctrl+S)
    pub fn save(&self) -> Result<(), AutomationError> {
        self.ctrl_key("s")
    }

    /// Open (Ctrl+O)
    pub fn open(&self) -> Result<(), AutomationError> {
        self.ctrl_key("o")
    }

    /// Find (Ctrl+F)
    pub fn find(&self) -> Result<(), AutomationError> {
        self.ctrl_key("f")
    }

    /// Switch window (Alt+Tab)
    pub fn switch_window(&self) -> Result<(), AutomationError> {
        self.send_keys("%{TAB}")
    }
}

impl Default for InputSimulator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_simulator_creation() {
        let simulator = InputSimulator::new();
        // Just verify it can be created
        assert!(std::mem::size_of_val(&simulator) > 0);
    }

    #[test]
    fn test_mouse_button_types() {
        assert_eq!(MouseButton::Left, MouseButton::Left);
        assert_ne!(MouseButton::Left, MouseButton::Right);
    }

    #[test]
    fn test_key_modifier_types() {
        assert_eq!(KeyModifier::Ctrl, KeyModifier::Ctrl);
        assert_ne!(KeyModifier::Ctrl, KeyModifier::Alt);
    }
}
