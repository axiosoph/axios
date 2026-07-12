# Roadmap

Axios is building a decentralized publishing and build substrate around
composition-addressing: signed, content-addressed bindings of names to
content, applied recursively from published sources to runtime closures —
see [README.md](README.md) for what that means and the current
architecture. This document tracks the plan to a working MVP: what each
milestone delivers, what depends on what, and what is deliberately out of
scope.

This is a living document. See [Document maintenance](#document-maintenance)
for how it stays current.

## Guiding principles

- **Design seams, not implementations.** Trait boundaries (executor,
  digest, anchor, storage backend) absorb future change rather than
  hard-coding today's choice.
- **Cache-skipping is the value proposition.** Every stage of the
  cryptographic chain — publish digest → action id → build record →
  composition root — must be independently verifiable and skippable.
- **Layer discipline is enforced, not aspirational.** A higher layer never
  imports a lower layer's implementation; violations are architectural
  bugs, not style preferences.
- **Specs and proofs lead; implementation follows.** Where a specification
  or formal model exists, it is the normative reference. Rust code that
  predates or contradicts it is corrected or replaced, not the other way
  around.

The decisions behind these principles are recorded in the project's
[Architecture Decision Records](docs/adr/): notably
[ADR-0001](docs/adr/0001-monorepo-workspace-architecture.md) (workspace
architecture), [ADR-0005](docs/adr/0005-hermetic-transactional-composition.md)
(the post-Nix composition substrate), and
[ADR-0006](docs/adr/0006-execution-as-the-primitive.md) (execution as the
substrate's dynamic primitive). The six-layer architecture they establish is
summarized in [README.md](README.md#architecture) and formalized in
[docs/models/publishing-stack-layers.md](docs/models/publishing-stack-layers.md).

## Milestones at a glance

Milestones are labeled **M0–M6**. (Some design and implementation-readiness
material elsewhere uses "Phase" for a different, finer-grained sequencing
concept scoped inside a single milestone — this document always says
"milestone," never "phase," to keep the two vocabularies from colliding.)

| Milestone                                  | Focus                                                                  | Status      | Depends on |
| :----------------------------------------- | :--------------------------------------------------------------------- | :---------- | :--------- |
| [M0](#m0--doctrine-the-substrate-decision) | Doctrine: composition-substrate ADR + eos re-scope decision            | Done        | —          |
| [M1](#m1--atom-stabilization)              | Atom protocol conformance and hardening                                | Not started | M0         |
| [M2](#m2--ion-extraction)                  | Ion frontend extraction from prototype code                            | Partial     | M1         |
| [M3](#m3--hermetic-fhs-builder)            | Hermetic build execution over an atom-DAG closure                      | Not started | M0         |
| [M4](#m4--analyzers--composer)             | Interface analysis and composition                                     | Not started | M3         |
| [M5](#m5--eos-mvp-on-the-atom-dag)         | Eos re-scoped as an atom-DAG scheduler                                 | Not started | M1, M0     |
| [M6](#m6--mvp-integration)                 | End-to-end MVP: add → resolve → lock → build → analyze → compose → run | Not started | M1–M5      |

M1→M2→M5 and M3→M4 are two largely independent tracks that converge at M6.
Estimated overall effort to MVP is roughly 4–6 months of focused work,
assuming both tracks proceed concurrently.

## M0 — Doctrine: the substrate decision

**Goal:** Decide the post-Nix build-execution architecture and record it as
binding doctrine, since every later milestone depends on this shape.

**Status: Done.** [ADR-0005](docs/adr/0005-hermetic-transactional-composition.md)
introduced the L2 "Hermetic Transactional Composition" (HTC) layer — a pure
content-addressed store, a signed composition object, and a mounted view,
replacing Nix's input-addressed store path. It also deleted the evaluation
stage from the primary path: eos schedules a DAG of atoms, not derivations
produced by an evaluator, and the executor (snix or a successor) sits behind
a trait rather than being an assumed backend.
[ADR-0006](docs/adr/0006-execution-as-the-primitive.md) refined this
further: execution — not build specifically — is the substrate's dynamic
primitive, with build, test, fetch-discovery, and runtime-closure capture
factored as policy variants of one `execute(request, world) → record`
operation, and it withdrew the "optional legacy passthrough" executor
entirely — there is no evaluator anywhere in the design, not even as a
compatibility fallback. The [HTC Software Architecture Document](docs/architecture/htc-sad.md)
elaborates both decisions in full.

## M1 — Atom stabilization

**Goal:** Lock the atom protocol layer (identity, addressing, publishing)
down against its specifications, and harden the signed-metadata append path
that later milestones depend on for facts (build records, interface
manifests).

**Status: Not started as an implementation effort.** A formal review of the
atom protocol's identity model, completed in July 2026, found that several
implementation bugs traced back to one root cause: conflating the protocol's
own content digests with the storage backend's content identifiers (for
example, a git object ID). That review produced concrete groundwork this
milestone consumes directly:

- The atom layer's formal foundations, since landed as committed documents
  this milestone consumes directly: the atom protocol-plane model
  ([docs/models/atom-model.md](docs/models/atom-model.md) — identity,
  metadata placement, signature anchoring, and the declared reproducibility
  mode) and the atom backend contract
  ([docs/specs/atom-backend-contract.md](docs/specs/atom-backend-contract.md)
  — the "typed seam" law and thirteen further obligations any storage
  backend must satisfy, with an honest per-obligation gap table for the
  git backend).
- Charter conformance test corpora with a documented violation inventory
  (the lock schema v2 work from the same review belongs to the ion
  milestone below).
- An extended TLA+/Alloy formalization of the atom charter transaction
  protocol (13 additional constraints).

**Scope:**

- Conformance-test the `atom-git` and `atom-core` crates against the current
  specifications: the pair-only `AtomId`, two-value lock entries, blake3-of-
  publish-digest store keying, name-anchored acquisition with moved-tip
  warning semantics, and the publish-time reproducibility mode field.
- Build the backend-conformance battery: an executable test suite that
  checks a storage backend against every obligation of the
  [atom backend contract](docs/specs/atom-backend-contract.md), turning the
  contract's per-obligation discharge claims into machine-checked verdicts.
  Its first and primary subject is the git backend itself — the contract's
  gap table enumerates exactly the rows it must close. This battery is an
  MVP acceptance artifact, and it is what a future non-git backend runs to
  earn conformance.
- Design the trust-model / acceptance-policy specification: trust-anchor
  configuration, standing divergence findings and their adjudication exits,
  and cache-service refusal hooks. Deliberately promoted from post-MVP to
  MVP scope — trust is a first-class part of the model, not an afterthought,
  and the MVP must demonstrate it. The atom-side semantics (the declared
  reproducibility mode and its violation classification) are already fixed
  by the protocol-plane model; this spec supplies the consumer-side policy
  language the execution model delegates.
- Open the storage/ingest layer early rather than late — the identity/backend
  conflation above was systemic, and `GitStore::ingest`'s current fix is
  known-incomplete (tracked in [issue #64](https://github.com/axiosoph/axios/issues/64)).
  Deferring this work risks rediscovering the same conflation a second time,
  later and more expensively.
- Harden signed-metadata append: builder-versus-claim-owner signer
  authorization, and a fact-append path distinct from the moved-tip warning
  that currently fires on every routine append.
- Derive property tests from the Alloy/TLA+ models; cut and tag a v0; freeze
  wire formats.

**Open items:**

- ~~Whether this milestone's formalization scope should explicitly include
  closure-root identity~~ — resolved by the protocol-plane model: an atom's
  dependency closure is a *projection* of its signed content (the lock
  determines it), not an independent identity-bearing object, so there is
  no separate closure-root identity to formalize; the closure root exists
  only as an input to action identity, exactly as the execution model
  already defines it.
- The git backend's storage encoding and ref layout for charter
  transactions — the signed founding declaration whose content digest is
  the atom-set's anchor — is not yet specified
  ([docs/specs/git-storage-format.md](docs/specs/git-storage-format.md),
  Open Questions #6). This is design work this milestone owns.
- Conformance test fixtures must span the digest algorithm-length axis
  (32/48/64-byte digests, i.e. ES256 vs. ES384 vs. ES512/Ed25519) rather than
  only the 32-byte case — a prior digest bug survived specifically because
  every existing fixture was 32 bytes.

**Estimated effort:** 3–6 weeks once started; conformance work against an
existing specification is comparatively low risk, and the added
trust-model specification is design work with its atom-side semantics
already fixed.

## M2 — Ion extraction

**Goal:** Lift the dependency-resolution and lock-file code proven out in
earlier prototype work into clean, specification-conformant crates. Ion is
the stack's user-facing **system compositor** (see
[README.md](README.md)); this milestone gives it a real body.

**Status: Partial.** Lock schema v2 types and a canonical encoder stub are
defined, along with a conformance corpus and validator. Extraction of the
resolution and locking algorithm itself into the `ion-manifest`,
`ion-resolve`, `ion-lock`, and `ion-eos` crates has not started.

**Scope:**

- Extract the proven resolution/lock logic into the crates above.
- Implement the lock plugin mechanism (an extension point for dependency
  types beyond atoms, such as pinned upstream fetches), with a trivial
  plugin type to prove the mechanism out.
- Validate that the `ion-eos` handoff to the build engine matches the
  minimal-pointer design in the Ion Software Architecture Document.
- Begin the manifest/lock intent redesign that
  [ADR-0006](docs/adr/0006-execution-as-the-primitive.md) names the
  substrate's most important open design surface: build-action and test
  parameters, an intent-kind discriminator, generalized fetch entries,
  and adopted upstream lockfiles all need a schema home after the removal
  of the evaluator-era composition surface. Full resolution spans M4/M5;
  the schema seam is owned here.

**Open item:** Ion's remaining open questions are mostly resolved by the
lock-file formalization work above, but its user-facing design — CLI
ergonomics, manifest authoring experience, error reporting — is a distinct
design effort, not formalization, and needs deliberate design attention
before this milestone's implementation begins. Ion is the entry point to the
whole stack for a new user, so this is treated as a scheduled prerequisite,
not an afterthought.

**Estimated effort:** 3–6 weeks; extraction plus API design is low-to-medium
risk.

## M3 — Hermetic FHS builder

**Goal:** Hermetic build execution over an atom-DAG closure — the
substrate's execution engine, built from production-tested components
rather than from scratch.

**Status: Not started.**

**Scope:**

- Integrate `castore` for blob and directory (Merkle tree) storage.
- Runtime-closure observation via ptrace-based syscall tracing (the
  instrument [ADR-0006](docs/adr/0006-execution-as-the-primitive.md)
  ratified after measuring roughly 25% wall-clock overhead against a
  FUSE-based approach's roughly 5.9×), applied as an execution-policy axis
  rather than a separate materialization tier.
- The FHS-view delta on the OCI build executor: mount the composed input
  tree as the process's actual root filesystem, not as a subdirectory the
  build has to be told about.
- A fetch proxy (HTTP(S) CONNECT with proxy CA injection, plus a git
  handler) with record and replay modes, so network fetches become a pinned,
  replayable part of the lock rather than an unpinned side channel.

**Exit demo:** build `curl`, `zlib`, and `cpython` from unmodified upstream
release tarballs, hermetically, with signed input closures and logged read
sets. This is the artifact that demonstrates what a Nix-based system cannot
do without patching.

**Estimated effort:** 4–8 weeks; medium risk (TLS interception friction in
the fetch proxy, and the licensing seam between this substrate and the
`snix` components it builds on).

## M4 — Analyzers + composer

**Goal:** Turn build outputs into composable, analyzable artifacts.

**Status: Not started.**

**Scope:**

- The composition object itself: format, Merkle root computation, signing,
  merge and conflict handling (a conflict is an explicit error, never a
  silent pick), and `composefs` mount emission.
- An ELF interface analyzer (provides/requires extraction, comparable in
  spirit to `rpm-elfdeps`).
- A Python interface analyzer (package-metadata provides, `ast`-derived
  import requires, plus check-phase observation).
- A namespace registry for analyzer-produced facts.
- The runtime closure computer: the satisfaction fixpoint, minimization, and
  closure-fault semantics that replace Nix's grep-based closure discovery.
- The lock's `fetch` plugin type, consuming M2's plugin mechanism and M3's
  fetch-proxy output.

**Exit demo:** swap an ABI-compatible OpenSSL into an existing composition —
a recorded satisfaction proof, a new composition root, and zero rebuilds.

**Estimated effort:** 4–8 weeks; medium risk — this is largely novel code
with no direct production analogue to build against.

## M5 — Eos MVP on the atom-DAG

**Goal:** The re-scoped build-scheduling engine, operating on atoms instead
of derivations.

**Status: Not started.** The scheduling theory itself (Graham/PEFT dispatch,
delay-cost fairness) is proven in TLA+ and Lean and is node-agnostic — it
transfers to atom-DAG scheduling unchanged. The current Rust implementation
predates the atom-DAG re-scope and is treated as a throwaway scaffold, not a
starting point.

**Scope:**

- Implement the eos daemon against the re-scoped specifications: atom-DAG
  intake from the ion handoff, and action identity computed from the atom
  closure, toolchain root, and action parameters.
- Wire the proven scheduling discipline to atom-granularity nodes.
- Build records as appended atom metadata.
- The executor trait, with the M3 hermetic FHS executor as the primary
  implementation.
- Re-validate scheduling behavior at atom granularity using the existing
  simulator and trace corpus.

**Open item:** a dependency-pin sweep across this project's own build
manifests should land before this milestone starts — a hermetic-build
project whose own dependencies aren't reproducibly pinned is an avoidable
irony, and one instance of exactly this problem (a floating dependency
version) was already found and fixed in mid-2026.

**Estimated effort:** 6–10 weeks; the largest single implementation
milestone, but guided throughout by an existing proof.

## M6 — MVP integration

**Goal:** The full vertical slice working end to end on a real project.

**Status: Not started.**

**Scope:**

- The complete flow: `add` → resolve → lock → build → analyze → compose →
  `run`, exercised against a small real project with a handful of upstream
  dependencies.
- The trust story, demonstrated end to end — the MVP is a substantial
  representation of the model, and trust is deliberately first-class in it:
  - A zero-trust round trip: publish an atom, then resolve and fully verify
    it (charter → claim → publish chain) from nothing but the source and
    the consumer's own trust anchors.
  - The backend-conformance battery (from the atom-stabilization milestone)
    green against the git backend.
  - A divergence alarm: an atom published under the declared-reproducible
    mode receives a conflicting build record from a trusted builder; the
    contradiction stands as a visible finding, cache service for the
    affected action is refusable by policy, and the adjudication exits are
    observable on the atom's own chain.
- Seed one upstream toolchain so the pipeline has something concrete to
  build with.
- Golden-path documentation written for a reader with no prior context on
  this project, and tested against that claim directly.

**Estimated effort:** 4–6 weeks; the seams it integrates are already typed
by the time this milestone starts, which is what keeps the risk low-to-
medium rather than high. The trust demonstrations add scope but not
novelty — they exercise machinery the earlier milestones build.

## Post-MVP horizon

Named for direction, not scheduled. These are real intentions, not
commitments with a timeline:

- Ingestion at scale (existing distribution and language-ecosystem package
  sets — Fedora, Debian, PyPI, and similar).
- DWARF-level (type-level) ABI satisfaction checking, beyond the symbol/
  version-level checking the MVP ships with.
- Canonical, append-only OS-layer compositions with generation switching
  (the territory `bootc`/OSTree-style systems occupy today).
- macOS and Windows executors.
- A capability-runtime (WASI) execution tier.
- Finer-than-atom action refinement inside eos, for cases where whole-atom
  granularity is too coarse.

## Non-goals

Strategic exclusions — deliberate, and each for a stated reason:

- **Cyphr integration.** The atom protocol is designed to eventually hand
  identity, signing, and storage to a cryptographic substrate ("Cyphr"),
  but its API isn't stable yet, and designing against a moving target
  wastes effort. This project designs the trait seams for that migration
  without building the integration.
- **A dynamic plugin runtime.** Ion may eventually support user-installable
  CLI plugins; no concrete use case justifies building that mechanism yet.
  Runtime backend selection is already handled by the executor trait, which
  is not the same problem as a plugin runtime.
- **A distributed build engine.** The build-engine and artifact-store traits
  are designed to admit a future remote/distributed implementation, but
  building that implementation — multi-node coordination, binary-cache
  negotiation — is a separate initiative. The MVP ships a single-process
  deployment using the same traits and scheduling logic a distributed
  engine would use.
- **Full feature parity with earlier prototype code.** This project ports
  proven concepts and formalized designs forward; it is not a 1:1
  reimplementation of exploratory code that preceded the current
  architecture, and some earlier functionality may not survive the
  restructuring.
- **A second (non-git) backend, and transport specification beyond it.**
  The storage-semantics half of backend agnosticism is no longer deferred:
  the [atom backend contract](docs/specs/atom-backend-contract.md) now
  specifies what any backend must provide, and the conformance battery
  makes it checkable. What remains deferred is *building* a second backend,
  and any wire/transport-protocol specification beyond what the contract
  and traits already require — both wait until a concrete non-git backend
  effort surfaces real requirements.
- **Cross-ecosystem adapters beyond the extension traits.** Concrete
  adapters for Cargo, npm, or other package ecosystems arrive when a
  concrete ecosystem needs one; the manifest and version-scheme traits are
  the extension surface, and building adapters ahead of demand is
  speculative work this project avoids.
- **Host system integration for the composition substrate.** The build
  substrate produces composition objects and the views built from them;
  owning host-level system integration on top of that (the way `bootc` or
  OSTree own a running system's generations) is a separate initiative.
- **Non-Linux executors.** The build substrate's first version depends on
  Linux-specific mechanisms (user namespaces, overlay filesystems,
  `fs-verity`). macOS and Windows executors are a real future need, but a
  distinct, post-MVP effort.

## Document maintenance

This roadmap reflects the plan and status as of the date of the most recent
commit that touched it — check `git log ROADMAP.md` for that date and for
the history of how the plan changed. It is updated whenever a milestone's
status changes, a dependency is added or resolved, or a decision recorded in
an ADR changes what a milestone's scope means. It does not track task-level
or week-level progress; for that, consult the repository's issue tracker and
commit history directly.
