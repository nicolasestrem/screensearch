// ! Version parsing and comparison utilities
//!
//! Provides semantic versioning support for update checking.

use std::cmp::Ordering;

/// Current application version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository for update checking
pub const GITHUB_REPO: &str = "nicolasestrem/screensearch";

/// GitHub API URL for latest release
pub const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/nicolasestrem/screensearch/releases/latest";

/// Semantic version representation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Version {
    /// Parse a version string (e.g., "0.2.0" or "v0.2.0")
    pub fn parse(version_str: &str) -> Option<Self> {
        let cleaned = version_str.trim().trim_start_matches('v');
        let parts: Vec<&str> = cleaned.split('.').collect();

        if parts.len() != 3 {
            return None;
        }

        Some(Version {
            major: parts[0].parse().ok()?,
            minor: parts[1].parse().ok()?,
            patch: parts[2].parse().ok()?,
        })
    }

    /// Get the current application version
    pub fn current() -> Self {
        Self::parse(VERSION).expect("Invalid package version in Cargo.toml")
    }

    /// Check if this version is newer than another
    pub fn is_newer_than(&self, other: &Self) -> bool {
        self > other
    }

    /// Convert to string representation
    pub fn to_string(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.major.cmp(&other.major) {
            Ordering::Equal => match self.minor.cmp(&other.minor) {
                Ordering::Equal => self.patch.cmp(&other.patch),
                other => other,
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_parsing() {
        let v = Version::parse("1.2.3").unwrap();
        assert_eq!(v.major, 1);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 3);

        let v = Version::parse("v0.2.0").unwrap();
        assert_eq!(v.major, 0);
        assert_eq!(v.minor, 2);
        assert_eq!(v.patch, 0);

        assert!(Version::parse("invalid").is_none());
        assert!(Version::parse("1.2").is_none());
    }

    #[test]
    fn test_version_comparison() {
        let v1 = Version::parse("1.0.0").unwrap();
        let v2 = Version::parse("1.0.1").unwrap();
        let v3 = Version::parse("1.1.0").unwrap();
        let v4 = Version::parse("2.0.0").unwrap();

        assert!(v2.is_newer_than(&v1));
        assert!(v3.is_newer_than(&v2));
        assert!(v4.is_newer_than(&v3));
        assert!(!v1.is_newer_than(&v2));
    }

    #[test]
    fn test_current_version() {
        let current = Version::current();
        assert_eq!(current.to_string(), VERSION);
    }
}
