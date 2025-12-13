//! UI element wrapper providing safe interaction with Windows UI elements

use crate::errors::AutomationError;
use crate::selector::Selector;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use uiautomation::controls::ControlType;
use uiautomation::core::UICondition;
use uiautomation::types::{Point, ScrollAmount, TreeScope, UIProperty};
use uiautomation::variants::Variant;
use uiautomation::{filters::*, inputs::*, patterns, UIAutomation};

/// Thread-safe wrapper around UIAutomation
#[derive(Clone)]
pub(crate) struct ThreadSafeAutomation(pub Arc<UIAutomation>);

unsafe impl Send for ThreadSafeAutomation {}
unsafe impl Sync for ThreadSafeAutomation {}

/// Thread-safe wrapper around UIElement
#[derive(Clone)]
pub(crate) struct ThreadSafeElement(pub Arc<uiautomation::UIElement>);

unsafe impl Send for ThreadSafeElement {}
unsafe impl Sync for ThreadSafeElement {}

/// Result of a click operation
#[derive(Debug, Clone)]
pub struct ClickResult {
    /// Method used to perform the click
    pub method: String,
    /// Coordinates where the click occurred
    pub coordinates: Option<(f64, f64)>,
    /// Additional details about the operation
    pub details: String,
}

/// Attributes of a UI element
#[derive(Debug, Clone)]
pub struct UIElementAttributes {
    /// Element role (control type)
    pub role: String,
    /// Element label
    pub label: Option<String>,
    /// Element value
    pub value: Option<String>,
    /// Element description
    pub description: Option<String>,
    /// Additional properties
    pub properties: HashMap<String, Option<serde_json::Value>>,
}

/// Safe wrapper around a Windows UI element
#[derive(Clone)]
pub struct UIElement {
    element: ThreadSafeElement,
    automation: ThreadSafeAutomation,
}

impl fmt::Debug for UIElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UIElement")
            .field("id", &self.id())
            .field("role", &self.role())
            .finish()
    }
}

impl UIElement {
    /// Create a new UIElement from raw UIAutomation types
    #[allow(clippy::arc_with_non_send_sync)]
    pub(crate) fn new(element: uiautomation::UIElement, automation: &ThreadSafeAutomation) -> Self {
        Self {
            element: ThreadSafeElement(Arc::new(element)),
            automation: automation.clone(),
        }
    }

    /// Get the automation ID of this element
    pub fn id(&self) -> Option<String> {
        self.element.0.get_automation_id().ok()
    }

    /// Get the control type (role) of this element
    pub fn role(&self) -> String {
        self.element
            .0
            .get_control_type()
            .map(|ct| format!("{:?}", ct))
            .unwrap_or_else(|_| "Unknown".to_string())
    }

    /// Get the name of this element
    pub fn name(&self) -> Option<String> {
        self.element.0.get_name().ok()
    }

    /// Get comprehensive attributes of this element
    pub fn attributes(&self) -> UIElementAttributes {
        let mut properties = HashMap::new();

        // Collect all important properties
        let property_list = vec![
            UIProperty::Name,
            UIProperty::HelpText,
            UIProperty::ValueValue,
            UIProperty::ControlType,
            UIProperty::AutomationId,
            UIProperty::FullDescription,
        ];

        for property in property_list {
            if let Ok(value) = self.element.0.get_property_value(property) {
                properties.insert(
                    format!("{:?}", property),
                    Some(serde_json::to_value(value.to_string()).unwrap_or_default()),
                );
            } else {
                properties.insert(format!("{:?}", property), None);
            }
        }

        UIElementAttributes {
            role: self.role(),
            label: self
                .element
                .0
                .get_labeled_by()
                .ok()
                .and_then(|e| e.get_name().ok()),
            value: self
                .element
                .0
                .get_property_value(UIProperty::ValueValue)
                .ok()
                .and_then(|v| v.get_string().ok()),
            description: self.element.0.get_help_text().ok(),
            properties,
        }
    }

    /// Get the bounding rectangle of this element (x, y, width, height)
    pub fn bounds(&self) -> Result<(f64, f64, f64, f64), AutomationError> {
        let rect = self
            .element
            .0
            .get_bounding_rectangle()
            .map_err(AutomationError::platform)?;

        Ok((
            rect.get_left() as f64,
            rect.get_top() as f64,
            rect.get_width() as f64,
            rect.get_height() as f64,
        ))
    }

    /// Get child elements
    pub fn children(&self) -> Result<Vec<UIElement>, AutomationError> {
        let children = self
            .element
            .0
            .get_cached_children()
            .map_err(AutomationError::platform)?;

        Ok(children
            .into_iter()
            .map(|child| UIElement::new(child, &self.automation))
            .collect())
    }

    /// Get parent element
    pub fn parent(&self) -> Result<Option<UIElement>, AutomationError> {
        match self.element.0.get_cached_parent() {
            Ok(parent) => Ok(Some(UIElement::new(parent, &self.automation))),
            Err(_) => Ok(None),
        }
    }

    /// Click on this element
    ///
    /// Attempts multiple strategies: direct click, clickable point, and center of bounds
    pub fn click(&self) -> Result<ClickResult, AutomationError> {
        // Try to focus first
        let _ = self.element.0.try_focus();

        tracing::debug!("Attempting to click element: {:?}", self);

        // Strategy 1: Direct click through UIAutomation
        if self.element.0.click().is_ok() {
            return Ok(ClickResult {
                method: "Direct".to_string(),
                coordinates: None,
                details: "Clicked using UIAutomation direct method".to_string(),
            });
        }

        // Strategy 2: Use clickable point
        if let Ok(Some(point)) = self.element.0.get_clickable_point() {
            let mouse = Mouse::default();
            mouse.click(point).map_err(AutomationError::platform)?;

            return Ok(ClickResult {
                method: "ClickablePoint".to_string(),
                coordinates: Some((point.get_x() as f64, point.get_y() as f64)),
                details: "Clicked at element's clickable point".to_string(),
            });
        }

        // Strategy 3: Use center of bounding rectangle
        if let Ok(rect) = self.element.0.get_bounding_rectangle() {
            let center_x = rect.get_left() + rect.get_width() / 2;
            let center_y = rect.get_top() + rect.get_height() / 2;

            let point = Point::new(center_x, center_y);
            let mouse = Mouse::default();

            mouse.click(point).map_err(AutomationError::platform)?;

            return Ok(ClickResult {
                method: "BoundsCenter".to_string(),
                coordinates: Some((center_x as f64, center_y as f64)),
                details: "Clicked at center of bounding rectangle".to_string(),
            });
        }

        Err(AutomationError::InvalidState(
            "Cannot determine click location for element".to_string(),
        ))
    }

    /// Double-click on this element
    pub fn double_click(&self) -> Result<ClickResult, AutomationError> {
        let _ = self.element.0.try_focus();

        let point = self
            .element
            .0
            .get_clickable_point()
            .map_err(AutomationError::platform)?
            .ok_or_else(|| AutomationError::InvalidState("No clickable point".to_string()))?;

        let mouse = Mouse::default();
        mouse
            .double_click(point)
            .map_err(AutomationError::platform)?;

        Ok(ClickResult {
            method: "DoubleClick".to_string(),
            coordinates: Some((point.get_x() as f64, point.get_y() as f64)),
            details: "Double-clicked using mouse".to_string(),
        })
    }

    /// Right-click on this element
    pub fn right_click(&self) -> Result<(), AutomationError> {
        let _ = self.element.0.try_focus();

        let point = self
            .element
            .0
            .get_clickable_point()
            .map_err(AutomationError::platform)?
            .ok_or_else(|| AutomationError::InvalidState("No clickable point".to_string()))?;

        let mouse = Mouse::default();
        mouse
            .right_click(point)
            .map_err(AutomationError::platform)?;

        Ok(())
    }

    /// Focus this element
    pub fn focus(&self) -> Result<(), AutomationError> {
        self.element
            .0
            .set_focus()
            .map_err(AutomationError::platform)
    }

    /// Type text into this element
    ///
    /// The element should be focused before typing
    pub fn type_text(&self, text: &str) -> Result<(), AutomationError> {
        let keyboard = Keyboard::default();
        keyboard.send_text(text).map_err(AutomationError::platform)
    }

    /// Press a key or key combination
    ///
    /// Supports SendKeys notation (e.g., "^c" for Ctrl+C)
    pub fn press_key(&self, key: &str) -> Result<(), AutomationError> {
        let keyboard = Keyboard::default();
        keyboard.send_keys(key).map_err(AutomationError::platform)
    }

    /// Get text content from this element and its descendants
    ///
    /// `max_depth` controls how deep to search in the element tree
    pub fn text(&self, max_depth: usize) -> Result<String, AutomationError> {
        let mut all_texts = Vec::new();
        self.extract_text(&mut all_texts, 0, max_depth)?;
        Ok(all_texts.join(" "))
    }

    /// Recursive helper for text extraction
    fn extract_text(
        &self,
        texts: &mut Vec<String>,
        current_depth: usize,
        max_depth: usize,
    ) -> Result<(), AutomationError> {
        if current_depth > max_depth {
            return Ok(());
        }

        // Check Name property
        if let Ok(name) = self.element.0.get_property_value(UIProperty::Name) {
            if let Ok(name_text) = name.get_string() {
                if !name_text.is_empty() {
                    texts.push(name_text);
                }
            }
        }

        // Check Value property
        if let Ok(value) = self.element.0.get_property_value(UIProperty::ValueValue) {
            if let Ok(value_text) = value.get_string() {
                if !value_text.is_empty() {
                    texts.push(value_text);
                }
            }
        }

        // Recurse into children
        if let Ok(children) = self.element.0.get_cached_children() {
            for child in children {
                let child_element = UIElement::new(child, &self.automation);
                child_element.extract_text(texts, current_depth + 1, max_depth)?;
            }
        }

        Ok(())
    }

    /// Set the value of this element
    pub fn set_value(&self, value: &str) -> Result<(), AutomationError> {
        let value_pattern = self
            .element
            .0
            .get_pattern::<patterns::UIValuePattern>()
            .map_err(|e| {
                AutomationError::UnsupportedOperation(format!(
                    "Element does not support value pattern: {}",
                    e
                ))
            })?;

        value_pattern
            .set_value(value)
            .map_err(AutomationError::platform)
    }

    /// Check if element is enabled
    pub fn is_enabled(&self) -> Result<bool, AutomationError> {
        self.element
            .0
            .is_enabled()
            .map_err(AutomationError::platform)
    }

    /// Check if element is visible (not offscreen)
    pub fn is_visible(&self) -> Result<bool, AutomationError> {
        self.element
            .0
            .is_offscreen()
            .map(|offscreen| !offscreen)
            .map_err(AutomationError::platform)
    }

    /// Scroll the element
    ///
    /// Direction: "up", "down", "left", "right"
    pub fn scroll(&self, direction: &str, amount: f64) -> Result<(), AutomationError> {
        let scroll_pattern = self
            .element
            .0
            .get_pattern::<patterns::UIScrollPattern>()
            .map_err(|e| {
                AutomationError::UnsupportedOperation(format!(
                    "Element does not support scrolling: {}",
                    e
                ))
            })?;

        let scroll_amount = if amount > 0.0 {
            ScrollAmount::SmallIncrement
        } else if amount < 0.0 {
            ScrollAmount::SmallDecrement
        } else {
            ScrollAmount::NoAmount
        };

        let times = amount.abs() as usize;
        for _ in 0..times {
            match direction {
                "up" => scroll_pattern
                    .scroll(ScrollAmount::NoAmount, scroll_amount)
                    .map_err(AutomationError::platform)?,
                "down" => scroll_pattern
                    .scroll(ScrollAmount::NoAmount, scroll_amount)
                    .map_err(AutomationError::platform)?,
                "left" => scroll_pattern
                    .scroll(scroll_amount, ScrollAmount::NoAmount)
                    .map_err(AutomationError::platform)?,
                "right" => scroll_pattern
                    .scroll(scroll_amount, ScrollAmount::NoAmount)
                    .map_err(AutomationError::platform)?,
                _ => {
                    return Err(AutomationError::InvalidArgument(format!(
                        "Invalid scroll direction: {}",
                        direction
                    )))
                }
            }
        }

        Ok(())
    }

    /// Perform a named action on this element
    pub fn perform_action(&self, action: &str) -> Result<(), AutomationError> {
        match action {
            "focus" => self.focus(),
            "invoke" => {
                let invoke_pattern = self
                    .element
                    .0
                    .get_pattern::<patterns::UIInvokePattern>()
                    .map_err(|e| AutomationError::UnsupportedOperation(e.to_string()))?;
                invoke_pattern.invoke().map_err(AutomationError::platform)
            }
            "click" => self.click().map(|_| ()),
            "double_click" => self.double_click().map(|_| ()),
            "right_click" => self.right_click(),
            "toggle" => {
                let toggle_pattern = self
                    .element
                    .0
                    .get_pattern::<patterns::UITogglePattern>()
                    .map_err(|e| AutomationError::UnsupportedOperation(e.to_string()))?;
                toggle_pattern.toggle().map_err(AutomationError::platform)
            }
            "expand" => {
                let expand_pattern = self
                    .element
                    .0
                    .get_pattern::<patterns::UIExpandCollapsePattern>()
                    .map_err(|e| AutomationError::UnsupportedOperation(e.to_string()))?;
                expand_pattern.expand().map_err(AutomationError::platform)
            }
            "collapse" => {
                let collapse_pattern = self
                    .element
                    .0
                    .get_pattern::<patterns::UIExpandCollapsePattern>()
                    .map_err(|e| AutomationError::UnsupportedOperation(e.to_string()))?;
                collapse_pattern
                    .collapse()
                    .map_err(AutomationError::platform)
            }
            _ => Err(AutomationError::UnsupportedOperation(format!(
                "Action '{}' not supported",
                action
            ))),
        }
    }

    /// Find elements within this element using a selector
    pub fn find_elements(&self, selector: &Selector) -> Result<Vec<UIElement>, AutomationError> {
        // Use specialized methods that don't need conditions for most selectors
        match selector {
            Selector::Text(_) | Selector::Name(_) | Selector::Role { .. } | Selector::Id(_) => {
                // These use matchers, fall back to finding first and wrapping in Vec
                match self.find_element(selector) {
                    Ok(elem) => Ok(vec![elem]),
                    Err(_) => Ok(vec![]),
                }
            }
            _ => {
                // For other selector types, use conditions
                match self.selector_to_condition(selector) {
                    Ok(condition) => {
                        let elements = self
                            .element
                            .0
                            .find_all(TreeScope::Subtree, &condition)
                            .map_err(|e| {
                                AutomationError::element_not_found(format!("{}: {}", selector, e))
                            })?;

                        Ok(elements
                            .into_iter()
                            .map(|elem| UIElement::new(elem, &self.automation))
                            .collect())
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// Find first element within this element using a selector
    pub fn find_element(&self, selector: &Selector) -> Result<UIElement, AutomationError> {
        match selector {
            Selector::Text(text) => self.find_by_text(text),
            Selector::Name(name) => self.find_by_name(name),
            Selector::Role { role, name } => self.find_by_role(role, name.as_deref()),
            Selector::Id(id) => self.find_by_id(id),
            _ => match self.selector_to_condition(selector) {
                Ok(condition) => {
                    let element = self
                        .element
                        .0
                        .find_first(TreeScope::Subtree, &condition)
                        .map_err(|e| {
                            AutomationError::element_not_found(format!("{}: {}", selector, e))
                        })?;
                    Ok(UIElement::new(element, &self.automation))
                }
                Err(e) => Err(e),
            },
        }
    }

    /// Helper: Find by text using matcher
    fn find_by_text(&self, text: &str) -> Result<UIElement, AutomationError> {
        let filter = OrFilter {
            left: Box::new(NameFilter {
                value: text.to_string(),
                casesensitive: false,
                partial: true,
            }),
            right: Box::new(ControlTypeFilter {
                control_type: ControlType::Text,
            }),
        };

        let matcher = self
            .automation
            .0
            .create_matcher()
            .from_ref(&self.element.0)
            .filter(Box::new(filter))
            .depth(10)
            .timeout(3000);

        let element = matcher
            .find_first()
            .map_err(|e| AutomationError::element_not_found(format!("text '{}': {}", text, e)))?;

        Ok(UIElement::new(element, &self.automation))
    }

    /// Helper: Find by name using matcher
    fn find_by_name(&self, name: &str) -> Result<UIElement, AutomationError> {
        let matcher = self
            .automation
            .0
            .create_matcher()
            .from_ref(&self.element.0)
            .contains_name(name)
            .depth(10)
            .timeout(3000);

        let element = matcher
            .find_first()
            .map_err(|e| AutomationError::element_not_found(format!("name '{}': {}", name, e)))?;

        Ok(UIElement::new(element, &self.automation))
    }

    /// Helper: Find by role
    fn find_by_role(&self, role: &str, name: Option<&str>) -> Result<UIElement, AutomationError> {
        let control_type = map_role_to_control_type(role);

        let mut matcher = self
            .automation
            .0
            .create_matcher()
            .from_ref(&self.element.0)
            .control_type(control_type)
            .timeout(3000);

        if let Some(n) = name {
            matcher = matcher.contains_name(n);
        }

        let element = matcher
            .find_first()
            .map_err(|e| AutomationError::element_not_found(format!("role '{}': {}", role, e)))?;

        Ok(UIElement::new(element, &self.automation))
    }

    /// Helper: Find by automation ID
    fn find_by_id(&self, id: &str) -> Result<UIElement, AutomationError> {
        let condition = self
            .automation
            .0
            .create_property_condition(UIProperty::AutomationId, Variant::from(id), None)
            .map_err(AutomationError::platform)?;

        let element = self
            .element
            .0
            .find_first(TreeScope::Subtree, &condition)
            .map_err(|e| AutomationError::element_not_found(format!("id '{}': {}", id, e)))?;

        Ok(UIElement::new(element, &self.automation))
    }

    /// Convert selector to UIAutomation condition (not used for most selectors)
    fn selector_to_condition(&self, selector: &Selector) -> Result<UICondition, AutomationError> {
        match selector {
            Selector::Role { role, .. } => {
                let control_type = map_role_to_control_type(role);
                self.automation
                    .0
                    .create_property_condition(
                        UIProperty::ControlType,
                        Variant::from(control_type as i32),
                        None,
                    )
                    .map_err(AutomationError::platform)
            }
            Selector::Id(id) => self
                .automation
                .0
                .create_property_condition(
                    UIProperty::AutomationId,
                    Variant::from(id.as_str()),
                    None,
                )
                .map_err(AutomationError::platform),
            _ => Err(AutomationError::UnsupportedOperation(format!(
                "Selector type not supported for condition conversion: {:?}",
                selector
            ))),
        }
    }

}

impl PartialEq for UIElement {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.element.0, &other.element.0)
    }
}

impl Eq for UIElement {}

impl Hash for UIElement {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Use automation ID for hashing
        if let Some(id) = self.id() {
            id.hash(state);
        } else {
            // Fallback to pointer address
            Arc::as_ptr(&self.element.0).hash(state);
        }
    }
}

/// Map generic role names to Windows ControlType
fn map_role_to_control_type(role: &str) -> ControlType {
    match role.to_lowercase().as_str() {
        "window" => ControlType::Window,
        "button" => ControlType::Button,
        "checkbox" => ControlType::CheckBox,
        "menu" => ControlType::Menu,
        "menuitem" => ControlType::MenuItem,
        "dialog" => ControlType::Window,
        "text" => ControlType::Text,
        "edit" | "input" | "textfield" => ControlType::Edit,
        "tree" => ControlType::Tree,
        "treeitem" => ControlType::TreeItem,
        "datagrid" => ControlType::DataGrid,
        "list" => ControlType::List,
        "listitem" => ControlType::ListItem,
        "combobox" => ControlType::ComboBox,
        "tab" => ControlType::Tab,
        "tabitem" => ControlType::TabItem,
        "toolbar" => ControlType::ToolBar,
        "image" => ControlType::Image,
        "hyperlink" => ControlType::Hyperlink,
        "progressbar" => ControlType::ProgressBar,
        "radiobutton" => ControlType::RadioButton,
        "scrollbar" => ControlType::ScrollBar,
        "slider" => ControlType::Slider,
        "spinner" => ControlType::Spinner,
        "statusbar" => ControlType::StatusBar,
        "tooltip" => ControlType::ToolTip,
        "group" => ControlType::Group,
        "document" => ControlType::Document,
        "pane" => ControlType::Pane,
        "header" => ControlType::Header,
        "headeritem" => ControlType::HeaderItem,
        "table" => ControlType::Table,
        "titlebar" => ControlType::TitleBar,
        "separator" => ControlType::Separator,
        _ => ControlType::Custom,
    }
}
