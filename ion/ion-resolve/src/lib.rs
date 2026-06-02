//! Dependency resolution utilities for the ion frontend.
//!
//! Provides semver constraint matching and version comparison helpers.
//! A full SAT-based resolver and lock file generator are planned but
//! not yet implemented.

use semver::{Version, VersionReq};

/// Helper to parse a version string and check if it matches a semver constraint.
///
/// Implements [version-semantics-semver] using the standard `semver` crate.
pub fn matches_constraint(version_str: &str, constraint_str: &str) -> Result<bool, String> {
    let version = Version::parse(version_str)
        .map_err(|e| format!("Invalid version format '{}': {}", version_str, e))?;
    let requirement = VersionReq::parse(constraint_str)
        .map_err(|e| format!("Invalid version requirement '{}': {}", constraint_str, e))?;
    Ok(requirement.matches(&version))
}

/// Helper to compare two versions for total order sorting.
pub fn compare_versions(a_str: &str, b_str: &str) -> Result<std::cmp::Ordering, String> {
    let a =
        Version::parse(a_str).map_err(|e| format!("Invalid version format '{}': {}", a_str, e))?;
    let b =
        Version::parse(b_str).map_err(|e| format!("Invalid version format '{}': {}", b_str, e))?;
    Ok(a.cmp(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_constraint() {
        assert!(matches_constraint("1.2.3", ">= 1.0.0").unwrap());
        assert!(matches_constraint("1.2.3", "^1.2.0").unwrap());
        assert!(!matches_constraint("2.0.0", "^1.2.0").unwrap());
        assert!(matches_constraint("1.2.3", "*").unwrap());
        assert!(matches_constraint("1.2.3", "= 1.2.3").unwrap());
        assert!(matches_constraint("2.1.3", ">=2.0.0, <3.0.0").unwrap());

        assert!(matches_constraint("invalid", ">=1.0.0").is_err());
        assert!(matches_constraint("1.2.3", "invalid").is_err());
    }

    #[test]
    fn test_compare_versions() {
        assert_eq!(
            compare_versions("1.2.3", "1.2.3").unwrap(),
            std::cmp::Ordering::Equal
        );
        assert_eq!(
            compare_versions("1.2.3", "1.2.4").unwrap(),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_versions("1.2.5", "1.2.4").unwrap(),
            std::cmp::Ordering::Greater
        );
        assert_eq!(
            compare_versions("2.0.0", "1.9.9").unwrap(),
            std::cmp::Ordering::Greater
        );

        assert!(compare_versions("invalid", "1.2.3").is_err());
        assert!(compare_versions("1.2.3", "invalid").is_err());
    }
}
