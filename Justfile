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

# Run all fuzzers sequentially (defaults to 10 seconds each)
fuzz args="-max_total_time=10":
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

# Run the raw URI parser fuzzer in the atom layer
fuzz-uri args="":
    cd atom && cargo fuzz run uri_parser -- {{args}}

# Run the coz verification signature fuzzer in the atom layer
fuzz-verification args="":
    cd atom && cargo fuzz run coz_verification -- {{args}}

# Run the raw lock file TOML fuzzer in the eos layer
fuzz-lock-raw args="":
    cd eos && cargo fuzz run lock_parser_raw -- {{args}}

# Run the structured lock file fuzzer in the eos layer
fuzz-lock-structured args="":
    cd eos && cargo fuzz run lock_parser_structured -- {{args}}

# Run the manifest TOML fuzzer in the ion layer
fuzz-manifest args="":
    cd ion && cargo fuzz run manifest_parser -- {{args}}
