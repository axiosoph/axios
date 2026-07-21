# Atom Workspace (L1 - Protocol) Reference Guide

The `atom` workspace contains the foundational crates of the Axios content-addressed source publishing stack.

> [!TIP]
> **Dynamic Discovery:** Do not rely solely on static documentation for directories or file paths. Always inspect the live filesystem structure (e.g., via `list_dir` or `tree`) and query `cargo metadata` or the workspace `Cargo.toml` to get the definitive set of member crates and dependencies.

## Architecture & Subcrates

Dependencies in the Axios stack flow strictly downward: `ion` (L4) -> `eos` (L3) -> `atom` (L1).
Therefore, **crates in `atom` must never import anything from `eos` or `ion`.**

> **Identity model:** An atom's identity is the czd of its claim record —
> the signed record that binds a label to an anchor, exactly once. The
> label is a declared field of the claim; the anchor is never carried as
> data — it is discovered by walking the record log back to its charter
> genesis. The two never act as a fused unit; only for an unclaimed dev
> atom does identity degenerate to a digest computed from the two
> together. Lock entries are `(set, label) → {version, publish_czd}`. See
> [ADR-0007](../docs/adr/0007-atom-version-integrity-system.md) (§3
> genesis-once; §4 anchor vs. position) and the
> [FAQ](../www/content/explanation/faq.md) (Part I) for the full model.

### Subcrates

- **[`atom-id`](atom-id)**: Identity primitives, including labels, digests, and verified names.
- **[`atom-uri`](atom-uri)**: Parsing and construction of content-addressed Atom URIs.
- **[`atom-core`](atom-core)**: Core protocol traits, primarily `AtomSource` and `AtomRegistry`.
- **[`atom-git`](atom-git)**: Git bridge implementing the protocol traits over Git references.

## Key Design Principles for L1

1. **Strict Dependency Budget:** Protocol crates like `atom-id` and `atom-core` target <= 5 non-standard dependencies. Be extremely selective when adding external crates. Bridge/implementation crates (such as `atom-git`) do not have this strict constraint.
2. **Design Seams for Cyphr Transition:** The protocol will eventually migrate from BLAKE3/Git-tag identity and signature primitives to Cyphr. Define interfaces using generic parameters or traits rather than concrete types to allow for clean seams:
   - Use abstract traits like `Digest` rather than hardcoding BLAKE3/SHA256 digests in core logic.
   - Use trait bounds for mapping Git tags to general transactions.
3. **Rust Edition:** Uses Rust Edition 2024. Use modern idioms where appropriate.

## Specifications

This workspace is strictly spec-driven. Before starting work, you must inspect the contents of the specs directory (`../docs/specs/`) to find relevant files:

- **Contextual Relevance:** Review all specifications that are contextually relevant to the work at hand.
- **Cross-Cutting Concerns:** Evaluate cross-cutting concerns that may require reviewing specification files outside this immediate workspace (e.g., boundaries between layer interfaces).
- **No Ad-Hoc Decisions:** Do not make assumptions or ad-hoc design decisions if a specification is unclear, ambiguous, or missing details. Stop and surface unknowns so they can be explicitly discussed, resolved, and documented.

## Local Commands

All commands for L1 should be run from the `atom/` workspace directory:

- **Build/Check:** `cargo check` / `cargo build`
- **Test:** `cargo test`
- **Lint:** `cargo clippy --all-targets -- -D warnings`
- **Format:** `cargo fmt --check`
