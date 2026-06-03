+++
title = "Setting up a contributor environment"
description = "Configure your local Nix development environment, run tests, format code, and execute fuzz harnesses on the Axios codebase"
quadrant = "How-To"
audience = "Axios contributors and code reviewers"
+++

Set up your local environment and learn the standard workflow for contributing to the Axios stack.

## Prerequisites

Install the **Nix package manager** before continuing.

## Step 1: Enter the development shell

Axios uses Nix to pin its toolchain, libraries, and formatters.

1. Navigate to the root of your clone.
2. Enter the shell:
   ```bash
   nix-shell
   ```
   If you have `direnv`, run `direnv allow` instead; it loads the environment automatically.
3. Verify the toolchain:
   ```bash
   just --version
   treefmt --version
   ```

## Step 2: Format code

The project enforces formatting across Rust, Nix, TOML, Markdown, and JSON using `treefmt`.

1. Run from the repository root:
   ```bash
   treefmt
   ```
2. Check that modified files pass before committing.

## Step 3: Run tests

The `just` task runner orchestrates workspace operations.

1. Run unit and property tests across all workspaces:
   ```bash
   just test
   ```
2. All tests in `atom`, `eos`, `ion`, and `alurl` should compile and pass.

## Step 4: Lint

Run clippy and format checks before submitting:

1. Clippy:
   ```bash
   cargo clippy --manifest-path atom/Cargo.toml
   cargo clippy --manifest-path eos/Cargo.toml
   cargo clippy --manifest-path ion/Cargo.toml
   cargo clippy --manifest-path alurl/Cargo.toml
   ```
2. Format check (Rust 2024 edition, strict):
   ```bash
   cargo fmt --manifest-path atom/Cargo.toml -- --check
   cargo fmt --manifest-path eos/Cargo.toml -- --check
   cargo fmt --manifest-path ion/Cargo.toml -- --check
   ```

## Step 5: Fuzz

The project uses `cargo-bolero` for fuzz testing URI parsers, Coz signatures, lockfiles, and manifests.

1. Run all fuzzers (10 seconds each by default):
   ```bash
   just fuzz
   ```
2. Target a specific fuzzer:
   - URI parser: `just fuzz-uri`
   - Signature verification: `just fuzz-verification`
   - Raw lockfile parser: `just fuzz-lock-raw`
   - Structured lockfile serialization: `just fuzz-lock-structured`
   - Manifest TOML parser: `just fuzz-manifest`
3. Custom timing:
   ```bash
   just fuzz "-T 60s --profile release"
   ```

## Step 6: Build the documentation site

The doc site lives in `www/` and is built with `sukr`:

1. Process specs and ADRs:
   ```bash
   cd www && python3 process_docs.py
   ```
2. Build:
   ```bash
   sukr
   ```
3. Output goes to `www/public/`.

## Commit conventions

Use conventional commits. Imperative mood, subject line under 50 characters, body wrapped at 72.

- `feat:` — New user-visible functionality
- `fix:` — Bug fixes
- `refactor:` — Code restructuring, no behavior change (no changelog entry)
- `docs:` — Documentation-only changes
- Breaking changes: `change!:`, `remove!:`

Run `treefmt` before every commit.

## Where things are

- Specifications: `docs/specs/`
- Architecture decision records: `docs/adr/`
- Rust toolchain: pinned in `rust-toolchain.toml` (edition 2024)
- Formatting: `treefmt.toml` orchestrates `rustfmt`, `taplo`, `nixfmt`, `prettier`, `shfmt`
- Task runner: `just --list` for all available recipes
