//! Window management utilities

use crate::errors::AutomationError;
use serde::{Deserialize, Serialize};

/// Information about a window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Window title
    pub title: String,
    /// Process name
    pub process_name: String,
    /// X coordinate
    pub x: i32,
    /// Y coordinate
    pub y: i32,
    /// Width
    pub width: i32,
    /// Height
    pub height: i32,
    /// Is visible
    pub is_visible: bool,
    /// Is minimized
    pub is_minimized: bool,
    /// Window handle (HWND as usize)
    #[serde(skip)]
    pub handle: usize,
}

/// Window manager for enumerating and managing windows
pub struct WindowManager {}

impl WindowManager {
    /// Create a new window manager
    pub fn new() -> Self {
        Self {}
    }

    /// Enumerate all visible windows
    ///
    /// Returns a list of windows that are visible and not tool windows
    pub fn enumerate(&self) -> Result<Vec<WindowInfo>, AutomationError> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::*;
            use windows::Win32::UI::WindowsAndMessaging::*;

            let mut windows = Vec::new();

            unsafe {
                let result = EnumWindows(
                    Some(enum_windows_callback),
                    LPARAM(&mut windows as *mut _ as isize),
                );

                if result.is_ok() {
                    Ok(windows)
                } else {
                    Err(AutomationError::platform("Failed to enumerate windows"))
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AutomationError::UnsupportedOperation(
                "Window enumeration only supported on Windows".to_string(),
            ))
        }
    }

    /// Get the active (focused) window
    pub fn get_active(&self) -> Result<Option<WindowInfo>, AutomationError> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

            unsafe {
                let hwnd = GetForegroundWindow();
                if hwnd.0 == 0 {
                    return Ok(None);
                }

                get_window_info(hwnd).map(Some)
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AutomationError::UnsupportedOperation(
                "Window operations only supported on Windows".to_string(),
            ))
        }
    }

    /// Find windows by title (partial match)
    pub fn find_by_title(&self, title: &str) -> Result<Vec<WindowInfo>, AutomationError> {
        let all_windows = self.enumerate()?;
        Ok(all_windows
            .into_iter()
            .filter(|w| w.title.to_lowercase().contains(&title.to_lowercase()))
            .collect())
    }

    /// Find windows by process name
    pub fn find_by_process(&self, process_name: &str) -> Result<Vec<WindowInfo>, AutomationError> {
        let all_windows = self.enumerate()?;
        Ok(all_windows
            .into_iter()
            .filter(|w| {
                w.process_name
                    .to_lowercase()
                    .contains(&process_name.to_lowercase())
            })
            .collect())
    }

    /// Focus (activate) a window by handle
    pub fn focus_window(&self, handle: usize) -> Result<(), AutomationError> {
        #[cfg(target_os = "windows")]
        {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

            unsafe {
                let hwnd = HWND(handle as isize);
                let result = SetForegroundWindow(hwnd);
                if result.as_bool() {
                    Ok(())
                } else {
                    Err(AutomationError::platform("Failed to focus window"))
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            Err(AutomationError::UnsupportedOperation(
                "Window operations only supported on Windows".to_string(),
            ))
        }
    }
}

impl Default for WindowManager {
    fn default() -> Self {
        Self::new()
    }
}

// Windows-specific implementations
#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Threading::*;
    use windows::Win32::UI::WindowsAndMessaging::*;

    /// Callback for EnumWindows
    pub(super) unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = &mut *(lparam.0 as *mut Vec<WindowInfo>);

        // Check if window should be included
        if should_include_window(hwnd) {
            if let Ok(info) = get_window_info(hwnd) {
                windows.push(info);
            }
        }

        TRUE // Continue enumeration
    }

    /// Check if a window should be included in enumeration
    unsafe fn should_include_window(hwnd: HWND) -> bool {
        use windows::Win32::UI::WindowsAndMessaging::{
            GW_OWNER, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
        };

        // Must be visible
        if !IsWindowVisible(hwnd).as_bool() {
            return false;
        }

        // Check extended window style
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;

        // Skip tool windows unless they have WS_EX_APPWINDOW
        if (ex_style & WS_EX_TOOLWINDOW.0) != 0 && (ex_style & WS_EX_APPWINDOW.0) == 0 {
            return false;
        }

        // Skip windows with owners (unless they have WS_EX_APPWINDOW)
        if GetWindow(hwnd, GW_OWNER).0 != 0 && (ex_style & WS_EX_APPWINDOW.0) == 0 {
            return false;
        }

        // Must have a title
        let title_len = GetWindowTextLengthW(hwnd);
        if title_len == 0 {
            return false;
        }

        true
    }

    /// Get window information from HWND
    pub(super) unsafe fn get_window_info(hwnd: HWND) -> Result<WindowInfo, AutomationError> {
        // Get window title
        let title_len = GetWindowTextLengthW(hwnd);
        let mut title_buf = vec![0u16; (title_len + 1) as usize];
        GetWindowTextW(hwnd, &mut title_buf);
        let title = String::from_utf16_lossy(&title_buf[..title_len as usize]);

        // Get window rect
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).map_err(AutomationError::platform)?;

        // Get process ID and name
        let mut process_id: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));

        let process_name = if process_id != 0 {
            get_process_name(process_id).unwrap_or_else(|| "Unknown".to_string())
        } else {
            "Unknown".to_string()
        };

        // Check if minimized
        let is_minimized = IsIconic(hwnd).as_bool();

        Ok(WindowInfo {
            title,
            process_name,
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
            is_visible: true,
            is_minimized,
            handle: hwnd.0 as usize,
        })
    }

    /// Get process name from process ID
    unsafe fn get_process_name(process_id: u32) -> Option<String> {
        use windows::core::PWSTR;

        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;

        let mut buffer = vec![0u16; 260];
        let mut size = buffer.len() as u32;

        let pwstr = PWSTR::from_raw(buffer.as_mut_ptr());
        if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, pwstr, &mut size).is_ok() {
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            let name = std::path::Path::new(&path)
                .file_name()?
                .to_str()?
                .to_string();
            let _ = CloseHandle(handle);
            Some(name)
        } else {
            let _ = CloseHandle(handle);
            None
        }
    }
}

#[cfg(target_os = "windows")]
use windows_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_manager_creation() {
        let manager = WindowManager::new();
        assert!(std::mem::size_of_val(&manager) >= 0);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_enumerate_windows() {
        let manager = WindowManager::new();
        let result = manager.enumerate();
        assert!(result.is_ok());

        let windows = result.unwrap();
        // Should have at least one window on a running system
        assert!(!windows.is_empty(), "Should find at least one window");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_get_active_window() {
        let manager = WindowManager::new();
        let result = manager.get_active();
        assert!(result.is_ok());
    }
}
