//! Error types for UI automation operations

use thiserror::Error;

/// Errors that can occur during UI automation
#[derive(Error, Debug, Clone)]
pub enum AutomationError {
    /// Element not found with the given selector
    #[error("Element not found: {0}")]
    ElementNotFound(String),

    /// Operation timed out
    #[error("Operation timed out after {timeout_ms}ms: {operation}")]
    Timeout { operation: String, timeout_ms: u64 },

    /// Permission denied for the operation
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Platform-specific error from UIAutomation API
    #[error("Platform error: {0}")]
    PlatformError(String),

    /// Operation not supported on this element or platform
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    /// Invalid argument provided
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// Element is not in a valid state for the operation
    #[error("Invalid element state: {0}")]
    InvalidState(String),

    /// Internal error (should not happen under normal circumstances)
    #[error("Internal error: {0}")]
    Internal(String),
}

impl AutomationError {
    /// Create a new ElementNotFound error
    pub fn element_not_found(selector: impl std::fmt::Display) -> Self {
        Self::ElementNotFound(format!("No element matching selector: {}", selector))
    }

    /// Create a new Timeout error
    pub fn timeout(operation: impl Into<String>, timeout_ms: u64) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout_ms,
        }
    }

    /// Create a new PlatformError
    pub fn platform(error: impl std::fmt::Display) -> Self {
        Self::PlatformError(error.to_string())
    }

    /// Check if this error is a timeout
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    /// Check if this error is element not found
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::ElementNotFound(_))
    }
}
