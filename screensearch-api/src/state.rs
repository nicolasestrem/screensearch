//! Application state management

use screensearch_automation::AutomationEngine;
use screensearch_db::DatabaseManager;
use std::sync::Arc;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Database manager for querying captured data
    pub db: Arc<DatabaseManager>,

    /// Automation engine for UI control
    pub automation: Arc<AutomationEngine>,
}

impl AppState {
    /// Create new application state
    pub fn new(db: DatabaseManager, automation: AutomationEngine) -> Self {
        Self {
            db: Arc::new(db),
            automation: Arc::new(automation),
        }
    }
}
