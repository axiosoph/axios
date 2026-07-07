# ADR-0006: Execution as the Primitive — Actions, Trials, and the Death of the Evaluator

- **Status**: ACCEPTED
- **Date**: 2026-07-07
- **Deciders**: nrd
- **Normative elaboration**: [Execution Model](../models/execution-model.md)
- **Supersedes**: [ADR-0005](0005-hermetic-transactional-composition.md) §8
  (the optional passthrough-snix executor allowance) and §11 (the three
  materialization tiers, re-cut as orthogonal axes) |
  [ADR-0002](0002-decoupling-snix-backend.md) **wholesale** (§Tier 3 was
  already superseded by ADR-0005; Tiers 1/2/4 survived only as the
  optional snix executor's own spec — that executor is removed by this
  ADR, so their referent no longer exists)
- **Related**: [ADR-0005](0005-hermetic-transactional-composition.md),
  [HTC SAD](../architecture/htc-sad.md),
  [Eos SAD](../architecture/eos-sad.md),
  [Eos Scheduling Model](../models/eos-scheduling.md),
  [Publishing Stack Layers Model](../models/publishing-stack-layers.md)

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers

---

## Context

ADR-0005 established the Hermetic Transactional Composition substrate and
deleted the evaluation *stage* from the primary path, but it left two
things unfinished. First, it treated **build** as the substrate's
primitive operation, with observation (read-set capture) welded into the
build's materialization mechanism (a FUSE tier) — early implementation
work measured that welding at **~5.9× wall-clock overhead** on a real
openssl build versus bare metal, against **~25%** for ptrace+seccomp
syscall-level observation performing the equivalent capture. Second, it
allowed an "optional legacy passthrough-snix executor" as a Nix-interop
on-ramp, keeping evaluator-shaped doctrine (a third `BuildPlan` variant,
a `[compose]` lock section, an entire executor spec) alive in the corpus.

Meanwhile the formal work surfaced a deeper factorization: build, test,
fetch-discovery, and runtime-closure capture are not four mechanisms but
one operation under four *policies* — and the differences between them
(cacheable vs. not, gating vs. gated, rebuild-triggering vs. not) are
consequences of that stratification, not features to engineer separately.

## Decision

### 1. Execution is the primitive; build and test are policy strata [exec-primitive]

The substrate's single operation is
`execute(request, world) → record`, where a request is
`(view: composition, command: opaque argv, outputs, policy)` and the
policy assigns each ambient channel (network, clock, entropy, …) one of
`closed | pinned | open`. Requests stratify **by policy, never by
workload kind**:

- an **action** (no channel open) is world-independent; its **record**
  is a cacheable fact. A build is an action. A hermetic test is an
  action and caches like one.
- a **trial** (some channel open) is world-dependent; its record is an
  **attestation** — signed evidence, never a cache value. A networked
  test is a trial. Record-mode fetch discovery is a trial.

Canonical terms: **action / trial**, **record / attestation**. The
executor never interprets `command` — there is no interpreted language in
the trusted core. Full semantics, laws, identity discipline
(`action_id` / `req_digest` / record czd), and proof obligations P1–P7:
the [Execution Model](../models/execution-model.md).

Consequences that are theorems of the stratification rather than
features: test parameters occur only in test requests, so **editing test
configuration can never trigger a rebuild**; a failed trial gates a build
fact's *advertisement*, never its existence, so **a sandbox-hostile test
never forces a rebuild of a good build**; fetch discovery re-enters the
deterministic stratum only through **promotion into signed intent**
(lock entries) — or is bypassed entirely for ecosystems whose own
lockfiles (Cargo.lock et al.) ship inside the atom's sources and are
adopted directly as the pinned fetch-set via a per-ecosystem proxy
adapter.

### 2. Materialization and observation are orthogonal axes, not tiers [exec-axes-not-tiers]

ADR-0005 §11's Observe/Fast/Export "materialization tiers" conflated two
independent choices: *how a view is mounted* (bind mounts, composefs/
EROFS+fs-verity, export copies) and *whether file access is observed*.
"Observe" as a mount tier dissolves. Observation is an execution-policy
axis (`observe: none | trace`), applied to whichever materialization is
in use.

**ptrace(+seccomp filtering) is the ratified observation instrument** for
runtime-closure capture — measured at ~25% overhead with zero privilege
requirements inside the production rootless sandbox, versus FUSE-based
observation's measured 5.9×. This reverses an earlier design preference
for FUSE observation (chosen for requiring "zero ptrace/seccomp
machinery") on cost evidence. Sequencing rule: the FUSE read-set
machinery is removed from the snix fork only after the ptrace observer
proves read-coverage at openssl scale (`io_uring` and direct-syscall
surfaces included); observation-instrument coverage is an empirical
property recorded with every observation.

### 3. The evaluator is dead — entirely [exec-no-evaluator]

The "optional legacy passthrough-snix executor" is removed from the
design. It was never implemented; the system has no users; maintaining
doctrine, schemas, and wire surface for a superseded model inflates
implementation burden in exchange for nothing. This project asserts,
without a compatibility hedge, that **the evaluator is unnecessary**:

- The passthrough executor's spec (`eos-snix-backend.md`) is deleted.
- The third `BuildPlan` variant (`NeedsEvaluation`) and the
  `Plan = Derivation` binding disappear from the engine's model; the
  two-variant coproduct (`Cached | NeedsBuild`) stands alone.
- The lock's `[compose]` section (composer selection, `NixTrivial`,
  `[compose.args]`) is removed — a composer has no meaning without an
  evaluator. The successor intent schema (action params, test params,
  intent kinds) is the manifest/lock redesign, deliberately **not**
  performed by this ADR (see Consequences).
- The `eos-snix` crate is slated for removal as implementation cleanup;
  asserting the direction is this ADR's act, deleting code is not.
- ADR-0002's residual tiers lose their referent and the ADR is
  superseded wholesale; ADR-0003's deployment modes (embedded / daemon /
  distributed) are **unaffected** — they describe eos generally, not the
  dead executor.

### 4. Records accumulate; there is no canonical witness [exec-witness-accumulation]

Build records are signed facts accumulating in atom metadata — several
distinct output digests for one action, from several builders, is a
legitimate and useful state ("a trusted key has seen this action produce
these three hashes"). A cache hit is *∃ a witness acceptable under the
consumer's trust anchors*; the executor neither knows nor cares that
other witnesses exist. Downstream coherence requires no tie-break:
consumers bind the concrete output digests of whichever witness they
consumed, and witness selection at request formation is a recorded
choice over the fact snapshot. Witness multiplicity is surfaced as
reproducibility evidence, never reconciled away. (This corrects the
"concurrent identical builds produce identical content" justification
previously given in the eos scheduling model's scope note; the dispatch
design itself is unchanged.)

### 5. Runtime-only dependencies are real [exec-runtime-only-deps]

The runtime composition is **not** a subset of the build closure. Nix's
runtime ⊆ build containment was an accident of its discovery mechanism
(hash-scanning output bytes can only find paths present at build time) —
an accident that forces rebuilds to change a CA bundle and makes the GPU
driver an impurity hack. This substrate deliberately does not inherit
it: containment is per-view and structural (a process cannot read
outside its own view), while runtime-only dependencies (CA certificates,
drivers, computed plugin loads) are supplied by the runtime composition,
justified by declared bindings, and grown by fail-closed closure faults
feeding monotone refinement.

## Consequences

### Positive

- One executor and one policy surface replace four bespoke mechanisms
  (build sandbox, test harness, fetch recorder, closure tracer).
- Hermetic tests become cacheable facts; networked tests stop poisoning
  builds; test-config edits stop triggering rebuilds — each structurally.
- The evaluator's removal deletes an entire spec, a wire surface's
  design burden, a lock section, and a standing three-variant special
  case from the formal models.
- Build-time observation overhead drops out of the default path
  entirely; observation is paid only by traced executions.

### Negative / burden

- The snix fork obligation is unchanged (castore + build daemon —
  neither is the evaluator), but the fork's FUSE read-set machinery
  becomes scheduled-for-removal debt, gated on ptrace coverage evidence.
- The lock and manifest lose `[compose]` with no successor in this ADR:
  **the manifest/lock redesign is now the substrate's most important
  open design surface** — action params, test/check params, the intent
  kind discriminator (package / environment / generator), fetch-entry
  generalization (consumer-side requires edges), and adopted-lockfile
  entries all need a home. Tracked as follow-up design work, not
  reconciliation.
- The execution model's proof obligations (P1–P7) stand as the
  verification queue; P7 (resolve determinism + materializer versioning
  + totality) gates the two-level cache-coherence claims.

### Neutral

- The scheduler's verified theory is unaffected: the Lean bounds are
  node-agnostic and transfer; the scheduling-model scope note's
  *justification* wording is amended (witnesses accumulate), not its
  design.
