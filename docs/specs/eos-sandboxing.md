# SPEC: Executor Sandbox Contract (Primary FHS Executor)

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  Historical note: prior to the ADR-0005 re-scope (2026-07-05), this document
  specified two separate sandboxing regimes — a confinement model for the
  since-deleted evaluation stage (Snix's language-level, encapsulation-boundary
  guarantee) and a build-execution model delegated to the snix builder. The
  first half is deleted, not amended: ADR-0005 [htc-atom-dag-executor-trait]
  (§6) removes the evaluation stage from the MVP's design surface entirely,
  so there is nothing left of that regime to specify. This document now
  specifies the primary executor's build sandbox contract in full, grounded
  in htc-sad.md — the layer that actually owns sandboxing, per the note
  below.
-->

## Domain

**Problem Domain:** The build sandbox is the security boundary that makes
hermetic composition possible: the only bytes a build process can read are
those materialized from a declared atom closure and toolchain composition,
and the only network path (if any) is a content-addressing fetch proxy. This
document specifies that contract for the **primary FHS executor** — the
default implementation of HTC's executor trait (htc-sad.md §3.5).

**Ownership note.** Though this file resides among the historical `eos-*`
spec set, the sandbox contract it specifies is owned by **L2 (HTC)**, not
L3 (eos) — `[boundary-L2-concerns]` (layer-boundaries.md §4.1) assigns "the
build function and sandboxed execution" to HTC; eos dispatches through the
executor trait and performs **zero** sandboxing of its own
(htc-sad.md §1.3's Execution boundary row; eos-sad.md §6.2, §6.4). This
document is the L2-owned contract eos's own spec cross-references rather
than restates.

Under the atom-DAG/executor-trait architecture
([ADR-0005](../adr/0005-hermetic-transactional-composition.md),
[htc-sad.md](../architecture/htc-sad.md)), sandboxing responsibilities are:

1. **FHS-view materialization**: the primary executor mounts a *composed*
   input tree — the atom closure plus toolchain composition, merged — as
   the build's rootfs, not a set of per-input directories. Inputs are
   read-only and digest-verified by the CAS.
2. **Network containment**: the sandbox has no network access except
   through HTC's record/replay fetch proxy, which turns network access into
   a pure function of previously-recorded (or freshly-recording) content.
3. **Read-set capture**: the castore FUSE daemon that materializes the FHS
   view is also the unbypassable observation point for what the build
   actually read, feeding both an enforcement check (reads ⊆ declared) and
   a provenance artifact (the `BuildRecord`'s read-set digest).
4. **Materialization tiers**: the same composition object is realized at one
   of three tiers depending on use (Observe for builds/checks, Fast for
   production runtime, Export for interop) — this document's sandbox
   concern is exclusively the Observe tier; §Materialization Tiers below
   scopes the other two only enough to disambiguate.

The Eos daemon (scheduler) has zero executor-implementation dependencies and
performs no sandboxing itself (eos-sad.md §2.1, §6.2). It dispatches build
actions to executor workers; which executor a given worker wraps — the
primary FHS executor (this document) or the optional legacy passthrough-snix
executor ([eos-snix-backend.md](eos-snix-backend.md)) — is opaque to the
scheduler.

**Model Reference:**

- [htc-sad.md](../architecture/htc-sad.md) — §1.1 (`[htc-declared-closure-enforced]`),
  §2 (Container View), §4 (Build Pipeline, Fetch Record/Replay), §5
  (Materialization and Runtime), §6.3 (Trace Observer)
- [ADR-0005](../adr/0005-hermetic-transactional-composition.md) — §6
  (`[htc-atom-dag-executor-trait]`), §7 (`[htc-fetch-set-lock-plugin]`), §11
  (`[htc-materialization-tiers]`)
- [eos-sad.md](../architecture/eos-sad.md) — §6.2 (Executor Isolation), §6.4
  (Build Sandboxing) — the L3-side restatement that this contract is wholly
  delegated, not performed, by the scheduler
- [eos-snix-backend.md](eos-snix-backend.md) — the optional legacy
  executor's own retained build/store mechanics (Derivation→BuildRequest
  conversion, platform sandbox dispatch); its Nix-expression evaluation
  capability is out of this document's scope (see that file's historical
  note)

**Criticality Tier:** High — build hermeticity governs the reproducibility
of the entire stack. Failure to contain build execution permits arbitrary
code execution with host-level access to bytes outside the declared closure.

---

## Constraints

### Sandbox Execution Model

The primary executor is one implementation of HTC's executor trait
(htc-sad.md §3.5): `build(atom_closure, toolchain_composition, action_params)
→ output tree`. Per action, the executor:

1. Materializes the composed atom closure and toolchain composition as a
   single FHS-view rootfs via the castore FUSE daemon (Observe tier,
   htc-sad.md §5.1) — reused, unmodified except for the read-set logging
   wrap (§6.3).
2. Runs upstream's own, unmodified build process inside an OCI/bwrap
   sandbox (`snix-build`, reused) against that rootfs.
3. Routes any network access exclusively through the fetch proxy; anything
   else is refused.
4. Ingests the output into the CAS, derives interface manifests, and signs
   a `BuildRecord` (htc-sad.md §2.3) whose `observed_read_set_digest` field
   this sandbox's read-set capture (below) populates.

Full pipeline diagram: htc-sad.md §2 (Container View), §4.1 (Build
Pipeline) — not reproduced here.

---

## Invariants

### FHS-View Materialization

**[eos-sandbox-fhs-materialization]**: The primary executor MUST mount the
*composed* atom closure and toolchain composition as a single FHS-view
rootfs (htc-sad.md §2, §4.1) — a merged tree at conventional FHS paths, not
a set of per-input directories addressed by an `inputs_dir` parameter (the
underlying `snix-build` gRPC surface's own layout convention,
[eos-snix-backend.md](eos-snix-backend.md) §Snix gRPC Build Protocol, which
this executor's composition-to-rootfs step sits above, not inside). Every
file and directory in the mounted view MUST be read-only and digest-verified
by the CAS at materialization time — a build process MUST NOT be able to
write into any part of the mounted closure.
`VERIFIED: unverified`

### Network Containment

**[eos-build-sandbox-network-containment]**: Build execution MUST NOT have
access to the external network, unless the sandbox routes that access
exclusively through HTC's content-addressing record/replay fetch proxy
(`[htc-fetch-set-lock-plugin]`, ADR-0005 §7; htc-sad.md §4.2). The proxy has
exactly two modes:

- **Record** (first build, explicitly impure and lock-writing): every
  response body becomes a CAS blob; every (normalized request → blob
  digest) tuple is written back into the lock, mechanically, like
  `cargo update`.
- **Replay** (every subsequent build, pure): the proxy serves only the
  recorded map. A request outside the recorded set MUST be refused and
  logged, never silently substituted or allowed through.

This supersedes the prior fixed-output-derivation (FOD) exception model —
there is no evaluator to produce a FOD marker; the recorded/replayed fetch
set is itself the exception mechanism, keyed by content rather than by a
derivation-level flag.
`VERIFIED: unverified`

### Read-Set Capture

**[eos-sandbox-read-set-capture]**: The castore FUSE daemon that
materializes the FHS view (§Sandbox Execution Model) MUST log `(path,
digest)` for every read of the composed closure during a build or
check-phase run — this daemon is the *only* source of those bytes, making it
an unbypassable observation point (htc-sad.md §6.3, the Trace Observer).
The resulting observed read-set MUST be a subset of the declared closure
(`[htc-declared-closure-enforced]`, htc-sad.md §1.1: reads ⊆ declared,
enforced by the sandbox, never merely trusted from the build's own
behavior). The read-set MUST be hashed and the digest MUST populate the
`BuildRecord.observed_read_set_digest` field (htc-sad.md §2.3) — this
document requires the executor capture and populate that field; it does not
redefine the `BuildRecord` schema itself.
`VERIFIED: unverified`

### Build Sandbox Delegation

**[eos-build-sandbox-delegation]**: Sandboxing is wholly the executor
implementation's responsibility, never the Eos daemon's or its scheduler's.
The Eos scheduler dispatches build actions to executor workers via Cap'n
Proto; the worker's executor-trait implementation (the primary FHS
executor, or the optional legacy passthrough-snix executor) applies
platform-appropriate sandboxing (OCI runtime or Bubblewrap, reusing
`snix-build`, htc-sad.md §2, §3.5). The daemon holds no opinion on how a
given executor achieves isolation (eos-sad.md §6.2).
`VERIFIED: unverified`

### Host Isolation

**[eos-build-sandbox-host-isolation]**: Build execution MUST NOT write
outside the temporary directory allocated for the build task. Inputs MUST
be mounted read-only (§FHS-View Materialization). This invariant is
strengthened by its declared-closure form: the build's *observed* read set
MUST be contained within its *declared* closure —
`[htc-declared-closure-enforced]` (htc-sad.md §1.1) — a containment property
the sandbox enforces directly (by denying reads outside the materialized
view), not one inferred after the fact from the read-set log
(§Read-Set Capture is the log; this invariant is the enforcement it
verifies).
`VERIFIED: unverified`

### Materialization Tiers

**[eos-sandbox-materialization-tiers]**: The composed view a build sandbox
mounts is one instance of three materialization tiers over the same
composition object (`[htc-materialization-tiers]`, ADR-0005 §11; htc-sad.md
§5):

| Tier | Mechanism | Use site |
| :--- | :--- | :--- |
| **Observe** | castore FUSE (this document's concern — builds and check-phase runs) | Read-set logging is active; never used for production runtime |
| **Fast** | composefs/EROFS + fs-verity | Production runtime views — no observation, no overhead |
| **Export** | plain copy / OCI image / tarball | Interop and deployment onto systems outside this substrate |

The sandbox this document specifies operates exclusively at the Observe
tier. Fast and Export tiers are runtime/deployment concerns outside build
sandboxing's scope — see htc-sad.md §5.2–§5.3 for their mechanics, not
restated here.
`VERIFIED: unverified`

---

## Forbidden States

**[no-fetch-outside-recorded-set]**: In replay mode, a build process's
network request MUST NOT be served unless it exactly matches an entry in
the lock's recorded fetch set. Any other request MUST result in a refused
connection, logged.
`VERIFIED: unverified`

**[no-read-outside-declared-closure]**: A build process MUST NOT be able to
read any byte not present in its materialized atom closure or toolchain
composition, plus whatever the fetch proxy separately permits (§Network
Containment). This is enforced by denying the read at the sandbox boundary,
not by post-hoc audit of the read-set log.
`VERIFIED: unverified`

**[no-write-outside-build-scratch]**: A build process MUST NOT write to any
path outside the scratch directory allocated for its build task. The
mounted closure is read-only; there is no writable path inside it.
`VERIFIED: unverified`

---

## Verification

| Constraint                              | Method               | Result     | Detail                                                                          |
| :--------------------------------------- | :-------------------- | :--------- | :------------------------------------------------------------------------------ |
| `eos-sandbox-fhs-materialization`       | Sandbox mount test    | UNVERIFIED | Verify a composed rootfs (not per-input dirs) is mounted read-only for a build  |
| `eos-build-sandbox-network-containment` | Build socket test     | UNVERIFIED | Verify non-proxy network access is refused; verify record/replay mode behavior |
| `eos-sandbox-read-set-capture`          | FUSE log inspection   | UNVERIFIED | Verify every read during a build is logged and the digest lands in `BuildRecord` |
| `eos-build-sandbox-delegation`          | Architecture audit    | UNVERIFIED | Verify Eos daemon/scheduler perform no sandboxing; all isolation is executor-side |
| `eos-build-sandbox-host-isolation`      | Filesystem probe test | UNVERIFIED | Attempt writes outside scratch dir and reads outside declared closure; verify denial |
| `eos-sandbox-materialization-tiers`     | Tier dispatch test    | UNVERIFIED | Verify builds/check-phases use Observe tier exclusively; no Fast/Export mount at build time |
| `no-fetch-outside-recorded-set`         | Replay-mode fuzz test | UNVERIFIED | Issue unrecorded requests in replay mode, verify refusal + log entry            |
| `no-read-outside-declared-closure`      | Sandbox escape test   | UNVERIFIED | Attempt reads of undeclared paths, verify denial at the mount boundary          |
| `no-write-outside-build-scratch`        | Sandbox escape test   | UNVERIFIED | Attempt writes to the read-only mounted closure, verify denial                 |

---

## Implications

1. **No Language-Level Eval Confinement Left to Rely On**: There is no
   evaluation stage, so there is no pure-evaluation confinement to
   substitute for OS-level sandboxing. All isolation for the primary
   executor is OS-level (namespaces, FUSE-mounted read-only views, the
   fetch proxy) — the sandbox contract this document specifies is the
   *entire* isolation story for a build, not a supplement to a language-level
   guarantee.

2. **FUSE Mounts Are Load-Bearing, Not Optional**: The castore FUSE daemon
   is both the mount mechanism for the Observe-tier rootfs and the
   unbypassable read-set observation point (§Read-Set Capture). This
   requires the host kernel to support FUSE and the executor process to
   have access to `/dev/fuse`.

3. **Record Mode Is Explicitly Impure**: A first build's fetch-recording
   pass is the one place in this substrate where a build result depends on
   live network state. This is intentional and mirrors a Nix FOD's
   equivalent impurity — it is bounded (write-back to the lock, then
   subsequent builds replay), not a general escape hatch.

4. **Closure Bloat Is Now Measurable**: Because reads are logged
   (§Read-Set Capture) against a declared closure, the delta — declared but
   never read — is a first-class, prunable artifact (htc-sad.md §6.3), not
   merely a byproduct of this sandbox's enforcement.
