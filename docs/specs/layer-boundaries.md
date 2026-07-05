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
(L1, L3, L4 — L2/HTC and L0/L5 are non-crate layers, see §1). The
_behavioral_ boundaries between layers are
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

The stack comprises six logical layers (renumbered 2026-07-05 per
[ADR-0005](../adr/0005-hermetic-transactional-composition.md) §9,
`[htc-layer-designation]`, which inserts L2/HTC and shifts eos,
ion, and plugins down one slot each). L1, L3, and L4 are active
workspaces; L0, L2, and L5 are defined for completeness and forward
compatibility — L2/HTC has a landed ADR and SAD
([htc-sad.md](../architecture/htc-sad.md)) but no crate workspace
yet (spec authorship and implementation are P3/P4 work).

```
L5  Plugins    Plugin crates extending ion (future)
L4  ion/       Frontend: CLI, manifests, resolution
L3  eos/       Engine: builds, stores, scheduling
L2  HTC        Build-execution & composition substrate: CAS, compositions,
                interface manifests, build records, fetch-proxy execution,
                closure computation, materialization (no crate workspace yet)
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
| `eos-core`     | Contract        | L3    |
| `eos-proto`    | Contract (wire) | L3    |
| `eos-snix`     | Implementation  | L3    |
| `eos-daemon`   | Implementation  | L3    |
| `eos`          | Implementation  | L3    |
| `ion-manifest` | Contract        | L4    |
| `ion-resolve`  | Contract        | L4    |
| `ion-lock`     | Contract        | L4    |
| `ion-eos`      | Bridge          | L4    |
| `ion-cli`      | Implementation  | L4    |

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
Permitted:     ion → eos-core      (L4 → L3 contract)
Permitted:     ion → atom-core     (L4 → L1 contract)
Permitted:     eos → atom-core     (L3 → L1 contract)
FORBIDDEN:     atom → eos-core     (L1 → L3)
FORBIDDEN:     atom → ion-manifest (L1 → L4)
FORBIDDEN:     eos → ion-manifest  (L3 → L4)
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
| Lock file (`atom.lock`)         | `ion-resolve`      | `ion-eos`      | **`ion-lock` (L4)** |

> [!IMPORTANT]
> The lock file is NOT an inter-layer contract type — it is ion's
> serialization format for resolved dependencies. The actual
> inter-layer contract is `EvalRequest` and its constituents in
> `eos-core`. See `[boundary-lock-ownership]` (§4.2) for rationale.

### §3.2 — Dependency Budget for Contract Crates

**[boundary-contract-dep-budget]**: Contract crates at L1 MUST
have ≤ 5 non-`std` dependencies. Contract crates at L3 and L4
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

- Identity primitives: `AtomId` (`(anchor, label)` pair), `Label`, `Tag`, `Identifier`, `Anchor`
- URI parsing: `RawAtomUri`, `AtomUri`, alias resolution
- Protocol traits: `AtomSource`, `AtomRegistry`, `AtomStore`
- Protocol types: `Manifest`, `VersionScheme`, `ClaimPayload`,
  `PublishPayload`
- Storage backends: git bridge (`atom-git`), future Cyphr bridge
- **Atom store**: content-addressed storage for atom source trees.
  This is the sole storage interface for atom content. Consumers
  (eos, ion) read atoms through the `AtomSource` trait and MUST
  NOT implement their own atom fetching or storage logic.
- **Signed-metadata-append channel**: post-publish fact publication
  (build records, interface manifests) appended to an atom's signed
  metadata (`[publish-payload-extensible]`, atom-transactions.md;
  the append transition, git-storage-format.md) — the mechanism L2
  (HTC) uses to record build provenance. Hardening (builder ≠
  claim-owner signer authorization, a fact-append vs. moved-tip-
  warning carve-out, a fact-kind convention) is an open gap
  (atom-sad §9; design campaign P1, ADR-0005 §Open Items).

L1 MUST NOT own: manifest _formats_ (that's L4), build recipes
(that's L2/L3), dependency resolution (that's L4), or lock files
(that's L4).

VERIFIED: _pending_

**[boundary-L2-concerns]**: L2 (HTC — Hermetic Transactional
Composition) owns:

- **CAS**: content-addressed blob/tree storage (`snix-castore`,
  reused). Compositions, interface manifests, and build records are
  all CAS-resident, addressed by their own canonical-serialization
  digest (htc-sad.md §2.4).
- **Composition objects**: the signed, content-addressed
  name→digest binding that is the closure object — the successor to
  a Nix derivation's output closure (htc-sad.md §2.1, §3.1).
- **Interface-manifest analysis**: deriving provides/requires facts
  from a build's output tree via namespace-plugin analyzers (ELF,
  Python, …), keyed `(ns, analyzer_czd, subject_digest)` (htc-sad.md
  §2.2, §3.2).
- **Build records**: SLSA-shaped per-action provenance, signed and
  appended via L1's metadata-append channel (htc-sad.md §2.3, §6.10).
- **Fetch-proxy execution**: the record/replay HTTP(S) CONNECT proxy
  that *executes* (never declares) the non-atom fetch entries L4
  (ion) records as lock plugin entries (htc-sad.md §4.2).
- **Closure computation**: the satisfaction fixpoint that computes a
  justified runtime composition from declared + observed requires
  (htc-sad.md §6.4).
- **Materialization/views**: mounting a composition as a runtime view
  at one of three tiers — Observe / Fast / Export (htc-sad.md §5).

L2 has no crate workspace yet — spec authorship is P3/P4 work
(htc-sad.md Appendix C); this concern list registers ownership ahead
of implementation, per this section's own ownership rule.

L2 MUST NOT own: atom identity or the lock's atom contribution
(that's L1), dependency resolution, the lock file, fetch-entry
_declaration_, or the manifest (that's L4), or scheduling policy,
worker placement, or the atom-DAG (that's L3 — L2 is what L3's
executor trait dispatches *to*, not the scheduler itself).

VERIFIED: _pending_

**[boundary-L3-concerns]**: L3 (eos) owns:

- Build engine trait: `BuildEngine`, `BuildPlan`
- Build execution: sandboxing, plan/apply lifecycle (dispatched
  through L2's executor trait — eos schedules; it does not build,
  ADR-0005 §6)
- Scheduling: job queues, work-stealing, lease management, executor
  dispatch — invoking L2's `build(atom_closure, toolchain, params)`
  per atom action; the atom-DAG traversal itself (ADR-0005 §6,
  htc-sad.md §3.5, §6.7)
- **Artifact store**: content-addressed storage for build outputs,
  provided by L2's CAS. The `ArtifactStore` trait in `eos-core` is
  the scheduler's read/cache-existence seam onto it — eos does not
  run its own store daemon (re-framed from the prior "snix store
  daemon" framing; which wire-first implementation backs the CAS is
  deferred to P3, ADR-0005 §10).
- Build input contract: `EvalRequest`, `ResolvedInput`,
  `ComposerConfig`, `BuildRequest`, `FetchDescriptor`, cache key
  computation
- Daemon infrastructure: network protocol, RPC, discovery

L3 MUST NOT own: manifest formats (that's L4), dependency
resolution (that's L4), CLI interface (that's L4), identity
primitives (that's L1), **atom storage or fetching** (that's L1),
**non-atom fetch-set declaration or execution** (declaration is
L4's, execution is L2's — ADR-0005 §7), or **frontend-specific
serialization formats** including the lock file (that's L4). Eos
reads atom content through the `AtomSource` trait (L1) and MUST NOT
implement its own atom fetching logic.

VERIFIED: _pending_

**[boundary-L4-concerns]**: L4 (ion) owns:

- Manifest format: `ion.toml` / `atom.toml` parsing and validation
- Dependency resolution: SAT solver, constraint matching, version
  comparison
- **Lock file format**: `atom.lock` types, parsing, validation,
  and production — ion's serialization of resolved dependencies
- **Fetch-set declaration**: non-atom dependency pins (source
  tarballs, crates, npm packages, …) as lock `[[deps]]` entries
  dispatched by `type` (e.g. `type = "fetch"`), per
  `[lock-type-extension-mechanism]` (lock-file-schema.md) — ion
  declares, never executes, a fetch (ADR-0005 §7). "Nix expressions"
  as a dependency class is removed from the MVP taxonomy; it
  survives only within the optional passthrough-snix legacy
  executor's scope (htc-sad.md §6.8).
- Lock file → eos contract translation: converting lock file
  content into `eos-core` types (`EvalRequest`, `ResolvedInput`)
- CLI interface: commands, user-facing output, dev workspace
  management
- Engine dispatch: compile-time generics selecting the build engine
- Bridge to eos: connection, RPC invocation, progress monitoring

L4 MUST NOT own: identity primitives (that's L1), build engine
internals (that's L3), fetch-set _execution_ (that's L2), or
storage backends (that's L1/L2).

VERIFIED: _pending_

**[boundary-L5-concerns]**: L5 (plugins) extends L4 with
ecosystem-specific dependency handling. Plugins MAY produce
lock file entries that conform to the lock file schema
(`[lock-type-extension-mechanism]`), but MUST NOT bypass ion's
resolution pipeline to interact with eos directly. Plugin
boundary constraints are deferred until the plugin system
matures.

VERIFIED: _pending_

### §4.2 — Lock File Ownership

**[boundary-lock-ownership]**: The lock file format (`atom.lock`)
is owned by L4 (ion). The lock file is ion's serialization of
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
ion-resolve ──produces──→ atom.lock  (ion's format, L4 types)
ion-eos     ──parses────→ LockFile   (L4 types)
ion-eos     ──translates→ EvalRequest, ResolvedInput (L3 contract)
ion-eos     ──submits───→ eos-daemon via RPC (L3 contract)
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
> `eos/eos/src/lock.rs` — an L3 implementation crate. They MUST
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
artifact store (L3) are architecturally distinct and MUST NOT
be conflated.

| Property            | Atom Store (L1)                         | Artifact Store (L3)                   |
| :------------------ | :-------------------------------------- | :------------------------------------ |
| **Data**            | Atom source trees (source code)         | Build outputs (derivations, binaries) |
| **Trait**           | `AtomSource` / `AtomStore` (atom-core)  | `ArtifactStore` (eos-core)            |
| **Addressing**      | `AtomId` (pair) / `blake3(publish_czd)` | Plan hash / output digest             |
| **Primary backend** | git                                     | CAS (L2/HTC; `snix-castore`-backed — the fork-vs-speak-upstream-snix call is deferred to P3, ADR-0005 §10) |
| **Populated by**    | Ion (ingestion), eos composite source   | Eos (build outputs)                   |
| **Read by**         | Eos (build inputs via `AtomSource`)     | Eos (cache hits), ion (build results) |

A host MAY run both an atom store and an artifact store, but
they are logically and physically separate. An implementation
MUST NOT store atom source trees in the artifact store or build
outputs in the atom store.

The atom store is the ONLY path through which atom content enters
the eos pipeline. Eos reads atom content through the `AtomSource`
trait and MUST NOT implement its own atom fetching or ingestion
logic (see `[boundary-L3-concerns]`). Verification of atom
integrity is the atom protocol's responsibility at ingestion
time — eos trusts atoms resolved from its `AtomSource`.

**Scheduler seam:** The scheduler MUST route all artifact-existence
queries through the `eos-core` `ArtifactStore` abstraction or the
Cap'n Proto `BuildJob` substitution surface — never through direct
snix gRPC calls in scheduler code. This preserves
`[boundary-contract-only]` at the scheduler boundary and is the
required interface pattern for cache-skip scans (see
[`eos-scheduler.md`](eos-scheduler.md)).

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
layer = "L3"
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

The following violations were identified as of the date of this
specification. Entries marked CLOSED have been remediated with
evidence; open entries require remediation.

### §6.1 — Lock types in `eos` instead of `ion`

**Violates:** `[boundary-lock-ownership]`, `[boundary-L3-concerns]`

**CLOSED** — `! test -f eos/eos/src/lock.rs` → exit 0.
`eos/eos/src/lock.rs` has been removed. All lock types (`LockFile`,
`Dependency`, `AtomDep`, `NixDep`, `NixGitDep`, `NixTarDep`,
`NixSrcDep`, `SetDetails`, `ComposeConfig`) now reside in
`ion/ion-lock/src/lib.rs`. `eos-core` carries `BuildRequest<D>` and
`FetchDescriptor` as the pre-fetch contract types
(`eos/eos-core/src/request.rs`). Cap'n Proto `submitBuild` accepts a
structured `BuildRequest` (`eos/eos-proto/schema/eos.capnp:83,92`).
The orchestrator takes pre-parsed `BuildRequest<Blake3Digest>`
(`eos/eos/src/orchestrator.rs:48`). `ion-eos` depends on `ion-lock`
(`ion/ion-eos/Cargo.toml:13`) and performs parse-and-translate before
any RPC call (`ion/ion-eos/src/lib.rs:274-382`).

### §6.2 — `ion-eos` ad-hoc TOML parsing

**Violates:** `[boundary-lock-ownership]` (indirectly)

**CLOSED** — `grep -q "ion_lock::LockFile::parse" ion/ion-eos/src/lib.rs` → exit 0.
`parse_and_translate` (`ion/ion-eos/src/lib.rs:274-282`) routes all
lock parsing through `ion_lock::LockFile::parse()` and `validate()`.
No ad-hoc TOML parsing of compose args remains; all fields including
`compose.args` are accessed via the structured `ion_lock::LockFile`
type returned by the parser.

### §6.3 — Eos receives raw lock content

**Violates:** `[boundary-L3-concerns]`

**CLOSED** — `grep -q "BuildRequest" eos/eos/src/orchestrator.rs` → exit 0.
`run_orchestrated_build()` (`eos/eos/src/orchestrator.rs:42-55`) now
accepts `request: &eos_core::request::BuildRequest<Blake3Digest>` —
structured `eos-core` types replace raw TOML. `ion-eos` calls
`parse_and_translate` to convert lock content into a `BuildRequest`
before any RPC call (`ion/ion-eos/src/lib.rs:274-382`). The
orchestrator's generic type parameters (`E: BuildEngine`, `S:
AtomSource`, `B: AtomContentBridge`, `I: ContentIngestService`)
eliminate snix-specific concrete types from the function signature.

### §6.4 — Eos daemon persists lock files to disk

**Violates:** `[boundary-L3-concerns]`

**CLOSED** — No lock directory configuration or filesystem lock-read
logic exists in `eos-daemon`. `eos-daemon/src/config.rs` defines no
lock-path CLI argument and no lock-directory resolver.
`eos-daemon/src/scheduler.rs` performs no lock file I/O. The daemon
receives `BuildRequest` via Cap'n Proto RPC exclusively. Check: `! rg
-ql 'lock.dir' eos/eos-daemon/src/` → empty output.

### §6.5 — Eos reimplements atom fetching

**Violates:** `[boundary-L3-concerns]`, `[boundary-L1-concerns]`

**Check:** `grep -n "curl\|fetch_git\|download_file\|extract_tarball" eos/eos/src/fetch.rs`

`eos/eos/src/fetch.rs` retains direct external-tool fetching for
non-atom external dependencies (`fetch_external`, lines 159–222):
`curl` for Nix/tarball/source deps and `git clone` for Nix-git deps.
`fetch_atom` (lines 224–259) correctly delegates atom resolution to
`AtomSource::resolve()` and bridge ingestion, eliminating the
atom-specific external fetch path. The remaining open scope is the
full composite `AtomSource` implementation (local store → registry →
ion peer) that routes all dependency resolution through
protocol-native abstractions.

Per the fetch-ownership split ([ADR-0005](../adr/0005-hermetic-transactional-composition.md)
§7, `[htc-fetch-set-lock-plugin]`): non-atom dependency fetching
(tarballs, git sources, …) is *declared* at L4 (ion, as lock
`[[deps]]` entries) and *executed* at L2 (HTC's record/replay
proxy) — eos owns neither end. `eos/eos/src/fetch.rs` performing
its own `curl`/`git clone` for these deps is pre-substrate residual
scope, not an intentional design placement; it is superseded by the
L4-declares/L2-executes split once HTC's fetch proxy lands (P3/P4).
The remaining violation is twofold: the composite `AtomSource`
pattern is incomplete, and this fetch path itself needs migration
off eos once the fetch proxy exists.

### §6.6 — `eos-daemon` depends on `atom-git` (L1 implementation crate)

**Violates:** `[boundary-impl-isolation]`, `[boundary-contract-only]`

**Check:** `grep -q "atom-git" eos/eos-daemon/Cargo.toml` → exit 0

`eos-daemon/Cargo.toml:11` declares a direct dependency on `atom-git`,
an L1 implementation crate. `eos-daemon/src/scheduler.rs:188-211`
uses `atom_git::GitSource::new()` to open the local workspace git
repository inside scheduled build tasks. `eos-daemon` is an L3
implementation crate; it MUST NOT depend on any L1 implementation
crate — this violates both `[boundary-impl-isolation]` (no L1 impl
in L3 impl) and `[boundary-contract-only]` (cross-workspace dep MUST
target a contract crate).

**Remediation path:** Campaign node P11. Introduce a `WorkspaceSource`
trait (or reuse `atom_core::AtomSource`) in `eos-core` and inject the
concrete `atom_git::GitSource` at the composition layer
(`eos-daemon/src/main.rs`). The scheduler receives the trait object;
the `atom-git` dependency moves to the composition entry point, not
the scheduler module.

---

## §7 — Decision Framework

When uncertainty arises about where a type or module belongs,
apply these questions in order:

1. **Does it appear in a cross-layer trait signature?**
   → It MUST live in the consuming layer's contract crate.

2. **Is it a serialization format specific to one frontend?**
   → It MUST live in that frontend's workspace (L4/L5).
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
| `[boundary-L2-concerns]`         | `UNVERIFIED`            | Cross-referenced with ADR-0005/htc-sad.md | New layer (2026-07-05); no crate workspace yet (P3/P4) |
| `[boundary-L3-concerns]`         | `VERIFIED: agent-check` | Cross-referenced with ADR-0001      | Amended: lock file removed from L3                |
| `[boundary-L4-concerns]`         | `VERIFIED: agent-check` | Cross-referenced with ADR-0001      | Amended: lock file format added to L4             |
| `[boundary-L5-concerns]`         | `VERIFIED: agent-check` | Deferred                            | Minimal constraints pending plugin maturity       |
| `[boundary-lock-ownership]`      | `VERIFIED: agent-check` | Cargo-atom test; format vs contract | §4.2 rationale                                    |
| `[boundary-bridge-crate]`        | `VERIFIED: agent-check` | `ion-eos` Cargo.toml verified       | Conforms                                          |
| `[boundary-store-separation]`    | `UNVERIFIED`            | Manual audit                        | New constraint; pending implementation            |
| `[boundary-ci-enforcement]`      | `UNVERIFIED`            | Script does not yet exist           | Pending implementation                            |
| `[boundary-crate-metadata]`      | `UNVERIFIED`            | Metadata not yet present            | Pending implementation                            |
| `[boundary-violation-budget]`    | `UNVERIFIED`            | File does not yet exist             | Pending implementation                            |
