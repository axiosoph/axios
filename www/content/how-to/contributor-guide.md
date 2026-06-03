+++
title = "Setting Up a Contributor Environment"
description = "How to configure your local Nix-based development environment, run standard tests, format code, and execute Bolero fuzzers on the Axios codebase"
quadrant = "How-To"
audience = "Axios contributors, developers, and code reviewers setting up their local workspaces"
+++

This guide shows you how to set up your local environment and run workspace tasks to contribute to the Axios stack.

## Prerequisites

Before starting, install the **Nix package manager** on your local machine.

## Step 1: Enter the Development Shell

Axios uses Nix to manage its toolchain, libraries, and formatter packages. Enter the development shell to load the pinned Rust compiler version and other required packages into your path.

1. Navigate to the root directory of your cloned repository.
2. Open the shell by running:
   ```bash
   nix-shell
   ```
   _Alternative: If you have `direnv` installed and configured, run `direnv allow` to load the environment automatically._
3. Verify that the Rust toolchain and tools are active in your session:
   ```bash
   just --version
   treefmt --version
   ```

## Step 2: Format Code Across Workspaces

The Axios codebase enforces clean formatting across all file types (Rust, Nix, TOML, Markdown, and JSON) using `treefmt`.

1. Run `treefmt` from the root directory of the repository to format all files:
   ```bash
   treefmt
   ```
2. Verify that all modified files pass checks before preparing commits.

## Step 3: Run the Test Suite

Axios uses a task runner called `just` to orchestrate common workspace operations.

1. Execute the comprehensive unit and property test suites across all independent workspaces:
   ```bash
   just test
   ```
2. Confirm that all tests in the `atom`, `eos`, `ion`, and `alurl` workspaces compile and pass.

## Step 4: Run Bolero Fuzz Tests

Axios uses `cargo-bolero` to perform robust fuzz testing on raw URI parsers, Coz verification signatures, lockfiles, and manifests.

1. Run the entire fuzzer suite sequentially (defaults to 10 seconds of fuzzing per target):
   ```bash
   just fuzz
   ```
2. Alternatively, target a specific fuzzer directly if you are debugging a particular parser or trait implementation:
   - **URI Parser**: `just fuzz-uri`
   - **Signature Verification**: `just fuzz-verification`
   - **Raw Lockfile parser**: `just fuzz-lock-raw`
   - **Structured Lockfile serialization**: `just fuzz-lock-structured`
   - **Manifest TOML parser**: `just fuzz-manifest`
3. Pass custom timing or profiling configurations if you need deeper execution bounds:
   ```bash
   just fuzz "-T 60s --profile release"
   ```
