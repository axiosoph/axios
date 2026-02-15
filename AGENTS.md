# Axios Agent Configuration

This file defines rules and context for AI agents working in the `axios/`
monorepo. It is a **Predicate** that supplements the root
[AGENTS.md](../AGENTS.md).

## Predicate System

This project uses [predicate](https://github.com/nrdxp/predicate).

> [!IMPORTANT]
> You **must** also review [../.agent/PREDICATE.md](../.agent/PREDICATE.md)
> and follow its instructions before beginning work.

**Active Personas:**

- rust.md (Rust idioms and patterns)
- depmap.md (DepMap MCP server usage)
- personalization.md (User naming preferences)

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

### Cyphrpass Transition

The atom protocol will eventually migrate identity, signing, and storage to
[Cyphrpass](https://cyphrme.com/cyphrpass). Design **seams** (trait boundaries,
generic parameters) — not concrete Cyphrpass types. Key mapping:

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
- **Manual Commits:** Agents never execute `git commit`.
- **Terminology Compliance:** Use only canonical terms from the glossary above.

---

> [!TIP]
> Use `/predicate` if you lose track of these rules or if the conversation
> becomes too long.
