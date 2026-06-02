# Axios Agent Configuration

This file defines rules, constraints, and architecture context for AI agents working in the `axios/` monorepo.

## Tooling and Agent Skills

This workspace relies on globally installed agent skills and plugins rather than custom, project-specific frameworks.

> [!IMPORTANT]
> **Heed Installed Agent Skills:** Review available globally installed agent skills and tools for task-specific and language-specific guidance before starting work:
>
> - **Rust Development:** Look for and adhere to installed Rust coding/linting agent skills for Rust-specific idioms and patterns.
> - **Git Workflows & Commits:** Check for workflow-specific agent skills. Automated commits are permitted and encouraged _only if_ they strictly adhere to the instructions of an installed commit hygiene agent skill. If no such skill is available, do not perform automated commits.
> - Balance global tooling with project constraints: Heed installed agent skills for general workflows and language conventions, but always prioritize this monorepo's architectural layers, terminology constraints, and local invariants. Do not ignore your tooling, but ensure its application aligns with the project context.

---

## Spec-Driven Development

This project is strictly spec-driven. Agents must regularly consult the specification documents inside the [docs/specs/](docs/specs) directory to ensure design correctness.

- **Review Specifications First:** Before implementing features or making modifications, locate and read the corresponding spec file (e.g. L1/Atom specs, L2/Eos specs, or L3/Ion specs).
- **No Ad-Hoc Decisions:** Do not make assumptions or ad-hoc design decisions if a specification is unclear, ambiguous, or missing details.
- **Surface Unknowns:** If you encounter a gap or ambiguity in the specifications, halt and surface the unknown immediately so that it can be explicitly discussed, resolved, and documented.

---

## Project Overview

Axios is a decentralized, content-addressed source publishing stack decomposed
into three independent Cargo workspaces mapped to a layered architecture:

```
L3  ion    ── CLI, manifests, resolution (user-facing)
L2  eos    ── build engine, stores, runtime (evaluation)
L1  atom   ── protocol, identity, addressing (foundation)
```

Dependencies flow strictly downward: ion → eos → atom. Each workspace is an
independent Cargo workspace with path-based inter-workspace deps.

For architecture details, see:

- [ADR-0001](docs/adr/0001-monorepo-workspace-architecture.md)
- [Charter](docs/charters/decentralized-publishing-stack.md)
- [Formal Model](docs/models/publishing-stack-layers.md)

---

## Terminology Glossary

> [!CAUTION]
> Use **only** the canonical terms below. Legacy terms from the original eka
> codebase must **never** appear in new code, documentation, or conversation.
> If you catch yourself using a deprecated term, stop and correct it.

| Canonical Term | Definition                                                                         | Deprecated Aliases (NEVER use) |
| :------------- | :--------------------------------------------------------------------------------- | :----------------------------- |
| **Anchor**     | Cryptographic commitment (e.g. genesis commit hash) establishing atom-set identity | genesis, root, Root            |
| **Atom-id**    | Content-addressed digest: `digest(anchor, label)`. Globally unique.                | AtomId, atom_id                |
| **Atom-set**   | Collection of atoms sharing a common anchor (a single repository)                  | (none)                         |
| **Label**      | Human-readable name for an atom within an atom-set                                 | Name                           |
| **Digest**     | Abstract content-addressed hash. Algorithm is not hardcoded.                       | AtomDigest, Blake3Hash         |
| **Plan**       | Engine-specific build recipe (`BuildEngine::Plan` associated type)                 | derivation, drv                |
| **Output**     | Engine-specific build result (`BuildEngine::Output` associated type)               | build result                   |
| **Artifact**   | Content-addressed blob in an artifact store                                        | (none)                         |
| **Revision**   | A specific commit in source history                                                | (none)                         |

### Naming Conventions in Code

- Rust types use `PascalCase`: `AtomId`, `AtomDigest`, `AtomSet`
- Fields and variables use `snake_case`: `atom_id`, `anchor`, `label`
- The glossary governs **concept names** — code identifiers follow Rust convention
- When generics are needed, prefer descriptive bounds: `D: Digest`, not `H` or `T`

### Cyphr Transition

The atom protocol will eventually migrate identity, signing, and storage to
[Cyphr](https://cyphrme.com/cyphr). Design **seams** (trait boundaries,
generic parameters) — not concrete Cyphr types. Key mapping:

| Current (atom-git) | Future (atom-cyphr) | Migration Surface        |
| :----------------- | :------------------ | :----------------------- |
| BLAKE3 digest      | czd (Coz Digest)    | `Digest` trait impls     |
| Bare anchor hash   | Principal Root (PR) | `Anchor` trait/type      |
| Git tag metadata   | Coz transactions    | `atom-core` trait bounds |

---

## Build & Commands

Rust edition 2024, toolchain pinned in `../rust-toolchain.toml` (1.90.0).

Each workspace is independent — run commands from the workspace root:

| Task   | Command                                      |
| :----- | :------------------------------------------- |
| Check  | `cargo check` (from `atom/`, `eos/`, `ion/`) |
| Test   | `cargo test` (from workspace root)           |
| Format | `cargo fmt` (from workspace root)            |
| Lint   | `cargo clippy` (from workspace root)         |

---

## Workspace Crates

> [!TIP]
> **Dynamic Discovery:** Crates and dependency layouts evolve. Rather than relying solely on this static list, always use dynamic discovery tools (e.g., query `cargo metadata` or inspect the root `Cargo.toml`) to determine the live set of active crates and dependencies.

### atom/ (L1 — Protocol)

| Crate       | Purpose                                              |
| :---------- | :--------------------------------------------------- |
| `atom-id`   | Identity primitives: labels, digests, verified names |
| `atom-uri`  | Atom URI parsing and construction                    |
| `atom-core` | Protocol traits: `AtomSource`, `AtomRegistry`        |
| `atom-git`  | Git bridge: legacy storage backend                   |

### eos/ (L2 — Runtime)

| Crate       | Purpose                                       |
| :---------- | :-------------------------------------------- |
| `eos-core`  | Engine traits: `BuildEngine`, `ArtifactStore` |
| `eos-store` | Store implementations and ingest pipeline     |
| `eos`       | Orchestration: wires engine + store           |

### ion/ (L3 — Frontend)

| Crate          | Purpose                                        |
| :------------- | :--------------------------------------------- |
| `ion-manifest` | `ion.toml` manifest parsing                    |
| `ion-resolve`  | Dependency resolution (SAT solver, lock files) |
| `ion-cli`      | CLI binary                                     |

---

## Architecture Principles

1. **Design seams, not implementations.** Trait boundaries absorb future
   change (Cyphr, remote engines, new ecosystems).
2. **Abstract by default.** Digest algorithms, anchor types, and storage
   backends are generic. Concrete types live in bridge crates (atom-git).
3. **Dependency budget.** Protocol crates (atom-id, atom-core) target
   ≤ 5 non-std dependencies. Bridge crates have no such limit.
4. **Layer discipline.** L2 never imports L3. L1 never imports L2.
   Violations are architectural bugs.
5. **Cache-skipping is the value proposition.** Every stage of the
   cryptographic chain must be independently verifiable and skippable.

---

## Invariants

- **No Schema Changes:** The C.O.R.E. YAML grammar is rigid.
- **Halt on Ambiguity:** Never rationalize an assumption. Stop and ask.
- **Verification Required:** Every plan step must be verified.
- **Commit Boundaries:** Pause and justify before every commit point.
- **Commit Hygiene:** Automated commits are permitted _only_ when adhering precisely to the instructions of an installed commit hygiene agent skill. If no such skill is installed or available in your toolset, automated commits are strictly prohibited and you must default to manual commits or consult the user.
- **Terminology Compliance:** Use only canonical terms from the glossary above.

---

> [!TIP]
> Refer to the workspace-specific `AGENTS.md` files in the subdirectories (e.g. `atom/AGENTS.md`, `eos/AGENTS.md`, `ion/AGENTS.md`) for more targeted context on each layer.
