# Default recipe: list available recipes
default:
    @just --list

# Build the documentation site and generate the search index
doc:
    python3 www/process_docs.py
    sukr -c www/site.toml
    pagefind --site www/public

# Run all unit and property tests across all workspaces
test:
    @echo "Running tests in 'atom' workspace..."
    cargo test --manifest-path atom/Cargo.toml
    @echo "Running tests in 'eos' workspace..."
    cargo test --manifest-path eos/Cargo.toml
    @echo "Running tests in 'htc' workspace..."
    cargo test --manifest-path htc/Cargo.toml
    @echo "Running tests in 'ion' workspace..."
    cargo test --manifest-path ion/Cargo.toml
    @echo "Running tests in 'alurl' crate..."
    cargo test --manifest-path alurl/Cargo.toml

# Run the doctrine-trap lints (self-test, then scan the tree)
lint:
    @echo "Self-testing the doctrine lints..."
    python3 tools/lints/doctrine_lint.py --self-test
    @echo "Scanning the tree for doctrine-trap violations..."
    python3 tools/lints/doctrine_lint.py

# Composes lints, the compliance tracker (regenerates the on-path manifest),
# and the coverage check around the full test target. Exits non-zero on any
# failure; the coverage step reflects tracker/rekey-owned annotation coverage.
# Single-entry CI gate: lints + all workspace tests + spec-compliance coverage
gate:
    just lint
    just test
    @echo "Regenerating the compliance manifest..."
    python3 docs/compliance_tracker.py
    @echo "Checking on-path constraint coverage..."
    python3 docs/check_constraint_coverage.py

# Run clippy with the CI warning gate across the four workspaces
clippy:
    @echo "Running clippy in 'atom' workspace..."
    cargo clippy --manifest-path atom/Cargo.toml --all-targets -- -D warnings
    @echo "Running clippy in 'eos' workspace..."
    cargo clippy --manifest-path eos/Cargo.toml --all-targets -- -D warnings
    @echo "Running clippy in 'htc' workspace..."
    cargo clippy --manifest-path htc/Cargo.toml --all-targets -- -D warnings
    @echo "Running clippy in 'ion' workspace..."
    cargo clippy --manifest-path ion/Cargo.toml --all-targets -- -D warnings

# cargo fmt --check does not resolve targets via --manifest-path, so each
# check below runs from inside its workspace directory.
# Check formatting across the four workspaces
fmt-check:
    @echo "Checking format in 'atom' workspace..."
    cd atom && cargo fmt --check
    @echo "Checking format in 'eos' workspace..."
    cd eos && cargo fmt --check
    @echo "Checking format in 'htc' workspace..."
    cd htc && cargo fmt --check
    @echo "Checking format in 'ion' workspace..."
    cd ion && cargo fmt --check

# --offline restricts lychee to local files and blocks network requests, so
# external URLs are out of scope; only relative link targets are checked.
# Audit relative-path link targets in docs/, README.md, and ROADMAP.md
link-audit:
    nix run nixpkgs#lychee -- --offline --no-progress docs README.md ROADMAP.md

# The TLA+/Alloy model check is a separate manually-dispatched job
# (docs/specs/run_model_check.sh) and is intentionally not part of this gate.
# Single-entry local reproduction of the push/PR CI gate set
ci:
    just lint
    just test
    just clippy
    just fmt-check
    just link-audit

# Run all Bolero fuzzers sequentially (defaults to 10 seconds each)
fuzz args="-T 10s --profile release":
    @echo "Running fuzz-uri with {{args}}..."
    just fuzz-uri "{{args}}"
    @echo "Running fuzz-verification with {{args}}..."
    just fuzz-verification "{{args}}"
    @echo "Running fuzz-lock-raw with {{args}}..."
    just fuzz-lock-raw "{{args}}"
    @echo "Running fuzz-lock-structured with {{args}}..."
    just fuzz-lock-structured "{{args}}"
    @echo "Running fuzz-manifest with {{args}}..."
    just fuzz-manifest "{{args}}"

# Run the raw URI parser fuzzer via Bolero
fuzz-uri args="-T 10s --profile release":
    cargo bolero test --manifest-path atom/Cargo.toml -p atom-uri --corpus-dir atom/atom-uri/fuzz/corpus/test_raw_atom_uri_roundtrip_bolero tests::proptests::test_raw_atom_uri_roundtrip_bolero {{args}}

# Run the coz verification signature fuzzer via Bolero
fuzz-verification args="-T 10s --profile release":
    cargo bolero test --manifest-path atom/Cargo.toml -p atom-id --corpus-dir atom/atom-id/fuzz/corpus/test_verify_robustness_bolero tests::test_verify_robustness_bolero {{args}}

# Run the raw lock file TOML fuzzer via Bolero
fuzz-lock-raw args="-T 10s --profile release":
    cargo bolero test --manifest-path ion/Cargo.toml -p ion-lock --corpus-dir ion/ion-lock/fuzz/corpus/test_lock_file_parse_raw_no_panic tests::test_lock_file_parse_raw_no_panic {{args}}

# Run the structured lock file fuzzer via Bolero
fuzz-lock-structured args="-T 10s --profile release":
    cargo bolero test --manifest-path ion/Cargo.toml -p ion-lock --corpus-dir ion/ion-lock/fuzz/corpus/test_lock_file_roundtrip tests::test_lock_file_roundtrip {{args}}

# Run the manifest TOML fuzzer via Bolero
fuzz-manifest args="-T 10s --profile release":
    cargo bolero test --manifest-path ion/Cargo.toml -p ion-manifest --corpus-dir ion/ion-manifest/fuzz/corpus/ion_manifest_parse_no_panic proptests::ion_manifest_parse_no_panic {{args}}
