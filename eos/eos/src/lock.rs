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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetDetails {
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

        // Collect all atoms in deps by id
        let mut atoms = HashMap::new();
        for dep in &self.deps {
            if let Dependency::Atom(atom_dep) = dep
                && atoms.insert(atom_dep.id.clone(), atom_dep).is_some()
            {
                return Err(format!(
                    "Duplicate atom dependency with id: {}",
                    atom_dep.id
                ));
            }
        }

        // 2. [lock-atom-set-ref]: set fields must match [sets] keys
        for atom in atoms.values() {
            if !self.sets.contains_key(&atom.set) {
                return Err(format!(
                    "Atom {} references undeclared set: {}",
                    atom.id, atom.set
                ));
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
