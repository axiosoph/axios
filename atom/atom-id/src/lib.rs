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

use std::fmt;
use std::str::FromStr;

pub use charter::{CharterPayload, CharterStore, TYP_CHARTER};
#[cfg(feature = "serde")]
pub use charter::{verify_bootstrap_gate, verify_charter, verify_succession_chain};
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
    /// Opaque identity digest of the owner (e.g., Coz thumbprint or Cyphr PR).
    ///
    /// Spec constraint: `[owner-abstract]`.
    #[cfg_attr(feature = "serde", serde(with = "serde_b64"))]
    pub owner: Vec<u8>,
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
        owner: Vec<u8>,
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
        owner: Vec<u8>,
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
    /// automatically, and leaves `mode` and `meta` unset — use
    /// [`PublishPayload::effective_mode`] to read the resolved mode, and
    /// set `meta`/`mode` directly on the returned value if needed.
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

/// Verify a claim-replacement's two-authority requirement.
///
/// **Deliberately unimplemented — Phase 1.** A replacement claim's
/// authority is a materially new kind of verification beyond the
/// single-message check [`verify_claim`] performs: checking that the
/// signing key is authorized by EITHER `prior`'s `owner` (the ordinary,
/// unmarked owner-replacement path) OR `charter_owner` (the
/// governance-replacement path, which additionally MUST carry
/// `governance: true` on `replacement`) — and rejecting any signer
/// outside both. Declaring this seam now (without a working validator)
/// lets later phases de-stub it without reshaping the call surface.
///
/// Spec constraints: `[claim-replacement-authority]`,
/// `[claim-replacement-transition]`.
#[cfg(feature = "serde")]
pub fn verify_claim_replacement(
    _replacement: &ClaimPayload,
    _prior: &ClaimPayload,
    _charter_owner: &[u8],
) -> Result<(), VerifyError> {
    unimplemented!(
        "Phase 1: claim-replacement two-authority verification is a specified deliverable, not a \
         default — see docs/specs/atom-transactions.md [claim-replacement-authority] and \
         [claim-replacement-transition]"
    )
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
// Pipeline verification (no-charter subset)
// ============================================================================
//
// The remaining Local Verification steps checkable without walking a
// charter succession chain (`docs/specs/atom-transactions.md`,
// "Verification Pipeline" → "Local Verification"): steps 6 (claim side
// only), 8, 11, and 13. Steps 2, 3, 7, 9, 10, 12 require charter data
// (walking the succession chain or resolving charter fields) and are
// out of scope here — see `n3-verify-charter-steps`.

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
/// payload field; nothing in this crate currently binds
/// `tmb(publish.key) == publish.tmb` (the Verification Pipeline's own
/// step 6 table row, `docs/specs/atom-transactions.md`, is scoped to
/// "charter/claim," explicitly omitting publish). Calling this
/// function without independently establishing that binding — e.g. by
/// computing the thumbprint of whatever key actually signed the
/// publish (`coz_rs::compute_thumbprint_for_alg`) and confirming it
/// equals `publish.tmb` before trusting this check's result — lets an
/// attacker sign with any key while declaring `publish.tmb =
/// claim.owner`, and this function will wrongly report "authorized."
///
/// Spec constraints: `[owner-authorization-delegated]`,
/// `[publish-transition]`.
#[cfg(feature = "serde")]
pub fn verify_publish_authorized(
    publish: &PublishPayload,
    claim: &ClaimPayload,
) -> Result<(), VerifyError> {
    if publish.tmb.as_bytes() != claim.owner.as_slice() {
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
