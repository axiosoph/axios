//! Atom URI parsing and alias-aware resolution.
//!
//! An atom URI identifies a specific atom, optionally within a remote source
//! and at a specific version. The grammar is:
//!
//! ```text
//! [source::] label [@version]
//! ```
//!
//! - **source** — a URL, SCP-style address, path, or `+`-prefixed alias (resolved via [`alurl`]).
//!   Split from the atom-ref by the **rightmost** `::` delimiter.
//! - **label** — a validated [`Label`] identifying the atom within its set.
//! - **version** — an unparsed [`RawVersion`] string. Interpretation is deferred to a
//!   [`VersionScheme`](atom_id::VersionScheme) implementor.
//!
//! The `@` for version extraction uses **rightmost** split to avoid ambiguity
//! with `@` in source URLs (e.g., `git@github.com:repo::atom@1.0`).
//!
//! # Types
//!
//! - [`RawAtomUri`] — parsed but unresolved (alias not yet expanded).
//! - [`AtomUri`] — fully resolved (source aliases expanded via [`AliasMap`]).
//!
//! # Examples
//!
//! ```
//! use atom_uri::RawAtomUri;
//!
//! // Bare label
//! let uri: RawAtomUri = "my-atom".parse().unwrap();
//! assert_eq!(uri.label().to_string(), "my-atom");
//! assert!(uri.source().is_none());
//! assert!(uri.version().is_none());
//!
//! // Source + label + version
//! let uri: RawAtomUri = "github.com/owner/repo::my-atom@1.0.0".parse().unwrap();
//! assert_eq!(uri.source().unwrap(), "github.com/owner/repo");
//! assert_eq!(uri.label().to_string(), "my-atom");
//! assert_eq!(uri.version().unwrap().as_str(), "1.0.0");
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

use std::fmt;
use std::str::FromStr;

pub use alurl::{AliasMap, AliasSource, AliasedUrl};
pub use atom_id::{Label, RawVersion};

// ============================================================================
// Errors
// ============================================================================

/// Errors during atom URI parsing or resolution.
#[derive(Debug)]
pub enum UriError {
    /// No label found after `::` delimiter or in bare input.
    MissingLabel,
    /// The label portion failed [`Label`] validation.
    InvalidLabel(atom_id::Error),
    /// An empty version string after `@` (e.g., `atom@`).
    EmptyVersion,
    /// Alias resolution failed during [`RawAtomUri::resolve`].
    AliasError(alurl::ResolveError),
}

impl fmt::Display for UriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLabel => write!(f, "missing atom label"),
            Self::InvalidLabel(e) => write!(f, "invalid atom label: {e}"),
            Self::EmptyVersion => write!(f, "empty version after '@'"),
            Self::AliasError(e) => write!(f, "alias resolution failed: {e}"),
        }
    }
}

impl std::error::Error for UriError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidLabel(e) => Some(e),
            Self::AliasError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<atom_id::Error> for UriError {
    fn from(e: atom_id::Error) -> Self {
        Self::InvalidLabel(e)
    }
}

impl From<alurl::ResolveError> for UriError {
    fn from(e: alurl::ResolveError) -> Self {
        Self::AliasError(e)
    }
}

// ============================================================================
// RawAtomUri
// ============================================================================

/// A parsed but unresolved atom URI.
///
/// Contains the raw source string (which may include `+`-prefixed aliases),
/// a validated [`Label`], and an optional [`RawVersion`]. No alias resolution
/// has been performed — call [`resolve`](RawAtomUri::resolve) to expand
/// aliases via an [`AliasMap`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawAtomUri {
    source: Option<String>,
    label: Label,
    version: Option<RawVersion>,
}

impl RawAtomUri {
    /// The source component, if present (before `::` delimiter).
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// The atom label.
    #[must_use]
    pub fn label(&self) -> &Label {
        &self.label
    }

    /// The raw version string, if present (after `@`).
    #[must_use]
    pub fn version(&self) -> Option<&RawVersion> {
        self.version.as_ref()
    }

    /// Resolve aliases in the source component.
    ///
    /// If the source contains a `+`-prefixed alias at a valid host position,
    /// it is expanded via the [`AliasMap`]. If no source is present, or the
    /// source contains no alias, resolution still succeeds.
    ///
    /// # Errors
    ///
    /// - [`UriError::AliasError`] — alias resolution failed (not found, invalid name, or cycle).
    pub fn resolve(&self, map: &AliasMap) -> Result<AtomUri, UriError> {
        let resolved_source = match &self.source {
            Some(src) => Some(map.resolve(src)?),
            None => None,
        };

        Ok(AtomUri {
            source: resolved_source,
            label: self.label.clone(),
            version: self.version.clone(),
        })
    }
}

impl FromStr for RawAtomUri {
    type Err = UriError;

    /// Parse an atom URI string.
    ///
    /// Grammar: `[source::] label [@version]`
    ///
    /// Uses rightmost `::` for source split and rightmost `@` for version
    /// split to avoid ambiguity with `@` in URLs and `::` in paths.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, atom_ref) = match s.rsplit_once("::") {
            Some((src, rest)) => (Some(src.to_string()), rest),
            None => (None, s),
        };

        // Split label from version at rightmost '@'.
        let (label_str, version) = match atom_ref.rsplit_once('@') {
            Some((lbl, ver)) => {
                if ver.is_empty() {
                    return Err(UriError::EmptyVersion);
                }
                (lbl, Some(RawVersion::new(ver.to_owned())))
            },
            None => (atom_ref, None),
        };

        if label_str.is_empty() {
            return Err(UriError::MissingLabel);
        }

        let label = Label::try_from(label_str)?;

        Ok(RawAtomUri {
            source,
            label,
            version,
        })
    }
}

impl fmt::Display for RawAtomUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(src) = &self.source {
            write!(f, "{src}::")?;
        }
        write!(f, "{}", self.label)?;
        if let Some(ver) = &self.version {
            write!(f, "@{ver}")?;
        }
        Ok(())
    }
}

// ============================================================================
// AtomUri
// ============================================================================

/// A fully resolved atom URI.
///
/// The source component (if present) has been processed through
/// [`AliasMap::resolve`], yielding an [`AliasedUrl`] that is either
/// `Expanded` (alias was found and expanded) or `Raw` (no alias detected).
#[derive(Debug, Clone)]
pub struct AtomUri {
    source: Option<AliasedUrl>,
    label: Label,
    version: Option<RawVersion>,
}

impl AtomUri {
    /// The resolved source, if present.
    #[must_use]
    pub fn source(&self) -> Option<&AliasedUrl> {
        self.source.as_ref()
    }

    /// The resolved source URL string, if present.
    ///
    /// Returns the expanded URL for aliased sources, or the raw URL for
    /// non-aliased sources. Returns `None` if no source component exists.
    #[must_use]
    pub fn source_url(&self) -> Option<&str> {
        self.source.as_ref().map(|s| s.url())
    }

    /// The atom label.
    #[must_use]
    pub fn label(&self) -> &Label {
        &self.label
    }

    /// The raw version string, if present.
    #[must_use]
    pub fn version(&self) -> Option<&RawVersion> {
        self.version.as_ref()
    }
}

impl fmt::Display for AtomUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(src) = &self.source {
            write!(f, "{}::", src.url())?;
        }
        write!(f, "{}", self.label)?;
        if let Some(ver) = &self.version {
            write!(f, "@{ver}")?;
        }
        Ok(())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn aliases(pairs: &[(&str, &str)]) -> AliasMap {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // ========================================================================
    // Parsing: bare labels
    // ========================================================================

    #[test]
    fn bare_label() {
        let uri: RawAtomUri = "my-atom".parse().unwrap();
        assert_eq!(uri.source(), None);
        assert_eq!(uri.label().to_string(), "my-atom");
        assert_eq!(uri.version(), None);
    }

    #[test]
    fn bare_label_with_version() {
        let uri: RawAtomUri = "my-atom@^1.0".parse().unwrap();
        assert_eq!(uri.source(), None);
        assert_eq!(uri.label().to_string(), "my-atom");
        assert_eq!(uri.version().unwrap().as_str(), "^1.0");
    }

    #[test]
    fn bare_label_unicode() {
        let uri: RawAtomUri = "λ".parse().unwrap();
        assert_eq!(uri.label().to_string(), "λ");
    }

    // ========================================================================
    // Parsing: source + label
    // ========================================================================

    #[test]
    fn source_and_label() {
        let uri: RawAtomUri = "github.com/owner/repo::my-atom".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "github.com/owner/repo");
        assert_eq!(uri.label().to_string(), "my-atom");
        assert_eq!(uri.version(), None);
    }

    #[test]
    fn source_and_label_with_version() {
        let uri: RawAtomUri = "github.com/owner/repo::my-atom@1.0.0".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "github.com/owner/repo");
        assert_eq!(uri.label().to_string(), "my-atom");
        assert_eq!(uri.version().unwrap().as_str(), "1.0.0");
    }

    #[test]
    fn scp_style_source() {
        let uri: RawAtomUri = "git@github.com:owner/repo::my-atom@^1".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "git@github.com:owner/repo");
        assert_eq!(uri.label().to_string(), "my-atom");
        assert_eq!(uri.version().unwrap().as_str(), "^1");
    }

    #[test]
    fn scheme_url_source() {
        let uri: RawAtomUri = "https://example.com/repo::foo@^1".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "https://example.com/repo");
        assert_eq!(uri.label().to_string(), "foo");
        assert_eq!(uri.version().unwrap().as_str(), "^1");
    }

    #[test]
    fn path_source() {
        let uri: RawAtomUri = "/foo/bar/baz::my-atom".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "/foo/bar/baz");
        assert_eq!(uri.label().to_string(), "my-atom");
    }

    #[test]
    fn relative_path_source() {
        let uri: RawAtomUri = "foo/bar/baz::my-atom".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "foo/bar/baz");
        assert_eq!(uri.label().to_string(), "my-atom");
    }

    #[test]
    fn empty_source() {
        let uri: RawAtomUri = "::foo".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "");
        assert_eq!(uri.label().to_string(), "foo");
    }

    // ========================================================================
    // Parsing: edge cases
    // ========================================================================

    #[test]
    fn at_in_source_not_version() {
        let uri: RawAtomUri = "git@github.com:repo::atom@1.0".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "git@github.com:repo");
        assert_eq!(uri.label().to_string(), "atom");
        assert_eq!(uri.version().unwrap().as_str(), "1.0");
    }

    #[test]
    fn credentials_with_at_and_version() {
        let uri: RawAtomUri = "https://user:pass@example.com/repo::id@^0.2"
            .parse()
            .unwrap();
        assert_eq!(uri.source().unwrap(), "https://user:pass@example.com/repo");
        assert_eq!(uri.label().to_string(), "id");
        assert_eq!(uri.version().unwrap().as_str(), "^0.2");
    }

    #[test]
    fn multiple_double_colons_rightmost_wins() {
        let uri: RawAtomUri = "a::b::c".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "a::b");
        assert_eq!(uri.label().to_string(), "c");
    }

    #[test]
    fn port_in_source() {
        let uri: RawAtomUri = "https://example.com:8080/repo::foo@^1".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "https://example.com:8080/repo");
        assert_eq!(uri.label().to_string(), "foo");
    }

    #[test]
    fn aliased_source() {
        let uri: RawAtomUri = "+gh/owner/repo::my-atom".parse().unwrap();
        assert_eq!(uri.source().unwrap(), "+gh/owner/repo");
        assert_eq!(uri.label().to_string(), "my-atom");
    }

    // ========================================================================
    // Parsing: errors
    // ========================================================================

    #[test]
    fn missing_label_after_delimiter() {
        let result = "source::".parse::<RawAtomUri>();
        assert!(matches!(result, Err(UriError::MissingLabel)));
    }

    #[test]
    fn missing_label_after_delimiter_with_version() {
        let result = "source::@1.0".parse::<RawAtomUri>();
        assert!(matches!(result, Err(UriError::MissingLabel)));
    }

    #[test]
    fn empty_version_string() {
        let result = "atom@".parse::<RawAtomUri>();
        assert!(matches!(result, Err(UriError::EmptyVersion)));
    }

    #[test]
    fn invalid_label_digit_start() {
        let result = "source::123bad".parse::<RawAtomUri>();
        assert!(matches!(result, Err(UriError::InvalidLabel(_))));
    }

    // ========================================================================
    // Display roundtrip
    // ========================================================================

    #[test]
    fn display_bare() {
        let uri: RawAtomUri = "my-atom".parse().unwrap();
        assert_eq!(uri.to_string(), "my-atom");
    }

    #[test]
    fn display_full() {
        let uri: RawAtomUri = "github.com/repo::my-atom@1.0".parse().unwrap();
        assert_eq!(uri.to_string(), "github.com/repo::my-atom@1.0");
    }

    // ========================================================================
    // Resolution
    // ========================================================================

    #[test]
    fn resolve_no_source() {
        let map = AliasMap::new();
        let uri: RawAtomUri = "my-atom@1.0".parse().unwrap();
        let resolved = uri.resolve(&map).unwrap();
        assert!(resolved.source().is_none());
        assert_eq!(resolved.label().to_string(), "my-atom");
        assert_eq!(resolved.version().unwrap().as_str(), "1.0");
    }

    #[test]
    fn resolve_raw_source() {
        let map = AliasMap::new();
        let uri: RawAtomUri = "github.com/repo::my-atom".parse().unwrap();
        let resolved = uri.resolve(&map).unwrap();
        assert_eq!(resolved.source_url().unwrap(), "github.com/repo");
    }

    #[test]
    fn resolve_aliased_source() {
        let map = aliases(&[("gh", "github.com")]);
        let uri: RawAtomUri = "+gh/owner/repo::my-atom".parse().unwrap();
        let resolved = uri.resolve(&map).unwrap();
        assert_eq!(resolved.source_url().unwrap(), "github.com/owner/repo");
        assert_eq!(resolved.label().to_string(), "my-atom");
    }

    #[test]
    fn resolve_alias_error_propagated() {
        let map = AliasMap::new();
        let uri: RawAtomUri = "+unknown/repo::my-atom".parse().unwrap();
        let result = uri.resolve(&map);
        assert!(matches!(result, Err(UriError::AliasError(_))));
    }

    #[test]
    fn resolve_scp_with_alias() {
        let map = aliases(&[("gh", "github.com")]);
        let uri: RawAtomUri = "git@+gh:owner/repo::this-atom@^1".parse().unwrap();
        let resolved = uri.resolve(&map).unwrap();
        assert_eq!(resolved.source_url().unwrap(), "git@github.com:owner/repo");
        assert_eq!(resolved.label().to_string(), "this-atom");
        assert_eq!(resolved.version().unwrap().as_str(), "^1");
    }

    #[test]
    fn resolved_display() {
        let map = aliases(&[("gh", "github.com")]);
        let uri: RawAtomUri = "+gh/owner/repo::my-atom@1.0".parse().unwrap();
        let resolved = uri.resolve(&map).unwrap();
        assert_eq!(resolved.to_string(), "github.com/owner/repo::my-atom@1.0");
    }
}
