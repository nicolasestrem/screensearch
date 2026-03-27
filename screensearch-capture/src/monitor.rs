//! Monitor enumeration and information
//!
//! This module provides functionality to enumerate and get information about
//! all connected display monitors using Windows APIs.

use crate::{CaptureError, Result};
use std::sync::Mutex;
use windows::Win32::{
    Foundation::{BOOL, LPARAM, RECT},
    Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW},
};

/// Information about a display monitor
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Monitor index (0-based)
    pub index: usize,

    /// Monitor device name
    pub name: String,

    /// Monitor width in pixels
    pub width: u32,

    /// Monitor height in pixels
    pub height: u32,

    /// Position X coordinate
    pub x: i32,

    /// Position Y coordinate
    pub y: i32,

    /// Whether this is the primary monitor
    pub is_primary: bool,

    /// Internal monitor handle (kept for potential future use)
    #[allow(dead_code)]
    pub(crate) handle: isize,
}

impl MonitorInfo {
    /// Enumerate all available monitors
    pub fn enumerate() -> Result<Vec<MonitorInfo>> {
        unsafe {
            let monitors: Mutex<Vec<MonitorInfo>> = Mutex::new(Vec::new());

            // Enumerate all monitors
            let monitors_ptr = &monitors as *const Mutex<Vec<MonitorInfo>> as isize;

            EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(enum_monitors_callback),
                LPARAM(monitors_ptr),
            );

            let mut result = monitors.into_inner().map_err(|e| {
                CaptureError::WindowsApiError(format!("Failed to get monitors: {}", e))
            })?;

            // Sort by index and assign proper indices
            result.sort_by_key(|m| m.index);
            for (i, monitor) in result.iter_mut().enumerate() {
                monitor.index = i;
            }

            if result.is_empty() {
                return Err(CaptureError::WindowsApiError(
                    "No monitors found".to_string(),
                ));
            }

            Ok(result)
        }
    }

    /// Get the primary monitor
    pub fn primary() -> Result<MonitorInfo> {
        let monitors = Self::enumerate()?;
        monitors
            .into_iter()
            .find(|m| m.is_primary)
            .ok_or_else(|| CaptureError::WindowsApiError("No primary monitor found".to_string()))
    }

    /// Get a specific monitor by index
    pub fn by_index(index: usize) -> Result<MonitorInfo> {
        let monitors = Self::enumerate()?;
        monitors
            .into_iter()
            .nth(index)
            .ok_or(CaptureError::InvalidMonitor(index))
    }
}

/// Callback function for EnumDisplayMonitors
unsafe extern "system" fn enum_monitors_callback(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprect: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let monitors_ptr = lparam.0 as *const Mutex<Vec<MonitorInfo>>;
    let monitors = &*monitors_ptr;

    // Get monitor info
    let mut monitor_info = MONITORINFOEXW {
        monitorInfo: windows::Win32::Graphics::Gdi::MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    if GetMonitorInfoW(hmonitor, &mut monitor_info.monitorInfo as *mut _ as *mut _).as_bool() {
        let rect = monitor_info.monitorInfo.rcMonitor;
        let width = (rect.right - rect.left) as u32;
        let height = (rect.bottom - rect.top) as u32;
        let is_primary = monitor_info.monitorInfo.dwFlags & 1 != 0; // MONITORINFOF_PRIMARY

        // Extract device name
        let name_end = monitor_info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(monitor_info.szDevice.len());
        let name = String::from_utf16_lossy(&monitor_info.szDevice[..name_end]);

        let info = MonitorInfo {
            index: 0, // Will be updated after enumeration
            name,
            width,
            height,
            x: rect.left,
            y: rect.top,
            is_primary,
            handle: hmonitor.0,
        };

        if let Ok(mut guard) = monitors.lock() {
            guard.push(info);
        }
    }

    BOOL::from(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enumerate_monitors() {
        let monitors = MonitorInfo::enumerate().expect("Failed to enumerate monitors");
        assert!(!monitors.is_empty(), "Should find at least one monitor");

        for monitor in &monitors {
            tracing::info!(
                "Monitor {}: {}x{} at ({}, {}), primary: {}",
                monitor.index,
                monitor.width,
                monitor.height,
                monitor.x,
                monitor.y,
                monitor.is_primary
            );
            assert!(monitor.width > 0);
            assert!(monitor.height > 0);
        }
    }

    #[test]
    fn test_primary_monitor() {
        let primary = MonitorInfo::primary().expect("Failed to get primary monitor");
        assert!(primary.is_primary);
        assert!(primary.width > 0);
        assert!(primary.height > 0);
        tracing::info!("Primary monitor: {}x{}", primary.width, primary.height);
    }

    #[test]
    fn test_monitor_by_index() {
        let monitor = MonitorInfo::by_index(0).expect("Failed to get monitor 0");
        assert_eq!(monitor.index, 0);
        assert!(monitor.width > 0);
        assert!(monitor.height > 0);
    }

    #[test]
    fn test_invalid_monitor_index() {
        let result = MonitorInfo::by_index(999);
        assert!(result.is_err());
    }
}
