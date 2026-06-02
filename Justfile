# Default recipe: list available recipes
default:
    @just --list

# Run all unit and property tests across all workspaces
test:
    @echo "Running tests in 'atom' workspace..."
    cargo test --manifest-path atom/Cargo.toml
    @echo "Running tests in 'eos' workspace..."
    cargo test --manifest-path eos/Cargo.toml
    @echo "Running tests in 'ion' workspace..."
    cargo test --manifest-path ion/Cargo.toml

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
    cargo bolero test --manifest-path atom/Cargo.toml -p atom-uri tests::proptests::test_raw_atom_uri_roundtrip_bolero {{args}}

# Run the coz verification signature fuzzer via Bolero
fuzz-verification args="-T 10s --profile release":
    cargo bolero test --manifest-path atom/Cargo.toml -p atom-id tests::test_verify_robustness_bolero {{args}}

# Run the raw lock file TOML fuzzer via Bolero
fuzz-lock-raw args="-T 10s --profile release":
    cargo bolero test --manifest-path eos/Cargo.toml -p eos lock::tests::test_lock_file_parse_raw_no_panic {{args}}

# Run the structured lock file fuzzer via Bolero
fuzz-lock-structured args="-T 10s --profile release":
    cargo bolero test --manifest-path eos/Cargo.toml -p eos lock::tests::test_lock_file_roundtrip {{args}}

# Run the manifest TOML fuzzer via Bolero
fuzz-manifest args="-T 10s --profile release":
    cargo bolero test --manifest-path ion/Cargo.toml -p ion-manifest proptests::ion_manifest_parse_no_panic {{args}}
