use crate::errors::AutomationError;
use crate::selector::Selector;
use std::time::Duration;

#[derive(Clone)]
pub struct ThreadSafeAutomation;

#[derive(Clone)]
pub struct UIElement;

impl UIElement {
    pub fn click(&self) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }
    
    pub fn text(&self, _depth: usize) -> Result<String, AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub fn role(&self) -> String {
        "stub".to_string()
    }

    pub fn bounds(&self) -> Result<(f64, f64, f64, f64), AutomationError> {
         Ok((0.0, 0.0, 0.0, 0.0))
    }

    pub fn name(&self) -> Option<String> {
        Some("stub".to_string())
    }

    pub fn is_enabled(&self) -> Result<bool, AutomationError> {
        Ok(false)
    }

    pub fn is_visible(&self) -> Result<bool, AutomationError> {
        Ok(false)
    }
}

pub struct UIElementAttributes;

pub struct AutomationEngine;

impl AutomationEngine {
    pub fn new() -> Result<Self, AutomationError> {
        Ok(Self)
    }

    pub fn root(&self) -> Result<UIElement, AutomationError> {
        Ok(UIElement)
    }

    pub async fn find_element(&self, _selector: &Selector) -> Result<UIElement, AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }
    
    pub async fn find_element_with_timeout(&self, _selector: &Selector, _timeout: Duration) -> Result<UIElement, AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub fn find_elements(&self, _selector: &Selector) -> Result<Vec<UIElement>, AutomationError> {
        Ok(vec![])
    }

    pub async fn wait_for<F>(&self, _timeout: Duration, _condition: F) -> Result<(), AutomationError> 
    where F: FnMut() -> Result<bool, AutomationError> 
    {
        Ok(())
    }
    
    pub async fn open_app(&self, _app_name: &str) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub async fn open_url(&self, _url: &str, _browser: Option<&str>) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub fn click(&self, _x: i32, _y: i32, _button: MouseButton) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub fn type_text(&self, _text: &str, _delay_ms: Option<u64>) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub fn scroll(&self, _direction: ScrollDirection, _amount: i32) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub fn press_key(&self, _key: KeyCode, _modifiers: &[KeyCode]) -> Result<(), AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }

    pub async fn get_text(&self, _selector: &Selector) -> Result<String, AutomationError> {
        Err(AutomationError::platform("Not implemented on Linux"))
    }
}


// Input Enums
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Control,
    Alt,
    Shift,
    Enter,
    // Add others as needed if accessed publicly
}

impl KeyCode {
    pub fn to_sendkeys(&self) -> &'static str {
        ""
    }

    pub fn from_name(_name: &str) -> Option<Self> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModifier {
    Control,
    Alt,
    Shift,
}

pub struct InputSimulator;

pub struct WindowManager;
pub struct WindowInfo;

pub type ClickButton = MouseButton;
pub type ElementSelector = Selector;
pub type ClickResult = ();
