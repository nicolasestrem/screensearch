//! Computer automation endpoint handlers

use crate::error::{AppError, Result};
use crate::models::{
    AutomationResponse, ClickRequest, ElementInfo, FindElementsRequest, GetTextRequest,
    GetTextResponse, KeyPressRequest, ListElementsRequest, OpenAppRequest, OpenUrlRequest,
    ScrollRequest, TypeRequest,
};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use screensearch_automation::{
    KeyCode, MouseButton as ClickButton, ScrollDirection, Selector as ElementSelector,
};
use std::sync::Arc;
use tracing::{debug, error};

/// POST /automation/find-elements - Locate UI elements
///
/// Finds UI elements matching the provided selector string.
///
/// # Request Body
/// - selector: Element selector string (e.g., name, automation ID)
/// - timeout_ms: Optional timeout in milliseconds (default: 5000)
pub async fn find_elements(
    State(state): State<Arc<AppState>>,
    Json(req): Json<FindElementsRequest>,
) -> Result<Json<Vec<ElementInfo>>> {
    debug!("Find elements request: selector={}", req.selector);

    // Create selector (use name-based selector for now)
    let selector = ElementSelector::name(&req.selector);

    // Find elements
    match state.automation.find_elements(&selector) {
        Ok(elements) => {
            debug!("Found {} elements", elements.len());
            // Convert UIElements to ElementInfo
            let element_infos: Result<Vec<ElementInfo>> =
                elements.iter().map(ElementInfo::from_ui_element).collect();
            Ok(Json(element_infos?))
        }
        Err(e) => {
            error!("Failed to find elements: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/click - Click at coordinates
///
/// Performs a mouse click at the specified screen coordinates.
///
/// # Request Body
/// - x: X coordinate
/// - y: Y coordinate
/// - button: Optional button type ("left", "right", "middle", default: "left")
pub async fn click(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ClickRequest>,
) -> Result<Json<AutomationResponse>> {
    debug!(
        "Click request: ({}, {}), button={:?}",
        req.x, req.y, req.button
    );

    let button = match req.button.as_deref() {
        Some("right") => ClickButton::Right,
        Some("middle") => ClickButton::Middle,
        _ => ClickButton::Left,
    };

    match state.automation.click(req.x, req.y, button) {
        Ok(_) => Ok(Json(AutomationResponse {
            success: true,
            message: Some(format!("Clicked at ({}, {})", req.x, req.y)),
        })),
        Err(e) => {
            error!("Click failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/type - Type text into active element
///
/// Types the provided text with optional delay between characters.
///
/// # Request Body
/// - text: Text to type
/// - delay_ms: Optional delay between characters in milliseconds
pub async fn type_text(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TypeRequest>,
) -> Result<Json<AutomationResponse>> {
    debug!("Type text request: {} chars", req.text.len());

    match state.automation.type_text(&req.text, req.delay_ms) {
        Ok(_) => Ok(Json(AutomationResponse {
            success: true,
            message: Some(format!("Typed {} characters", req.text.len())),
        })),
        Err(e) => {
            error!("Type text failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/scroll - Scroll action
///
/// Scrolls in the specified direction by the given amount.
///
/// # Request Body
/// - direction: Scroll direction ("up", "down", "left", "right")
/// - amount: Scroll amount (in lines/pixels)
pub async fn scroll(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ScrollRequest>,
) -> Result<Json<AutomationResponse>> {
    debug!("Scroll request: {} by {}", req.direction, req.amount);

    let direction = match req.direction.to_lowercase().as_str() {
        "up" => ScrollDirection::Up,
        "down" => ScrollDirection::Down,
        "left" => ScrollDirection::Left,
        "right" => ScrollDirection::Right,
        _ => {
            return Err(AppError::InvalidRequest(format!(
                "Invalid scroll direction: {}",
                req.direction
            )))
        }
    };

    match state.automation.scroll(direction, req.amount) {
        Ok(_) => Ok(Json(AutomationResponse {
            success: true,
            message: Some(format!("Scrolled {} by {}", req.direction, req.amount)),
        })),
        Err(e) => {
            error!("Scroll failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/press-key - Press keyboard key
///
/// Presses a keyboard key with optional modifiers.
///
/// # Request Body
/// - key: Key name (e.g., "enter", "escape", "a")
/// - modifiers: Optional modifier keys (e.g., ["ctrl", "shift"])
pub async fn press_key(
    State(state): State<Arc<AppState>>,
    Json(req): Json<KeyPressRequest>,
) -> Result<Json<AutomationResponse>> {
    debug!(
        "Press key request: {}, modifiers={:?}",
        req.key, req.modifiers
    );

    // Parse key
    let key = KeyCode::from_name(&req.key)
        .ok_or_else(|| AppError::InvalidRequest(format!("Invalid key: {}", req.key)))?;

    // Parse modifiers
    let modifiers: Result<Vec<KeyCode>> = req
        .modifiers
        .unwrap_or_default()
        .iter()
        .map(|m| {
            KeyCode::from_name(m)
                .ok_or_else(|| AppError::InvalidRequest(format!("Invalid modifier: {}", m)))
        })
        .collect();

    let modifiers = modifiers?;

    match state.automation.press_key(key, &modifiers) {
        Ok(_) => Ok(Json(AutomationResponse {
            success: true,
            message: Some(format!("Pressed key: {}", req.key)),
        })),
        Err(e) => {
            error!("Press key failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/get-text - Extract text from UI element
///
/// Gets the text content from a UI element matching the selector.
///
/// # Request Body
/// - selector: Element selector string
pub async fn get_text(
    State(state): State<Arc<AppState>>,
    Json(req): Json<GetTextRequest>,
) -> Result<Json<GetTextResponse>> {
    debug!("Get text request: selector={}", req.selector);

    let selector = ElementSelector::name(&req.selector);

    match state.automation.get_text(&selector).await {
        Ok(text) => Ok(Json(GetTextResponse { text })),
        Err(e) => {
            error!("Get text failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/list-elements - List interactive elements
///
/// Lists all interactive UI elements in the active window or under a root element.
///
/// # Request Body
/// - root_selector: Optional root element selector (defaults to active window)
pub async fn list_elements(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ListElementsRequest>,
) -> Result<Json<Vec<ElementInfo>>> {
    debug!("List elements request: root={:?}", req.root_selector);

    // If root selector provided, create selector
    let selector = if let Some(root) = req.root_selector {
        ElementSelector::name(&root)
    } else {
        // Default to finding all elements (wildcard selector)
        ElementSelector::name("*")
    };

    // Find elements
    match state.automation.find_elements(&selector) {
        Ok(elements) => {
            debug!("Found {} elements", elements.len());
            // Convert UIElements to ElementInfo
            let element_infos: Result<Vec<ElementInfo>> =
                elements.iter().map(ElementInfo::from_ui_element).collect();
            Ok(Json(element_infos?))
        }
        Err(e) => {
            error!("Failed to list elements: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/open-app - Launch application
///
/// Opens an application by name or path.
///
/// # Request Body
/// - app_name: Application name or path (e.g., "notepad", "C:\\Program Files\\app.exe")
pub async fn open_app(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenAppRequest>,
) -> Result<Json<AutomationResponse>> {
    debug!("Open app request: {}", req.app_name);

    match state.automation.open_app(&req.app_name).await {
        Ok(_) => Ok(Json(AutomationResponse {
            success: true,
            message: Some(format!("Opened application: {}", req.app_name)),
        })),
        Err(e) => {
            error!("Open app failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

/// POST /automation/open-url - Open URL in browser
///
/// Opens a URL in the default web browser.
///
/// # Request Body
/// - url: URL to open
pub async fn open_url(
    State(state): State<Arc<AppState>>,
    Json(req): Json<OpenUrlRequest>,
) -> Result<Json<AutomationResponse>> {
    debug!("Open URL request: {}", req.url);

    // Validate URL format
    if !req.url.starts_with("http://") && !req.url.starts_with("https://") {
        return Err(AppError::InvalidRequest(
            "URL must start with http:// or https://".to_string(),
        ));
    }

    // Use Chrome as default browser (per user preference)
    match state.automation.open_url(&req.url, Some("chrome")).await {
        Ok(_) => Ok(Json(AutomationResponse {
            success: true,
            message: Some(format!("Opened URL: {}", req.url)),
        })),
        Err(e) => {
            error!("Open URL failed: {}", e);
            Err(AppError::Automation(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_button_parsing() {
        assert!(matches!(
            parse_button_from_string("left"),
            ClickButton::Left
        ));
        assert!(matches!(
            parse_button_from_string("right"),
            ClickButton::Right
        ));
    }

    fn parse_button_from_string(s: &str) -> ClickButton {
        match s {
            "right" => ClickButton::Right,
            "middle" => ClickButton::Middle,
            _ => ClickButton::Left,
        }
    }

    #[test]
    fn test_scroll_direction_parsing() {
        assert!(matches!(
            parse_scroll_direction("up"),
            Some(ScrollDirection::Up)
        ));
        assert!(matches!(
            parse_scroll_direction("down"),
            Some(ScrollDirection::Down)
        ));
        assert!(parse_scroll_direction("invalid").is_none());
    }

    fn parse_scroll_direction(s: &str) -> Option<ScrollDirection> {
        match s.to_lowercase().as_str() {
            "up" => Some(ScrollDirection::Up),
            "down" => Some(ScrollDirection::Down),
            "left" => Some(ScrollDirection::Left),
            "right" => Some(ScrollDirection::Right),
            _ => None,
        }
    }
}
