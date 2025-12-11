//! Automation engine - core interface for UI automation

use crate::element::{ThreadSafeAutomation, UIElement};
use crate::errors::AutomationError;
use crate::input::InputSimulator;
use crate::selector::Selector;
use crate::window::WindowManager;
use std::sync::Arc;
use std::time::Duration;
use uiautomation::controls::ControlType;
use uiautomation::types::{TreeScope, UIProperty};
use uiautomation::variants::Variant;
use uiautomation::UIAutomation;

/// Main automation engine for Windows UI automation
///
/// This is the primary entry point for all automation operations.
/// It wraps the Windows UIAutomation API and provides a safe, ergonomic interface.
pub struct AutomationEngine {
    automation: ThreadSafeAutomation,
    window_manager: WindowManager,
    input_simulator: InputSimulator,
}

impl AutomationEngine {
    /// Create a new automation engine
    ///
    /// # Errors
    ///
    /// Returns an error if the UIAutomation COM interface cannot be initialized.
    pub fn new() -> Result<Self, AutomationError> {
        let automation = UIAutomation::new().map_err(|e| {
            AutomationError::platform(format!("Failed to initialize UIAutomation: {}", e))
        })?;

        let automation = ThreadSafeAutomation(Arc::new(automation));
        let window_manager = WindowManager::new();
        let input_simulator = InputSimulator::new();

        Ok(Self {
            automation,
            window_manager,
            input_simulator,
        })
    }

    /// Get the root UI element (desktop)
    pub fn root(&self) -> Result<UIElement, AutomationError> {
        let root = self
            .automation
            .0
            .get_root_element()
            .map_err(AutomationError::platform)?;

        Ok(UIElement::new(root, &self.automation))
    }

    /// Get the currently focused element
    pub fn focused_element(&self) -> Result<UIElement, AutomationError> {
        let element = self
            .automation
            .0
            .get_focused_element()
            .map_err(AutomationError::platform)?;

        Ok(UIElement::new(element, &self.automation))
    }

    /// Find the first element matching a selector
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use screen_automation::{AutomationEngine, Selector};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = AutomationEngine::new()?;
    /// let button = engine.find_element(&Selector::role("button").with_name("OK")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_element(&self, selector: &Selector) -> Result<UIElement, AutomationError> {
        self.find_element_with_timeout(selector, Duration::from_secs(30))
            .await
    }

    /// Find element with a custom timeout
    pub async fn find_element_with_timeout(
        &self,
        selector: &Selector,
        timeout: Duration,
    ) -> Result<UIElement, AutomationError> {
        let start = std::time::Instant::now();
        let root = self.root()?;

        loop {
            match root.find_element(selector) {
                Ok(element) => return Ok(element),
                Err(e) => {
                    if start.elapsed() >= timeout {
                        return Err(AutomationError::timeout(
                            format!("Finding element with selector: {}", selector),
                            timeout.as_millis() as u64,
                        ));
                    }

                    if !e.is_not_found() {
                        return Err(e);
                    }

                    // Wait before retrying
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    /// Find all elements matching a selector
    pub fn find_elements(&self, selector: &Selector) -> Result<Vec<UIElement>, AutomationError> {
        let root = self.root()?;
        root.find_elements(selector)
    }

    /// Get all application windows
    pub fn applications(&self) -> Result<Vec<UIElement>, AutomationError> {
        let root = self
            .automation
            .0
            .get_root_element()
            .map_err(AutomationError::platform)?;

        let condition = self
            .automation
            .0
            .create_property_condition(
                UIProperty::ControlType,
                Variant::from(ControlType::Window as i32),
                None,
            )
            .map_err(AutomationError::platform)?;

        let elements = root
            .find_all(TreeScope::Children, &condition)
            .map_err(AutomationError::platform)?;

        Ok(elements
            .into_iter()
            .map(|elem| UIElement::new(elem, &self.automation))
            .collect())
    }

    /// Find application by name
    ///
    /// Searches for a window with a title containing the given name
    pub async fn application(&self, name: &str) -> Result<UIElement, AutomationError> {
        let root = self
            .automation
            .0
            .get_root_element()
            .map_err(AutomationError::platform)?;

        // Try to find by matcher first
        let matcher = self
            .automation
            .0
            .create_matcher()
            .control_type(ControlType::Window)
            .contains_name(name)
            .from_ref(&root)
            .depth(7)
            .timeout(5000);

        match matcher.find_first() {
            Ok(element) => Ok(UIElement::new(element, &self.automation)),
            Err(_) => {
                // Fallback: try to find by process name
                self.find_by_process_name(name).await
            }
        }
    }

    /// Helper to find window by process name
    async fn find_by_process_name(&self, process_name: &str) -> Result<UIElement, AutomationError> {
        // Get PID from process name
        let pid = get_pid_by_name(process_name).ok_or_else(|| {
            AutomationError::element_not_found(format!(
                "No process found with name: {}",
                process_name
            ))
        })?;

        let root = self
            .automation
            .0
            .get_root_element()
            .map_err(AutomationError::platform)?;

        let condition = self
            .automation
            .0
            .create_property_condition(UIProperty::ProcessId, Variant::from(pid), None)
            .map_err(AutomationError::platform)?;

        let element = root
            .find_first(TreeScope::Subtree, &condition)
            .map_err(|e| {
                AutomationError::element_not_found(format!(
                    "Window for process {}: {}",
                    process_name, e
                ))
            })?;

        Ok(UIElement::new(element, &self.automation))
    }

    /// Open an application
    ///
    /// Uses PowerShell's `start` command to launch the application
    ///
    /// Note: This method does not wait for or return the application window.
    /// Use `application()` separately if you need to interact with the window.
    pub async fn open_application(&self, app_name: &str) -> Result<(), AutomationError> {
        let app_name_for_closure = app_name.to_string();

        // Run blocking Command in thread pool and wait for app to start
        tokio::task::spawn_blocking(move || {
            let status = std::process::Command::new("powershell")
                .args([
                    "-NoProfile",
                    "-WindowStyle",
                    "hidden",
                    "-Command",
                    "start",
                    &app_name_for_closure,
                ])
                .status()
                .map_err(|e| {
                    AutomationError::platform(format!("Failed to execute command: {}", e))
                })?;

            if !status.success() {
                return Err(AutomationError::platform(format!(
                    "Failed to open application: {}",
                    app_name_for_closure
                )));
            }

            // Wait for application to start
            std::thread::sleep(Duration::from_millis(1000));
            Ok(())
        })
        .await
        .map_err(|e| AutomationError::platform(format!("Task join error: {}", e)))??;

        Ok(())
    }

    /// Open URL in default browser
    ///
    /// Note: This method does not wait for or return the browser window.
    /// Use `application()` separately if you need to interact with the browser.
    pub async fn open_url(&self, url: &str, browser: Option<&str>) -> Result<(), AutomationError> {
        let url_for_closure = url.to_string();
        let browser_for_closure = browser.unwrap_or("").to_string();

        // Run blocking Command in thread pool
        tokio::task::spawn_blocking(move || {
            let mut cmd = std::process::Command::new("powershell");
            cmd.args(["-NoProfile", "-WindowStyle", "hidden", "-Command", "start"]);

            if !browser_for_closure.is_empty() {
                cmd.arg(&browser_for_closure);
            }
            cmd.arg(&url_for_closure);

            let status = cmd.status().map_err(|e| {
                AutomationError::platform(format!("Failed to execute command: {}", e))
            })?;

            if !status.success() {
                return Err(AutomationError::platform("Failed to open URL".to_string()));
            }

            // Wait for browser to start
            std::thread::sleep(Duration::from_millis(500));
            Ok(())
        })
        .await
        .map_err(|e| AutomationError::platform(format!("Task join error: {}", e)))??;

        Ok(())
    }

    /// Get window manager for window operations
    pub fn windows(&self) -> &WindowManager {
        &self.window_manager
    }

    /// Get input simulator for direct input operations
    pub fn input(&self) -> &InputSimulator {
        &self.input_simulator
    }

    /// Wait for a condition to be true
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use screen_automation::{AutomationEngine, Selector};
    /// # use std::time::Duration;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let engine = AutomationEngine::new()?;
    ///
    /// engine.wait_for(Duration::from_secs(10), || {
    ///     engine.find_elements(&Selector::role("button")).map(|buttons| !buttons.is_empty())
    /// }).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn wait_for<F>(
        &self,
        timeout: Duration,
        mut condition: F,
    ) -> Result<(), AutomationError>
    where
        F: FnMut() -> Result<bool, AutomationError>,
    {
        let start = std::time::Instant::now();

        loop {
            match condition() {
                Ok(true) => return Ok(()),
                Ok(false) => {
                    if start.elapsed() >= timeout {
                        return Err(AutomationError::timeout(
                            "Waiting for condition",
                            timeout.as_millis() as u64,
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    if start.elapsed() >= timeout {
                        return Err(AutomationError::timeout(
                            format!("Waiting for condition (last error: {})", e),
                            timeout.as_millis() as u64,
                        ));
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }

    // Convenience methods for screen-api compatibility

    /// Click at coordinates with specified button
    ///
    /// Wrapper around `input().click_at()` for convenience
    pub fn click(&self, x: i32, y: i32, button: crate::MouseButton) -> Result<(), AutomationError> {
        self.input().click_at(x, y, button)
    }

    /// Type text with optional delay between characters
    ///
    /// Wrapper around `input().type_text()` with optional character delay
    pub fn type_text(&self, text: &str, delay_ms: Option<u64>) -> Result<(), AutomationError> {
        if let Some(delay) = delay_ms {
            // Type with delay between chars
            for ch in text.chars() {
                self.input().type_text(&ch.to_string())?;
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
            Ok(())
        } else {
            self.input().type_text(text)
        }
    }

    /// Scroll in specified direction
    ///
    /// Sends arrow key presses to simulate scrolling
    pub fn scroll(
        &self,
        direction: crate::ScrollDirection,
        amount: i32,
    ) -> Result<(), AutomationError> {
        use crate::ScrollDirection;

        let key = match direction {
            ScrollDirection::Up => "{UP}",
            ScrollDirection::Down => "{DOWN}",
            ScrollDirection::Left => "{LEFT}",
            ScrollDirection::Right => "{RIGHT}",
        };

        for _ in 0..amount {
            self.input().send_keys(key)?;
        }
        Ok(())
    }

    /// Press key with optional modifiers
    ///
    /// Builds SendKeys string with modifiers and executes
    pub fn press_key(
        &self,
        key: crate::KeyCode,
        modifiers: &[crate::KeyCode],
    ) -> Result<(), AutomationError> {
        use crate::KeyCode;

        // Build SendKeys string with modifiers
        let mut keys = String::new();

        for modifier in modifiers {
            match modifier {
                KeyCode::Control => keys.push('^'),
                KeyCode::Alt => keys.push('%'),
                KeyCode::Shift => keys.push('+'),
                _ => {}
            }
        }

        // Add main key
        keys.push_str(key.to_sendkeys());

        self.input().send_keys(&keys)
    }

    /// Get text from element matching selector
    ///
    /// Wrapper that finds element and extracts text
    pub async fn get_text(&self, selector: &crate::Selector) -> Result<String, AutomationError> {
        let element = self.find_element(selector).await?;
        element.text(5) // depth of 5 for text extraction
    }

    /// Open application (alias for open_application)
    ///
    /// Compatibility alias for screen-api
    pub async fn open_app(&self, app_name: &str) -> Result<(), AutomationError> {
        self.open_application(app_name).await
    }
}

impl Default for AutomationEngine {
    fn default() -> Self {
        Self::new().expect("Failed to create automation engine")
    }
}

/// Get process ID by process name
fn get_pid_by_name(name: &str) -> Option<i32> {
    let command = format!(
        "Get-Process | Where-Object {{ $_.MainWindowTitle -ne '' -and $_.Name -like '*{}*' }} | ForEach-Object {{ $_.Id }}",
        name
    );

    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "hidden", "-Command", &command])
        .output()
        .ok()?;

    if output.status.success() {
        let pid_str = String::from_utf8_lossy(&output.stdout);
        pid_str.lines().next()?.trim().parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_creation() {
        let engine = AutomationEngine::new();
        assert!(engine.is_ok());
    }

    #[tokio::test]
    async fn test_get_root() {
        let engine = AutomationEngine::new().unwrap();
        let root = engine.root();

        // Root element retrieval may fail in some environments
        match root {
            Ok(r) => {
                println!("Root element: {}", r.role());
            }
            Err(e) => {
                println!("Warning: Could not get root element: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_get_applications() {
        let engine = AutomationEngine::new().unwrap();
        let apps = engine.applications();

        // Just verify it doesn't crash - may or may not find applications
        // depending on system state
        match apps {
            Ok(apps) => {
                println!("Found {} applications", apps.len());
            }
            Err(e) => {
                println!("Warning: Could not enumerate applications: {}", e);
            }
        }
    }
}
