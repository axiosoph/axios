//! Concrete manifest format for the ion frontend.
//!
//! Implements the [`Manifest`] trait for `ion.toml`, including the
//! Compose system (With/As).

use std::collections::HashMap;

use atom_id::{Label, RawVersion};
use serde::{Deserialize, Serialize};

/// Represents a parsed `atom.toml` or `ion.toml` manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IonManifest {
    /// Required package section.
    pub package: PackageSection,
    /// Required compose section.
    pub compose: ComposeSection,
    /// Optional dependencies section.
    #[serde(default)]
    pub deps: DepsSection,
}

/// Declares the atom's label, version, and optional metadata/sets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageSection {
    /// Human-readable package label.
    pub label: Label,
    /// Semantic version of the package.
    pub version: RawVersion,
    /// Optional description.
    pub description: Option<String>,
    /// Declared sets mapping tag name to set mirrors.
    #[serde(default)]
    pub sets: HashMap<String, SetDetails>,
}

/// Mirrors list for an atom-set.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetDetails {
    /// Mirror URLs or the local sentinel `::`.
    pub mirrors: Vec<String>,
}

/// Composer configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeSection {
    /// Special composer or atom identifier.
    pub r#use: Option<String>,
    /// Pinned version of the composer.
    pub at: Option<String>,
    /// Evaluation entrypoint.
    pub entry: Option<String>,
    /// Verbatim arguments passed to the evaluator.
    #[serde(default)]
    pub args: HashMap<String, String>,
}

/// Dependency declarations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DepsSection {
    /// Dependencies grouped by declared sets.
    #[serde(default)]
    pub from: HashMap<String, HashMap<String, String>>,
}

impl atom_core::Manifest for IonManifest {
    fn label(&self) -> &Label {
        &self.package.label
    }

    fn version(&self) -> &RawVersion {
        &self.package.version
    }
}

impl IonManifest {
    /// Parses a manifest from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns an error if the manifest violates required sections or set constraints.
    pub fn parse(content: &str) -> Result<Self, String> {
        let manifest: Self =
            toml::from_str(content).map_err(|e| format!("Failed to parse manifest: {}", e))?;

        // [set-declaration-completeness]: Every set name referenced in deps.from.<set> MUST have a corresponding entry in package.sets.
        for set in manifest.deps.from.keys() {
            if !manifest.package.sets.contains_key(set) {
                return Err(format!(
                    "Set '{}' referenced in dependency but not declared under package.sets",
                    set
                ));
            }
        }

        // [set-mirror-minimum]: Each set declared in package.sets MUST contain at least one mirror.
        for (name, set_details) in &manifest.package.sets {
            if set_details.mirrors.is_empty() {
                return Err(format!("Set '{}' has no mirrors", name));
            }
        }

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_manifest_parsing() {
        let content = r#"
            [package]
            label = "my_package"
            version = "1.0.0"
            description = "A valid package"

            [package.sets.main]
            mirrors = ["https://example.com/mirror"]

            [compose]
            entry = "default.nix"

            [deps.from.main]
            dependency_a = "2.0.0"
        "#;

        let manifest = IonManifest::parse(content).unwrap();
        assert_eq!(manifest.package.label.as_ref(), "my_package");
        assert_eq!(manifest.package.version.as_str(), "1.0.0");
        assert_eq!(manifest.compose.entry.as_deref(), Some("default.nix"));
    }

    #[test]
    fn test_manifest_missing_required_sections() {
        let content = r#"
            [package]
            label = "my_package"
            version = "1.0.0"
        "#;
        // Missing [compose] section entirely
        assert!(IonManifest::parse(content).is_err());
    }

    #[test]
    fn test_manifest_deny_unknown_fields() {
        let content = r#"
            [package]
            label = "my_package"
            version = "1.0.0"
            unknown_field = "invalid"

            [compose]
        "#;
        assert!(IonManifest::parse(content).is_err());
    }

    #[test]
    fn test_set_declaration_completeness() {
        let content = r#"
            [package]
            label = "my_package"
            version = "1.0.0"

            [compose]

            [deps.from.undeclared_set]
            dep = "1.0.0"
        "#;
        assert!(IonManifest::parse(content).is_err());
    }

    #[test]
    fn test_set_mirror_minimum() {
        let content = r#"
            [package]
            label = "my_package"
            version = "1.0.0"

            [package.sets.main]
            mirrors = [] # empty mirror list

            [compose]
        "#;
        assert!(IonManifest::parse(content).is_err());
    }
}
