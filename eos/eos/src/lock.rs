//! Lock file parsing and validation.

use std::collections::{HashMap, HashSet};

use atom_id::AtomId;

/// Represents a parsed lock file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockFile {
    /// Schema version (must be 0).
    pub version: u64,
    /// Declared atom-sets.
    pub sets: HashMap<String, SetDetails>,
    /// Composer configuration.
    pub compose: ComposeConfig,
    /// Unified dependency array.
    pub deps: Vec<Dependency>,
}

/// Details of an atom-set in the lock file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SetDetails {
    /// Human-readable atom-set identifier.
    pub tag: String,
    /// Mirrors for fetching this atom-set.
    pub mirrors: Vec<String>,
}

/// Composer configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeConfig {
    /// Composer atom identifier (or special values like "nix" or "static").
    #[serde(default)]
    pub r#use: Option<String>,
    /// Pinned version of the composer atom.
    pub at: Option<String>,
    /// Evaluation entrypoint path.
    pub entry: Option<String>,
    /// Verbatim arguments passed to the evaluator.
    #[serde(default)]
    pub args: HashMap<String, String>,
}

/// Unified dependency item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Dependency {
    /// An atom dependency.
    #[serde(rename = "atom")]
    Atom(AtomDep),
    /// A plain Nix file fetch.
    #[serde(rename = "nix")]
    Nix(NixDep),
    /// A Nix Git repository fetch.
    #[serde(rename = "nix+git")]
    NixGit(NixGitDep),
    /// A Nix Tarball fetch.
    #[serde(rename = "nix+tar")]
    NixTar(NixTarDep),
    /// A Nix Source File fetch.
    #[serde(rename = "nix+src")]
    NixSrc(NixSrcDep),
}

impl Dependency {
    /// Returns the owner of the dependency if specified.
    #[must_use]
    pub fn owner(&self) -> Option<&AtomId> {
        match self {
            Dependency::Atom(_) => None,
            Dependency::Nix(d) => d.owner.as_ref(),
            Dependency::NixGit(d) => d.owner.as_ref(),
            Dependency::NixTar(d) => d.owner.as_ref(),
            Dependency::NixSrc(d) => d.owner.as_ref(),
        }
    }

    /// Returns the human-readable name/label component of the dependency.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Dependency::Atom(d) => &d.label,
            Dependency::Nix(d) => &d.name,
            Dependency::NixGit(d) => &d.name,
            Dependency::NixTar(d) => &d.name,
            Dependency::NixSrc(d) => &d.name,
        }
    }
}

/// An atom dependency.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AtomDep {
    /// Human-readable label within the atom-set.
    pub label: String,
    /// Resolved semantic version.
    pub version: String,
    /// Anchor hash referencing [sets.<anchor>].
    pub set: String,
    /// Pinned Git revision (commit hash).
    pub rev: Option<String>,
    /// Content-addressed atom identifier.
    pub id: AtomId,
    /// Transitive atom dependencies.
    #[serde(default)]
    pub requires: Vec<AtomId>,
    /// Direct vs transitive flag.
    #[serde(default = "default_true")]
    pub direct: bool,
}

fn default_true() -> bool {
    true
}

/// A plain Nix file fetch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NixDep {
    /// Human-readable identifier.
    pub name: String,
    /// Fetch URL.
    pub url: String,
    /// SRI-format content hash.
    pub hash: String,
    /// Owning atom.
    #[serde(default)]
    pub owner: Option<AtomId>,
}

/// A Nix Git repository fetch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NixGitDep {
    /// Human-readable identifier.
    pub name: String,
    /// Git repository URL.
    pub url: String,
    /// Pinned commit hash.
    pub rev: String,
    /// Resolved version.
    #[serde(default)]
    pub version: Option<String>,
    /// Owning atom.
    #[serde(default)]
    pub owner: Option<AtomId>,
}

/// A Nix Tarball fetch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NixTarDep {
    /// Human-readable identifier.
    pub name: String,
    /// Tarball URL.
    pub url: String,
    /// SRI-format content hash.
    pub hash: String,
    /// Owning atom.
    #[serde(default)]
    pub owner: Option<AtomId>,
}

/// A Nix Source File fetch.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NixSrcDep {
    /// Human-readable identifier.
    pub name: String,
    /// Source URL.
    pub url: String,
    /// SRI-format content hash.
    pub hash: String,
    /// Owning atom.
    #[serde(default)]
    pub owner: Option<AtomId>,
}

impl LockFile {
    /// Parses a lock file from a TOML string.
    ///
    /// # Errors
    ///
    /// Returns a parsing error if the TOML is invalid.
    pub fn parse(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Validates lock file structural invariants.
    ///
    /// # Errors
    ///
    /// Returns a validation error description if any invariant is violated.
    pub fn validate(&self) -> Result<(), String> {
        // 1. [lock-version-field]: version must be 0
        if self.version != 0 {
            return Err(format!("Unsupported lock file version: {}", self.version));
        }

        // Validate set declarations
        for (anchor, set_details) in &self.sets {
            // [lock-set-key-format]: anchor hash format (40 or 64 lowercase hex chars)
            if !((anchor.len() == 40 || anchor.len() == 64)
                && anchor
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))
            {
                return Err(format!("Invalid set anchor key format: '{}'", anchor));
            }
            // [lock-set-tag]: tag field must be non-empty string
            if set_details.tag.trim().is_empty() {
                return Err(format!("Set '{}' has an empty tag", anchor));
            }
            // [lock-set-mirrors]: mirrors field must be non-empty
            if set_details.mirrors.is_empty() {
                return Err(format!("Set '{}' has no mirrors", anchor));
            }
            // [lock-set-mirror-local-sentinel]: if "::" is present, it must be the sole entry
            let has_local = set_details.mirrors.iter().any(|m| m == "::");
            if has_local && set_details.mirrors.len() > 1 {
                return Err(format!(
                    "Set '{}' contains local sentinel '::' along with other mirrors",
                    anchor
                ));
            }
        }

        // Collect and validate atom dependencies
        let mut atoms = HashMap::new();
        for dep in &self.deps {
            if let Dependency::Atom(atom_dep) = dep {
                // [lock-atom-label]: must be a valid Label grammar representation
                if let Err(e) = atom_id::Label::try_from(atom_dep.label.as_str()) {
                    return Err(format!(
                        "Invalid label '{}' in atom ID {}: {}",
                        atom_dep.label, atom_dep.id, e
                    ));
                }

                // [lock-atom-version]: must be valid SemVer
                if let Err(e) = semver::Version::parse(&atom_dep.version) {
                    return Err(format!(
                        "Invalid semver version '{}' in atom {}: {}",
                        atom_dep.version, atom_dep.id, e
                    ));
                }

                // [lock-atom-set-ref]: set must exist in self.sets
                let set_details = self.sets.get(&atom_dep.set).ok_or_else(|| {
                    format!(
                        "Atom {} references undeclared set: {}",
                        atom_dep.id, atom_dep.set
                    )
                })?;

                // [lock-atom-rev-optional]: rev may be absent only for local sets ("::")
                let is_local = set_details.mirrors.len() == 1 && set_details.mirrors[0] == "::";
                if !is_local && atom_dep.rev.is_none() {
                    return Err(format!(
                        "Atom {} is remote (set {}) but lacks a rev field",
                        atom_dep.id, atom_dep.set
                    ));
                }

                if atoms.insert(atom_dep.id.clone(), atom_dep).is_some() {
                    return Err(format!(
                        "Duplicate atom dependency with id: {}",
                        atom_dep.id
                    ));
                }
            }
        }

        // 3. [lock-requires-closure] & [lock-owner-closure]
        // Check requires closure
        for atom in atoms.values() {
            for req in &atom.requires {
                if !atoms.contains_key(req) {
                    return Err(format!(
                        "Atom {} requires missing dependency: {}",
                        atom.id, req
                    ));
                }
            }
        }

        // Check owner closure for non-atom deps
        for dep in &self.deps {
            if let Some(owner) = dep.owner()
                && !atoms.contains_key(owner)
            {
                return Err(format!(
                    "Dependency {} owned by missing atom: {}",
                    dep.name(),
                    owner
                ));
            }
        }

        // Check compose.use closure if it's an atom-id
        if let Some(ref use_str) = self.compose.r#use
            && use_str != "nix"
            && use_str != "static"
        {
            let compose_atom_id = use_str
                .parse::<AtomId>()
                .map_err(|e| format!("Invalid atom ID in compose.use: {}", e))?;
            if !atoms.contains_key(&compose_atom_id) {
                return Err(format!(
                    "Composer atom {} specified in compose.use is missing from deps",
                    compose_atom_id
                ));
            }
        }

        // 4. [lock-dag-acyclicity]
        // Detect cycles in requires graph using DFS
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();

        fn has_cycle(
            node: &AtomId,
            atoms: &HashMap<AtomId, &AtomDep>,
            visited: &mut HashSet<AtomId>,
            rec_stack: &mut HashSet<AtomId>,
        ) -> bool {
            if rec_stack.contains(node) {
                return true;
            }
            if visited.contains(node) {
                return false;
            }

            visited.insert(node.clone());
            rec_stack.insert(node.clone());

            if let Some(atom_dep) = atoms.get(node) {
                for neighbor in &atom_dep.requires {
                    if has_cycle(neighbor, atoms, visited, rec_stack) {
                        return true;
                    }
                }
            }

            rec_stack.remove(node);
            false
        }

        for atom_id in atoms.keys() {
            if !visited.contains(atom_id)
                && has_cycle(atom_id, &atoms, &mut visited, &mut rec_stack)
            {
                return Err("Dependency graph contains cycles".to_string());
            }
        }

        Ok(())
    }
}

// TODO: migrate to ion-lock
#[cfg(test)]
mod tests {
    use bolero::check;

    use super::*;

    fn generate_valid_lockfile(
        driver: &mut arbitrary::Unstructured<'_>,
    ) -> Result<LockFile, arbitrary::Error> {
        let num_sets = driver.int_in_range(1..=3)?;
        let mut sets = HashMap::new();
        let mut anchors = Vec::new();

        for _ in 0..num_sets {
            let anchor_bytes = driver.arbitrary::<[u8; 20]>()?;
            let anchor_hex = hex::encode(anchor_bytes);

            let tag = format!("set-{}", driver.arbitrary::<u8>()?);
            let mirrors = if driver.arbitrary::<bool>()? {
                vec!["::".to_string()]
            } else {
                vec![format!("https://git.example.com/{}.git", tag)]
            };

            sets.insert(anchor_hex.clone(), SetDetails { tag, mirrors });
            anchors.push((anchor_bytes.to_vec(), anchor_hex));
        }

        // Generate some atoms
        let num_atoms = driver.int_in_range(1..=5)?;
        let mut atom_deps: Vec<AtomDep> = Vec::new();
        let mut generated_ids = std::collections::HashSet::new();

        for i in 0..num_atoms {
            let (anchor_bytes, anchor_hex) = driver.choose(&anchors)?.clone();

            // Loop to get a unique label for this anchor
            let label;
            let mut attempts = 0;
            loop {
                let mut label_chars = vec![driver.int_in_range(b'a'..=b'z')? as char];
                for _ in 0..driver.int_in_range(0..=8)? {
                    let c = match driver.int_in_range(0..=1)? {
                        0 => driver.int_in_range(b'a'..=b'z')? as char,
                        1 => driver.int_in_range(b'0'..=b'9')? as char,
                        _ => '-',
                    };
                    label_chars.push(c);
                }
                let cand_label = label_chars.into_iter().collect::<String>();
                if !generated_ids.contains(&(anchor_bytes.clone(), cand_label.clone())) {
                    label = cand_label;
                    break;
                }
                attempts += 1;
                if attempts > 10 {
                    return Err(arbitrary::Error::IncorrectFormat);
                }
            }
            generated_ids.insert((anchor_bytes.clone(), label.clone()));

            let version = format!(
                "{}.{}.{}",
                driver.int_in_range(0..=9)?,
                driver.int_in_range(0..=9)?,
                driver.int_in_range(0..=9)?
            );

            // rev is required if remote, optional if local
            let set_details = sets.get(&anchor_hex).unwrap();
            let is_local = set_details.mirrors.len() == 1 && set_details.mirrors[0] == "::";
            let rev = if is_local {
                if driver.arbitrary::<bool>()? {
                    Some(hex::encode(driver.arbitrary::<[u8; 20]>()?))
                } else {
                    None
                }
            } else {
                Some(hex::encode(driver.arbitrary::<[u8; 20]>()?))
            };

            let label_parsed = atom_id::Label::try_from(label.as_str()).unwrap();
            let anchor_struct = atom_id::Anchor::new(anchor_bytes);
            let id = AtomId::new(anchor_struct, label_parsed);

            // Topological sort trick for acyclic requires:
            // atom_i can only require atoms from 0..i
            let mut requires = Vec::new();
            if i > 0 {
                for j in 0..i {
                    if driver.arbitrary::<bool>()? {
                        requires.push(atom_deps[j].id.clone());
                    }
                }
            }

            atom_deps.push(AtomDep {
                label,
                version,
                set: anchor_hex,
                rev,
                id,
                requires,
                direct: driver.arbitrary::<bool>()?,
            });
        }

        // Add other dependency types (Nix, NixGit, etc.)
        let mut deps = Vec::new();
        for atom_dep in atom_deps.clone() {
            deps.push(Dependency::Atom(atom_dep));
        }

        let num_other_deps = driver.int_in_range(0..=3)?;
        for _ in 0..num_other_deps {
            let name = format!("dep-{}", driver.arbitrary::<u8>()?);
            let url = format!("https://example.com/{}.nix", name);
            let hash = format!("sha256:{}", hex::encode(driver.arbitrary::<[u8; 32]>()?));
            let owner = if driver.arbitrary::<bool>()? && !atom_deps.is_empty() {
                Some(driver.choose(&atom_deps)?.id.clone())
            } else {
                None
            };

            let dep = match driver.int_in_range(0..=2)? {
                0 => Dependency::Nix(NixDep {
                    name,
                    url,
                    hash,
                    owner,
                }),
                1 => Dependency::NixGit(NixGitDep {
                    name,
                    url,
                    rev: hex::encode(driver.arbitrary::<[u8; 20]>()?),
                    version: Some("1.0.0".to_string()),
                    owner,
                }),
                2 => Dependency::NixTar(NixTarDep {
                    name,
                    url,
                    hash,
                    owner,
                }),
                _ => Dependency::NixSrc(NixSrcDep {
                    name,
                    url,
                    hash,
                    owner,
                }),
            };
            deps.push(dep);
        }

        // Composer
        let r#use = if driver.arbitrary::<bool>()? && !atom_deps.is_empty() {
            Some(driver.choose(&atom_deps)?.id.to_string())
        } else {
            match driver.int_in_range(0..=1)? {
                0 => Some("nix".to_string()),
                1 => Some("static".to_string()),
                _ => None,
            }
        };

        let compose = ComposeConfig {
            r#use,
            at: Some("1.0.0".to_string()),
            entry: Some("default.nix".to_string()),
            args: HashMap::new(),
        };

        Ok(LockFile {
            version: 0,
            sets,
            compose,
            deps,
        })
    }

    #[test]
    fn test_lock_file_roundtrip() {
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let mut driver = arbitrary::Unstructured::new(bytes);
            if let Ok(lockfile) = generate_valid_lockfile(&mut driver) {
                let toml_str = toml::to_string(&lockfile).expect("failed to serialize LockFile");
                let parsed = LockFile::parse(&toml_str).expect("failed to parse LockFile");
                parsed.validate().expect("failed to validate LockFile");

                assert_eq!(parsed.version, lockfile.version);
                assert_eq!(parsed.sets.len(), lockfile.sets.len());
                assert_eq!(parsed.deps.len(), lockfile.deps.len());
            }
        });
    }

    #[test]
    fn test_lock_file_acyclicity_invariant() {
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let mut driver = arbitrary::Unstructured::new(bytes);
            if let Ok(lockfile) = generate_valid_lockfile(&mut driver) {
                let mut mutated = lockfile.clone();
                let mut atom_indices = Vec::new();
                for (idx, dep) in mutated.deps.iter().enumerate() {
                    if let Dependency::Atom(_) = dep {
                        atom_indices.push(idx);
                    }
                }

                if atom_indices.len() >= 2 {
                    let id_0 = match &mutated.deps[atom_indices[0]] {
                        Dependency::Atom(a) => a.id.clone(),
                        _ => unreachable!(),
                    };
                    let id_1 = match &mutated.deps[atom_indices[1]] {
                        Dependency::Atom(a) => a.id.clone(),
                        _ => unreachable!(),
                    };

                    if let Dependency::Atom(a) = &mut mutated.deps[atom_indices[0]] {
                        a.requires.push(id_1);
                    }
                    if let Dependency::Atom(a) = &mut mutated.deps[atom_indices[1]] {
                        a.requires.push(id_0);
                    }

                    assert!(mutated.validate().is_err(), "Cyclic graph was not rejected");
                }
            }
        });
    }

    #[test]
    fn test_lock_file_requires_closure_invariant() {
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let mut driver = arbitrary::Unstructured::new(bytes);
            if let Ok(lockfile) = generate_valid_lockfile(&mut driver) {
                let mut mutated = lockfile.clone();
                let mut found = false;
                for dep in &mut mutated.deps {
                    if let Dependency::Atom(a) = dep {
                        let invalid_label = atom_id::Label::try_from("nonexistent").unwrap();
                        let invalid_id = AtomId::new(a.id.anchor().clone(), invalid_label);
                        a.requires.push(invalid_id);
                        found = true;
                        break;
                    }
                }
                if found {
                    assert!(
                        mutated.validate().is_err(),
                        "Dangling requires was not rejected"
                    );
                }
            }
        });
    }

    #[test]
    fn test_lock_file_owner_closure_invariant() {
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let mut driver = arbitrary::Unstructured::new(bytes);
            if let Ok(lockfile) = generate_valid_lockfile(&mut driver) {
                let mut mutated = lockfile.clone();
                let mut found = false;
                for dep in &mut mutated.deps {
                    let invalid_label = atom_id::Label::try_from("nonexistent").unwrap();
                    let invalid_id = AtomId::new(atom_id::Anchor::new(vec![0; 20]), invalid_label);
                    match dep {
                        Dependency::Nix(d) => {
                            d.owner = Some(invalid_id);
                            found = true;
                        },
                        Dependency::NixGit(d) => {
                            d.owner = Some(invalid_id);
                            found = true;
                        },
                        Dependency::NixTar(d) => {
                            d.owner = Some(invalid_id);
                            found = true;
                        },
                        Dependency::NixSrc(d) => {
                            d.owner = Some(invalid_id);
                            found = true;
                        },
                        _ => {},
                    }
                    if found {
                        break;
                    }
                }
                if found {
                    assert!(
                        mutated.validate().is_err(),
                        "Dangling owner was not rejected"
                    );
                }
            }
        });
    }

    #[test]
    fn test_lock_file_atom_set_ref_invariant() {
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let mut driver = arbitrary::Unstructured::new(bytes);
            if let Ok(lockfile) = generate_valid_lockfile(&mut driver) {
                let mut mutated = lockfile.clone();
                let mut found = false;
                for dep in &mut mutated.deps {
                    if let Dependency::Atom(a) = dep {
                        let mut undeclared = "0000000000000000000000000000000000000000".to_string();
                        if mutated.sets.contains_key(&undeclared) {
                            undeclared = "1111111111111111111111111111111111111111".to_string();
                        }
                        a.set = undeclared;
                        found = true;
                        break;
                    }
                }
                if found {
                    assert!(
                        mutated.validate().is_err(),
                        "Undeclared set reference was not rejected"
                    );
                }
            }
        });
    }

    #[test]
    fn test_lock_file_version_invariant() {
        check!().with_type::<Vec<u8>>().for_each(|bytes| {
            let mut driver = arbitrary::Unstructured::new(bytes);
            if let Ok(lockfile) = generate_valid_lockfile(&mut driver) {
                let mut mutated = lockfile.clone();
                mutated.version = 1;
                assert!(
                    mutated.validate().is_err(),
                    "Unsupported version was not rejected"
                );
            }
        });
    }
}
