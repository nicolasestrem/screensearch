//! Update checker module
//!
//! Checks for new releases on GitHub and notifies users.

use crate::version::{Version, GITHUB_RELEASES_URL};
use serde::Deserialize;
use tracing::{debug, error, info};

/// Update information from GitHub
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub version: String,
    pub download_url: String,
    pub release_notes: String,
    pub published_at: String,
}

/// GitHub release API response (partial)
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: String,
}

/// Check for updates in the background
///
/// This function is non-blocking and has a 10-second timeout.
/// Returns None if no update is available or if check fails.
pub async fn check_updates() -> Option<UpdateInfo> {
    debug!("Checking for updates...");

    // Create HTTP client with timeout
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(format!("ScreenSearch/{}", crate::version::VERSION))
        .build()
        .ok()?;

    // Query GitHub API
    let response = match client.get(GITHUB_RELEASES_URL).send().await {
        Ok(resp) => resp,
        Err(e) => {
            debug!("Failed to check for updates: {}", e);
            return None;
        }
    };

    if !response.status().is_success() {
        debug!("GitHub API returned status: {}", response.status());
        return None;
    }

    // Parse response
    let release: GitHubRelease = match response.json().await {
        Ok(release) => release,
        Err(e) => {
            error!("Failed to parse GitHub release: {}", e);
            return None;
        }
    };

    // Parse versions
    let current_version = Version::current();
    let latest_version = Version::parse(&release.tag_name)?;

    // Compare versions
    if latest_version.is_newer_than(&current_version) {
        info!(
            "Update available: {} -> {}",
            current_version.to_string(),
            latest_version.to_string()
        );

        Some(UpdateInfo {
            version: latest_version.to_string(),
            download_url: release.html_url,
            release_notes: release.body.unwrap_or_default(),
            published_at: release.published_at,
        })
    } else {
        debug!("Already on latest version: {}", current_version.to_string());
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_github_release_parsing() {
        let json = r#"{
            "tag_name": "v0.2.0",
            "html_url": "https://github.com/nicolasestrem/screensearch/releases/tag/v0.2.0",
            "body": "Release notes here",
            "published_at": "2025-01-15T10:00:00Z"
        }"#;

        let release: GitHubRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.tag_name, "v0.2.0");
        assert!(release.body.is_some());
    }

    #[tokio::test]
    async fn test_update_check_current_version() {
        // This test would require mocking the HTTP client
        // For now, just verify the function signature is correct
        let _result = check_updates().await;
    }
}
