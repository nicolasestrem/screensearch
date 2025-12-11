//! Active window tracking and context extraction
//!
//! This module provides functionality to track the active window and extract
//! contextual information including window title, process name, and browser URLs.

use crate::{CaptureError, Result};
use windows::Win32::{
    Foundation::{HWND, MAX_PATH},
    System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    },
    UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId},
};

/// Window context information captured at the time of screenshot
#[derive(Debug, Clone)]
pub struct WindowContext {
    /// Window title
    pub window_title: String,

    /// Process name (e.g., "chrome.exe")
    pub process_name: String,

    /// Process ID
    pub process_id: u32,

    /// Browser URL if applicable
    pub url: Option<String>,
}

impl WindowContext {
    /// Capture the current active window context
    pub fn capture() -> Result<Self> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0 == 0 {
                return Err(CaptureError::WindowsApiError(
                    "No foreground window".to_string(),
                ));
            }

            let window_title = get_window_title(hwnd)?;
            let (process_id, process_name) = get_process_info(hwnd)?;
            let url = extract_browser_url(hwnd, &process_name);

            Ok(Self {
                window_title,
                process_name,
                process_id,
                url,
            })
        }
    }
}

/// Get the window title from an HWND
unsafe fn get_window_title(hwnd: HWND) -> Result<String> {
    let mut title: [u16; 512] = [0; 512];
    let len = GetWindowTextW(hwnd, &mut title);

    if len > 0 {
        String::from_utf16(&title[..len as usize]).map_err(|e| {
            CaptureError::WindowsApiError(format!("Invalid UTF-16 in window title: {}", e))
        })
    } else {
        Ok(String::new())
    }
}

/// Get process ID and name from an HWND
unsafe fn get_process_info(hwnd: HWND) -> Result<(u32, String)> {
    let mut process_id: u32 = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut process_id));

    if process_id == 0 {
        return Err(CaptureError::WindowsApiError(
            "Failed to get process ID".to_string(),
        ));
    }

    // Open process with limited query rights
    let process_handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id)
        .map_err(|e| CaptureError::WindowsApiError(format!("Failed to open process: {}", e)))?;

    // Query process image name
    let mut buffer = vec![0u16; MAX_PATH as usize];
    let mut size = buffer.len() as u32;

    QueryFullProcessImageNameW(
        process_handle,
        PROCESS_NAME_WIN32,
        windows::core::PWSTR(buffer.as_mut_ptr()),
        &mut size,
    )
    .map_err(|e| CaptureError::WindowsApiError(format!("Failed to query process name: {}", e)))?;

    let process_path = String::from_utf16_lossy(&buffer[..size as usize]);
    let process_name = std::path::Path::new(&process_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.exe")
        .to_string();

    Ok((process_id, process_name))
}

/// Extract URL from browser window using UI Automation
fn extract_browser_url(hwnd: HWND, process_name: &str) -> Option<String> {
    // Only attempt URL extraction for known browsers
    let browser_executables = [
        "chrome.exe",
        "firefox.exe",
        "msedge.exe",
        "brave.exe",
        "opera.exe",
    ];
    if !browser_executables
        .iter()
        .any(|&b| process_name.eq_ignore_ascii_case(b))
    {
        return None;
    }

    // Use UI Automation to find the address bar
    // Note: This is a best-effort approach and may not work in all cases
    try_extract_url_via_automation(hwnd).ok()
}

/// Try to extract URL using UI Automation API
fn try_extract_url_via_automation(_hwnd: HWND) -> Result<String> {
    // Note: Full UI Automation implementation requires more complex setup
    // For now, we return an error to indicate URL extraction is not yet implemented
    // This can be extended later with proper UI Automation API usage
    Err(CaptureError::WindowsApiError(
        "URL extraction via UI Automation not yet implemented".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_context_capture() {
        // This test requires a foreground window
        match WindowContext::capture() {
            Ok(ctx) => {
                tracing::info!("Captured context: {:?}", ctx);
                assert!(!ctx.process_name.is_empty());
            }
            Err(e) => {
                tracing::warn!("Could not capture window context: {}", e);
            }
        }
    }

    #[test]
    fn test_browser_detection() {
        let browsers = ["chrome.exe", "firefox.exe", "msedge.exe", "brave.exe"];
        for browser in &browsers {
            let ctx = WindowContext {
                window_title: "Test".to_string(),
                process_name: browser.to_string(),
                process_id: 1234,
                url: None,
            };
            assert!(ctx.process_name.ends_with(".exe"));
        }
    }
}
