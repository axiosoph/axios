# Eos Workspace (L2 - Runtime) Reference Guide

The `eos` workspace contains the execution engine, storage pipelines, and runtime components of the Axios publishing stack.

> [!TIP]
> **Dynamic Discovery:** File layouts, configuration defaults, and socket paths are subject to change. Always verify the live directory structure, configuration structures, and run dynamic checks (such as listing daemon CLI arguments or querying Cargo metadata) to verify exact targets.

## Architecture & Subcrates

Dependencies flow: `ion` (L3) -> `eos` (L2) -> `atom` (L1).
**Crates in `eos` must never import anything from `ion`.** They can only depend on L2 crates or L1 (`atom`) crates.

### Subcrates

- **[`eos-core`](eos-core)**: Domain traits and core types (`BuildEngine`, `ArtifactStore`, `AtomIndex`).
- **[`eos-store`](eos-store)**: Local file layout, storage backend ingestion, and content verification.
- **[`eos-proto`](eos-proto)**: Cap'n Proto RPC schema definitions (`eos.capnp`) and build-time code generation bindings.
- **[`eos-snix`](eos-snix)**: Concrete build engine and store implementation backed by the Snix suite.
- **[`eos-daemon`](eos-daemon)**: Hosts the `eosd` RPC daemon binary, wrapping evaluation logic inside containerized sandbox subprocesses.
- **[`eos`](eos)**: Orchestrator integrating the store, build engine, and scheduling primitives.

## Key Design Principles for L2

1. **Sandboxing and Hermeticity:** Evaluation executes Nix/Snix expressions which can perform arbitrary IO or sub-execution. Ensure the subprocess worker model is used with strict sandbox wrapping (`bwrap` on Linux, macOS Seatbelt/Birdcage) to isolate evaluations.
2. **Two-Tier Caching:**
   - _Evaluation Cache_: Maps inputs + arguments to build plans.
   - _Build Cache_: Maps build plans to artifacts.
3. **Cap'n Proto RPC Interface:** Exposed over Unix Domain Sockets. Ensure capability boundaries and reference types match schema definitions.

## Specifications

This workspace is strictly spec-driven. Before starting work, you must inspect the contents of the specs directory (`../docs/specs/`) to find relevant files:

- **Contextual Relevance:** Review all specifications that are contextually relevant to the work at hand.
- **Cross-Cutting Concerns:** Evaluate cross-cutting concerns that may require reviewing specification files outside this immediate workspace (e.g., boundaries between layer interfaces).
- **No Ad-Hoc Decisions:** Do not make assumptions or ad-hoc design decisions if a specification is unclear, ambiguous, or missing details. Stop and surface unknowns so they can be explicitly discussed, resolved, and documented.

## Local Commands

All commands for L2 should be run from the `eos/` workspace directory:

- **Build/Check:** `cargo check` / `cargo build`
- **Test:** `cargo test`
- **Lint:** `cargo clippy --all-targets -- -D warnings`
- **Format:** `cargo fmt --check`
- **Run Daemon:** `cargo run --bin eosd -- --socket-path /tmp/eos.sock`
