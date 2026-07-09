//! ion-lock v2 schema types (`schema = 2`).
//!
//! See `docs/specs/lock-file-schema.md:131-336`. This module is the v2 type
//! skeleton landed alongside v1 (`ion/ion-lock/src/lib.rs`); the byte-exact
//! canonical encoder ([lock-canonical-form]) is a stubbed Phase 2
//! deliverable — see [`LockFileV2::to_canonical`].
//!
//! `anchor`/`charter_head`/`publish` are Coz-signed transaction digests and
//! use [`atom_id::Czd`] (re-exported from `coz-rs`). `snapshot` and
//! `fetch.<name>.digest` are algorithm-prefixed content/VCS-object hashes
//! with no Coz signing structure behind them — no typed wrapper for that
//! shape exists anywhere in this workspace yet, so they stay `String` at
//! the skeleton stage (Delegated, IBC N-lockv2 §Decision Rights).

use std::collections::HashMap;

use atom_id::Czd;

/// The v2 lock file: schema version + sets + deps + fetch
/// (`docs/specs/lock-file-schema.md:119-130`).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LockFileV2 {
    /// Schema version; MUST be 2 (`[lock-schema-version]`).
    pub schema: u64,
    /// Set aliases to their anchor/charter/discovery data (`[sets]`).
    pub sets: HashMap<String, SetEntry>,
    /// Ground dependency pins, nested by `(set, label)` (`[deps.<set>.<label>]`).
    pub deps: HashMap<String, HashMap<String, DepEntry>>,
    /// Promoted fetch pins (`[fetch.<name>]`).
    pub fetch: HashMap<String, FetchEntry>,
}

/// A `[sets.<alias>]` entry: set anchor and discovery snapshot
/// (`docs/specs/lock-file-schema.md:131-192`). The alias key itself is the
/// human-facing name; there is no separate `tag` field (v1's `SetDetails.tag`
/// has no v2 successor — see the schema's `[sets.core]` example).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SetEntry {
    /// `czd(charter₀)` — the founding-charter digest; immutable for the
    /// set's lifetime (`[lock-set-anchor]`).
    pub anchor: Czd,
    /// Digest of the *effective* (possibly succeeded) charter; equals
    /// `anchor` absent succession, MAY advance, MUST NOT regress
    /// (`[lock-set-charter-head]`).
    pub charter_head: Czd,
    /// Algorithm-prefixed object id of the set repository's tip commit at
    /// discovery time (`[lock-set-snapshot]`).
    pub snapshot: String,
    /// Transport hints (URLs, or the `"::"` local sentinel); never identity
    /// (`[lock-set-mirrors]`).
    pub mirrors: Vec<String>,
}

/// A `[deps.<set>.<label>]` entry: a ground dependency pin
/// (`docs/specs/lock-file-schema.md:194-244`). The nesting key path IS
/// `(set, label)` — there is no `set`/`label`/`id`/`rev`/`direct` field on
/// the entry itself (all structurally eliminated vs v1's `AtomDep`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DepEntry {
    /// The bare publish czd — this entry's identity (`[lock-dep-publish]`).
    pub publish: Czd,
    /// Exact, non-empty published version string, byte-verbatim as
    /// published; no normalization (`[lock-dep-version]`).
    pub version: String,
    /// Dotted key paths (`"<set>.<label>"` or `"fetch.<name>"`) naming this
    /// entry's direct dependencies, sorted bytewise (`[lock-dep-requires]`).
    pub requires: Vec<String>,
}

/// A `[fetch.<name>]` entry: a promoted, non-regenerable fetch pin
/// (`docs/specs/lock-file-schema.md:246-274`). Unifies v1's `NixDep`/
/// `NixGitDep`/`NixTarDep`/`NixSrcDep` into one shape; there is no `owner`
/// back-pointer (`[lock-dep-requires]`: "Provider-side owner back-pointers
/// MUST NOT exist").
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FetchEntry {
    /// Algorithm-prefixed content digest of the fetched payload — the
    /// identity (`[lock-fetch-digest]`).
    pub digest: String,
    /// Transport hint; MUST NOT be treated as authoritative
    /// (`[lock-fetch-digest]`).
    pub url: String,
}

impl LockFileV2 {
    /// The canonical, byte-exact serialization (`[lock-canonical-form]`).
    ///
    /// **Deliberately unimplemented.** Fixed section order (`schema`,
    /// `[sets]`, `[deps]`, `[fetch]`), bytewise-sorted keys at every
    /// nesting level, LF, minimal escaping — a specified Phase 2
    /// deliverable, not a default `Serialize` derive. A casual
    /// `#[derive(Serialize)]` / `toml::to_string` here would silently
    /// become the identity that `[lock-no-plan-digest]` and
    /// `[lock-recomputability]` require to be byte-reproducible; do not
    /// treat it as canonical without discharging that obligation. See
    /// `docs/specs/lock-file-schema.md:278-288`.
    pub fn to_canonical(&self) -> String {
        unimplemented!(
            "[lock-canonical-form]: byte-exact canonical serialization is a specified Phase 2 \
             deliverable, not a default — see docs/specs/lock-file-schema.md:278-288"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_czd(seed: u8) -> Czd {
        Czd::from_bytes(vec![seed; 32])
    }

    fn sample_lock() -> LockFileV2 {
        let mut sets = HashMap::new();
        sets.insert(
            "core".to_string(),
            SetEntry {
                anchor: dummy_czd(1),
                charter_head: dummy_czd(1),
                snapshot: "sha1:b03d55e1".to_string(),
                mirrors: vec!["::".to_string()],
            },
        );

        let mut core_deps = HashMap::new();
        core_deps.insert(
            "gcc".to_string(),
            DepEntry {
                publish: dummy_czd(2),
                version: "13.3.0".to_string(),
                requires: vec![],
            },
        );
        let mut deps = HashMap::new();
        deps.insert("core".to_string(), core_deps);

        let mut fetch = HashMap::new();
        fetch.insert(
            "libfoo-vendor-models".to_string(),
            FetchEntry {
                digest: "blake3:aa31f6c0".to_string(),
                url: "https://files.example.com/models-4.2.tar.zst".to_string(),
            },
        );

        LockFileV2 {
            schema: 2,
            sets,
            deps,
            fetch,
        }
    }

    /// c5-compile / Acceptance Criteria 1: the v2 types round-trip through
    /// the same TOML path v1's `LockFile` uses.
    #[test]
    fn v2_lock_round_trips_through_toml() {
        let lock = sample_lock();
        let toml_str = toml::to_string(&lock).expect("serialize LockFileV2");
        let parsed: LockFileV2 = toml::from_str(&toml_str).expect("parse LockFileV2");

        assert_eq!(parsed.schema, 2);
        assert_eq!(parsed.sets.len(), 1);
        assert_eq!(parsed.deps["core"]["gcc"].version, "13.3.0");
        assert_eq!(
            parsed.fetch["libfoo-vendor-models"].url,
            sample_lock().fetch["libfoo-vendor-models"].url
        );
    }

    /// c-charter-head-present: SetEntry MUST carry a `charter_head` field
    /// distinct from `anchor` — this is the post-succession case
    /// (`[lock-set-charter-head]`, lock-file-schema.md:167-174), where a
    /// successor charter is the effective charter but `anchor` remains the
    /// immutable founding digest.
    #[test]
    fn charter_head_is_distinct_from_anchor() {
        let entry = SetEntry {
            anchor: dummy_czd(1),
            charter_head: dummy_czd(2),
            snapshot: "sha1:deadbeef".to_string(),
            mirrors: vec!["::".to_string()],
        };
        assert_ne!(entry.anchor, entry.charter_head);
    }

    /// c-encoder-stub-honest: the canonical encoder is an explicit,
    /// phase-tagged stub — not a silent `derive(Serialize)` masquerading as
    /// canonical form. Red/ignored per the campaign's expected-fail
    /// convention; N-lints inventories this obligation.
    #[test]
    #[ignore = "[lock-canonical-form] Phase 2: canonical encoder is unimplemented by design"]
    fn canonical_form_is_byte_exact() {
        let lock = sample_lock();
        let _ = lock.to_canonical();
    }
}
