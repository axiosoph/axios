# CHARTER: Decentralized Publishing Stack

<!--
  CHARTER documents are strategic framing artifacts.
  They define purpose and priorities, not solutions.

  Explore in sketches; frame in the charter.

  See: workflows/charter.md for the full protocol specification.
-->

## Purpose

Package publishing today forces a false choice. Traditional registries
(crates.io, npm) own naming and discovery but know nothing about
reproducible builds. Nix achieves reproducibility but couples identity
to a single monorepo (nixpkgs) and a single evaluation model, making
ecosystem-agnostic publishing structurally impossible.

The people who have this problem are package authors who want
content-addressed, cryptographically verifiable publishing without
surrendering identity to a central authority — and consumers who want
reproducible artifacts without accepting one registry's metadata format,
version policy, or evaluation engine as prerequisite.

The eka codebase proved these concepts work. It also proved that
coupling identity, evaluation, and user interaction into one crate makes
each concern unreformable. This initiative decomposes those concerns into
three independent projects — atom, eos, and ion — each with the
architectural boundaries needed to evolve independently.

## North Star

A developer publishes a package from any ecosystem — Cargo, npm, C, or
something that doesn't yet exist — via a single, ecosystem-agnostic
transaction. That transaction binds a human-readable label to a
content-addressed identity verifiable from source, with no central
registry controlling the namespace.

A consumer resolves and builds that package with cryptographic cache
skipping at every stage of the chain:

```
AtomId → Version → Revision → Plan → Output
```

If the plan exists, skip evaluation. If the artifact exists, skip the
build. The full dependency graph is content-addressed end to end, and
every step is auditable by anyone with access to the source repository.

The runtime engine, manifest format, and version scheme are all
pluggable — the only decisions baked in are the ones that _must_ be.

## Prerequisites

The existing [ion-atom-restructuring plan](../plans/ion-atom-restructuring.md)
predates this charter. Its Phase 1 (monorepo scaffold) is a mechanical
prerequisite: creating workspace directories, skeleton crates, and
inter-workspace path dependencies. This phase executes directly from the
existing plan before the charter's workstreams begin.

Phase 1 makes no trait design decisions — it creates the containers
that subsequent workstreams populate.

## Workstreams

<!--
  Ordered priorities. Each workstream spawns an independent sketch→plan→core cycle.
  A workstream should be independently explorable — if it's too large, it's a sub-charter;
  if it's too small, it's a plan item.
-->

1. **Formal Layer Model** — Formalize the trait boundaries between atom, eos, and ion using the SDMA toolkit before implementation begins. Surface structural issues that prose descriptions miss.
   - Spawns: `/model` cycle → `docs/models/publishing-stack-layers.md`
   - Status: **Complete**

2. **Atom Protocol Library** — Establish the protocol foundation: identity primitives (atom-id), URI parsing (atom-uri), trait surface (atom-core), and the git bridge (atom-git). Everything else depends on this.
   - Spawns: `.sketches/atom-protocol.md`
   - Draws from: existing plan Phases 2–5, informed by the formal model
   - Specification: `docs/specs/atom-transactions.md` (40 constraints, 13 machine-verified)
   - Status: **In Progress** — specification complete, implementation pending

3. **Eos Runtime Engine** — Extract the evaluation/build/store layer: `BuildEngine` plan/apply, `ArtifactStore`, and snix integration. Decouples the build executor from the user frontend.
   - Spawns: `.sketches/eos-runtime.md`
   - Draws from: existing plan Phases 6–8, informed by the formal model
   - Status: Not Started

4. **Ion Frontend** — Build the user-facing layer: CLI, concrete `ion.toml` manifest, SAT resolver, and workspace coordination. Ion is the planner; it decides what to build and dispatches to eos.
   - Spawns: `.sketches/ion-frontend.md`
   - Draws from: existing plan Phases 9–11, informed by the formal model
   - Status: Not Started

5. **Integration** — End-to-end validation across all three workspaces: the full data flow from manifest through resolution, ingestion, planning, building, to artifact. Cryptographic chain verification.
   - Spawns: `.sketches/stack-integration.md`
   - Draws from: existing plan Phase 12
   - Status: Not Started

Implementation workstreams (2–4) draw their phase definitions from the
existing plan. If the formal model (workstream 1) reveals issues with
the plan's trait designs, those workstreams spawn new focused
sketch→plan cycles rather than executing stale assumptions.

## Non-Goals

<!--
  Strategic exclusions — what this initiative deliberately won't address and WHY.
  These are different from plan-level non-goals (which are tactical and phase-scoped).
  A good strategic non-goal explains the reasoning, not just the exclusion.
-->

- **Cyphrpass integration.** Cyphrpass will eventually own identity, signing, and storage beneath atom. But its API surface isn't stable, and designing against a moving target wastes effort. This initiative designs _seams_ for Cyphrpass (trait boundaries, transaction-centric vocabulary) without building the integration.

- **Dynamic plugin runtime.** Ion may someday support user-installable plugins for CLI extension. No concrete use case exists yet. Runtime backends (nix, snix) are architectural boundaries handled by `BuildEngine`, not plugins.

- **Distributed eos.** `BuildEngine` and `ArtifactStore` are designed for a future `RemoteEngine`. But building the distributed engine — scheduling, multi-node coordination, binary cache negotiation — is a separate initiative. This stack ships with an embedded engine for single-user development.

- **Full eka feature parity.** This is a port of proven concepts, not a 1:1 reimplementation. Some eka features may not survive the restructuring.

- **Backend-agnostic transport specification.** The atom transaction protocol is backend-agnostic by design — traits define the extension surface. Specifying abstract transport or storage semantics beyond the trait signatures is deferred until a non-git backend surfaces concrete requirements.

- **Cross-ecosystem adapters.** Concrete adapters for Cargo, npm, or other ecosystems arrive when concrete ecosystems want atom. The `Manifest` and `VersionScheme` traits provide the extension surface.

## Appetite

A major, multi-month commitment with sustained attention. This is the
foundational rearchitecture of a 2-year-old project.

The formal model (workstream 1) and atom protocol (workstream 2) are the
highest-value, lowest-risk work. The model grounds the architecture
formally; atom stands alone as a publishable library. Eos and ion are
larger and will take longer, but each layer is independently valuable —
partial completion is acceptable; partial coupling is not.

If any workstream exceeds its expected scope, descope rather than push
harder.

## References

<!--
  Link to downstream plans, sketches, and ADRs spawned by this charter.
  This section grows as workstreams progress through the pipeline.
-->

### Prior Art

The following artifacts were produced _before_ this charter existed. They
established the technical architecture that this charter now frames
strategically:

- Sketch: [ion-atom-restructuring](../../.sketches/2026-02-07-ion-atom-restructuring.md) (10 challenge iterations)
- Plan: [ion-atom-restructuring](../plans/ion-atom-restructuring.md) (12-phase execution blueprint)
- ADR: [0001-monorepo-workspace-architecture](../adr/0001-monorepo-workspace-architecture.md)
- Charter sketch: [charter-framing](../../.sketches/2026-02-15-charter-framing.md)

### Downstream Artifacts

- Model: [publishing-stack-layers](../models/publishing-stack-layers.md) (olog + coalgebras + session types)
- Spec: [atom-transactions](../specs/atom-transactions.md) (40 constraints, BCP 14)
- TLA+: [AtomTransactions](../specs/tla/AtomTransactions.tla) (temporal safety, 2 configs)
- Alloy: [atom_structure](../specs/alloy/atom_structure.als) (structural assertions, scope 4)
- Sketch: [atom-protocol-plan](../../.sketches/2026-02-15-atom-protocol-plan.md)
- Plan: [atom-protocol-library](../plans/atom-protocol-library.md)
