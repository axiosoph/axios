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
    @echo "Running tests in 'ion' workspace..."
    cargo test --manifest-path ion/Cargo.toml
    @echo "Running tests in 'alurl' crate..."
    cargo test --manifest-path alurl/Cargo.toml


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
