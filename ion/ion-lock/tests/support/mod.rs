//! Structural conformance validator for the ion-lock v2 corpus (`ion/ion-lock/corpus/v2/`).
//!
//! This is test-only infrastructure (N-lock-corpus IBC, Non-Goals: no new
//! lock schema surface, no edits to `v2.rs`/`lib.rs`). It checks:
//!
//! 1. **Raw structural check** — walks the document as a generic [`toml::Value`] and rejects any
//!    key outside the schema's known set at every nesting level. `LockFileV2`'s `Deserialize` has
//!    no `#[serde(deny_unknown_fields)]` (N-lock-corpus IBC premise p-no-deny-unknown) and so
//!    silently ignores an injected forbidden section; this check is what actually enforces the
//!    `[lock-no-*]` deliberate-absence constraints (lock-file-schema.md:296-336).
//! 2. **Typed parse** — via `LockFileV2`'s landed `Deserialize`.
//! 3. **Closure/acyclicity** — every `requires` edge resolves to an existing entry
//!    (`[lock-requires-resolvable]`), the requires graph among dep entries is acyclic
//!    (`[lock-requires-acyclic]`), and every `[sets]` entry is referenced by at least one dep entry
//!    (`[lock-set-referenced]`).
//!
//! It deliberately does NOT call the stubbed `to_canonical` encoder — that
//! remains a Phase 2 deliverable (IBC Non-Goals).

use ion_lock::LockFileV2;
use toml::Value;

/// Why a candidate lock document failed structural validation.
#[derive(Debug)]
pub enum ValidationError {
    /// The document is not well-formed TOML.
    RawParse(String),
    /// A key outside the v2 schema's known set was found at the given
    /// dotted path (e.g. `"compose"`, `"deps.core.gcc.params"`).
    ForbiddenKey(String),
    /// The document is well-formed TOML but does not match `LockFileV2`'s
    /// shape (typed `Deserialize` failed).
    TypedParse(String),
    /// A `requires` edge names an entry absent from this lock.
    DanglingRequires(String),
    /// The `requires` graph among dep entries contains a cycle.
    Cycle(String),
    /// A `[sets]` entry is not referenced by any dep entry.
    SetNotReferenced(String),
    /// A `[deps]` key names a set alias absent from `[sets]`.
    SetUndeclared(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RawParse(e) => write!(f, "raw TOML parse failed: {e}"),
            Self::ForbiddenKey(path) => write!(f, "forbidden key at '{path}'"),
            Self::TypedParse(e) => write!(f, "typed parse failed: {e}"),
            Self::DanglingRequires(edge) => write!(f, "dangling requires edge: '{edge}'"),
            Self::Cycle(desc) => write!(f, "cyclic requires graph: {desc}"),
            Self::SetNotReferenced(alias) => {
                write!(f, "set '{alias}' is not referenced by any dep entry")
            },
            Self::SetUndeclared(set) => write!(f, "deps key names undeclared set '{set}'"),
        }
    }
}

const TOP_LEVEL_KEYS: &[&str] = &["schema", "sets", "deps", "fetch"];
const SET_ENTRY_KEYS: &[&str] = &["anchor", "charter_head", "snapshot", "mirrors"];
const DEP_ENTRY_KEYS: &[&str] = &["publish", "version", "requires"];
const FETCH_ENTRY_KEYS: &[&str] = &["digest", "url"];

/// Validate a raw lock document against the full structural surface: parse,
/// raw forbidden-section/extra-key check, and closure/acyclicity. Returns
/// the parsed [`LockFileV2`] on success.
pub fn validate(raw: &str) -> Result<LockFileV2, ValidationError> {
    let value: Value = raw
        .parse()
        .map_err(|e: toml::de::Error| ValidationError::RawParse(e.to_string()))?;
    check_allowed_keys(&value)?;

    let lock: LockFileV2 =
        toml::from_str(raw).map_err(|e| ValidationError::TypedParse(e.to_string()))?;

    check_closure(&lock)?;

    Ok(lock)
}

/// Reject any key, at any nesting level the v2 schema defines, outside its
/// known set (`[lock-no-*]`, lock-file-schema.md:296-336).
fn check_allowed_keys(value: &Value) -> Result<(), ValidationError> {
    let top = value
        .as_table()
        .ok_or_else(|| ValidationError::ForbiddenKey("<root is not a table>".to_string()))?;

    for key in top.keys() {
        if !TOP_LEVEL_KEYS.contains(&key.as_str()) {
            return Err(ValidationError::ForbiddenKey(key.clone()));
        }
    }

    if let Some(sets) = top.get("sets").and_then(Value::as_table) {
        for (alias, entry) in sets {
            let entry_table = entry.as_table().ok_or_else(|| {
                ValidationError::ForbiddenKey(format!("sets.{alias} is not a table"))
            })?;
            for key in entry_table.keys() {
                if !SET_ENTRY_KEYS.contains(&key.as_str()) {
                    return Err(ValidationError::ForbiddenKey(format!("sets.{alias}.{key}")));
                }
            }
        }
    }

    if let Some(deps) = top.get("deps").and_then(Value::as_table) {
        for (set, labels) in deps {
            let labels_table = labels.as_table().ok_or_else(|| {
                ValidationError::ForbiddenKey(format!("deps.{set} is not a table"))
            })?;
            for (label, entry) in labels_table {
                let entry_table = entry.as_table().ok_or_else(|| {
                    ValidationError::ForbiddenKey(format!("deps.{set}.{label} is not a table"))
                })?;
                for key in entry_table.keys() {
                    if !DEP_ENTRY_KEYS.contains(&key.as_str()) {
                        return Err(ValidationError::ForbiddenKey(format!(
                            "deps.{set}.{label}.{key}"
                        )));
                    }
                }
            }
        }
    }

    if let Some(fetch) = top.get("fetch").and_then(Value::as_table) {
        for (name, entry) in fetch {
            let entry_table = entry.as_table().ok_or_else(|| {
                ValidationError::ForbiddenKey(format!("fetch.{name} is not a table"))
            })?;
            for key in entry_table.keys() {
                if !FETCH_ENTRY_KEYS.contains(&key.as_str()) {
                    return Err(ValidationError::ForbiddenKey(format!("fetch.{name}.{key}")));
                }
            }
        }
    }

    Ok(())
}

/// `[lock-requires-resolvable]`, `[lock-requires-acyclic]`,
/// `[lock-set-referenced]` — the closure/acyclicity structural half of c1a.
fn check_closure(lock: &LockFileV2) -> Result<(), ValidationError> {
    // Every set alias appearing in a deps key path (already the only key
    // path shape deps carries) must have a `[sets]` entry, and every
    // `[sets]` entry must be referenced by at least one dep entry
    // ([lock-set-referenced]).
    for set in lock.deps.keys() {
        if !lock.sets.contains_key(set) {
            return Err(ValidationError::SetUndeclared(set.clone()));
        }
    }
    for alias in lock.sets.keys() {
        if !lock.deps.contains_key(alias) {
            return Err(ValidationError::SetNotReferenced(alias.clone()));
        }
    }

    // Every requires edge must resolve to an existing dep or fetch entry
    // ([lock-requires-resolvable]).
    for (set, labels) in &lock.deps {
        for (label, entry) in labels {
            for edge in &entry.requires {
                if !edge_resolves(lock, edge) {
                    return Err(ValidationError::DanglingRequires(format!(
                        "{set}.{label} -> {edge}"
                    )));
                }
            }
        }
    }

    // The requires graph among dep entries must be acyclic
    // ([lock-requires-acyclic]). Fetch entries carry no requires field and
    // so cannot participate in a cycle; only dep-to-dep edges matter here.
    detect_cycle(lock)
}

fn edge_resolves(lock: &LockFileV2, edge: &str) -> bool {
    if let Some(name) = edge.strip_prefix("fetch.") {
        return lock.fetch.contains_key(name);
    }
    match edge.split_once('.') {
        Some((set, label)) => lock
            .deps
            .get(set)
            .is_some_and(|labels| labels.contains_key(label)),
        None => false,
    }
}

fn detect_cycle(lock: &LockFileV2) -> Result<(), ValidationError> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Visiting,
        Done,
    }

    let mut state: std::collections::HashMap<String, State> = std::collections::HashMap::new();

    fn visit(
        lock: &LockFileV2,
        node: &str,
        state: &mut std::collections::HashMap<String, State>,
        path: &mut Vec<String>,
    ) -> Result<(), ValidationError> {
        match state.get(node) {
            Some(State::Done) => return Ok(()),
            Some(State::Visiting) => {
                path.push(node.to_string());
                return Err(ValidationError::Cycle(path.join(" -> ")));
            },
            None => {},
        }

        let Some((set, label)) = node.split_once('.') else {
            return Ok(());
        };
        let Some(entry) = lock.deps.get(set).and_then(|labels| labels.get(label)) else {
            return Ok(());
        };

        state.insert(node.to_string(), State::Visiting);
        path.push(node.to_string());
        for edge in &entry.requires {
            // Fetch edges are leaves w.r.t. the dep-entry cycle graph.
            if edge.starts_with("fetch.") {
                continue;
            }
            visit(lock, edge, state, path)?;
        }
        path.pop();
        state.insert(node.to_string(), State::Done);
        Ok(())
    }

    for (set, labels) in &lock.deps {
        for label in labels.keys() {
            let node = format!("{set}.{label}");
            let mut path = Vec::new();
            visit(lock, &node, &mut state, &mut path)?;
        }
    }
    Ok(())
}
