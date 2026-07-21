# Axios Agent Configuration

This file defines rules, constraints, and architecture context for AI agents working in the `axios/` monorepo.

## Tooling and Agent Skills

This workspace relies on globally installed agent skills and plugins rather than custom, project-specific frameworks.

> [!IMPORTANT]
> **Heed Installed Agent Skills & Environments:** Review available globally installed agent skills, tools, and local development environments before starting work:
>
> - **Rust Development:** Look for and adhere to installed Rust coding/linting agent skills for Rust-specific idioms and patterns.
> - **Nix Tooling:** Check if the `IN_NIX_SHELL` environment variable is set. If it is set, assume all development environment tools are already available and do not redundantly invoke `nix-shell`.
> - **Git Workflows & Commits:** Check for workflow-specific agent skills. Automated commits are permitted and encouraged _only if_ they strictly adhere to the instructions of an installed commit hygiene agent skill. If no such skill is available, do not perform automated commits.
> - **Formatting & treefmt:** Before any commit, format the workspace using `treefmt` to keep the codebase clean intra-commit.
> - Balance global tooling with project constraints: Heed installed agent skills for general workflows and language conventions, but always prioritize this monorepo's architectural layers, terminology constraints, and local invariants. Do not ignore your tooling, but ensure its application aligns with the project context.

---

## Spec-Driven Development

This project is strictly spec-driven. Agents must regularly consult the specification documents inside the [docs/specs/](docs/specs) directory to ensure design correctness.

- **Review Specifications First:** Before implementing features or making modifications, locate and read the corresponding spec file (e.g. L1/Atom specs, L3/Eos specs, or L4/Ion specs).
- **No Ad-Hoc Decisions:** Do not make assumptions or ad-hoc design decisions if a specification is unclear, ambiguous, or missing details.
- **Surface Unknowns:** If you encounter a gap or ambiguity in the specifications, halt and surface the unknown immediately so that it can be explicitly discussed, resolved, and documented.
- **Ground-Truth Direction (L2/HTC):** The composition substrate has a landed
  ADR and SAD but no spec yet (spec authorship is P3/P4 work). Until then,
  treat [ADR-0005](docs/adr/0005-hermetic-transactional-composition.md) and
  [htc-sad.md](docs/architecture/htc-sad.md) as the normative direction for
  anything touching build execution, composition, or the atom-DAG re-scope —
  they take precedence over any stale evaluation/derivation framing still
  present elsewhere in this tree.

---

## Project Overview

> [!IMPORTANT]
> **Read the FAQ first for whole-system context.** The
> [whole-project FAQ](www/content/explanation/faq.md) covers the entire
> project in one place: what each layer is for, why the design is shaped
> the way it is, the verification/evidence discipline, and an honest map
> of what actually runs today versus what is only specified. It is
> written for both humans and agents — treat it as the fastest route to
> comprehending the full scope before diving into specs or code.

Axios is a decentralized, content-addressed source publishing stack decomposed
into three independent Cargo workspaces mapped to a six-layer architecture:

```
L5  Plugins    Plugin crates extending ion (future)
L4  ion/       Frontend: CLI, manifests, resolution
L3  eos/       Engine: builds, stores, scheduling
L2  HTC        Build-execution & composition substrate: CAS, compositions,
                interface manifests, build records, fetch-proxy execution,
                closure computation, materialization (skeleton workspace: htc/)
L1  atom/      Protocol: identity, addressing, publishing
L0  Cyphr      Cryptographic substrate (external; future)
```

Dependencies flow strictly downward: ion → eos → atom, with eos dispatching
build execution through HTC's executor trait (no crate dependency yet — see
[htc-sad.md](docs/architecture/htc-sad.md)). Each workspace is an
independent Cargo workspace with path-based inter-workspace deps.

For architecture details, see:

- [ADR-0001](docs/adr/0001-monorepo-workspace-architecture.md)
- [Roadmap](ROADMAP.md)
- [Formal Model](docs/models/publishing-stack-layers.md)

---

## Terminology Glossary

> [!CAUTION]
> Use **only** the canonical terms below. Legacy terms from the original eka
> codebase must **never** appear in new code, documentation, or conversation.
> If you catch yourself using a deprecated term, stop and correct it.

| Canonical Term         | Definition                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | Deprecated Aliases (NEVER use)    |
| :--------------------- | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :-------------------------------- |
| **Anchor**             | Cryptographic commitment establishing atom-set identity: `czd(charter₀)`, the coz digest of the set's signed founding charter transaction (backend-agnostic; immutable across succession)                                                                                                                                                                                                                                                                                                                                                                                               | genesis, root, Root               |
| **AtomId**             | The claimed atom's identity: the czd of its claim record — the signed record that binds a label to an anchor, exactly once. The label is a declared field of the claim; the anchor is never carried as data — it is discovered by walking the record log back to its charter genesis (ADR-0007 §4). The two never act as a fused unit. Before a claim exists (an unclaimed dev atom), identity degenerates to a digest computed from anchor and label together — the sole case where the two combine, as a digest's preimage. Lock entries are `(set, label) → {version, publish_czd}`. | hashed atom id, atom_id-as-digest |
| **Atom-set**           | Collection of atoms sharing a common anchor (a single repository)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | (none)                            |
| **Label**              | Human-readable name for an atom within an atom-set                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      | Name                              |
| **AtomDigest**         | Hash-algorithm-tagged content digest (`{alg, cad}`); wire form `<token>:<encoding>`, per-algorithm-conventional encoding (base64url-unpadded for the coz-native SHA family, lowercase hex for git SHA-1 and BLAKE3)                                                                                                                                                                                                                                                                                                                                                                     | Digest, Blake3Hash                |
| **Plan**               | Engine-specific build recipe (`BuildEngine::Plan` associated type); for the primary executor this is the atom action — `(atom_czd_closure_root, toolchain_composition_root, action_params)`, identified by `action_id`                                                                                                                                                                                                                                                                                                                                                                  | derivation, drv                   |
| **Output**             | Engine-specific build result (`BuildEngine::Output` associated type)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | build result                      |
| **Artifact**           | Content-addressed blob in an artifact store                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             | (none)                            |
| **Revision**           | A specific commit in source history                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | (none)                            |
| **Atom**               | Signed, content-addressed snapshot of sources + manifest + lock — build intent (L1; unchanged by the HTC substrate)                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | (none)                            |
| **Action**             | One invocation of `build`; not a persistent noun on its own — its identity is `action_id`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               | (none)                            |
| **Composition**        | Signed, content-addressed binding of names → digests (L2/HTC); the closure object, successor to a derivation's output closure                                                                                                                                                                                                                                                                                                                                                                                                                                                           | (none)                            |
| **View**               | A composition mounted at runtime via composefs                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | (none)                            |
| **Interface Manifest** | Derived, static provides/requires facts about a build's output tree, keyed by `(analyzer, subject)`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | (none)                            |

### Naming Conventions in Code

- Rust types use `PascalCase`: `AtomId`, `AtomSet`, `StorePath`
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
| Format | `treefmt` (from workspace root)              |
| Lint   | `cargo clippy` (from workspace root)         |

---

## Workspace Crates

> [!TIP]
> **Dynamic Discovery:** Crates and dependency layouts evolve. Rather than relying solely on this static list, always use dynamic discovery tools (e.g., query `cargo metadata` or inspect the root `Cargo.toml`) to determine the live set of active crates and dependencies.

### atom/ (L1 — Protocol)

| Crate       | Purpose                                              |
| :---------- | :--------------------------------------------------- |
| `atom-id`   | Identity primitives: labels, anchors, verified names |
| `atom-uri`  | Atom URI parsing and construction                    |
| `atom-core` | Protocol traits: `AtomSource`, `AtomRegistry`        |
| `atom-git`  | Git bridge: legacy storage backend                   |

### eos/ (L3 — Runtime)

| Crate        | Purpose                                                |
| :----------- | :----------------------------------------------------- |
| `eos-core`   | Engine traits: `BuildEngine`, `ArtifactStore`          |
| `eos-proto`  | Cap'n Proto wire schema and generated bindings         |
| `eos-snix`   | Slated for removal (evaluator eradicated, ADR-0006 §3) |
| `eos-daemon` | Scheduler, executor worker pool, RPC server            |
| `eos`        | Orchestration: wires engine + store                    |

### ion/ (L4 — Frontend)

| Crate          | Purpose                                                     |
| :------------- | :---------------------------------------------------------- |
| `ion-manifest` | `atom.toml` manifest parsing                                |
| `ion-resolve`  | Dependency resolution (SAT solver)                          |
| `ion-lock`     | Lock schema and (de)serialization                           |
| `ion-eos`      | Bridge: client interface to the eos daemon over Cap'n Proto |
| `ion-cli`      | CLI binary                                                  |

---

## Architecture Principles

1. **Design seams, not implementations.** Trait boundaries absorb future
   change (Cyphr, remote engines, new ecosystems).
2. **Abstract by default.** Digest algorithms, anchor types, and storage
   backends are generic. Concrete types live in bridge crates (atom-git).
3. **Dependency budget.** Protocol crates (atom-id, atom-core) target
   ≤ 5 non-std dependencies. Bridge crates have no such limit.
4. **Layer discipline.** L3 never imports L4. L2 never imports L3.
   Violations are architectural bugs.
5. **Cache-skipping is the value proposition.** Every stage of the
   cryptographic chain must be independently verifiable and skippable.

---

## Invariants

- **No Schema Changes:** The C.O.R.E. YAML grammar is rigid.
- **Halt on Ambiguity:** Never rationalize an assumption. Stop and ask.
- **Verification Required:** Every plan step must be verified.
- **Commit Boundaries:** Pause and justify before every commit point.
- **Commit Hygiene:** Automated commits are permitted _only_ when adhering precisely to the instructions of an installed commit hygiene agent skill. If no such skill is installed or available in your toolset, automated commits are strictly prohibited and you must default to manual commits or consult the user. Additionally, you must run `treefmt` before committing.
- **Terminology Compliance:** Use only canonical terms from the glossary above.
- **Self-Contained Documentation:** Committed docs must NEVER reference
  uncommitted paths (`.scratch/`, `.sketches/`) or acknowledge their
  existence — scratch is process-internal. If a committed doc needs
  context from a scratch file, inline that context. A reference the
  reader cannot follow is a broken contract.

## Design-Readiness Discipline (the standing refinement loop)

This project is spec-first: designs are verified by a _loop_, not a
one-shot review, because exhaustiveness against reality is undecidable.
Three evidence classes catch three disjoint error classes — never
substitute one for another:

1. **Decorrelated review** (a zero-context reviewer reading primary
   sources from first principles) catches contradictions _within_ the
   written corpus.
2. **Whole-system / intent review** (the intent-holder, or a reviewer
   armed with the full system picture) catches misfit between the corpus
   and unwritten intent or sibling-layer reality — the class formal
   review structurally cannot see (e.g., importing a centralized-cache
   invariant a decentralized trust design never needed).
3. **Empirical spikes** catch misfit with reality itself (measured costs,
   kernel behavior, privilege walls). Reality is the only oracle for
   "does it work."

**Readiness criterion:** proceed to implementation when the next-cheapest
error-discovery method _is building_ — never when "no errors remain"
(unprovable). Checkable proxies: (a) the seam map has no _unmapped_ seams
(known-thin is acceptable if the implementation path doesn't cross it
yet); (b) every layer-boundary has a named invariant, owner, and
evaluator; (c) proof obligations gating the first steps are discharged or
carry explicit tripwires; (d) load-bearing intent is _written down_ —
unwritten vision cannot be caught contradicting anything.

**The amendment loop:** contradictions discovered while building are
treated like closure faults — fail closed, amend the model with a
versioned changelog entry, never silently absorb. Formal docs carry
correction history as evidence the loop runs.

---

> [!TIP]
> Refer to the workspace-specific `AGENTS.md` files in the subdirectories (e.g. `atom/AGENTS.md`, `eos/AGENTS.md`, `ion/AGENTS.md`) for more targeted context on each layer.
