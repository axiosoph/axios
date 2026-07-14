//! # Atom Identity
//!
//! Identity primitives for the Atom protocol: validated naming types and
//! the protocol-level atom identity pair.
//!
//! ## Naming Hierarchy
//!
//! Three validated string types forming a strict hierarchy:
//!
//! > [`Identifier`] ⊂ [`Label`] ⊂ [`Tag`]
//!
//! - **Identifier**: UAX #31 compliant Unicode identifiers.
//! - **Label**: Identifiers plus hyphen (`-`).
//! - **Tag**: Labels plus separators (`:` and `.`).
//!
//! All input is NFKC-normalized and capped at 128 bytes.
//!
//! ## Atom Identity
//!
//! An [`AtomId`] is the unique identity of an atom — the pair of an
//! [`Anchor`] and a [`Label`]. Identity is determined solely by these
//! two values: two atoms with the same anchor and label ARE the same
//! atom, regardless of version, owner, or hash algorithm.
//!
//! The `AtomId` is algorithm-free and permanent. Compact hash
//! representations for store indexing (`AtomDigest`) are a separate
//! concern, computed downstream by stores and ingestors.
//!
//! ## Anchor
//!
//! An [`Anchor`] is an opaque byte vector establishing atom-set identity.
//! Its derivation is fixed by charter: `Anchor == czd(charter₀)`, the coz
//! digest of the atom-set's founding charter (spec `[charter-anchor]`).
//!
//! [Coz]: https://github.com/Cyphrme/Coz

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

mod charter;
mod digest;
mod name;
#[cfg(feature = "serde")]
mod serde_alg;
#[cfg(feature = "serde")]
mod serde_b64;

/// Serde bridge for `Option<Vec<u8>>` via base64url-unpadded encoding.
///
/// `serde_b64` (this crate's sibling module) only covers `Vec<u8>` —
/// `content_hash` needs the `Option` wrapper too, since it is `None` on
/// the wire whenever absent (`[content-hash-obligation]`'s schema tier),
/// mirroring `atom-git`'s own hand-rolled `option_b64` pattern
/// (`atom-git/src/source.rs`) for the same shape.
#[cfg(feature = "serde")]
mod serde_b64_option {
    use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
    use serde::{Deserialize, Deserializer, Serializer};

    pub(crate) fn serialize<S: Serializer>(
        opt: &Option<Vec<u8>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match opt {
            Some(bytes) => serializer.serialize_str(&Base64UrlUnpadded::encode_string(bytes)),
            None => serializer.serialize_none(),
        }
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<u8>>, D::Error> {
        let opt: Option<String> = Option::deserialize(deserializer)?;
        match opt {
            Some(s) => Base64UrlUnpadded::decode_vec(&s)
                .map(Some)
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

use std::fmt;
use std::str::FromStr;

#[cfg(feature = "serde")]
pub use charter::{
    CharterLink, verify_bootstrap_gate, verify_charter, verify_charter_chain_signatures,
    verify_succession_chain,
};
pub use charter::{CharterPayload, CharterStore, TYP_CHARTER};
pub use coz_rs::{Alg, Cad, Czd, Thumbprint, canonical, canonical_hash_for_alg};
pub use digest::{AtomDigest, DigestParseError, HashAlg};
pub use name::{Identifier, Label, Name, Tag};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "serde")]
pub use serde_json;
use thiserror::Error;

/// Maximum byte length for validated name types.
pub const NAME_MAX: usize = 128;

/// Transaction type for atom claims.
///
/// Spec constraint: `[claim-typ]`.
pub const TYP_CLAIM: &str = "atom/claim";

/// Transaction type for atom version publishes.
///
/// Spec constraint: `[publish-typ]`.
pub const TYP_PUBLISH: &str = "atom/publish";

/// Spec shorthand for [`Thumbprint`] (Coz key thumbprint).
pub type Tmb = Thumbprint;

// ============================================================================
// Anchor
// ============================================================================

/// An opaque anchor establishing atom-set identity.
///
/// The anchor pins an atom-set to an immutable reference point in the
/// source's history. Its derivation is fixed by charter: `Anchor ==
/// czd(charter₀)`, the coz digest of the atom-set's founding charter
/// (spec `[charter-anchor]`) — backend-agnostic, since the charter is a
/// coz object regardless of backend.
///
/// Displayed and serialized as a base64url-unpadded string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Anchor(Vec<u8>);

impl Anchor {
    /// Create an anchor from raw bytes.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The raw anchor bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Encode as a base64url-unpadded string.
    #[must_use]
    pub fn to_b64(&self) -> String {
        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        Base64UrlUnpadded::encode_string(&self.0)
    }
}

impl fmt::Display for Anchor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_b64())
    }
}

impl AsRef<[u8]> for Anchor {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for Anchor {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

#[cfg(feature = "serde")]
impl Serialize for Anchor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_b64())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Anchor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        let bytes = Base64UrlUnpadded::decode_vec(&s).map_err(serde::de::Error::custom)?;
        Ok(Self(bytes))
    }
}

// ============================================================================
// AtomId
// ============================================================================

/// The unique protocol-level identity of an atom.
///
/// An `AtomId` is the pair of an [`Anchor`] and a [`Label`]. Identity is
/// determined solely by these two values — two atoms with the same anchor
/// and label ARE the same atom, regardless of version, owner, or hash
/// algorithm.
///
/// The `AtomId` is algorithm-free and permanent. Compact hash
/// representations for store indexing are a downstream concern.
///
/// Serialized as `<anchor_b64ut>::<label>` (double-colon delimited).
///
/// ```text
/// dGVzdA::my-package
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AtomId {
    anchor: Anchor,
    label: Label,
}

impl AtomId {
    /// Construct an `AtomId` from an anchor and label.
    pub fn new(anchor: Anchor, label: Label) -> Self {
        Self { anchor, label }
    }

    /// The anchor establishing atom-set identity.
    #[must_use]
    pub fn anchor(&self) -> &Anchor {
        &self.anchor
    }

    /// The atom's label within its atom-set.
    #[must_use]
    pub fn label(&self) -> &Label {
        &self.label
    }
}

/// Display as `<anchor_b64ut>::<label>` (double-colon delimited).
impl fmt::Display for AtomId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.anchor.to_b64(), self.label)
    }
}

/// Parse from `<anchor_b64ut>::<label>` format.
impl FromStr for AtomId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (anchor_str, label_str) = s.split_once("::").ok_or(Error::InvalidFormat)?;

        use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
        let anchor_bytes =
            Base64UrlUnpadded::decode_vec(anchor_str).map_err(|_| Error::InvalidAnchor)?;

        let label = Label::try_from(label_str).map_err(|_| Error::InvalidLabel)?;

        Ok(Self {
            anchor: Anchor::new(anchor_bytes),
            label,
        })
    }
}

#[cfg(feature = "serde")]
impl Serialize for AtomId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for AtomId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ============================================================================
// OwnerKind / OwnerRef
// ============================================================================

/// Which external identity framework interprets an [`OwnerRef`]'s `value`.
///
/// Required and explicit on every owner-reference — there is no implicit
/// default, not even for `SingleKey`; a producer MUST tag every
/// owner-reference explicitly, and a consumer encountering an absent `kind`
/// field on the wire MUST treat it as a hard parse error, never a fallback.
///
/// `Hierarchical` and `RootedIdentity` are named and reserved — not yet
/// implemented. A consumer encountering either MUST reject cleanly (treat
/// the `OwnerRef` as unauthorizable) rather than attempt to interpret
/// `value`.
///
/// Spec constraint: `[owner-kind-required]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum OwnerKind {
    /// A raw Coz key thumbprint. `value` = key thumbprint bytes. The only
    /// tier with a working authorization evaluator today.
    SingleKey,
    /// OpenPGP-style master key + subkeys. `value` = master key
    /// fingerprint. Named and reserved — not yet implemented.
    Hierarchical,
    /// A Cyphr Principal Root identity. `value` = PR digest. Named and
    /// reserved — not yet implemented.
    RootedIdentity,
}

/// One kind-tagged, opaque identity digest — the protocol's unit of
/// identity.
///
/// `value` MUST be treated as an opaque byte vector; the protocol imposes
/// no interpretation on its contents beyond what `kind` names.
/// `ClaimPayload.owner` is a single `OwnerRef` (`[claim-owner-single]`);
/// `CharterPayload.owner` is a non-empty set of them
/// (`[charter-owner-set]`) — the two payloads share this same shape at
/// different cardinalities, not different owner concepts.
///
/// Spec constraint: `[owner-abstract]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OwnerRef {
    /// Which identity framework interprets `value`.
    pub kind: OwnerKind,
    /// Opaque identity digest, meaning determined entirely by `kind`.
    #[cfg_attr(feature = "serde", serde(with = "serde_b64"))]
    pub value: Vec<u8>,
}

impl OwnerRef {
    /// Construct an owner-reference.
    pub fn new(kind: OwnerKind, value: Vec<u8>) -> Self {
        Self { kind, value }
    }

    /// Construct a `single-key` owner-reference from a key thumbprint.
    pub fn single_key(tmb: &Thumbprint) -> Self {
        Self {
            kind: OwnerKind::SingleKey,
            value: tmb.as_bytes().to_vec(),
        }
    }

    /// Whether a signing key with thumbprint `tmb` is authorized by this
    /// owner-reference, under `kind`'s own authorization semantics
    /// (`[owner-authorization-delegated]`'s per-value rule).
    ///
    /// `Hierarchical` and `RootedIdentity` are named and reserved: this
    /// always returns `false` for them rather than attempting to interpret
    /// `value` — a clean rejection, not an error, matching
    /// `[owner-kind-required]`'s "treat as unauthorizable" directive.
    #[must_use]
    pub fn authorizes(&self, tmb: &Thumbprint) -> bool {
        match self.kind {
            OwnerKind::SingleKey => self.value == tmb.as_bytes(),
            OwnerKind::Hierarchical | OwnerKind::RootedIdentity => false,
        }
    }
}

/// Whether a signing key with thumbprint `tmb` is authorized by ANY entry
/// in an owner set, evaluated under each entry's own `kind`.
///
/// Set membership is a disjunction over single-valued authorization, never
/// a distinct mechanism of its own (`[owner-authorization-delegated]`'s set
/// composition rule, the charter-owner case).
#[must_use]
pub fn owner_set_authorizes(owners: &[OwnerRef], tmb: &Thumbprint) -> bool {
    owners.iter().any(|o| o.authorizes(tmb))
}

/// Deserialize `CharterPayload.owner`, rejecting an empty set outright.
///
/// `[charter-owner-set-non-empty]`: a charter whose owner set would contain
/// zero entries is a charter nobody could ever claim under. Enforced here
/// (deserialization) and in [`CharterPayload::new`] (construction) — the
/// two points data can enter this type.
#[cfg(feature = "serde")]
pub(crate) fn deserialize_non_empty_owner_set<'de, D>(
    deserializer: D,
) -> Result<Vec<OwnerRef>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let owners: Vec<OwnerRef> = Deserialize::deserialize(deserializer)?;
    if owners.is_empty() {
        return Err(serde::de::Error::custom(
            "charter owner set must be non-empty ([charter-owner-set-non-empty])",
        ));
    }
    Ok(owners)
}

#[cfg(test)]
mod owner_tests {
    use super::*;

    fn tmb(byte: u8) -> Thumbprint {
        Thumbprint::from_bytes(vec![byte; 4])
    }

    #[test]
    fn single_key_authorizes_matching_thumbprint() {
        let owner = OwnerRef::single_key(&tmb(7));
        assert!(owner.authorizes(&tmb(7)));
    }

    #[test]
    fn single_key_rejects_mismatched_thumbprint() {
        let owner = OwnerRef::single_key(&tmb(7));
        assert!(!owner.authorizes(&tmb(9)));
    }

    #[test]
    fn hierarchical_and_rooted_identity_never_authorize() {
        let hierarchical = OwnerRef::new(OwnerKind::Hierarchical, vec![7; 4]);
        let rooted = OwnerRef::new(OwnerKind::RootedIdentity, vec![7; 4]);
        // Even a byte-identical `value` to a would-be matching thumbprint
        // must not authorize -- these tiers have no working evaluator and
        // MUST reject cleanly rather than fall back to single-key
        // comparison semantics.
        assert!(!hierarchical.authorizes(&tmb(7)));
        assert!(!rooted.authorizes(&tmb(7)));
    }

    #[test]
    fn owner_set_authorizes_is_a_disjunction() {
        let owners = vec![
            OwnerRef::single_key(&tmb(1)),
            OwnerRef::single_key(&tmb(2)),
            OwnerRef::single_key(&tmb(3)),
        ];
        assert!(owner_set_authorizes(&owners, &tmb(2)));
        assert!(!owner_set_authorizes(&owners, &tmb(9)));
    }

    #[test]
    fn owner_set_authorizes_rejects_empty_set() {
        assert!(!owner_set_authorizes(&[], &tmb(1)));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn owner_kind_has_no_default_and_missing_kind_is_a_parse_error() {
        // No `kind` field at all on the wire must be a hard parse error --
        // not a fallback to `single-key` or any other default.
        let json = serde_json::json!({ "value": "AQID" });
        let result: Result<OwnerRef, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "an OwnerRef with no `kind` field must fail to deserialize, not default"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn owner_kind_wire_names_match_spec() {
        let owner = OwnerRef::new(OwnerKind::SingleKey, vec![1, 2, 3]);
        let json = serde_json::to_value(&owner).unwrap();
        assert_eq!(json["kind"], "single-key");

        let owner = OwnerRef::new(OwnerKind::Hierarchical, vec![1, 2, 3]);
        let json = serde_json::to_value(&owner).unwrap();
        assert_eq!(json["kind"], "hierarchical");

        let owner = OwnerRef::new(OwnerKind::RootedIdentity, vec![1, 2, 3]);
        let json = serde_json::to_value(&owner).unwrap();
        assert_eq!(json["kind"], "rooted-identity");
    }

    #[cfg(feature = "serde")]
    #[test]
    fn owner_kind_rejects_unknown_variant() {
        let json = serde_json::json!({ "kind": "quantum-resistant", "value": "AQID" });
        let result: Result<OwnerRef, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }
}

// ============================================================================
// ClaimPayload
// ============================================================================

/// Payload for an `atom/claim` transaction.
///
/// Claims establish atom identity by binding an [`Anchor`] and [`Label`]
/// to an owner. The resulting signed Coz message becomes the atom's
/// identity claim.
///
/// A claim may also be a *replacement* of a prior claim for the same
/// `(anchor, label)`, per `[claim-replacement-authority]`: `prior` names
/// the czd of the replaced claim, and `governance` marks the replacement
/// as a governance seizure (signed by the effective charter's owner
/// rather than the replaced claim's owner).
///
/// Spec constraints: `[claim-typ]`, `[symmetric-payloads]`,
/// `[owner-abstract]`, `[claim-replacement-authority]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ClaimPayload {
    /// The signing algorithm.
    #[cfg_attr(feature = "serde", serde(with = "serde_alg"))]
    pub alg: Alg,
    /// The atom-set anchor.
    pub anchor: Anchor,
    /// Mandatory marking for a governance replacement — a replacement
    /// signed by the effective charter's owner rather than the replaced
    /// claim's owner. `false` for a founding/ordinary claim or an owner
    /// replacement.
    ///
    /// Spec constraint: `[claim-replacement-authority]`.
    pub governance: bool,
    /// The atom label.
    pub label: Label,
    /// Timestamp (seconds since Unix epoch) for fork disambiguation.
    pub now: u64,
    /// Single owner-reference: the one identity accountable for this
    /// label.
    ///
    /// Spec constraints: `[owner-abstract]`, `[claim-owner-single]`.
    pub owner: OwnerRef,
    /// PURL type identifying the package ecosystem (e.g., "cargo").
    pub pkg: String,
    /// The czd of the claim this one replaces. `None` for a
    /// founding/ordinary claim.
    ///
    /// Spec constraint: `[claim-replacement-authority]`.
    pub prior: Option<Czd>,
    /// Source revision hash at claim time (temporal floor).
    #[cfg_attr(feature = "serde", serde(with = "serde_b64"))]
    pub src: Vec<u8>,
    /// Coz key thumbprint of the signing key.
    pub tmb: Thumbprint,
    /// Transaction type — always [`TYP_CLAIM`].
    pub typ: String,
    /// Ecosystem-specific extensions, nested here per
    /// `[claim-payload-extensible]` (root JSON keys are otherwise
    /// reserved for protocol fields). `None` when no extensions are
    /// present.
    #[cfg(feature = "serde")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, serde_json::Value>>,
}

impl ClaimPayload {
    /// Construct a new (non-replacement) claim payload.
    ///
    /// Takes an [`AtomId`] to ensure that the anchor and label come from
    /// a validated identity pair. Sets `typ` to [`TYP_CLAIM`], `prior` to
    /// `None`, and `governance` to `false` automatically. Use
    /// [`ClaimPayload::new_replacement`] to construct a replacement claim.
    pub fn new(
        alg: Alg,
        id: AtomId,
        now: u64,
        owner: OwnerRef,
        pkg: String,
        src: Vec<u8>,
        tmb: Thumbprint,
    ) -> Self {
        Self {
            alg,
            anchor: id.anchor,
            governance: false,
            label: id.label,
            now,
            owner,
            pkg,
            prior: None,
            src,
            tmb,
            typ: TYP_CLAIM.to_owned(),
            #[cfg(feature = "serde")]
            meta: None,
        }
    }

    /// Construct a new claim-replacement payload.
    ///
    /// Mirrors [`ClaimPayload::new`] but sets `prior` to the czd of the
    /// claim being replaced. Pass `governance: false` for an owner
    /// replacement (the ordinary, unmarked path) or `governance: true`
    /// for a governance replacement (a first-class, visible seizure
    /// event) — see `[claim-replacement-authority]`. Which authority
    /// actually justifies the replacement is a verification-time concern
    /// ([`verify_claim_replacement`], deliberately unimplemented —
    /// Phase 1), not something this constructor checks.
    #[allow(clippy::too_many_arguments)]
    pub fn new_replacement(
        alg: Alg,
        id: AtomId,
        now: u64,
        owner: OwnerRef,
        pkg: String,
        prior: Czd,
        governance: bool,
        src: Vec<u8>,
        tmb: Thumbprint,
    ) -> Self {
        Self {
            alg,
            anchor: id.anchor,
            governance,
            label: id.label,
            now,
            owner,
            pkg,
            prior: Some(prior),
            src,
            tmb,
            typ: TYP_CLAIM.to_owned(),
            #[cfg(feature = "serde")]
            meta: None,
        }
    }
}

// ============================================================================
// PublishPayload
// ============================================================================

/// The reproducibility mode a `PublishPayload` declares — `[publish-mode]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Mode {
    /// Every action the publish denotes yields `record_core`-equal
    /// records at fixed `action_id` (atom-model.md §6).
    Reproducible,
    /// Asserts nothing beyond witness accumulation. The spec's default
    /// when `mode` is absent from the wire.
    Witnessed,
}

/// Payload for an `atom/publish` transaction.
///
/// Publishes bind a version of an atom to a content snapshot, chaining
/// back to the authorizing claim via the `claim` field.
///
/// Spec constraints: `[publish-typ]`, `[symmetric-payloads]`,
/// `[publish-chains-claim]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PublishPayload {
    /// The signing algorithm.
    #[cfg_attr(feature = "serde", serde(with = "serde_alg"))]
    pub alg: Alg,
    /// The atom-set anchor.
    pub anchor: Anchor,
    /// The [`Czd`] of the authorizing claim.
    ///
    /// Spec constraint: `[publish-chains-claim]`.
    pub claim: Czd,
    /// Atom snapshot hash (the published artifact).
    #[cfg_attr(feature = "serde", serde(with = "serde_b64"))]
    pub dig: Vec<u8>,
    /// The atom label.
    pub label: Label,
    /// Timestamp (seconds since Unix epoch).
    pub now: u64,
    /// Subdirectory path in source content tree.
    pub path: String,
    /// Source revision hash (provenance).
    #[cfg_attr(feature = "serde", serde(with = "serde_b64"))]
    pub src: Vec<u8>,
    /// Coz key thumbprint of the signing key.
    pub tmb: Thumbprint,
    /// The atom version (unparsed).
    pub version: RawVersion,
    /// Transaction type — always [`TYP_PUBLISH`].
    pub typ: String,
    /// Declared reproducibility mode. `None` on the wire (and here)
    /// means `witnessed`, the spec's `[publish-mode]` default — read
    /// [`PublishPayload::effective_mode`] rather than this field
    /// directly when the resolved value is what matters.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub mode: Option<Mode>,
    /// BLAKE3 content-tree digest — `[content-hash-is-tree-digest]`.
    ///
    /// OPTIONAL (schema-tier of `[content-hash-obligation]`): `None` means
    /// no claim beyond `dig` is made. Where `Some`, it MUST have been
    /// computed by [`atom_core::content_hash`] over the same content
    /// entries the backend's own content-tree construction consumes, and
    /// set here BEFORE signing — a value added post-signature carries no
    /// cryptographic assurance and is not this field. A consumer that
    /// resolves a payload carrying `Some` MUST recompute and reject on
    /// mismatch (consumer-tier of `[content-hash-obligation]`); this
    /// crate does not enforce that obligation itself, since verification
    /// requires the resolved content, which this crate never holds.
    #[cfg_attr(feature = "serde", serde(with = "serde_b64_option"))]
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub content_hash: Option<Vec<u8>>,
    /// Ecosystem-specific extensions, nested here per
    /// `[publish-payload-extensible]` (root JSON keys are otherwise
    /// reserved for protocol fields). `None` when no extensions are
    /// present.
    #[cfg(feature = "serde")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Map<String, serde_json::Value>>,
}

impl PublishPayload {
    /// Construct a new publish payload.
    ///
    /// Takes an [`AtomId`] to ensure that the anchor and label come from
    /// a validated identity pair. Sets `typ` to [`TYP_PUBLISH`]
    /// automatically, and leaves `mode`, `content_hash`, and `meta` unset
    /// — use [`PublishPayload::effective_mode`] to read the resolved
    /// mode, and set `meta`/`mode`/`content_hash` directly on the
    /// returned value if needed (before signing, for `content_hash` —
    /// `[content-hash-is-tree-digest]`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        alg: Alg,
        id: AtomId,
        claim: Czd,
        dig: Vec<u8>,
        now: u64,
        path: String,
        src: Vec<u8>,
        tmb: Thumbprint,
        version: RawVersion,
    ) -> Self {
        Self {
            alg,
            anchor: id.anchor,
            claim,
            dig,
            label: id.label,
            now,
            path,
            src,
            tmb,
            version,
            typ: TYP_PUBLISH.to_owned(),
            mode: None,
            content_hash: None,
            #[cfg(feature = "serde")]
            meta: None,
        }
    }

    /// The effective reproducibility mode: `mode` if set, else the
    /// spec's `[publish-mode]` default (`witnessed`).
    pub fn effective_mode(&self) -> Mode {
        self.mode.unwrap_or(Mode::Witnessed)
    }
}

// ============================================================================
// RawVersion
// ============================================================================

/// An unparsed version string.
///
/// `RawVersion` is deliberately opaque — it does **not** implement `Deref`,
/// `AsRef<str>`, or `Into<String>`. Access the inner value only via
/// [`as_str()`](RawVersion::as_str). This ensures that version strings
/// cannot be silently used as plain strings; parsing is always explicit
/// through a [`VersionScheme`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RawVersion(String);

impl RawVersion {
    /// Wrap a string as an unparsed version.
    ///
    /// No validation is performed — any string is a valid raw version.
    /// Interpretation is deferred to a [`VersionScheme`] implementor.
    pub fn new(s: String) -> Self {
        Self(s)
    }

    /// The raw version string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RawVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RawVersion {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

// ============================================================================
// VersionScheme
// ============================================================================

/// Abstract version comparison scheme.
///
/// Concrete version formats (semver, calver, etc.) implement this trait
/// to provide parsing and comparison for [`RawVersion`] strings. The
/// `atom-id` crate defines no concrete schemes — those live in
/// ecosystem-specific crates (e.g., `ion-manifest` for semver).
pub trait VersionScheme {
    /// A parsed, comparable version value.
    type Version: fmt::Display + Ord;

    /// A version constraint (e.g., `>=1.0, <2.0`).
    type Requirement;

    /// Errors produced during parsing.
    type Error: std::error::Error;

    /// Parse a raw version string into a structured version.
    fn parse_version(&self, raw: &RawVersion) -> Result<Self::Version, Self::Error>;

    /// Parse a version requirement string.
    fn parse_requirement(&self, raw: &str) -> Result<Self::Requirement, Self::Error>;

    /// Check whether a version satisfies a requirement.
    fn matches(&self, version: &Self::Version, req: &Self::Requirement) -> bool;
}

// ============================================================================
// Errors
// ============================================================================

/// Errors produced by atom identity operations.
#[derive(Error, Debug, PartialEq, Eq)]
pub enum Error {
    /// The name is empty.
    #[error("cannot be empty")]
    Empty,
    /// The name contains invalid characters.
    #[error("contains invalid characters: '{0}'")]
    InvalidCharacters(String),
    /// The name starts with an invalid character.
    #[error("cannot start with: '{0}'")]
    InvalidStart(char),
    /// The name contains invalid Unicode.
    #[error("must be valid unicode")]
    InvalidUnicode,
    /// The name exceeds the maximum allowed length.
    #[error("cannot be more than {} bytes", NAME_MAX)]
    TooLong,
    /// Atom ID string does not match `anchor_b64ut::label` format.
    #[error("invalid atom ID format, expected `anchor_b64ut::label`")]
    InvalidFormat,
    /// Invalid base64url anchor encoding.
    #[error("invalid anchor encoding")]
    InvalidAnchor,
    /// Invalid label in atom ID.
    #[error("invalid label in atom ID")]
    InvalidLabel,
    /// A charter's owner set would contain zero entries.
    ///
    /// Spec constraint: `[charter-owner-set-non-empty]`.
    #[error("charter owner set must be non-empty")]
    EmptyOwnerSet,
}

/// Errors produced by transaction verification.
#[cfg(feature = "serde")]
#[derive(Error, Debug)]
pub enum VerifyError {
    /// The cryptographic signature is invalid for the given payload and key.
    #[error("invalid signature")]
    InvalidSignature,
    /// The payload JSON could not be parsed into the expected type.
    #[error("payload parse error: {0}")]
    PayloadParse(#[from] serde_json::Error),
    /// The signing algorithm is not supported by coz-rs.
    #[error("unsupported algorithm: {0}")]
    UnsupportedAlgorithm(String),
    /// The `typ` field does not match the expected transaction type.
    #[error("wrong typ: expected {expected}, got {actual}")]
    WrongTyp {
        /// The expected transaction type.
        expected: &'static str,
        /// The actual transaction type found in the payload.
        actual: String,
    },
    /// A signing key was not authorized by the required owner.
    ///
    /// Spec constraints: `[charter-succession]` (charter-chain
    /// authorization), `[owner-authorization-delegated]` (claim/publish
    /// single-key authorization — e.g. Verification Pipeline step 11).
    #[error("unauthorized: signing key not authorized by the required owner")]
    Unauthorized,
    /// A succession chain was empty; a chain requires at least a founding
    /// charter.
    #[error("empty succession chain")]
    EmptyChain,
    /// A succession chain's first element carries a `prior` — only the
    /// founding charter (no `prior`) may begin a chain.
    ///
    /// Spec constraint: `[charter-anchor]`.
    #[error("succession chain does not begin with a founding charter")]
    NotFoundingCharter,
    /// A succession chain element at a position other than the first
    /// carries no `prior` — a parentless element MUST only ever appear at
    /// the start of a chain, checked directly rather than inferred from
    /// caller-supplied ordering.
    ///
    /// Spec constraint: `[charter-anchor]`.
    #[error("succession chain element at index {index} carries no `prior`, but is not first")]
    ParentlessNonFirstElement {
        /// The chain index of the offending element.
        index: usize,
    },
    /// Two charters in a succession chain name the same `prior` — a
    /// set-authority fork that MUST fail closed rather than pick a branch.
    ///
    /// Spec constraint: `[charter-succession-linear]`.
    #[error("divergent successors: two charters share the same prior")]
    DivergentSuccessors,
    /// A served succession chain does not demonstrably extend past a
    /// consumer's previously recorded head — a rollback, not a
    /// legitimate shorter chain.
    ///
    /// Spec constraint: `[chain-monotonicity]`.
    #[error("chain regression: served chain does not extend past the recorded head")]
    ChainRegression,
    /// A claim's payload-declared `tmb` does not match the thumbprint
    /// computed from its actual signing key.
    ///
    /// Spec constraint: `[claim-key-required]`.
    #[error("thumbprint mismatch: declared tmb does not match the signing key")]
    ThumbprintMismatch,
    /// A publish's `claim` field does not match the referenced claim's
    /// actual czd.
    ///
    /// Spec constraint: `[publish-chains-claim]`.
    #[error("claim chain mismatch: publish.claim does not match the claim's czd")]
    ClaimChainMismatch,
    /// A payload's `(anchor, label)` does not match the expected `AtomId`.
    ///
    /// Spec constraint: `[symmetric-payloads]`.
    #[error("AtomId mismatch: payload's (anchor, label) does not match the expected AtomId")]
    AtomIdMismatch,
    /// A claim's `anchor` does not equal the founding charter's czd —
    /// distinct from [`Self::ClaimChainMismatch`], which is about
    /// publish→claim linkage, not claim→charter linkage.
    ///
    /// Spec constraint: `[claim-chains-charter]` (Verification Pipeline step 7).
    #[error("claim charters mismatch: claim.anchor does not equal czd(charter\u{2080})")]
    ClaimChartersMismatch,
    /// The strict `charter.now < claim.now < publish.now` ordering does
    /// not hold.
    ///
    /// Spec constraint: Verification Pipeline step 9.
    #[error("temporal order violation: charter.now < claim.now < publish.now does not hold")]
    TemporalOrderViolation,
    /// A claim-replacement's `now` does not strictly exceed the replaced
    /// claim's `now`.
    ///
    /// Spec constraint: `[claim-replacement-transition]` PRE.
    #[error(
        "replacement not after prior: replacement.now does not exceed the replaced claim's now"
    )]
    ReplacementNotAfterPrior,
    /// A claim-replacement's `(anchor, label)` differs from the replaced
    /// claim's — a replacement never alters identity.
    ///
    /// Spec constraint: `[claim-replacement-transition]` PRE.
    #[error(
        "replacement identity changed: replacement's (anchor, label) differs from the replaced \
         claim's"
    )]
    ReplacementIdentityChanged,
}

// ============================================================================
// Verification functions
// ============================================================================

/// Verify a signed `atom/claim` transaction.
///
/// Validates the Coz signature, deserializes the payload, and checks
/// that `typ` is [`TYP_CLAIM`]. Returns the parsed [`ClaimPayload`]
/// on success.
///
/// The caller provides raw key bytes — key storage and discovery is
/// not this crate's concern.
///
/// Spec constraints: `[sig-over-pay]`, `[claim-typ]`, `[claim-key-required]`.
#[cfg(feature = "serde")]
pub fn verify_claim(
    pay_json: &[u8],
    sig: &[u8],
    alg: &str,
    pub_key: &[u8],
) -> Result<ClaimPayload, VerifyError> {
    verify_signature(pay_json, sig, alg, pub_key)?;
    let payload: ClaimPayload = serde_json::from_slice(pay_json)?;
    if payload.typ != TYP_CLAIM {
        return Err(VerifyError::WrongTyp {
            expected: TYP_CLAIM,
            actual: payload.typ,
        });
    }
    Ok(payload)
}

/// Verify a signed `atom/publish` transaction.
///
/// Validates the Coz signature, deserializes the payload, and checks
/// that `typ` is [`TYP_PUBLISH`]. Returns the parsed [`PublishPayload`]
/// on success.
///
/// Spec constraints: `[sig-over-pay]`, `[publish-typ]`.
#[cfg(feature = "serde")]
pub fn verify_publish(
    pay_json: &[u8],
    sig: &[u8],
    alg: &str,
    pub_key: &[u8],
) -> Result<PublishPayload, VerifyError> {
    verify_signature(pay_json, sig, alg, pub_key)?;
    let payload: PublishPayload = serde_json::from_slice(pay_json)?;
    if payload.typ != TYP_PUBLISH {
        return Err(VerifyError::WrongTyp {
            expected: TYP_PUBLISH,
            actual: payload.typ,
        });
    }
    Ok(payload)
}

/// Verify a claim-replacement's two-authority requirement (Verification
/// Pipeline step 12).
///
/// Checks that `replacement`'s signing key (`replacement.tmb`) is
/// authorized by EXACTLY one of the two authorities
/// `[claim-replacement-authority]` names:
///
/// - **owner replacement** — `prior.owner` authorizes `replacement.tmb`
///   (`[owner-authorization-delegated]`'s per-value rule): the ordinary path, no marking required.
/// - **governance replacement** — some entry in `charter_owners` authorizes `replacement.tmb` (the
///   set composition rule) AND `replacement.governance == true`: a signer authorized by the
///   effective charter's owner set but NOT marking the replacement as a governance seizure is
///   treated the same as an unauthorized signer — `[claim-replacement-authority]`'s "MUST carry
///   `governance: true`" is itself part of the authority check, not a separate concern.
///
/// A signer matching neither path is rejected with
/// [`VerifyError::Unauthorized`] — the same variant
/// [`verify_publish_authorized`] and [`verify_succession_chain`] use for
/// single-key authorization failures elsewhere in this crate.
///
/// Beyond authority, also checks `[claim-replacement-transition]` PRE:
/// `replacement.now` MUST strictly exceed `prior.now`
/// ([`VerifyError::ReplacementNotAfterPrior`]), and `(replacement.anchor,
/// replacement.label)` MUST equal `(prior.anchor, prior.label)`
/// ([`VerifyError::ReplacementIdentityChanged`]) — a replacement never
/// alters identity.
///
/// Spec constraints: `[claim-replacement-authority]`,
/// `[claim-replacement-transition]`.
#[cfg(feature = "serde")]
pub fn verify_claim_replacement(
    replacement: &ClaimPayload,
    prior: &ClaimPayload,
    charter_owners: &[OwnerRef],
) -> Result<(), VerifyError> {
    let owner_authorized = prior.owner.authorizes(&replacement.tmb);
    let governance_authorized =
        replacement.governance && owner_set_authorizes(charter_owners, &replacement.tmb);
    if !owner_authorized && !governance_authorized {
        return Err(VerifyError::Unauthorized);
    }
    if replacement.now <= prior.now {
        return Err(VerifyError::ReplacementNotAfterPrior);
    }
    if replacement.anchor != prior.anchor || replacement.label != prior.label {
        return Err(VerifyError::ReplacementIdentityChanged);
    }
    Ok(())
}

/// Compute the Coz digest (`czd`) of a signed message from its raw wire
/// components: the canonical payload bytes and the signature.
///
/// Per the Coz spec, `czd` is `digest({"cad","sig"})` — a cryptographic
/// identifier independently recomputable by any party from `pay` and `sig`
/// alone. It refers to a particular signed message the same way `cad`
/// refers to a particular payload, and it must never be conflated with
/// wherever the message happens to be stored (e.g. a git object id):
/// storage location is an implementation accident, not a property of the
/// signed content.
///
/// This is the single, canonical way callers should derive a claim's or
/// publish's identity from a `(pay_json, sig, alg)` triple; hand-rolling
/// the algorithm dispatch at each call site risks divergence.
///
/// Spec constraints: `[czd-recalculatable]`, `[sig-over-pay]`.
#[cfg(feature = "serde")]
pub fn czd_for_alg(pay_json: &[u8], sig: &[u8], alg: &str) -> Result<Czd, VerifyError> {
    let cad = coz_rs::canonical_hash_for_alg(pay_json, alg, None)
        .ok_or_else(|| VerifyError::UnsupportedAlgorithm(alg.to_string()))?;
    coz_rs::czd_for_alg(&cad, sig, alg)
        .ok_or_else(|| VerifyError::UnsupportedAlgorithm(alg.to_string()))
}

/// Verify a Coz signature over raw JSON payload bytes.
///
/// Shared logic for [`verify_claim`] and [`verify_publish`].
#[cfg(feature = "serde")]
fn verify_signature(
    pay_json: &[u8],
    sig: &[u8],
    alg: &str,
    pub_key: &[u8],
) -> Result<(), VerifyError> {
    match coz_rs::verify_json(pay_json, sig, alg, pub_key) {
        Some(true) => Ok(()),
        Some(false) => Err(VerifyError::InvalidSignature),
        None => Err(VerifyError::UnsupportedAlgorithm(alg.to_owned())),
    }
}

// ============================================================================
// Pipeline verification
// ============================================================================
//
// The Local Verification steps (`docs/specs/atom-transactions.md`,
// "Verification Pipeline" → "Local Verification") that operate on
// claim/publish payloads: steps 6 (claim side only), 7, 8, 9, 10, 11, 12,
// and 13. Steps 2 and 3 walk the charter succession chain itself and
// live in `charter.rs` (`verify_charter_chain_signatures`,
// `verify_succession_chain`) alongside the rest of the charter-chain
// idiom.

/// Verify a claim's declared thumbprint against its actual signing key
/// (Verification Pipeline step 6, claim side).
///
/// Checks `tmb(claim.key) == claim.pay.tmb`: the thumbprint computed
/// from the raw public key that signed the claim must match the
/// thumbprint the claim's own payload declares. A valid signature alone
/// does not establish this — any key can validly sign its own payload
/// while that payload asserts an unrelated `tmb`, defeating the TOFU key
/// binding `[claim-key-required]` exists to establish (and which later
/// authorization checks, e.g. [`verify_publish_authorized`], rely on
/// being trustworthy). The charter-side instance of this same step
/// (`tmb(charter.key) == charter.pay.tmb`) is out of scope for this
/// no-charter subset.
///
/// Spec constraint: `[claim-key-required]`.
#[cfg(feature = "serde")]
pub fn verify_claim_key_thumbprint(
    claim: &ClaimPayload,
    alg: &str,
    pub_key: &[u8],
) -> Result<(), VerifyError> {
    let computed = coz_rs::compute_thumbprint_for_alg(alg, pub_key)
        .ok_or_else(|| VerifyError::UnsupportedAlgorithm(alg.to_string()))?;
    if computed != claim.tmb {
        return Err(VerifyError::ThumbprintMismatch);
    }
    Ok(())
}

/// Verify a publish's declared thumbprint against its actual signing key —
/// the publish-side instance of Verification Pipeline step 6.
///
/// Checks `tmb(publish.key) == publish.pay.tmb`, exactly mirroring
/// [`verify_claim_key_thumbprint`]'s claim-side check. This closes the
/// soundness precondition [`verify_publish_authorized`]'s own doc comment
/// names precisely: the Verification Pipeline's step 6 table row is scoped
/// to "charter/claim," explicitly omitting publish, which is why no
/// binding check previously existed here — `publish.tmb` is a
/// self-declared payload field, and a tag carrying its own embedded `key`
/// (a caller's key-fallback convenience) verifies its signature fine
/// without this check ever confirming that embedded key matches the
/// `tmb` the payload claims. A caller MUST call this (or otherwise
/// establish the same binding) before trusting
/// [`verify_publish_authorized`]'s result.
///
/// Spec constraints: `[owner-authorization-delegated]`,
/// `[publish-transition]`.
#[cfg(feature = "serde")]
pub fn verify_publish_key_thumbprint(
    publish: &PublishPayload,
    alg: &str,
    pub_key: &[u8],
) -> Result<(), VerifyError> {
    let computed = coz_rs::compute_thumbprint_for_alg(alg, pub_key)
        .ok_or_else(|| VerifyError::UnsupportedAlgorithm(alg.to_string()))?;
    if computed != publish.tmb {
        return Err(VerifyError::ThumbprintMismatch);
    }
    Ok(())
}

/// Verify a claim chains to its founding charter (Verification Pipeline
/// step 7).
///
/// Checks `claim.anchor == czd(charter₀)` (`[claim-chains-charter]`) —
/// the claim-level analogue of [`verify_publish_chains_claim`]'s
/// charter : claim :: claim : publish relationship. The founding
/// charter's czd is recomputed independently from its own raw wire
/// components (payload JSON, signature, algorithm) via [`czd_for_alg`]
/// rather than trusted from a caller-supplied value, mirroring step 8's
/// same anti-assertion discipline.
///
/// `founding_pay_json`/`founding_sig`/`founding_alg` are the founding
/// charter's (`chain[0]`, no `prior`) own wire components — resolving
/// which charter in a chain is the founding one is the caller's job
/// (the same division of labor [`crate::verify_succession_chain`]
/// establishes for chain resolution generally).
///
/// Spec constraint: `[claim-chains-charter]`.
#[cfg(feature = "serde")]
pub fn verify_claim_chains_charter(
    claim: &ClaimPayload,
    founding_pay_json: &[u8],
    founding_sig: &[u8],
    founding_alg: &str,
) -> Result<(), VerifyError> {
    let founding_czd = czd_for_alg(founding_pay_json, founding_sig, founding_alg)?;
    if claim.anchor.as_bytes() != founding_czd.as_bytes() {
        return Err(VerifyError::ClaimChartersMismatch);
    }
    Ok(())
}

/// Verify a publish's claim-chain link (Verification Pipeline step 8).
///
/// Checks `publish.claim == czd(claim)`. The claim's czd is recomputed
/// independently from its own raw wire components (payload JSON,
/// signature, algorithm) via [`czd_for_alg`] rather than trusted from a
/// caller-supplied value, so a publish cannot merely assert a chain it
/// does not actually hold.
///
/// Spec constraint: `[publish-chains-claim]`.
#[cfg(feature = "serde")]
pub fn verify_publish_chains_claim(
    publish: &PublishPayload,
    claim_pay_json: &[u8],
    claim_sig: &[u8],
    claim_alg: &str,
) -> Result<(), VerifyError> {
    let claim_czd = czd_for_alg(claim_pay_json, claim_sig, claim_alg)?;
    if publish.claim != claim_czd {
        return Err(VerifyError::ClaimChainMismatch);
    }
    Ok(())
}

/// Verify the strict temporal ordering across a charter/claim/publish
/// triple (Verification Pipeline step 9).
///
/// Checks `charter.now < claim.now < publish.now`, strictly — an equal
/// timestamp at either boundary is a violation, not a pass. `charter` is
/// the effective charter (the same resolved payload
/// [`verify_claim_authorized_by_charter`] takes for step 10), not
/// necessarily the founding charter step 7 anchors to.
///
/// Spec constraint: Verification Pipeline step 9.
#[cfg(feature = "serde")]
pub fn verify_temporal_ordering(
    charter: &CharterPayload,
    claim: &ClaimPayload,
    publish: &PublishPayload,
) -> Result<(), VerifyError> {
    if !(charter.now < claim.now && claim.now < publish.now) {
        return Err(VerifyError::TemporalOrderViolation);
    }
    Ok(())
}

/// Verify a claim's signer is authorized by the effective charter's
/// owner (Verification Pipeline step 10).
///
/// Checks `claim.tmb == effective_charter.owner`
/// (`[claim-charter-authorization]`) — distinct from step 3's per-link
/// chain authorization ([`crate::verify_succession_chain`]): this checks
/// the CLAIM against the chain's resolved tail, not one charter link
/// against the previous one. `effective_charter` is the single resolved
/// payload the caller has already selected as the chain's current tail
/// — this function does not walk or resolve the chain itself, matching
/// how [`crate::verify_bootstrap_gate`] and [`verify_publish_authorized`]
/// take single already-resolved payloads rather than chains.
///
/// A mismatch is reported as [`VerifyError::Unauthorized`], the same
/// variant [`verify_publish_authorized`] and
/// [`crate::verify_succession_chain`] use for single-key authorization
/// failures elsewhere in this crate.
///
/// Spec constraint: `[claim-charter-authorization]`.
#[cfg(feature = "serde")]
pub fn verify_claim_authorized_by_charter(
    claim: &ClaimPayload,
    effective_charter: &CharterPayload,
) -> Result<(), VerifyError> {
    if !owner_set_authorizes(&effective_charter.owner, &claim.tmb) {
        return Err(VerifyError::Unauthorized);
    }
    Ok(())
}

/// Verify a publish's signer is authorized by the claim's owner
/// (Verification Pipeline step 11).
///
/// Checks `publish.tmb` against `claim.owner` under the single-key
/// identity framework's byte-equality semantics: `publish.tmb ==
/// claim.owner` (`[owner-authorization-delegated]`). Richer identity
/// frameworks (hierarchical, rooted-identity) resolve authorization
/// differently and are a caller concern this crate does not implement.
///
/// **Soundness precondition the caller MUST establish first — this
/// function does NOT check it.** `publish.tmb` is a self-declared
/// payload field; nothing in THIS function binds `tmb(publish.key) ==
/// publish.tmb` (the Verification Pipeline's own step 6 table row,
/// `docs/specs/atom-transactions.md`, is scoped to "charter/claim,"
/// explicitly omitting publish — [`verify_publish_key_thumbprint`] closes
/// that gap as its own, separate step). Calling this function without
/// first calling [`verify_publish_key_thumbprint`] (or otherwise
/// independently establishing that binding) lets an attacker sign with
/// any key while declaring `publish.tmb = claim.owner`, and this function
/// will wrongly report "authorized."
///
/// Spec constraints: `[owner-authorization-delegated]`,
/// `[publish-transition]`.
#[cfg(feature = "serde")]
pub fn verify_publish_authorized(
    publish: &PublishPayload,
    claim: &ClaimPayload,
) -> Result<(), VerifyError> {
    if !claim.owner.authorizes(&publish.tmb) {
        return Err(VerifyError::Unauthorized);
    }
    Ok(())
}

/// Verify a payload's `(anchor, label)` against an expected `AtomId`
/// (Verification Pipeline step 13).
///
/// Both `ClaimPayload` and `PublishPayload` carry `anchor`/`label`
/// directly (`[symmetric-payloads]`), so the caller extracts the pair
/// from whichever payload it holds and passes it here uniformly.
///
/// Spec constraint: `[symmetric-payloads]`.
#[cfg(feature = "serde")]
pub fn verify_atom_id(
    anchor: &Anchor,
    label: &Label,
    expected: &AtomId,
) -> Result<(), VerifyError> {
    if anchor != expected.anchor() || label != expected.label() {
        return Err(VerifyError::AtomIdMismatch);
    }
    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;

/// `content_hash` wire tests (c3-field-wired) — kept inline here rather
/// than in `tests.rs`, since this node's declared file surface covers
/// `lib.rs` only.
#[cfg(all(test, feature = "serde"))]
mod content_hash_wire_tests {
    fn fixture_payload() -> crate::PublishPayload {
        crate::PublishPayload::new(
            crate::Alg::ES256,
            crate::AtomId::new(
                crate::Anchor::new(vec![1, 2, 3, 4]),
                crate::Label::try_from("my-pkg").unwrap(),
            ),
            crate::Czd::from_bytes(vec![5, 6]),
            vec![7, 8],
            2000,
            "src/lib".into(),
            vec![9, 10],
            crate::Thumbprint::from_bytes(vec![10, 20, 30]),
            crate::RawVersion::new("1.0.0".into()),
        )
    }

    /// A payload with `content_hash` unset omits the field entirely on
    /// the wire (`skip_serializing_if`), rather than serializing `null`.
    #[test]
    fn absent_content_hash_is_omitted_from_wire() {
        let payload = fixture_payload();
        assert!(payload.content_hash.is_none());

        let json = serde_json::to_value(&payload).unwrap();
        assert!(
            !json.as_object().unwrap().contains_key("content_hash"),
            "content_hash key must be absent, not null, when unset"
        );
    }

    /// A payload with `content_hash` set round-trips through
    /// base64url-unpadded JSON exactly, mirroring `dig`/`src`'s encoding.
    #[test]
    fn present_content_hash_round_trips_base64() {
        let mut payload = fixture_payload();
        let digest = vec![0xAB; 32];
        payload.content_hash = Some(digest.clone());

        let json = serde_json::to_value(&payload).unwrap();
        let encoded = json
            .as_object()
            .unwrap()
            .get("content_hash")
            .expect("content_hash key must be present when set")
            .as_str()
            .expect("content_hash must serialize as a string");
        // base64url-unpadded, no '+' '/' or '=' padding characters.
        assert!(!encoded.contains('+') && !encoded.contains('/') && !encoded.contains('='));

        let round_tripped: crate::PublishPayload = serde_json::from_value(json).unwrap();
        assert_eq!(round_tripped.content_hash, Some(digest));
    }
}
