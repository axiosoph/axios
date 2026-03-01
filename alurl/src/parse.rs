//! Host position detection and alias classification.
//!
//! Implements the URL structure awareness needed to locate `+`-prefixed
//! aliases at valid host positions per the spec's `[host-position-only]`
//! constraint.

use crate::ResolveError;

// ============================================================================
// Types
// ============================================================================

/// Result of classifying an input string for alias presence.
pub(crate) enum Classification<'a> {
    /// Input contains a valid alias at a host position.
    Aliased {
        /// Everything before the `+` sigil (scheme, credentials).
        prefix: &'a str,
        /// The alias name (after `+`, before separator or end).
        alias_name: &'a str,
        /// Separator character and opaque suffix, if present.
        suffix: Option<(char, &'a str)>,
    },
    /// Input does not contain an alias at a host position.
    Raw,
}

// ============================================================================
// Functions
// ============================================================================

/// Classify an input string for alias presence.
///
/// Implements the host position detection algorithm:
/// 1. Check for scheme (`://`) and skip past it.
/// 2. Determine authority boundary (first `/` for scheme URLs, first `/` or `:` for bare/SCP).
/// 3. Find last `@` within authority to skip credentials.
/// 4. Check for `+` at the resulting host position.
/// 5. If `+` found, extract and validate the alias name (UAX #31).
pub(crate) fn classify(input: &str) -> Result<Classification<'_>, ResolveError> {
    if input.is_empty() {
        return Ok(Classification::Raw);
    }

    let host_pos = find_host_position(input);

    if !input[host_pos..].starts_with('+') {
        return Ok(Classification::Raw);
    }

    let name_start = host_pos + 1;
    let remaining = &input[name_start..];

    // Find end of alias name: first '/' or ':' or end of string.
    let name_len = remaining.find(['/', ':']).unwrap_or(remaining.len());

    let alias_name = &remaining[..name_len];
    validate_alias_name(alias_name)?;

    let prefix = &input[..host_pos];
    let after_name = name_start + name_len;

    let suffix = if after_name < input.len() {
        let sep = input.as_bytes()[after_name] as char;
        let rest = &input[after_name + 1..];
        Some((sep, rest))
    } else {
        None
    };

    Ok(Classification::Aliased {
        prefix,
        alias_name,
        suffix,
    })
}

/// Locate the host position in an input string.
///
/// Per spec `[host-position-only]`:
/// 1. Check for a valid scheme (`://`) and skip past it.
/// 2. Determine the authority boundary — first `/` if scheme is present, first `/` or `:`
///    otherwise.
/// 3. Find the last `@` within the authority block to skip credentials.
/// 4. Host position is immediately after the last `@`, or at the start of the authority if no `@`
///    is found.
fn find_host_position(input: &str) -> usize {
    let (after_scheme, has_scheme) = find_scheme_end(input);

    // Authority boundary depends on whether a scheme was found.
    let search = &input[after_scheme..];
    let authority_end = if has_scheme {
        // URL context: authority ends at first '/'.
        search.find('/').map(|p| after_scheme + p)
    } else {
        // Bare/SCP context: authority ends at first '/' or ':'.
        search.find(['/', ':']).map(|p| after_scheme + p)
    }
    .unwrap_or(input.len());

    let authority = &input[after_scheme..authority_end];

    match authority.rfind('@') {
        Some(at_pos) => after_scheme + at_pos + 1,
        None => after_scheme,
    }
}

/// Find the end of a valid URI scheme (`://`).
///
/// Returns `(position_after_separator, true)` if a valid scheme is found,
/// or `(0, false)` otherwise. A valid scheme starts with an ASCII letter
/// and contains only ASCII alphanumeric characters, `+`, `-`, or `.`
/// (RFC 3986 §3.1).
fn find_scheme_end(input: &str) -> (usize, bool) {
    if let Some(pos) = input.find("://") {
        let scheme = &input[..pos];
        let valid = !scheme.is_empty()
            && scheme.as_bytes()[0].is_ascii_alphabetic()
            && scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'));
        if valid {
            return (pos + 3, true);
        }
    }
    (0, false)
}

/// Validate that a string is a valid UAX #31 Identifier.
///
/// The first character must satisfy `is_xid_start`, and all subsequent
/// characters must satisfy `is_xid_continue`. An empty string is invalid.
fn validate_alias_name(name: &str) -> Result<(), ResolveError> {
    let mut chars = name.chars();

    match chars.next() {
        Some(c) if unicode_ident::is_xid_start(c) => {},
        _ => return Err(ResolveError::InvalidAliasName(name.to_string())),
    }

    for c in chars {
        if !unicode_ident::is_xid_continue(c) {
            return Err(ResolveError::InvalidAliasName(name.to_string()));
        }
    }

    Ok(())
}
