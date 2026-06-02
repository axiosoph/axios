//! Concrete manifest format for the ion frontend.
//!
//! Implements the [`Manifest`] trait for `ion.toml`, including the
//! Compose system (With/As).

use std::collections::HashMap;

use atom_id::{Label, RawVersion};
use serde::{Deserialize, Serialize};

/// Represents a parsed `atom.toml` or `ion.toml` manifest file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SetDetails {
    /// Mirror URLs or the local sentinel `::`.
    pub mirrors: Vec<String>,
}

/// Composer configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
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

#[cfg(test)]
mod proptests {
    use std::collections::HashMap;

    use super::*;

    fn generate_valid_manifest(
        driver: &mut arbitrary::Unstructured<'_>,
    ) -> Result<IonManifest, arbitrary::Error> {
        // Generate valid label for package
        let mut label_chars = vec![driver.int_in_range(b'a'..=b'z')? as char];
        for _ in 0..driver.int_in_range(0..=8)? {
            let c = match driver.int_in_range(0..=1)? {
                0 => driver.int_in_range(b'a'..=b'z')? as char,
                1 => driver.int_in_range(b'0'..=b'9')? as char,
                _ => '-',
            };
            label_chars.push(c);
        }
        let label_str = label_chars.into_iter().collect::<String>();
        let label =
            Label::try_from(label_str.as_str()).map_err(|_| arbitrary::Error::IncorrectFormat)?;

        // Generate version
        let version_str = format!(
            "{}.{}.{}",
            driver.int_in_range(0..=99)?,
            driver.int_in_range(0..=99)?,
            driver.int_in_range(0..=99)?
        );
        let version = version_str
            .parse::<RawVersion>()
            .map_err(|_| arbitrary::Error::IncorrectFormat)?;

        let description = if driver.arbitrary::<bool>()? {
            Some(driver.arbitrary::<String>()?)
        } else {
            None
        };

        // Generate sets (minimum 1 mirror per set per [set-mirror-minimum])
        let num_sets = driver.int_in_range(0..=3)?;
        let mut sets = HashMap::new();
        let mut set_names = Vec::new();
        for _ in 0..num_sets {
            let set_name = format!("set-{}", driver.arbitrary::<u8>()?);
            let num_mirrors = driver.int_in_range(1..=3)?;
            let mut mirrors = Vec::new();
            for _ in 0..num_mirrors {
                mirrors.push(format!(
                    "https://example.com/mirror-{}",
                    driver.arbitrary::<u8>()?
                ));
            }
            sets.insert(set_name.clone(), SetDetails { mirrors });
            set_names.push(set_name);
        }

        let package = PackageSection {
            label,
            version,
            description,
            sets,
        };

        // Generate compose
        let r#use = if driver.arbitrary::<bool>()? {
            Some(format!("composer-{}", driver.arbitrary::<u8>()?))
        } else {
            None
        };
        let at = if driver.arbitrary::<bool>()? {
            Some(format!("{}.0.0", driver.arbitrary::<u8>()?))
        } else {
            None
        };
        let entry = if driver.arbitrary::<bool>()? {
            Some("default.nix".to_string())
        } else {
            None
        };
        let mut args = HashMap::new();
        let num_args = driver.int_in_range(0..=3)?;
        for _ in 0..num_args {
            args.insert(
                format!("arg-{}", driver.arbitrary::<u8>()?),
                driver.arbitrary::<String>()?,
            );
        }
        let compose = ComposeSection {
            r#use,
            at,
            entry,
            args,
        };

        // Generate deps (keys must be subset of set_names per [set-declaration-completeness])
        let mut from = HashMap::new();
        if !set_names.is_empty() {
            let num_dep_sets = driver.int_in_range(0..=set_names.len())?;
            for _ in 0..num_dep_sets {
                let set_name = driver.choose(&set_names)?.clone();
                let mut dep_map = HashMap::new();
                let num_deps = driver.int_in_range(0..=3)?;
                for _ in 0..num_deps {
                    let dep_label = format!("dep-{}", driver.arbitrary::<u8>()?);
                    let dep_version = format!("{}.0.0", driver.arbitrary::<u8>()?);
                    dep_map.insert(dep_label, dep_version);
                }
                from.insert(set_name, dep_map);
            }
        }
        let deps = DepsSection { from };

        Ok(IonManifest {
            package,
            compose,
            deps,
        })
    }

    #[test]
    fn ion_manifest_parse_roundtrip() {
        bolero::check!().with_type::<Vec<u8>>().for_each(|data| {
            let mut driver = arbitrary::Unstructured::new(data);
            if let Ok(manifest) = generate_valid_manifest(&mut driver) {
                let serialized = toml::to_string(&manifest).unwrap();
                let parsed = IonManifest::parse(&serialized).unwrap();
                assert_eq!(parsed, manifest);
            }
        });
    }

    #[test]
    fn ion_manifest_parse_no_panic() {
        bolero::check!().with_type::<Vec<u8>>().for_each(|data| {
            if let Ok(s) = std::str::from_utf8(data) {
                let _ = IonManifest::parse(s);
            }
        });
    }

    #[test]
    fn ion_manifest_invalid_rejected() {
        bolero::check!().with_type::<Vec<u8>>().for_each(|data| {
            let mut driver = arbitrary::Unstructured::new(data);
            if let Ok(mut manifest) = generate_valid_manifest(&mut driver) {
                match driver.int_in_range(0..=4).unwrap_or(0) {
                    0 => {
                        if !manifest.package.sets.is_empty() {
                            let keys: Vec<String> = manifest.package.sets.keys().cloned().collect();
                            if let Ok(k) = driver.choose(&keys) {
                                if let Some(details) = manifest.package.sets.get_mut(k) {
                                    details.mirrors.clear();
                                }
                                let serialized = toml::to_string(&manifest).unwrap();
                                assert!(IonManifest::parse(&serialized).is_err());
                            }
                        }
                    },
                    1 => {
                        let undeclared = "undeclared-set-name".to_string();
                        if !manifest.package.sets.contains_key(&undeclared) {
                            let mut dep_map = HashMap::new();
                            dep_map.insert("dep-name".to_string(), "1.0.0".to_string());
                            manifest.deps.from.insert(undeclared, dep_map);
                            let serialized = toml::to_string(&manifest).unwrap();
                            assert!(IonManifest::parse(&serialized).is_err());
                        }
                    },
                    2 => {
                        // Mutate the serialized TOML string to have an invalid label starting with
                        // a digit.
                        let serialized = toml::to_string(&manifest).unwrap();
                        let target_str = format!("label = \"{}\"", manifest.package.label.as_ref());
                        let serialized = serialized.replace(&target_str, "label = \"123invalid\"");
                        assert!(IonManifest::parse(&serialized).is_err());
                    },
                    3 => {
                        // Remove the required [compose] section.
                        if let Ok(mut value) =
                            toml::to_string(&manifest).unwrap().parse::<toml::Value>()
                        {
                            if let Some(table) = value.as_table_mut() {
                                table.remove("compose");
                                let serialized = toml::to_string(&value).unwrap();
                                assert!(IonManifest::parse(&serialized).is_err());
                            }
                        }
                    },
                    _ => {
                        // Inject an unrecognized field to test deny_unknown_fields.
                        if let Ok(mut value) =
                            toml::to_string(&manifest).unwrap().parse::<toml::Value>()
                        {
                            if let Some(table) = value.as_table_mut() {
                                if let Some(package) =
                                    table.get_mut("package").and_then(|v| v.as_table_mut())
                                {
                                    package.insert(
                                        "unknown_field_xyz".to_string(),
                                        toml::Value::String("invalid".to_string()),
                                    );
                                    let serialized = toml::to_string(&value).unwrap();
                                    assert!(IonManifest::parse(&serialized).is_err());
                                }
                            }
                        }
                    },
                }
            }
        });
    }
}
