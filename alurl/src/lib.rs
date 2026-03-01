//! Structure-preserving URL alias detection and expansion.
//!
//! Alurl detects `+`-prefixed aliases at host positions within URL-like
//! strings and expands them via a user-provided [`AliasMap`]. It understands
//! enough URL structure (scheme, credentials, separators) to locate aliases,
//! but performs no validation, normalization, or scheme inference.
//!
//! # Design
//!
//! Alurl is a pure function library: given an input string and an `AliasMap`,
//! it produces a deterministic output string. No I/O, no side effects, no
//! external dependencies beyond [`unicode-ident`] for alias name validation.
//!
//! # Examples
//!
//! ```
//! use alurl::{AliasMap, AliasedUrl};
//!
//! let mut aliases = AliasMap::new();
//! aliases.insert("gh", "github.com");
//!
//! let result = aliases.resolve("+gh/owner/repo").unwrap();
//! assert_eq!(result.url(), "github.com/owner/repo");
//! ```

use std::collections::HashMap;

mod parse;

// ============================================================================
// Types
// ============================================================================

/// Concrete alias mapping with resolution logic.
///
/// Newtype wrapper around a hash map. Keys are alias names, values are host
/// strings (e.g., `"github.com"`). Alias values SHOULD NOT contain schemes —
/// this allows one alias to work across multiple transports.
///
/// All resolution logic (lookup, recursive expansion, cycle detection) is
/// owned by this type via [`AliasMap::resolve`].
#[derive(Debug, Clone)]
pub struct AliasMap(HashMap<String, String>);

/// Result of alias resolution.
///
/// Either the input contained a `+`-prefixed alias at a valid host position
/// and has been expanded, or the input contained no alias and is passed
/// through as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AliasedUrl {
    /// The input was aliased and has been expanded.
    Expanded {
        /// The original alias name (first in the chain, before recursive
        /// resolution). Enables diagnostic messages referencing user input.
        alias: String,
        /// The fully expanded URL string.
        url: String,
    },
    /// The input was not aliased. Contains the exact input string.
    Raw(String),
}

/// Errors during alias resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// The alias name was not found in the [`AliasMap`].
    AliasNotFound(String),
    /// A `+` was found at a host position but the alias name fails
    /// UAX #31 validation.
    InvalidAliasName(String),
    /// Recursive resolution encountered the same alias twice.
    CycleDetected {
        /// The full chain of alias names forming the cycle.
        chain: Vec<String>,
    },
}

// ============================================================================
// Traits
// ============================================================================

/// Abstract configuration loading interface.
///
/// Implementors read aliases from whatever source (TOML, JSON, env vars) into
/// an [`AliasMap`]. Resolution logic is NOT the implementor's concern — alurl
/// handles that once it has the map.
pub trait AliasSource {
    /// Error type for loading failures.
    type Error: std::error::Error;
    /// Load aliases into an [`AliasMap`].
    fn load(&self) -> Result<AliasMap, Self::Error>;
}

// ============================================================================
// Impls — AliasedUrl
// ============================================================================

impl AliasedUrl {
    /// Returns the resolved URL string, whether aliased or raw.
    #[must_use]
    pub fn url(&self) -> &str {
        match self {
            Self::Expanded { url, .. } => url,
            Self::Raw(url) => url,
        }
    }
}

// ============================================================================
// Impls — AliasMap
// ============================================================================

impl AliasMap {
    /// Creates an empty alias map.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Creates an alias map with pre-allocated capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self(HashMap::with_capacity(capacity))
    }

    /// Inserts an alias mapping.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.insert(name.into(), value.into());
    }

    /// Resolves aliases in the input string.
    ///
    /// Detects a `+`-prefixed alias at a valid host position, expands it
    /// using the map, and returns the result. Resolution is recursive:
    /// if the expanded value itself contains a `+` at a host position,
    /// it is resolved again (with cycle detection).
    ///
    /// If no alias is present, the input is returned as [`AliasedUrl::Raw`].
    ///
    /// # Errors
    ///
    /// - [`ResolveError::AliasNotFound`] — `+` at host position but alias name not in map.
    /// - [`ResolveError::InvalidAliasName`] — alias name fails UAX #31.
    /// - [`ResolveError::CycleDetected`] — recursive resolution loops.
    pub fn resolve(&self, input: &str) -> Result<AliasedUrl, ResolveError> {
        let classified = parse::classify(input)?;

        match classified {
            parse::Classification::Raw => Ok(AliasedUrl::Raw(input.to_string())),
            parse::Classification::Aliased {
                prefix,
                alias_name,
                suffix,
            } => {
                let original_alias = alias_name.to_string();
                let mut chain = vec![original_alias.clone()];

                let value = self
                    .0
                    .get(alias_name)
                    .ok_or_else(|| ResolveError::AliasNotFound(alias_name.to_string()))?;

                let expanded = reconstruct(prefix, value, suffix);
                self.resolve_recursive(&expanded, &original_alias, &mut chain)
            },
        }
    }

    /// Recursive resolution with cycle detection.
    fn resolve_recursive(
        &self,
        input: &str,
        original_alias: &str,
        chain: &mut Vec<String>,
    ) -> Result<AliasedUrl, ResolveError> {
        let classified = parse::classify(input)?;

        match classified {
            parse::Classification::Raw => Ok(AliasedUrl::Expanded {
                alias: original_alias.to_string(),
                url: input.to_string(),
            }),
            parse::Classification::Aliased {
                prefix,
                alias_name,
                suffix,
            } => {
                if chain.iter().any(|n| n == alias_name) {
                    chain.push(alias_name.to_string());
                    return Err(ResolveError::CycleDetected {
                        chain: chain.clone(),
                    });
                }
                chain.push(alias_name.to_string());

                let value = self
                    .0
                    .get(alias_name)
                    .ok_or_else(|| ResolveError::AliasNotFound(alias_name.to_string()))?;

                let expanded = reconstruct(prefix, value, suffix);
                self.resolve_recursive(&expanded, original_alias, chain)
            },
        }
    }
}

impl Default for AliasMap {
    fn default() -> Self {
        Self::new()
    }
}

impl From<HashMap<String, String>> for AliasMap {
    fn from(map: HashMap<String, String>) -> Self {
        Self(map)
    }
}

impl<S1, S2> FromIterator<(S1, S2)> for AliasMap
where
    S1: Into<String>,
    S2: Into<String>,
{
    fn from_iter<I: IntoIterator<Item = (S1, S2)>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        )
    }
}

// ============================================================================
// Impls — ResolveError
// ============================================================================

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AliasNotFound(name) => write!(f, "alias not found: {name}"),
            Self::InvalidAliasName(name) => write!(f, "invalid alias name: {name}"),
            Self::CycleDetected { chain } => {
                write!(f, "alias cycle detected: {}", chain.join(" → "))
            },
        }
    }
}

impl std::error::Error for ResolveError {}

// ============================================================================
// Private helpers
// ============================================================================

/// Reconstruct the expanded string: prefix + resolved + separator + suffix.
fn reconstruct(prefix: &str, resolved: &str, suffix: Option<(char, &str)>) -> String {
    let extra = suffix.as_ref().map(|(_, s)| s.len() + 1).unwrap_or(0);
    let mut result = String::with_capacity(prefix.len() + resolved.len() + extra);
    result.push_str(prefix);
    result.push_str(resolved);
    if let Some((sep, rest)) = suffix {
        result.push(sep);
        result.push_str(rest);
    }
    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests;
