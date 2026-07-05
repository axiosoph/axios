# Ion Workspace (L4 - Frontend) Reference Guide

The `ion` workspace contains the developer-facing command line interfaces, manifest schemas, and dependency resolvers for the Axios publishing stack.

> [!TIP]
> **Dynamic Discovery:** Manifest configurations and resolver options can evolve. Always check the live files, test specifications, and inspect Cargo schemas or schemas defined in manifests rather than hardcoding static structure assumptions.

## Architecture & Subcrates

Dependencies flow: `ion` (L3) -> `eos` (L2) -> `atom` (L1).
`ion` is the top layer. It interacts with the local system, parses manifests, solves dependencies, and invokes the L2 `eosd` daemon via RPC.

### Subcrates

- **[`ion-manifest`](ion-manifest)**: Parses user-defined manifests (`ion.toml`).
- **[`ion-resolve`](ion-resolve)**: Solves dependency graphs and computes lockfiles.
- **[`ion-eos`](ion-eos)**: Client interface to communicate with the L2 `eosd` daemon over Unix Domain Sockets using Cap'n Proto.
- **[`ion-cli`](ion-cli)**: Entry point CLI binary (`ion` executable).

## Key Design Principles for L3

1. **Manifest Rigidity:** Manifest parsing rules must strictly respect schema bounds. Never implement unverified schema deviations without explicit instruction.
2. **Client-Server Boundaries:** Delegate all actual build executions to the `eos` daemon via `ion-eos` RPC. Do not perform builds or sandboxing directly inside the L3 CLI.

## Specifications

This workspace is strictly spec-driven. Before starting work, you must inspect the contents of the specs directory (`../docs/specs/`) to find relevant files:

- **Contextual Relevance:** Review all specifications that are contextually relevant to the work at hand.
- **Cross-Cutting Concerns:** Evaluate cross-cutting concerns that may require reviewing specification files outside this immediate workspace (e.g., boundaries between layer interfaces).
- **No Ad-Hoc Decisions:** Do not make assumptions or ad-hoc design decisions if a specification is unclear, ambiguous, or missing details. Stop and surface unknowns so they can be explicitly discussed, resolved, and documented.

## Local Commands

All commands for L3 should be run from the `ion/` workspace directory:

- **Build/Check:** `cargo check` / `cargo build`
- **Test:** `cargo test`
- **Lint:** `cargo clippy --all-targets -- -D warnings`
- **Format:** `cargo fmt --check`
