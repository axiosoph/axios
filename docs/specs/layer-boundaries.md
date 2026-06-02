# SPEC: Layer Boundary Constraints

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.
-->

## Domain

**Problem Domain:** The Axios publishing stack is decomposed into
three workspaces (atom, eos, ion) mapped to a layered architecture
(L1, L2, L3). The _behavioral_ boundaries between layers are
formalized by the coalgebras and session types in the
[layer model](../models/publishing-stack-layers.md). What remains
unspecified — and what this document constrains — are the
_structural ownership_ boundaries: which workspace owns which
concerns, which crate may depend on which, and where inter-layer
contract types reside.

Without these constraints, boundary violations accumulate silently.
A type that belongs in a contract crate migrates into an
implementation crate. A frontend crate acquires a dependency on
a backend implementation detail. These violations compound: each
one makes the next one feel natural, until the layered architecture
becomes aspirational rather than enforced.

**Model Reference:**

- [publishing-stack-layers.md](../models/publishing-stack-layers.md) —
  behavioral boundaries (coalgebras, session types)
- [ADR-0001](../adr/0001-monorepo-workspace-architecture.md) —
  workspace structure decision

**Related Specs:**

- [ion-eos-contract.md](ion-eos-contract.md) — semantic handoff
  contract; this spec constrains the _structural_ side of that
  boundary
- [lock-file-schema.md](lock-file-schema.md) — schema of the
  principal ion→eos interface artifact

**Criticality Tier:** Medium — violations do not cause immediate
runtime failures, but they erode the architecture's ability to
absorb change (Cyphr migration, alternative frontends, remote
engine deployment). The constraints are mechanically enforceable
via dependency analysis.

---

## §1 — Layer Architecture

The stack comprises five logical layers. Only L1–L3 are active
workspaces; L0 and L4 are defined for completeness and forward
compatibility.

```
L4  Plugins    Plugin crates extending ion (future)
L3  ion/       Frontend: CLI, manifests, resolution
L2  eos/       Engine: evaluation, builds, stores, scheduling
L1  atom/      Protocol: identity, addressing, publishing
L0  Cyphr      Cryptographic substrate (external; future)
```

Each active workspace contains two kinds of crates:

| Kind                     | Naming Convention                          | Purpose                                                                                                            |
| :----------------------- | :----------------------------------------- | :----------------------------------------------------------------------------------------------------------------- |
| **Contract crate**       | `<workspace>-core`                         | Defines the traits, types, and error enums that constitute the layer's public API surface. Lean dependency budget. |
| **Implementation crate** | `<workspace>-<impl>` or bare `<workspace>` | Implements contract traits using concrete backends. Heavier dependencies permitted.                                |

Examples:

| Crate          | Kind            | Layer |
| :------------- | :-------------- | :---- |
| `atom-id`      | Contract        | L1    |
| `atom-core`    | Contract        | L1    |
| `atom-uri`     | Contract        | L1    |
| `atom-git`     | Implementation  | L1    |
| `eos-core`     | Contract        | L2    |
| `eos-proto`    | Contract (wire) | L2    |
| `eos-snix`     | Implementation  | L2    |
| `eos-daemon`   | Implementation  | L2    |
| `eos`          | Implementation  | L2    |
| `ion-manifest` | Contract        | L3    |
| `ion-resolve`  | Contract        | L3    |
| `ion-lock`     | Contract        | L3    |
| `ion-eos`      | Bridge          | L3    |
| `ion-cli`      | Implementation  | L3    |

> [!NOTE]
> `ion-eos` is a **bridge crate** — it connects two layers by
> depending on a lower layer's contract crate (`eos-core`,
> `eos-proto`) and adapting it for the upper layer. Bridge crates
> are permitted to depend on contract crates from the layer below,
> but MUST NOT depend on implementation crates.

---

## §2 — Dependency Direction

### §2.1 — Downward-Only Rule

**[boundary-downward-only]**: Dependencies between workspaces MUST
flow strictly downward through the layer stack. A crate at layer
Lₙ MUST NOT depend (directly or transitively) on any crate at
layer Lₘ where m > n.

VERIFIED: _pending_

```
Permitted:     ion → eos-core      (L3 → L2 contract)
Permitted:     ion → atom-core     (L3 → L1 contract)
Permitted:     eos → atom-core     (L2 → L1 contract)
FORBIDDEN:     atom → eos-core     (L1 → L2)
FORBIDDEN:     atom → ion-manifest (L1 → L3)
FORBIDDEN:     eos → ion-manifest  (L2 → L3)
```

### §2.2 — Contract-Only Across Boundaries

**[boundary-contract-only]**: Cross-workspace dependencies MUST
target contract crates, not implementation crates. A crate in
workspace W₁ MUST NOT depend on an implementation crate in
workspace W₂.

VERIFIED: _pending_

```
Permitted:     ion-cli → eos-core       (contract)
Permitted:     ion-eos → eos-proto      (contract/wire)
FORBIDDEN:     ion-cli → eos            (implementation)
FORBIDDEN:     ion-cli → eos-snix       (implementation)
FORBIDDEN:     ion-cli → eos-daemon     (implementation)
FORBIDDEN:     ion-eos → eos            (implementation)
```

### §2.3 — Implementation Isolation

**[boundary-impl-isolation]**: Implementation crates MUST NOT be
depended upon by any crate outside their own workspace.

VERIFIED: _pending_

This is the crate-level corollary of `[boundary-contract-only]`
but also covers intra-workspace-but-cross-boundary scenarios.
`atom-git` MUST NOT be depended upon by `eos-core` or `eos-snix`.
Implementation details of one layer's backend are invisible to
all other layers.

### §2.4 — Intra-Workspace Freedom

**[boundary-intra-workspace]**: Within a single workspace,
dependency direction is unconstrained. Implementation crates
MAY depend on their workspace's contract crates and on each
other as needed.

VERIFIED: _pending_

```
Permitted:     eos-snix → eos-core      (impl → contract, same workspace)
Permitted:     eos-daemon → eos         (impl → impl, same workspace)
Permitted:     eos → eos-core           (impl → contract, same workspace)
```

---

## §3 — Contract Type Placement

### §3.1 — Interface Types in Contract Crates

**[boundary-contract-types]**: Types that appear in the signature
of any cross-layer trait method, or that are exchanged as the
serialized interface artifact between two layers, MUST reside in
a contract crate of the _consuming_ layer.

VERIFIED: _pending_

**Rationale:** The consuming layer defines its input contract. The
producing layer depends on the consumer's contract crate to
serialize conforming output. This mirrors how `eos-core` defines
`BuildEngine` (the contract eos exposes) and ion depends on
`eos-core` to invoke it.

| Interface artifact              | Producer           | Consumer       | Contract crate      |
| :------------------------------ | :----------------- | :------------- | :------------------ |
| `BuildEngine` trait             | —                  | `ion-cli`      | `eos-core`          |
| `EvalRequest` / `ResolvedInput` | `ion-eos` (bridge) | `eos` engine   | `eos-core`          |
| `AtomSource` trait              | —                  | `eos-core`     | `atom-core`         |
| `Manifest` trait                | —                  | `ion-manifest` | `atom-core`         |
| Cap'n Proto schema              | `eos-daemon`       | `ion-eos`      | `eos-proto`         |
| Lock file (`atom.lock`)         | `ion-resolve`      | `ion-eos`      | **`ion-lock` (L3)** |

> [!IMPORTANT]
> The lock file is NOT an inter-layer contract type — it is ion's
> serialization format for resolved dependencies. The actual
> inter-layer contract is `EvalRequest` and its constituents in
> `eos-core`. See `[boundary-lock-ownership]` (§4.2) for rationale.

### §3.2 — Dependency Budget for Contract Crates

**[boundary-contract-dep-budget]**: Contract crates at L1 MUST
have ≤ 5 non-`std` dependencies. Contract crates at L2 and L3
SHOULD have ≤ 10 non-`std` dependencies. Implementation crates
have no dependency budget.

VERIFIED: _pending_

**Rationale:** Contract crates define the API surface that every
consumer must transitively compile. A bloated contract crate
imposes its dependency tree on the entire downstream graph.
The L1 budget of 5 is inherited from ADR-0001.

### §3.3 — No Backend Leakage

**[boundary-no-backend-leakage]**: Contract crate types MUST NOT
expose backend-specific types in their public API. Snix types,
gix types, nix-compat types, and other backend-specific types
MUST NOT appear in the public signatures of `eos-core`, `atom-core`,
or any other contract crate.

VERIFIED: _pending_

```
FORBIDDEN:     eos-core::BuildEngine<Plan = Derivation>
                                       ^^^^^^^^^^^^ snix type

Permitted:     eos-core::BuildEngine { type Plan; }
                                       (associated type — opaque)
```

---

## §4 — Ownership Boundaries

### §4.1 — Layer Concerns

Each layer owns a specific concern domain. Concerns not listed are
unowned — they require an explicit ownership decision (which SHOULD
be recorded as an amendment to this spec) before implementation
proceeds.

**[boundary-L1-concerns]**: L1 (atom) owns:

- Identity primitives: `AtomId`, `Label`, `Tag`, `Identifier`, `Anchor`
- Content addressing: `AtomDigest`, digest computation
- URI parsing: `RawAtomUri`, `AtomUri`, alias resolution
- Protocol traits: `AtomSource`, `AtomRegistry`, `AtomStore`
- Protocol types: `Manifest`, `VersionScheme`, `ClaimPayload`,
  `PublishPayload`
- Storage backends: git bridge (`atom-git`), future Cyphr bridge
- **Atom store**: content-addressed storage for atom source trees.
  This is the sole storage interface for atom content. Consumers
  (eos, ion) read atoms through the `AtomSource` trait and MUST
  NOT implement their own atom fetching or storage logic.

L1 MUST NOT own: manifest _formats_ (that's L3), build recipes
(that's L2), dependency resolution (that's L3), or lock files
(that's L3).

VERIFIED: _pending_

**[boundary-L2-concerns]**: L2 (eos) owns:

- Build engine trait: `BuildEngine`, `BuildPlan`
- Build execution: evaluation, sandboxing, plan/apply lifecycle
- Scheduling: job queues, work-stealing, lease management
- **Artifact store**: content-addressed storage for build outputs
  (derivations, compiled artifacts). Defined by the `ArtifactStore`
  trait in `eos-core`. Backend: snix store (primary).
- Build input contract: `EvalRequest`, `ResolvedInput`,
  `ComposerConfig`, `BuildRequest`, `FetchDescriptor`, cache key
  computation
- Daemon infrastructure: network protocol, RPC, discovery
- Non-atom dependency fetching: Nix expressions, tarballs, git
  sources. These are not atoms and do not flow through the atom
  protocol.

L2 MUST NOT own: manifest formats (that's L3), dependency
resolution (that's L3), CLI interface (that's L3), identity
primitives (that's L1), **atom storage or fetching** (that's L1),
or **frontend-specific serialization formats** including the lock
file (that's L3). Eos reads atom content through the `AtomSource`
trait (L1) and MUST NOT implement its own atom fetching logic.

VERIFIED: _pending_

**[boundary-L3-concerns]**: L3 (ion) owns:

- Manifest format: `ion.toml` / `atom.toml` parsing and validation
- Dependency resolution: SAT solver, constraint matching, version
  comparison
- **Lock file format**: `atom.lock` types, parsing, validation,
  and production — ion's serialization of resolved dependencies
- Lock file → eos contract translation: converting lock file
  content into `eos-core` types (`EvalRequest`, `ResolvedInput`)
- CLI interface: commands, user-facing output, dev workspace
  management
- Engine dispatch: compile-time generics selecting the build engine
- Bridge to eos: connection, RPC invocation, progress monitoring

L3 MUST NOT own: identity primitives (that's L1), build engine
internals (that's L2), or storage backends (that's L1/L2).

VERIFIED: _pending_

**[boundary-L4-concerns]**: L4 (plugins) extends L3 with
ecosystem-specific dependency handling. Plugins MAY produce
lock file entries that conform to the lock file schema
(`[lock-type-extension-mechanism]`), but MUST NOT bypass ion's
resolution pipeline to interact with eos directly. Plugin
boundary constraints are deferred until the plugin system
matures.

VERIFIED: _pending_

### §4.2 — Lock File Ownership

**[boundary-lock-ownership]**: The lock file format (`atom.lock`)
is owned by L3 (ion). The lock file is ion's serialization of
resolved dependencies — it is a frontend-specific artifact, not
an inter-layer contract type.

VERIFIED: _pending_

**Rationale — the cargo-atom test:**

If a hypothetical `cargo-atom` frontend published and consumed
atoms, it would use `Cargo.lock` — a completely different format.
Eos should not need to understand `Cargo.lock`, just as it should
not need to understand `atom.lock`. What eos needs is the
_structured build request_ expressible in `eos-core` types:
`EvalRequest`, `ResolvedInput`, `ComposerConfig`. Those types
already exist. The lock file is just ion's serialization of them.

**Architectural flow:**

```
ion-resolve ──produces──→ atom.lock  (ion's format, L3 types)
ion-eos     ──parses────→ LockFile   (L3 types)
ion-eos     ──translates→ EvalRequest, ResolvedInput (L2 contract)
ion-eos     ──submits───→ eos-daemon via RPC (L2 contract)
eos-daemon  ──receives──→ EvalRequest (eos-core types only)
```

Eos never sees the lock file format. The translation from lock
types to eos-core types happens in the `ion-eos` bridge crate,
which is the architecturally correct location: bridge crates
adapt the upper layer's representations into the lower layer's
contract surface.

**Crate placement — `ion-lock`:**

The lock file types SHOULD reside in a dedicated `ion-lock` crate
within the `ion/` workspace, rather than being embedded in
`ion-resolve`. This separation provides:

1. A clean import target for `ion-eos` (bridge parses lock files)
2. Independence from the SAT solver and resolution logic
3. A natural home for lock file validation (`[lock-dag-acyclicity]`,
   `[lock-requires-closure]`, etc.)
4. A clear ownership signal — the crate name declares the layer

`ion-lock` depends on `atom-id` (for `AtomId` in lock entries)
and `serde` + `toml` (for parsing). It does NOT depend on
`eos-core` — the lock types are purely ion's domain.

> [!IMPORTANT]
> **Current violation:** Lock file types currently reside in
> `eos/eos/src/lock.rs` — an L2 implementation crate. They MUST
> migrate to `ion/ion-lock/` per this constraint. See §6.1 for
> the migration path.

### §4.3 — Bridge Crate Constraints

**[boundary-bridge-crate]**: Bridge crates connect two adjacent
layers. They MUST:

1. Depend only on contract crates from the layer below (not
   implementation crates)
2. Reside in the upper layer's workspace
3. Adapt between the upper layer's representations and the
   lower layer's contract surface in both directions — not
   re-export verbatim

VERIFIED: _pending_

`ion-eos` is the canonical bridge crate. It resides in `ion/`,
depends on `eos-core` and `eos-proto`, and performs two
adaptations:

- **Downward:** parses lock files (`ion-lock`) and translates
  them into `eos-core` types for RPC submission
- **Upward:** wraps the Cap'n Proto RPC surface into a
  Rust-idiomatic `EosClient` API for ion's consumption

After lock file migration, `ion-eos` additionally depends on
`ion-lock` to parse lock files before translation.

### §4.4 — Store Separation

**[boundary-store-separation]**: The atom store (L1) and the
artifact store (L2) are architecturally distinct and MUST NOT
be conflated.

| Property            | Atom Store (L1)                        | Artifact Store (L2)                   |
| :------------------ | :------------------------------------- | :------------------------------------ |
| **Data**            | Atom source trees (source code)        | Build outputs (derivations, binaries) |
| **Trait**           | `AtomSource` / `AtomStore` (atom-core) | `ArtifactStore` (eos-core)            |
| **Addressing**      | `AtomId` / `AtomDigest`                | Plan hash / output digest             |
| **Primary backend** | git                                    | snix store                            |
| **Populated by**    | Ion (ingestion), eos composite source  | Eos (build outputs)                   |
| **Read by**         | Eos (build inputs via `AtomSource`)    | Eos (cache hits), ion (build results) |

A host MAY run both an atom store and an artifact store, but
they are logically and physically separate. An implementation
MUST NOT store atom source trees in the artifact store or build
outputs in the atom store.

The atom store is the ONLY path through which atom content enters
the eos pipeline. Eos reads atom content through the `AtomSource`
trait and MUST NOT implement its own atom fetching or ingestion
logic (see `[boundary-L2-concerns]`). Verification of atom
integrity is the atom protocol's responsibility at ingestion
time — eos trusts atoms resolved from its `AtomSource`.

VERIFIED: _pending_

---

## §5 — Enforcement

### §5.1 — Mechanical Verification

**[boundary-ci-enforcement]**: The layer boundary constraints in
§2 SHOULD be enforced by automated CI checks. A conformance
script MUST parse the `Cargo.toml` files of all workspace crates,
construct the dependency graph, and verify:

1. No upward cross-workspace dependency
   (`[boundary-downward-only]`)
2. No cross-workspace dependency on an implementation crate
   (`[boundary-contract-only]`)
3. Contract crate dependency budgets
   (`[boundary-contract-dep-budget]`)

The script SHOULD be implemented as a `cargo xtask` command or
a standalone Rust/shell script in `tools/`.

VERIFIED: _pending_

### §5.2 — Crate Classification

**[boundary-crate-metadata]**: Each crate's `Cargo.toml` SHOULD
carry a `[package.metadata.axios]` table declaring its
classification:

```toml
[package.metadata.axios]
layer = "L2"
kind = "contract"  # or "implementation" or "bridge"
```

If metadata is absent, the enforcement script MUST classify crates
by naming convention:

- `*-core`, `*-proto`, `*-id`, `*-uri` → contract
- `*-cli`, `*-daemon`, `*-git`, `*-snix` → implementation
- `*-eos` (in ion/) → bridge

VERIFIED: _pending_

### §5.3 — Violation Budgeting

**[boundary-violation-budget]**: Known violations MAY be
documented in a `boundary-exceptions.toml` file at the repo root.
Each exception MUST carry:

- The violating dependency edge (source → target)
- A rationale explaining why the exception exists
- A tracking issue or TODO for remediation

The enforcement script MUST allow exceptions listed in this file.
The exception count SHOULD trend toward zero.

VERIFIED: _pending_

---

## §6 — Known Violations

The following violations exist as of the date of this
specification. Each requires remediation.

### §6.1 — Lock types in `eos` instead of `ion`

**Violates:** `[boundary-lock-ownership]`, `[boundary-L2-concerns]`

`LockFile`, `Dependency`, `AtomDep`, `NixDep`, `NixGitDep`,
`NixTarDep`, `NixSrcDep`, `SetDetails`, `ComposeConfig`, and
`ComposeArgs` are defined in `eos/eos/src/lock.rs` — an L2
implementation crate. The lock file is ion's serialization format
and these types MUST migrate to L3.

**Migration path:**

1. Create `ion/ion-lock/` crate with the lock file types,
   `parse()`, and `validate()`
2. Add `ion-lock` dependency to `ion-eos`
3. **Design a pre-fetch `BuildRequest` type in `eos-core`.**
   Currently `eos-core` has `ResolvedInput<D>` (post-fetch:
   `store_path` + `digest`) but no pre-fetch dependency
   descriptor. The migration requires a new type — e.g.
   `FetchDescriptor` or an expanded `BuildRequest` — that
   carries the fetch metadata currently embedded in lock
   file types (URLs, expected digests, labels, type tags).
   This type is eos's pre-fetch input contract.
4. In `ion-eos`, parse the lock file _before_ RPC submission
   and translate lock entries into the new `eos-core`
   pre-fetch types
5. Update the Cap'n Proto schema / RPC to accept structured
   build requests rather than raw TOML lock content
6. Remove `eos/eos/src/lock.rs`, `eos/eos/src/fetch.rs` lock
   imports, and the lock parsing from `eos-daemon/scheduler.rs`
7. The orchestrator receives pre-parsed `eos-core` types
   directly — it fetches, verifies, and resolves inputs using
   the structured descriptors, then constructs `EvalRequest`
   from the resolved results

**Impact:** This migration touches the RPC boundary (Cap'n Proto
schema change), requires new `eos-core` contract types, and
changes the orchestrator's input signature. It is a structural
refactor, not a cosmetic one. It SHOULD be planned as a dedicated
workstream.

### §6.2 — `ion-eos` ad-hoc TOML parsing

**Violates:** `[boundary-lock-ownership]` (indirectly)

`ion-eos/src/lib.rs` performs ad-hoc TOML parsing of
`compose.args` (lines 78–92) instead of using lock file types.
Once `ion-lock` exists, `ion-eos` MUST use
`ion_lock::LockFile::parse()` to extract compose args from a
validated lock file, eliminating the ad-hoc parsing.

### §6.3 — Eos receives raw lock content

**Violates:** `[boundary-L2-concerns]`

`run_orchestrated_build()` in `eos/eos/src/orchestrator.rs`
accepts `lock_content: &str` and parses it into `LockFile`.
After migration, the orchestrator MUST accept pre-parsed
`eos-core` types — an `EvalRequest` (or a structured
`BuildRequest` type in `eos-core`) rather than raw TOML.

### §6.4 — Eos daemon persists lock files to disk

**Violates:** `[boundary-L2-concerns]`

`eos-daemon/src/scheduler.rs` reads lock file content from a
`locks_dir` on the host filesystem (`/tmp/eos-locks/{digest}.lock`).
`eos-daemon/src/config.rs` defines the `locks_dir` CLI argument
and `resolve_locks_dir()` helper. This is a remnant of the
pre-structured-request architecture where ion or an operator
copied raw lock files into place for the daemon to consume.

After the lock migration (§6.1), eos receives structured
`BuildRequest` types via RPC — no lock file touches the eos side
at all. The `locks_dir` configuration, `resolve_locks_dir()`
method, and all lock file I/O in the scheduler MUST be removed.

### §6.5 — Eos reimplements atom fetching

**Violates:** `[boundary-L2-concerns]`, `[boundary-L1-concerns]`

`eos/eos/src/fetch.rs` implements atom fetching from mirrors
(via `curl`, `git clone`, tarball extraction) — duplicating
functionality that belongs to the atom protocol (L1). Eos MUST
read atom content through the `AtomSource` interface, not
implement its own fetch-and-verify pipeline for atoms.

After migration, atom dependency resolution in eos uses
`AtomSource::resolve()` on a composite source (local store →
registry → ion peer). Non-atom dependency fetching (Nix
expressions, tarballs, git sources) remains in eos as those
are not atoms and do not flow through the atom protocol.

---

## §7 — Decision Framework

When uncertainty arises about where a type or module belongs,
apply these questions in order:

1. **Does it appear in a cross-layer trait signature?**
   → It MUST live in the consuming layer's contract crate.

2. **Is it a serialization format specific to one frontend?**
   → It MUST live in that frontend's workspace (L3/L4).
   The inter-layer contract is the _structured types_ the
   consumer defines, not the frontend's serialization format.

3. **Is it exchanged as structured data between layers?**
   → It MUST live in the consuming layer's contract crate.

4. **Does it depend on a specific backend (snix, gix, Cap'n
   Proto)?**
   → It MUST live in an implementation crate.

5. **Is it used only within a single workspace?**
   → It MAY live in any crate within that workspace.

6. **Is it a trait that defines an extension point?**
   → It MUST live in a contract crate.

7. **None of the above?**
   → Default to the contract crate of the layer whose concern
   domain (`[boundary-L*-concerns]`) covers the type's purpose.
   If ambiguous, HALT and record an ownership decision as an
   amendment to this spec.

---

## Verification Status

| Constraint                       | Tag                     | Method                              | Status                                            |
| :------------------------------- | :---------------------- | :---------------------------------- | :------------------------------------------------ |
| `[boundary-downward-only]`       | `UNVERIFIED`            | `cargo metadata` graph analysis     | Pending CI script                                 |
| `[boundary-contract-only]`       | `UNVERIFIED`            | `cargo metadata` graph analysis     | Pending CI script                                 |
| `[boundary-impl-isolation]`      | `UNVERIFIED`            | `cargo metadata` graph analysis     | Pending CI script                                 |
| `[boundary-intra-workspace]`     | `VERIFIED: agent-check` | By definition (unconstrained)       | Trivially satisfied                               |
| `[boundary-contract-types]`      | `UNVERIFIED`            | Manual audit                        | No current violations after lock reclassification |
| `[boundary-contract-dep-budget]` | `UNVERIFIED`            | `cargo metadata` dep count          | Pending CI script                                 |
| `[boundary-no-backend-leakage]`  | `UNVERIFIED`            | Public API audit                    | Pending                                           |
| `[boundary-L1-concerns]`         | `VERIFIED: agent-check` | Cross-referenced with ADR-0001      | Consistent                                        |
| `[boundary-L2-concerns]`         | `VERIFIED: agent-check` | Cross-referenced with ADR-0001      | Amended: lock file removed from L2                |
| `[boundary-L3-concerns]`         | `VERIFIED: agent-check` | Cross-referenced with ADR-0001      | Amended: lock file format added to L3             |
| `[boundary-L4-concerns]`         | `VERIFIED: agent-check` | Deferred                            | Minimal constraints pending plugin maturity       |
| `[boundary-lock-ownership]`      | `VERIFIED: agent-check` | Cargo-atom test; format vs contract | §4.2 rationale                                    |
| `[boundary-bridge-crate]`        | `VERIFIED: agent-check` | `ion-eos` Cargo.toml verified       | Conforms                                          |
| `[boundary-store-separation]`    | `UNVERIFIED`            | Manual audit                        | New constraint; pending implementation            |
| `[boundary-ci-enforcement]`      | `UNVERIFIED`            | Script does not yet exist           | Pending implementation                            |
| `[boundary-crate-metadata]`      | `UNVERIFIED`            | Metadata not yet present            | Pending implementation                            |
| `[boundary-violation-budget]`    | `UNVERIFIED`            | File does not yet exist             | Pending implementation                            |
