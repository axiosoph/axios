# Eos Workspace (L3 - Runtime) Reference Guide

The `eos` workspace contains the execution engine, storage pipelines, and runtime components of the Axios publishing stack.

> [!TIP]
> **Dynamic Discovery:** File layouts, configuration defaults, and socket paths are subject to change. Always verify the live directory structure, configuration structures, and run dynamic checks (such as listing daemon CLI arguments or querying Cargo metadata) to verify exact targets.

## Architecture & Subcrates

Dependencies flow: `ion` (L4) -> `eos` (L3) -> `atom` (L1).
**Crates in `eos` must never import anything from `ion`.** They can only depend on L3 crates or L1 (`atom`) crates.

### Subcrates

- **[`eos-core`](eos-core)**: Domain traits and core types (`BuildEngine`, `ArtifactStore`, `AtomIndex`).
- **[`eos-proto`](eos-proto)**: Cap'n Proto RPC schema definitions (`eos.capnp`) and build-time code generation bindings.
- **[`eos-snix`](eos-snix)**: Slated for removal — the passthrough executor it embodied was removed by [ADR-0006](../docs/adr/0006-execution-as-the-primitive.md) §3 (evaluator eradicated). Do not build on it.
- **[`eos-daemon`](eos-daemon)**: Hosts the `eosd` RPC daemon binary: the scheduler, its executor worker pool, and the RPC server that dispatches build actions to executor workers inside containerized sandboxes.
- **[`eos`](eos)**: Orchestrator integrating the store, build engine, and scheduling primitives.

## Key Design Principles for L3

1. **Sandboxing and Hermeticity:** The build function executes an unmodified upstream build process inside a materialized FHS view (a composition mounted via composefs). The sandbox is deny-by-default: the only bytes a build process can read are those materialized from the declared atom closure and toolchain composition, plus whatever the fetch proxy explicitly permits. The build's observed read set is checked against the declared closure — reads ⊆ declared — and that containment is _enforced by the sandbox_, not trusted from the build's own behavior (`htc-sad.md` §1.1 `[htc-declared-closure-enforced]`).
2. **Single-Tier Action-Id Cache:** There is no separate pre-build stage and no associated cache key ahead of it. `action_id = H(atom_czd_closure_root, toolchain_composition_root, action_params)` is the sole cache key: a matching `action_id` skips dispatch entirely and returns the cached output tree (`htc-sad.md` §6.5, ADR-0005 §2 `[htc-action-identity]`).
3. **Cap'n Proto RPC Interface:** Exposed over Unix Domain Sockets. Ensure capability boundaries and reference types match schema definitions.

## Specifications

This workspace is strictly spec-driven. Before starting work, you must inspect the contents of the specs directory (`../docs/specs/`) to find relevant files:

- **Contextual Relevance:** Review all specifications that are contextually relevant to the work at hand.
- **Cross-Cutting Concerns:** Evaluate cross-cutting concerns that may require reviewing specification files outside this immediate workspace (e.g., boundaries between layer interfaces).
- **No Ad-Hoc Decisions:** Do not make assumptions or ad-hoc design decisions if a specification is unclear, ambiguous, or missing details. Stop and surface unknowns so they can be explicitly discussed, resolved, and documented.

## Local Commands

All commands for L3 should be run from the `eos/` workspace directory:

- **Build/Check:** `cargo check` / `cargo build`
- **Test:** `cargo test`
- **Lint:** `cargo clippy --all-targets -- -D warnings`
- **Format:** `cargo fmt --check`
- **Run Daemon:** `cargo run --bin eosd -- --socket-path /tmp/eos.sock`
