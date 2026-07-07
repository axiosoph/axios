//! The **composition primitive** — skeleton types for
//! `docs/models/composition-model.md`.
//!
//! Scope discipline: this crate is the *value algebra only*. It knows
//! nothing about execution (see `htc-exec`), trust/signing (atom's coz
//! machinery), or storage backends (snix castore). Everything here is
//! plain data plus the one algorithm the model makes load-bearing: the
//! merge monoid `⊕` with denotational conflict detection (obligation P1).
//!
//! What is real here vs. skeleton:
//! - [`merge`] is a working implementation of the model's §3, with the monoid laws exercised in
//!   tests. Its conflict rule is a **sound over-approximation**: equal paths must be byte-identical
//!   entries, and any cross-composition prefix overlap involving a directory graft is rejected
//!   outright (checking graft *agreement* needs tree contents, i.e. a castore — out of this crate's
//!   scope). P1's full discharge refines this to exact denotational conflict detection.
//! - Interfaces (§4), certificates (§4), and the override operator (§7) are typed but not
//!   implemented; their doc comments say what the implementation owes.

use std::collections::BTreeMap;

/// A content digest of the artifact store (storage model §2). Plain
/// blake3 output in production; opaque bytes here so this crate stays
/// dependency-free. NOT a coz CZD — atoms' signing identifiers are a
/// different type owned by the atom layer.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Digest(pub [u8; 32]);

/// A conventional path inside a composition, as ordered components.
/// Component ordering gives `BTreeMap` the name-sorted enumeration the
/// denotation requires (composition model §2; storage model A3).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct CPath(pub Vec<String>);

impl CPath {
    /// Parse from a `/`-separated string; empty components are dropped.
    pub fn parse(s: &str) -> Self {
        CPath(
            s.split('/')
                .filter(|c| !c.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }

    /// Is `self` a strict (component-wise) prefix of `other`?
    pub fn is_strict_prefix_of(&self, other: &CPath) -> bool {
        self.0.len() < other.0.len() && other.0[..self.0.len()] == self.0[..]
    }
}

/// A content entry a composition may bind at a path (composition
/// model §2: `Content = blob | tree-node | symlink`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Entry {
    /// A blob, by digest.
    File { digest: Digest, executable: bool },
    /// A whole subtree graft, by root digest. This is the case that
    /// makes key-level conflict detection unsound (model §3, F5).
    DirGraft { root: Digest },
    /// A symlink with a literal target.
    Symlink { target: String },
}

/// A composition: a finite map from conventional paths to content
/// entries (composition model §2). Identity is the digest of its
/// canonical serialization; signing rides the atom layer and is not
/// modeled here.
///
/// `BTreeMap` is deliberate: iteration order == canonical (sorted)
/// order, so serialization canonicity (storage model A3 / obligation
/// P10) has no independent sort step to forget.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Composition {
    pub entries: BTreeMap<CPath, Entry>,
}

/// Why a merge is undefined (model §3: conflict is an explicit compose-
/// time error, never resolved silently).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum MergeConflict {
    /// Both sides bind `path` to entries that are not byte-identical.
    DisagreeingEntry { path: CPath },
    /// One side binds a path under the other side's directory graft (or
    /// grafts overlap). Denotational agreement cannot be checked without
    /// tree contents, so the skeleton rejects; P1's full implementation
    /// consults the store and detects *exact* denotational conflicts.
    GraftOverlap { graft_at: CPath, shadowed: CPath },
}

/// The merge monoid `c₁ ⊕ c₂` (composition model §3).
///
/// Laws (tested below, on representative values — the general proof is
/// obligation P1): identity (`c ⊕ ∅ = c`), commutativity, associativity,
/// idempotence (`c ⊕ c = c`); partial: returns `Err` exactly on
/// conflict.
pub fn merge(a: &Composition, b: &Composition) -> Result<Composition, MergeConflict> {
    // Equal keys must carry byte-identical entries.
    for (path, ea) in &a.entries {
        if let Some(eb) = b.entries.get(path) {
            if ea != eb {
                return Err(MergeConflict::DisagreeingEntry { path: path.clone() });
            }
        }
    }
    // Cross-composition prefix overlap involving a graft: conservative
    // conflict (denotations may overlap; agreement unknowable here).
    let graft_overlap = |from: &Composition, into: &Composition| {
        for (p, e) in &from.entries {
            if matches!(e, Entry::DirGraft { .. }) {
                for q in into.entries.keys() {
                    if p.is_strict_prefix_of(q) {
                        return Some(MergeConflict::GraftOverlap {
                            graft_at: p.clone(),
                            shadowed: q.clone(),
                        });
                    }
                }
            }
        }
        None
    };
    if let Some(c) = graft_overlap(a, b).or_else(|| graft_overlap(b, a)) {
        // Same-side prefix overlaps are a normal-form violation of the
        // *input*, not a merge conflict; constructors should prevent
        // them. Cross-side overlaps are the merge's job to refuse.
        return Err(c);
    }
    let mut entries = a.entries.clone();
    entries.extend(b.entries.iter().map(|(k, v)| (k.clone(), v.clone())));
    Ok(Composition { entries })
}

// ---------------------------------------------------------------------
// Interfaces: the typing of compositions (composition model §4).
// Typed skeleton only — the analyzer that derives manifests and the
// `satisfies` relation live with the interface layer's implementation.
// ---------------------------------------------------------------------

/// A namespace for interface facts (e.g. `elf`, `pkgconfig`, `env`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Namespace(pub String);

/// One provided or required capability inside a namespace.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Capability {
    pub ns: Namespace,
    pub name: String,
}

/// The derived, static facts about one tree: what it offers and needs
/// (model §4; htc-sad §2.2). Keyed by (analyzer, subject) upstream so
/// analyzer versions never overwrite each other's facts.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InterfaceManifest {
    pub subject: Digest,
    pub provides: Vec<Capability>,
    pub requires: Vec<Capability>,
}

/// A justified edge `required ↦ provider` (model §4). The satisfaction
/// proof's shape is the interface layer's to define; the binding only
/// records that one was checked.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Binding {
    pub required: Capability,
    pub provider: Digest,
}

/// An environment's coherence certificate (model §4): every internal
/// require bound, one provider per (ns, name) per scope, the choice
/// function recorded, the residual requires (ambient base) stated.
///
/// Obligation P8: formation terminates, is deterministic in
/// (intent, fact-set snapshot, choice policy), and the certificate is
/// recomputable byte-for-byte by any holder of both.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Certificate {
    pub bindings: Vec<Binding>,
    /// The declared ambient base — requires deliberately left unbound
    /// (kernel ABI, loader). Stated, never silent (model §4).
    pub residual: Vec<Capability>,
    /// Digest of the pinned fact-set snapshot formation consumed
    /// (model §6: snapshot pinning keeps formation algebraic).
    pub fact_snapshot: Digest,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(b: u8) -> Digest {
        Digest([b; 32])
    }
    fn file(p: &str, b: u8) -> (CPath, Entry) {
        (
            CPath::parse(p),
            Entry::File {
                digest: d(b),
                executable: false,
            },
        )
    }
    fn comp(entries: Vec<(CPath, Entry)>) -> Composition {
        Composition {
            entries: entries.into_iter().collect(),
        }
    }

    #[test]
    fn identity() {
        let c = comp(vec![file("/usr/lib/a", 1)]);
        assert_eq!(merge(&c, &Composition::default()).unwrap(), c);
        assert_eq!(merge(&Composition::default(), &c).unwrap(), c);
    }

    #[test]
    fn commutative_and_associative() {
        let a = comp(vec![file("/a", 1)]);
        let b = comp(vec![file("/b", 2)]);
        let c = comp(vec![file("/c", 3)]);
        assert_eq!(merge(&a, &b).unwrap(), merge(&b, &a).unwrap());
        assert_eq!(
            merge(&merge(&a, &b).unwrap(), &c).unwrap(),
            merge(&a, &merge(&b, &c).unwrap()).unwrap()
        );
    }

    #[test]
    fn idempotent() {
        let a = comp(vec![file("/a", 1), file("/b/c", 2)]);
        assert_eq!(merge(&a, &a).unwrap(), a);
    }

    #[test]
    fn agreeing_overlap_is_fine_disagreeing_is_conflict() {
        let a = comp(vec![file("/x", 1), file("/shared", 7)]);
        let ok = comp(vec![file("/y", 2), file("/shared", 7)]);
        let bad = comp(vec![file("/shared", 8)]);
        assert!(merge(&a, &ok).is_ok());
        assert_eq!(
            merge(&a, &bad).unwrap_err(),
            MergeConflict::DisagreeingEntry {
                path: CPath::parse("/shared")
            }
        );
    }

    #[test]
    fn graft_shadowing_is_detected_despite_disjoint_keys() {
        // The F5 case: {/usr ↦ Dir t} vs {/usr/lib/x ↦ File b} — keys
        // disjoint, denotations overlap. Key-level union would silently
        // shadow; the merge must refuse.
        let a = comp(vec![(CPath::parse("/usr"), Entry::DirGraft { root: d(9) })]);
        let b = comp(vec![file("/usr/lib/x", 1)]);
        assert!(matches!(
            merge(&a, &b),
            Err(MergeConflict::GraftOverlap { .. })
        ));
        assert!(matches!(
            merge(&b, &a),
            Err(MergeConflict::GraftOverlap { .. })
        ));
    }
}
