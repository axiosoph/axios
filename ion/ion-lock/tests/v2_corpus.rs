//! Lock v2 conformance corpus (N-lock-corpus, campaign/mvp-p0 c1a + c1a-canonical).
//!
//! Drives every fixture under `ion/ion-lock/corpus/v2/` through the
//! structural validator in `tests/support/mod.rs`: every golden MUST be
//! accepted, every violation MUST be rejected. Also demonstrates why the
//! raw forbidden-key check is load-bearing (typed `Deserialize` alone
//! silently accepts a `[lock-no-*]` violation), and byte-compares a
//! golden's digest fields against independently-computed expected forms
//! (c1a-canonical).

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::{ValidationError, validate};

fn corpus_dir(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("corpus/v2")
        .join(sub)
}

fn fixture(sub: &str, name: &str) -> String {
    let path = corpus_dir(sub).join(format!("{name}.toml"));
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn all_fixtures(sub: &str) -> Vec<(String, String)> {
    let dir = corpus_dir(sub);
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("reading dir {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().into_owned();
            let content = fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
            (name, content)
        })
        .collect()
}

/// c1a: the structural validator accepts every golden in the corpus.
#[test]
fn every_golden_is_accepted() {
    let goldens = all_fixtures("golden");
    assert!(
        !goldens.is_empty(),
        "corpus must contain at least one golden (Acceptance Criteria 1)"
    );
    for (name, content) in goldens {
        validate(&content).unwrap_or_else(|e| panic!("golden '{name}' was rejected: {e}"));
    }
}

/// c1a: the structural validator rejects every violation in the corpus.
#[test]
fn every_violation_is_rejected() {
    let violations = all_fixtures("violations");
    assert!(
        !violations.is_empty(),
        "corpus must contain violation fixtures (Acceptance Criteria 1)"
    );
    for (name, content) in violations {
        assert!(
            validate(&content).is_err(),
            "violation '{name}' was incorrectly accepted"
        );
    }
}

/// c1a: every `[lock-no-*]` deliberate-absence violation is caught
/// specifically by the raw forbidden-key check (p-violations).
#[test]
fn lock_no_star_violations_are_forbidden_key_errors() {
    const LOCK_NO_STAR: &[&str] = &[
        "lock-no-compose",
        "lock-no-params",
        "lock-no-toolchain-section",
        "lock-no-interfaces",
        "lock-no-override-state",
        "lock-no-foreign-metadata",
        "lock-no-plan-digest",
    ];
    for name in LOCK_NO_STAR {
        let content = fixture("violations", name);
        match validate(&content) {
            Err(ValidationError::ForbiddenKey(_)) => {},
            other => panic!("{name}: expected ForbiddenKey, got {other:?}"),
        }
    }
}

/// p-no-deny-unknown: `LockFileV2`'s `Deserialize` alone (bypassing the raw
/// forbidden-key check) silently accepts an injected `[compose]` section —
/// the exact defect the raw check exists to close. This is the "at least
/// one violation that typed-parse alone would accept" case c1a's evaluator
/// requires.
#[test]
fn typed_parse_alone_silently_accepts_a_lock_no_star_violation() {
    let content = fixture("violations", "lock-no-compose");
    let parsed: Result<ion_lock::LockFileV2, _> = toml::from_str(&content);
    assert!(
        parsed.is_ok(),
        "expected typed Deserialize alone to silently accept the injected [compose] section (no \
         #[serde(deny_unknown_fields)] on LockFileV2); got {parsed:?}"
    );
}

/// [lock-requires-resolvable]: a `requires` edge naming a nonexistent entry
/// is rejected, distinctly from a raw forbidden-key violation.
#[test]
fn dangling_requires_edge_is_rejected() {
    let content = fixture("violations", "dangling-requires");
    match validate(&content) {
        Err(ValidationError::DanglingRequires(_)) => {},
        other => panic!("expected DanglingRequires, got {other:?}"),
    }
}

/// [lock-requires-acyclic]: a 2-cycle in the requires graph is rejected.
#[test]
fn cyclic_requires_graph_is_rejected() {
    let content = fixture("violations", "cyclic-requires");
    match validate(&content) {
        Err(ValidationError::Cycle(_)) => {},
        other => panic!("expected Cycle, got {other:?}"),
    }
}

/// [lock-set-referenced]: a `[sets]` entry with no referencing dep entry is
/// rejected.
#[test]
fn unreferenced_set_is_rejected() {
    let content = fixture("violations", "unreferenced-set");
    match validate(&content) {
        Err(ValidationError::SetNotReferenced(_)) => {},
        other => panic!("expected SetNotReferenced, got {other:?}"),
    }
}

/// The byte sequence a hand-authored digest field encodes, matching the
/// generator used to hand-author the `golden/full.toml` fixture
/// (`(offset + i) mod 256`, independent of `AtomDigest`'s own code).
fn seq(offset: u8, n: usize) -> Vec<u8> {
    (0..n).map(|i| offset.wrapping_add(i as u8)).collect()
}

/// c1a-canonical: the golden's digest fields decode to their expected raw
/// bytes AND `Display` back to the exact expected `<token>:<encoding>`
/// literal, byte-for-byte — both directions of the round trip, not just
/// re-emitting whatever was read in.
#[test]
fn golden_digest_fields_are_byte_exact_canonical_form() {
    let content = fixture("golden", "full");
    let lock = validate(&content).expect("golden must validate");

    let core = &lock.sets["core"];
    assert_eq!(core.anchor.cad().as_bytes(), seq(0x00, 32));
    assert_eq!(
        core.anchor.to_string(),
        "sha256:AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8"
    );

    assert_eq!(core.charter_head.cad().as_bytes(), seq(0x10, 32));
    assert_eq!(
        core.charter_head.to_string(),
        "sha256:EBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8"
    );

    assert_eq!(core.snapshot.cad().as_bytes(), seq(0x80, 20));
    assert_eq!(
        core.snapshot.to_string(),
        "sha1:808182838485868788898a8b8c8d8e8f90919293"
    );

    let gcc = &lock.deps["core"]["gcc"];
    assert_eq!(gcc.publish.cad().as_bytes(), seq(0x20, 32));
    assert_eq!(
        gcc.publish.to_string(),
        "sha256:ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8"
    );

    let libfoo = &lock.deps["core"]["libfoo"];
    assert_eq!(libfoo.publish.cad().as_bytes(), seq(0x30, 32));
    assert_eq!(
        libfoo.publish.to_string(),
        "sha256:MDEyMzQ1Njc4OTo7PD0-P0BBQkNERUZHSElKS0xNTk8"
    );

    let openssl = &lock.deps["core"]["openssl"];
    assert_eq!(openssl.publish.cad().as_bytes(), seq(0x40, 32));
    assert_eq!(
        openssl.publish.to_string(),
        "sha256:QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8"
    );

    let zlib = &lock.deps["core"]["zlib-ng"];
    assert_eq!(zlib.publish.cad().as_bytes(), seq(0x50, 32));
    assert_eq!(
        zlib.publish.to_string(),
        "sha256:UFFSU1RVVldYWVpbXF1eX2BhYmNkZWZnaGlqa2xtbm8"
    );

    let fetch = &lock.fetch["libfoo-vendor-models"];
    assert_eq!(fetch.digest.cad().as_bytes(), seq(0xA0, 32));
    assert_eq!(
        fetch.digest.to_string(),
        "blake3:a0a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebf"
    );
}
