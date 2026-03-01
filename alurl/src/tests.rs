//! Tests covering all 19 normative spec constraints.
//!
//! Test vectors are derived from the resolution examples table in
//! `docs/specs/aliased-url-resolution.md`.

use super::*;

// ============================================================================
// Helpers
// ============================================================================

/// Build a test alias map from pairs.
fn aliases(pairs: &[(&str, &str)]) -> AliasMap {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

// ============================================================================
// [sigil-required]: + at host position → alias, else → raw
// ============================================================================

#[test]
fn sigil_present_bare() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh/owner/repo").unwrap();
    assert_eq!(result.url(), "github.com/owner/repo");
}

#[test]
fn sigil_absent_raw_url() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("https://example.com/foo").unwrap();
    assert_eq!(result, AliasedUrl::Raw("https://example.com/foo".into()));
}

#[test]
fn sigil_absent_raw_path() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("/tmp/local/repo").unwrap();
    assert_eq!(result, AliasedUrl::Raw("/tmp/local/repo".into()));
}

#[test]
fn sigil_absent_raw_scp() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("git@host:path").unwrap();
    assert_eq!(result, AliasedUrl::Raw("git@host:path".into()));
}

// ============================================================================
// [host-position-only]: + mid-path / in creds is NOT an alias
// ============================================================================

#[test]
fn plus_in_path_not_alias() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("https://example.com/+gh/foo").unwrap();
    assert_eq!(
        result,
        AliasedUrl::Raw("https://example.com/+gh/foo".into())
    );
}

#[test]
fn plus_in_credentials_not_alias() {
    let map = aliases(&[("myuser", "github.com")]);
    let result = map.resolve("https://+myuser:pass@github.com/repo").unwrap();
    assert_eq!(
        result,
        AliasedUrl::Raw("https://+myuser:pass@github.com/repo".into())
    );
}

// ============================================================================
// [alias-name-validated]: UAX #31 validation, InvalidAliasName error
// ============================================================================

#[test]
fn alias_name_digit_start_rejected() {
    let map = aliases(&[("123bad", "example.com")]);
    let result = map.resolve("+123bad/repo");
    assert!(matches!(result, Err(ResolveError::InvalidAliasName(_))));
}

#[test]
fn alias_name_empty_rejected() {
    let map = AliasMap::new();
    let result = map.resolve("+/foo");
    assert!(matches!(result, Err(ResolveError::InvalidAliasName(_))));
}

#[test]
fn alias_name_unicode_accepted() {
    let map = aliases(&[("café", "example.com")]);
    let result = map.resolve("+café/repo").unwrap();
    assert_eq!(result.url(), "example.com/repo");
}

// ============================================================================
// [separator-opaque-suffix]: / or : → separator, rest is opaque
// ============================================================================

#[test]
fn separator_slash_url_style() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh/owner/repo").unwrap();
    assert_eq!(result.url(), "github.com/owner/repo");
}

#[test]
fn separator_colon_scp_style() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh:owner/repo").unwrap();
    assert_eq!(result.url(), "github.com:owner/repo");
}

#[test]
fn separator_none_bare_alias() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh").unwrap();
    assert_eq!(result.url(), "github.com");
}

// ============================================================================
// [structure-preserving]: prefix + resolved + sep + suffix
// ============================================================================

#[test]
fn preserves_scheme_prefix() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("ssh://+gh/owner/repo").unwrap();
    assert_eq!(result.url(), "ssh://github.com/owner/repo");
}

#[test]
fn preserves_credentials_prefix() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("git@+gh:owner/repo").unwrap();
    assert_eq!(result.url(), "git@github.com:owner/repo");
}

#[test]
fn preserves_scheme_and_credentials() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("https://user:pass@+gh/repo").unwrap();
    assert_eq!(result.url(), "https://user:pass@github.com/repo");
}

#[test]
fn preserves_git_user_url_style() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("git@+gh/owner/repo").unwrap();
    assert_eq!(result.url(), "git@github.com/owner/repo");
}

// ============================================================================
// [suffix-opaque]: suffix passed through without modification
// ============================================================================

#[test]
fn suffix_with_port_pattern() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh:8080/owner/repo").unwrap();
    assert_eq!(result.url(), "github.com:8080/owner/repo");
}

#[test]
fn suffix_empty_trailing_slash() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh/").unwrap();
    assert_eq!(result.url(), "github.com/");
}

// ============================================================================
// [expansion-deterministic]: same input + AliasMap → same output
// ============================================================================

#[test]
fn deterministic_repeated_calls() {
    let map = aliases(&[("gh", "github.com")]);
    let r1 = map.resolve("+gh/repo").unwrap();
    let r2 = map.resolve("+gh/repo").unwrap();
    assert_eq!(r1, r2);
}

// ============================================================================
// [resolution-terminates]: cycle detection terminates all chains
// ============================================================================

#[test]
fn cycle_detected_two_aliases() {
    let map = aliases(&[("a", "+b"), ("b", "+a")]);
    let result = map.resolve("+a");
    match result {
        Err(ResolveError::CycleDetected { chain }) => {
            assert_eq!(chain.len(), 3); // ["a", "b", "a"]
            assert_eq!(chain.first(), chain.last());
        },
        other => panic!("expected CycleDetected, got {other:?}"),
    }
}

// ============================================================================
// [recursive-transparent]: stacked aliases resolve fully
// ============================================================================

#[test]
fn recursive_alias_chain() {
    let map = aliases(&[("work", "+gh/myorg"), ("gh", "github.com")]);
    let result = map.resolve("+work/myproject").unwrap();
    assert_eq!(
        result,
        AliasedUrl::Expanded {
            alias: "work".into(),
            url: "github.com/myorg/myproject".into(),
        }
    );
}

// ============================================================================
// [raw-preserves-input]: raw output equals input exactly
// ============================================================================

#[test]
fn raw_preserves_exact_input() {
    let map = AliasMap::new();
    let input = "https://example.com/path?q=1#frag";
    let result = map.resolve(input).unwrap();
    assert_eq!(result, AliasedUrl::Raw(input.into()));
}

// ============================================================================
// [expanded-preserves-alias]: original alias name preserved
// ============================================================================

#[test]
fn expanded_preserves_original_alias() {
    let map = aliases(&[("work", "+gh/myorg"), ("gh", "github.com")]);
    let result = map.resolve("+work/project").unwrap();
    match &result {
        AliasedUrl::Expanded { alias, .. } => assert_eq!(alias, "work"),
        other => panic!("expected Expanded, got {other:?}"),
    }
}

// ============================================================================
// [no-silent-fallback]: invalid alias → error, not Raw
// ============================================================================

#[test]
fn unknown_alias_errors() {
    let map = AliasMap::new();
    let result = map.resolve("+unknown/repo");
    assert!(matches!(result, Err(ResolveError::AliasNotFound(_))));
}

#[test]
fn invalid_name_errors_not_raw() {
    let map = AliasMap::new();
    let result = map.resolve("+123/repo");
    assert!(matches!(result, Err(ResolveError::InvalidAliasName(_))));
}

// ============================================================================
// [no-partial-expansion]: no + at host position in Expanded.url
// ============================================================================

#[test]
fn no_partial_expansion() {
    let map = aliases(&[("a", "+b"), ("b", "final.com")]);
    let result = map.resolve("+a/path").unwrap();
    assert!(!result.url().starts_with('+'));
}

// ============================================================================
// [no-scheme-injection]: bare alias output has no scheme
// ============================================================================

#[test]
fn bare_alias_no_scheme() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh/owner/repo").unwrap();
    assert!(!result.url().contains("://"));
}

// ============================================================================
// [error-diagnostic]: error types carry diagnostic info
// ============================================================================

#[test]
fn error_not_found_includes_name() {
    let map = AliasMap::new();
    match map.resolve("+missing/repo") {
        Err(ResolveError::AliasNotFound(name)) => assert_eq!(name, "missing"),
        other => panic!("expected AliasNotFound, got {other:?}"),
    }
}

#[test]
fn error_cycle_includes_chain() {
    let map = aliases(&[("x", "+y"), ("y", "+x")]);
    match map.resolve("+x") {
        Err(ResolveError::CycleDetected { chain }) => {
            assert!(chain.len() >= 3);
            assert!(chain.contains(&"x".to_string()));
            assert!(chain.contains(&"y".to_string()));
        },
        other => panic!("expected CycleDetected, got {other:?}"),
    }
}

// ============================================================================
// Spec resolution examples table (integration)
// ============================================================================

#[test]
fn spec_examples_table() {
    let map = aliases(&[
        ("gh", "github.com"),
        ("work", "+gh/myorg"),
        ("a", "+b"),
        ("b", "+a"),
    ]);

    // Expanded cases
    assert_eq!(
        map.resolve("+gh/owner/repo").unwrap().url(),
        "github.com/owner/repo"
    );
    assert_eq!(map.resolve("+gh").unwrap().url(), "github.com");
    assert_eq!(
        map.resolve("+gh:owner/repo").unwrap().url(),
        "github.com:owner/repo"
    );
    assert_eq!(
        map.resolve("ssh://+gh/owner/repo").unwrap().url(),
        "ssh://github.com/owner/repo"
    );
    assert_eq!(
        map.resolve("git@+gh:owner/repo").unwrap().url(),
        "git@github.com:owner/repo"
    );
    assert_eq!(
        map.resolve("git@+gh/owner/repo").unwrap().url(),
        "git@github.com/owner/repo"
    );
    assert_eq!(
        map.resolve("https://user:pass@+gh/repo").unwrap().url(),
        "https://user:pass@github.com/repo"
    );
    assert_eq!(
        map.resolve("+gh:8080/owner/repo").unwrap().url(),
        "github.com:8080/owner/repo"
    );
    assert_eq!(
        map.resolve("+work/myproject").unwrap().url(),
        "github.com/myorg/myproject"
    );

    // Error cases
    assert!(matches!(
        map.resolve("+a"),
        Err(ResolveError::CycleDetected { .. })
    ));
    assert!(matches!(
        map.resolve("+unknown/repo"),
        Err(ResolveError::AliasNotFound(_))
    ));

    // Raw cases
    assert!(matches!(
        map.resolve("https://example.com/foo").unwrap(),
        AliasedUrl::Raw(_)
    ));
    assert!(matches!(
        map.resolve("/tmp/local/repo").unwrap(),
        AliasedUrl::Raw(_)
    ));
    assert!(matches!(
        map.resolve("git@host:path").unwrap(),
        AliasedUrl::Raw(_)
    ));
}

// ============================================================================
// Edge cases: alias name character validation
// ============================================================================

#[test]
fn alias_name_hyphen_rejected() {
    let map = aliases(&[("my-alias", "example.com")]);
    let result = map.resolve("+my-alias/repo");
    // Hyphen is not XID_Continue — validate_alias_name rejects "my-alias".
    assert!(matches!(result, Err(ResolveError::InvalidAliasName(_))));
}

#[test]
fn alias_name_dot_rejected() {
    let map = aliases(&[("my.alias", "example.com")]);
    // Dot is not XID_Continue — validate_alias_name rejects "my.alias".
    let result = map.resolve("+my.alias/repo");
    assert!(matches!(result, Err(ResolveError::InvalidAliasName(_))));
}

#[test]
fn alias_name_underscore_accepted() {
    let map = aliases(&[("my_alias", "example.com")]);
    let result = map.resolve("+my_alias/repo").unwrap();
    assert_eq!(result.url(), "example.com/repo");
}

// ============================================================================
// Edge cases: scheme handling
// ============================================================================

#[test]
fn scheme_with_plus_in_name() {
    // svn+ssh:// has '+' in the scheme — must not be an alias sigil.
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("svn+ssh://+gh/owner/repo").unwrap();
    assert_eq!(result.url(), "svn+ssh://github.com/owner/repo");
}

#[test]
fn file_scheme_no_alias() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("file:///path/to/repo").unwrap();
    assert_eq!(result, AliasedUrl::Raw("file:///path/to/repo".into()));
}

#[test]
fn scheme_url_with_port_and_alias() {
    // https://+gh:8080/repo — scheme present, so authority boundary is '/'.
    // Authority = "+gh:8080", last '@' = none, host_pos = after scheme.
    // '+' at host position → alias "gh", separator ':', suffix "8080/repo".
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("https://+gh:8080/repo").unwrap();
    assert_eq!(result.url(), "https://github.com:8080/repo");
}

#[test]
fn ssh_scheme_with_credentials_and_alias() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("ssh://git@+gh/owner/repo").unwrap();
    assert_eq!(result.url(), "ssh://git@github.com/owner/repo");
}

#[test]
fn ssh_scheme_with_scp_style_colon_separator() {
    // Scheme present + credentials + ':' as separator (not port, not credential colon).
    // Authority boundary = first '/' (scheme-present rule), so '@' is found correctly.
    // Alias "gh" terminated by ':', suffix = "owner/repo".
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("ssh://git@+gh:owner/repo").unwrap();
    assert_eq!(result.url(), "ssh://git@github.com:owner/repo");
}

// ============================================================================
// Edge cases: @ handling
// ============================================================================

#[test]
fn multiple_at_signs_last_wins() {
    // user@proxy@+gh/repo — last '@' is before '+gh'.
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("user@proxy@+gh/repo").unwrap();
    assert_eq!(result.url(), "user@proxy@github.com/repo");
}

// ============================================================================
// Edge cases: boundary inputs
// ============================================================================

#[test]
fn empty_input_is_raw() {
    let map = AliasMap::new();
    let result = map.resolve("").unwrap();
    assert_eq!(result, AliasedUrl::Raw(String::new()));
}

#[test]
fn just_plus_alone_is_invalid() {
    let map = AliasMap::new();
    let result = map.resolve("+");
    assert!(matches!(result, Err(ResolveError::InvalidAliasName(_))));
}

#[test]
fn colon_only_suffix_empty_rest() {
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("+gh:").unwrap();
    assert_eq!(result.url(), "github.com:");
}

#[test]
fn plus_in_url_path_not_host_position() {
    // + appears after the first '/' — not at host position.
    let map = aliases(&[("gh", "github.com")]);
    let result = map.resolve("example.com/+gh/foo").unwrap();
    assert_eq!(result, AliasedUrl::Raw("example.com/+gh/foo".into()));
}

// ============================================================================
// Edge cases: recursion
// ============================================================================

#[test]
fn three_level_recursive_chain() {
    let map = aliases(&[
        ("team", "+org/myteam"),
        ("org", "+gh/myorg"),
        ("gh", "github.com"),
    ]);
    let result = map.resolve("+team/project").unwrap();
    assert_eq!(result.url(), "github.com/myorg/myteam/project");
    match &result {
        AliasedUrl::Expanded { alias, .. } => assert_eq!(alias, "team"),
        other => panic!("expected Expanded, got {other:?}"),
    }
}

#[test]
fn self_referencing_alias_detected() {
    let map = aliases(&[("loop", "+loop")]);
    match map.resolve("+loop") {
        Err(ResolveError::CycleDetected { chain }) => {
            assert_eq!(chain, vec!["loop", "loop"]);
        },
        other => panic!("expected CycleDetected, got {other:?}"),
    }
}
