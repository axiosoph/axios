# SPEC: Ion–Eos Contract

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
  See: docs/models/publishing-stack-layers.md for the algebraic domain model.
-->

## Domain

**Problem Domain:** Ion (L4 — the atom-native frontend) communicates
with eos (L3 — the network-first build daemon) to evaluate and build
resolved atoms. Ion connects to a running eos daemon via Cap'n Proto
RPC, submits a structured build request derived from the lock file,
monitors build progress through capability-based streaming, and
receives structured results or error diagnostics.

This spec constrains the contract between the two layers: what ion
transmits, what eos expects, what each is responsible for, how daemon
discovery works, and how capabilities are negotiated, exercised, and
released. The lock file is the information-complete input — its
schema is defined normatively in
[lock-file-schema.md](lock-file-schema.md); its ownership is defined
in [layer-boundaries.md §4.2](layer-boundaries.md). The lock file is
parsed by ion and transmitted as structured `eos-core` types over the
Cap’n Proto wire protocol defined normatively in
[eos-network-protocol.md](eos-network-protocol.md). This document
governs the semantic contract that binds those specifications at
the Ion–Eos boundary.

**Related Specs:**

- [lock-file-schema.md](lock-file-schema.md) — lock schema: `[sets]`,
  `[compose]`, `[compose.args]`, `[[deps]]`, field semantics
- [eos-network-protocol.md](eos-network-protocol.md) — Cap'n Proto
  schema, capability model, transport evolution, session lifecycle
- [eos-build-engine.md](eos-build-engine.md) — `BuildEngine` trait
  contracts, `BuildPlan` lifecycle, cache-skipping model
- [eos-scheduler.md](eos-scheduler.md) — job queue, deduplication,
  lease management, work-stealing
- [eos-snix-backend.md](eos-snix-backend.md) — Snix backend: eval
  threading, store mapping, sandbox dispatch
- [ion-manifest.md](ion-manifest.md) — manifest schema, plugin model
- [ion-resolution.md](ion-resolution.md) — resolution pipeline, lock
  production

**Criticality Tier:** High — the handoff is a trust boundary. Ion
trusts the lock file; eos trusts the artifacts it fetches and verifies.
The daemon connection is a security perimeter — miscommunication at
this boundary could lead to building the wrong atoms, bypassing
integrity checks, or leaking manifest internals into the build layer.

---

## Concepts

**Daemon Connection**: Ion connects to a running eos daemon over a
Cap'n Proto RPC transport. In v1, the transport is a Unix domain
socket at a well-known path. The daemon returns an `EosDaemon`
bootstrap capability upon successful connection and capability
negotiation. See [eos-network-protocol.md §Transport
Layer](eos-network-protocol.md) for the full transport specification.

**Build Request**: The unit of work submitted by ion to eos. Ion
parses the lock file, translates its contents into structured
`eos-core` types (dependency descriptors, composer configuration,
evaluation arguments), and serializes those types into a Cap’n Proto
message via `EosDaemon.submitBuild()`. The information content of the
lock file is the sole input; eos MUST NOT require auxiliary data.
Eos receives structured types, not raw TOML — the lock file format
is ion’s concern (see
[layer-boundaries.md §4.2](layer-boundaries.md)).

**Build Job**: A capability reference returned by `submitBuild()`,
representing a running or completed build. The `BuildJob` capability
supports progress attachment (`attachProgress`), cancellation
(`cancel`), and job identification (`getJobId`). Multiple ion
instances submitting build requests derived from identical lock
content receive capabilities referencing the same underlying job
(deduplication via `JobId`).

**Backend / Executor**: The concrete implementation eos's scheduler
dispatches a `build` invocation to, through the executor trait
([ADR-0005](../adr/0005-hermetic-transactional-composition.md) §6,
[htc-sad.md](../architecture/htc-sad.md) §3.5). The primary executor
is the FHS executor (L2, HTC) — no evaluator, no eval workers; it
executes upstream's own, unmodified build process inside a
materialized FHS view. An **optional legacy** passthrough-snix
executor (linking `snix-eval`/`snix-glue` in-process) MAY exist to
interoperate with pre-existing Nix-expression content; it is not the
default and is not required for the MVP path
([eos-snix-backend.md](eos-snix-backend.md) covers the legacy path).
The daemon itself has no backend dependencies beyond the executor
trait. Each executor has its own build mechanism, but all share one
contract: `build(atom closure, toolchain composition, action
params) → output tree`.

> **Terminology note (no semantic change):** Elsewhere in this
> document, "the backend evaluator" denotes whichever executor is
> active performing its own internal evaluation step — today, that
> is only the optional passthrough-snix legacy executor, which does
> run `snix-eval`/`snix-glue`. The primary FHS executor has no
> evaluation step at all; "evaluator" language attached to composer/
> compose-args handling below is executor-conditional, not a claim
> that every executor evaluates.

**Capability Negotiation**: On connection, ion queries the daemon's
capabilities via `EosDaemon.getCapabilities()` to discover supported
backend types, recognized plugin type tags, and the protocol API
version. This replaces out-of-band capability advertisement.

**Composer**: The atom (or trivial Nix expression, or static
configuration) identified by the lock file's `[compose]` section that
provides the import/evaluation logic for the root atom. Eos treats the
composer as opaque — it fetches and prepares the composer atom, then
hands control to the backend evaluator.

**Atom Store vs Artifact Store**: Eos interacts with two distinct
storage systems that MUST NOT be conflated:

- **Atom Store** (L1, atom protocol): Content-addressed storage for
  atom source trees. Accessed via the `AtomSource` trait. Populated
  by ion (ingestion from registries/local sources) and/or by eos's
  `AtomSource` composite (on-demand registry fetch). Backend: git
  (primary). This is NOT an eos concern — eos reads from it but does
  not define it.

- **Artifact Store** (L3, eos): Content-addressed storage for build
  outputs. Accessed via the `ArtifactStore` trait in `eos-core`.
  Backend: the CAS (L2, HTC) — see
  [layer-boundaries.md §4.4](layer-boundaries.md). This IS an eos
  concern architecturally (the trait); the storage backend itself
  lives one layer down.

These stores hold fundamentally different data types (source code vs
build artifacts), use different addressing schemes, and are managed by
different layers.

---

## Constraints

### Invariants

**[handoff-lock-sufficiency]**: The information contained in the lock
file MUST be the sole input to eos for dependency resolution and build
execution. Eos MUST NOT require access to ion’s manifest, plugin
state, resolution history, or any external state beyond what the lock
file encodes. The lock file is the information-complete contract.

The lock file format is owned by ion (L4) — see
[layer-boundaries.md §4.2](layer-boundaries.md). Ion parses the lock
file and translates its contents into structured `eos-core` types
before RPC submission. Eos receives the translated types, not the raw
lock file content. The schema of the lock file is defined in
[lock-file-schema.md](lock-file-schema.md).
`VERIFIED: unverified`

**[handoff-atom-fields]**: For each `type = "atom"` dependency in
`[[deps]]`, eos MUST receive the minimal pointer: `label`, `version`,
`set` (anchor hash referencing mirror information), and `publish_czd`
(the bare publish digest pin). These fields are transmitted as
structured `eos-core` dependency descriptor types, not as raw lock file
TOML. The `set` field references an `AtomSetInfo` entry in the
`BuildRequest`, which the `AtomSource` composite implementation uses for
registry mirror resolution (see
[atom-sourcing.md §Composite AtomSource](atom-sourcing.md)) — eos does
not directly fetch from URLs embedded in set data. Eos obtains the
source revision (`rev`), `dig`, and content by reading the atom's
self-describing git object (the signed publish payload + the peeled
commit), resolved via L1 `AtomSource` by `publish_czd`; these are
git-object-readable, not lock/wire-copied. Whether eos reads the object
directly or ion materializes wire fields from it at handoff time is an
implementation choice that does not change the contract's minimal
required set. The `(set, label)` pair is the dependency graph key (used
by `requires`, `owner`, and the composer reference). The minimal pointer
`(set, label, version, publish_czd)` is sufficient for eos's
`AtomSource` to: (1) resolve the atom from configured sources (local
store, registry mirrors, ion peer fallback), (2) identify the atom
within the dependency graph. See
[lock-file-schema.md §type = "atom"](lock-file-schema.md) for the
normative field definitions.
`VERIFIED: unverified`

**[handoff-plugin-fields]**: For each non-atom dependency (`type` ∈
{`nix`, `nix+git`, `nix+tar`, `nix+src`}) in `[[deps]]`, eos MUST
receive at minimum: `type` (type tag), `name` (human-readable
identifier and deduplication key), and the type-specific fetch
coordinate — `url` + `hash` for content-hashed types, or `url` +
`rev` for git-pinned types. These fields are transmitted as structured
`eos-core` dependency descriptor types. Eos dispatches on the type tag
to select the correct fetch and verification strategy. Eos does NOT
need to know which ion plugin produced the entry. See
[lock-file-schema.md §Extensibility Model](lock-file-schema.md)
for the type namespace convention.
`VERIFIED: unverified`

**[eos-verification-obligation]**: Verification responsibilities are
split by dependency type, reflecting each layer's integrity guarantees:

- **Atom dependencies** (`type = "atom"`): Integrity verification is
  the atom protocol's responsibility at ingestion time (see
  [atom-transactions.md §ingest-preserves-identity](atom-transactions.md)).
  Eos trusts atoms resolved from its `AtomSource` composite — the
  atom protocol guarantees that nothing enters an `AtomStore`
  unverified. Re-verification by eos is redundant and MUST NOT be
  treated as a required step.

- **Non-atom dependencies** (`type` ∈ {`nix`, `nix+tar`, `nix+src`,
  `nix+git`}): Eos MUST verify the fetched artifact's integrity
  against the dependency descriptor's cryptographic fields before
  using it. For content-hashed types (`nix`, `nix+tar`, `nix+src`),
  eos verifies against `hash` (SRI format). For git-pinned types
  (`nix+git`), git's own content-addressing via `rev` provides
  integrity.

In both cases, eos MUST NOT evaluate, import, or execute unverified
artifacts. See
[lock-file-schema.md §lock-hash-integrity](lock-file-schema.md)
for the digest invariants.
`VERIFIED: unverified`

**[eos-backend-agnosticism]**: Ion MUST NOT assume a specific eos
backend. The lock file format is backend-neutral — it contains no
Nix derivation paths, Snix store paths, or any backend-specific build
instructions. Backend selection is eos's responsibility. The lock
file's `type` tag namespace provides the dispatch mechanism: eos
routes each dependency entry to the appropriate backend fetcher based
on the type prefix. See
[lock-file-schema.md §lock-type-backend-dispatch](lock-file-schema.md).
`VERIFIED: unverified`

**[compose-handoff]**: The lock file’s `[compose]` section tells eos
which atom provides the import/evaluation logic for the root atom.
Ion translates the `[compose]` section into a `ComposerConfig`
(eos-core type) included in the build request. When the composer
references an atom-id, eos MUST fetch and prepare the referenced
composer atom before evaluating the root atom. The composer’s
internal logic (Nix import mechanism, module system) is opaque to
eos — the backend evaluator consumes it. The three composer variants
(`Atom`, `NixTrivial`, `Config`) are defined in
[lock-file-schema.md §[compose]](lock-file-schema.md).
`VERIFIED: unverified`

**[compose-args-passthrough]**: When the lock file contains a
`[compose.args]` table, eos MUST pass its key-value pairs to the
backend evaluator as evaluation arguments. These map to
`EvalRequest.eval_args` in the `BuildEngine` trait (see
[eos-build-engine.md](eos-build-engine.md)) and to the `evalArgs`
field of `EosDaemon.submitBuild()` in the Cap'n Proto schema (see
[eos-network-protocol.md](eos-network-protocol.md)). Eos MUST NOT
interpret, validate, or transform the argument values — they are
passed verbatim to the evaluator. The `[compose.args]` table is only
meaningful for the `Atom` and `NixTrivial` composer variants; eos
SHOULD ignore it for the `Config` variant.
`VERIFIED: unverified`

**[daemon-connection-required]**: Ion MUST connect to a running eos
daemon before submitting build requests. The connection is established
over a Cap'n Proto RPC transport. Ion MUST NOT attempt to invoke eos
as a library, spawn eos as a subprocess, or perform filesystem-based
handoff. The daemon model is the sole supported interaction pattern.
`VERIFIED: unverified`

**[result-reporting]**: Eos MUST report build results back to ion via
the `BuildJob` capability. Ion retrieves the current state via
`BuildJob.getJobId()` for identification and attaches a
`ProgressStream` callback via `BuildJob.attachProgress()` for
real-time updates. Upon build completion, the `BuildStatus.completed`
variant MUST include per-output `ArtifactInfo`: output tree digest
(content hash — the primary, executor-agnostic artifact identity)
and size. A store path is an executor-specific detail, not primary
identity; it MAY be included when the active executor produces one
(e.g. the passthrough-snix legacy executor), but ion MUST NOT depend
on its presence. Upon build failure, the `BuildStatus.failed`
variant MUST include structured error diagnostics (see
`[error-reporting-format]`).
`VERIFIED: unverified`

**[error-reporting-format]**: Build failures reported via
`BuildStatus.failed` MUST include structured error information
sufficient for ion to present actionable diagnostics. At minimum:

- **Error category**: One of: evaluation failure, build failure,
  fetch failure, sandbox violation, verification failure.
- **Human-readable message**: A descriptive string suitable for
  display to the user.
- **Structured metadata**: Phase at which the failure occurred
  (`evaluating`, `building`, `fetching`, `verifying`), exit code
  (when applicable), and a log snippet (truncated build output for
  context).

The Cap'n Proto `BuildStatus.failed` group carries the `error` (text)
and `exitCode` (int32) fields. Extended metadata is encoded in the
`error` field as a structured string or will be expanded in future
schema versions via Cap'n Proto's append-only field numbering.
`VERIFIED: unverified`

**[discovery-read-only]**: Ion MUST NOT use the `AtomDiscovery`
capability to mutate eos state. Discovery is observation-only — it
exposes read-only queries over eos's knowledge of available atoms.
Ion MUST NOT invoke any `AtomDiscovery` method with the expectation
of altering the underlying atom store, index, or daemon state. If
future `AtomDiscovery` extensions add write-like operations (e.g.,
pinning hints), they MUST be gated behind a separate, explicitly
requested capability.
`VERIFIED: unverified`

---

### Daemon Discovery

**[daemon-discovery-v1]**: In v1, ion discovers the eos daemon via a
well-known Unix domain socket path. The resolution order is:

1. The `$EOS_SOCKET` environment variable, if set. Its value MUST be
   an absolute path to an existing Unix domain socket file.
2. `$XDG_RUNTIME_DIR/eos/eos.sock`, if `$XDG_RUNTIME_DIR` is set.
3. If neither is available, ion MUST fail with a clear error
   indicating that no eos daemon was found.

Ion MUST NOT fall back to spawning a daemon on demand in v1. The
daemon is a separate process managed independently (e.g., via systemd,
manual invocation, or a future `eos daemon start` command).
`VERIFIED: unverified`

**[daemon-discovery-vN]**: In future versions, ion MAY support
additional discovery mechanisms: DNS-SD for local network daemons,
explicit configuration in `ion.toml`, or mDNS for ad-hoc clusters.
These mechanisms are NOT constrained by this spec; they will be
specified when implemented. The v1 Unix socket path MUST remain
supported as the default fallback.
`VERIFIED: unverified`

---

### Capability Model

**[capability-negotiation]**: After establishing a connection and
receiving the `EosDaemon` bootstrap capability, ion SHOULD invoke
`getCapabilities()` to discover the daemon's capabilities. The
response includes:

- **`supportedBackends`**: A list of backend identifiers the daemon
  can process (e.g., `"snix"`).
- **`apiVersion`**: The protocol version the daemon implements.

Ion SHOULD use this information to provide early diagnostics — e.g.,
warning the user if the lock file contains `type = "guix+git"` entries
but the daemon reports no Guix backend. However, ion MUST NOT use
capability information to transform or filter the lock file before
submission. The lock file is submitted as-is; eos performs the
definitive capability check.
`VERIFIED: unverified`

**[capability-mismatch-handling]**: If the lock file contains a
dependency with a `type` tag that eos does not recognize or whose
backend is not available, eos MUST reject the build with a clear error
identifying the unsupported type tag and the dependency entry that
triggered the rejection. Eos MUST NOT silently skip dependencies.
This error is reported via the `BuildStatus.failed` variant with an
error category of `fetch failure` and the unsupported type tag in the
error message.
`VERIFIED: unverified`

---

### Discovery Query

Ion queries eos for atom metadata — available packages, version
ranges, label searches — through the `AtomDiscovery` capability.
This is a read-only observation interface, distinct from the build
submission path.

**Capability Acquisition**: After connecting and obtaining the
`EosDaemon` bootstrap capability (per `[daemon-connect]`), ion
invokes `EosDaemon.discover()` to receive an `AtomDiscovery`
capability. This is a lightweight call that does not initiate a
build or alter daemon state.

**Query Operations**: The `AtomDiscovery` capability exposes three
operations:

| Method          | Signature                      | Purpose                                                                                                |
| :-------------- | :----------------------------- | :----------------------------------------------------------------------------------------------------- |
| `resolve(id)`   | `AtomId → Option<AtomMeta>`    | Look up specific atom metadata (label, version, set, mirrors) by `AtomId` — the `(anchor, label)` pair |
| `contains(id)`  | `AtomId → Bool`                | Existence check — returns whether eos has knowledge of the given atom                                  |
| `search(query)` | `SearchQuery → List<AtomMeta>` | Find atoms matching a label pattern, atom-set filter, version range, or combination thereof            |

**Use Cases**:

- **Dependency discovery**: Ion's resolution pipeline queries
  available atoms before constructing a lock file, enabling
  interactive version selection and constraint satisfaction.
- **Version browsing**: Users inspect available versions of a
  dependency without initiating a build (`ion list --versions`).
- **Available package listing**: Ion presents a catalogue of atoms
  known to the connected eos daemon (`ion search <pattern>`).

**Backing Store Evolution**:

- **v1**: Discovery results are backed by processed lock files and
  the local artifact store. The daemon indexes atoms it has
  previously fetched and verified. Coverage is limited to the
  daemon's local history.
- **vN**: Discovery is backed by peer gossip and a distributed
  index. The daemon participates in an atom discovery network,
  returning results from peers it has not directly fetched from.
  The `AtomDiscovery` interface remains identical; only the backing
  data source changes.

`AtomDiscovery` is subject to `[discovery-read-only]` — all
operations are pure queries with no side effects on daemon state.

---

### Transitions

**[daemon-connect]**: Ion establishes a connection to the eos daemon.

- **PRE**: The eos daemon is running and listening on a discoverable
  transport endpoint (Unix socket in v1). Ion has resolved the socket
  path per `[daemon-discovery-v1]`.
- **POST**: Ion opens a transport connection and the Cap'n Proto
  `TwoPartyVatNetwork` is established. A `HandshakeRequest` /
  `HandshakeResponse` exchange occurs per
  [eos-network-protocol.md §client-connect](eos-network-protocol.md).
  On success, ion receives the `EosDaemon` bootstrap capability. On
  failure (capability mismatch, authentication failure in vN), the
  connection is closed with a rejection reason.
  `VERIFIED: unverified`

**[build-request]**: Ion submits a build request to eos.

- **PRE**: Ion holds an `EosDaemon` capability. A reconciled lock file
  exists. All entries have been validated by ion's resolution pipeline.
  The lock file has been parsed by `ion-lock` and translated into
  structured `eos-core` types by `ion-eos`.
- **POST**: Ion invokes `EosDaemon.submitBuild(planDigest, ...)`
  with the structured build request. The `planDigest` is derived from
  the lock file content (see `[concurrent-builds]`). Eos returns a
  `BuildJob` capability. If an identical job already exists (same
  `JobId`), the existing `BuildJob` is returned (deduplication). The
  build proceeds asynchronously.
  `VERIFIED: unverified`

**[attach-progress]**: Ion attaches to a build's progress stream.

- **PRE**: Ion holds a `BuildJob` capability.
- **POST**: Ion invokes `BuildJob.attachProgress(callback)`, passing a
  client-implemented `ProgressStream` capability. Eos pushes
  `BuildStatus` updates via `callback.update()`. The `-> stream`
  annotation provides built-in backpressure. When the build completes
  or fails, eos invokes `callback.done()`. Ion MAY detach at any time
  by dropping the `ProgressStream` capability; the build continues
  unaffected (see
  [eos-network-protocol.md §no-cancel-on-drop](eos-network-protocol.md)).
  `VERIFIED: unverified`

**[fetch-verify-build]**: Eos resolves and prepares all dependencies
for evaluation.

Eos resolves each atom dependency from its configured `AtomSource`
composite (see
[atom-sourcing.md §Composite AtomSource](atom-sourcing.md)).
Resolution follows a priority chain: local store (cache hit), registry
mirrors (on-demand fetch via atom protocol), and the ion peer
(fallback for unreachable atoms). For non-atom dependencies, eos
fetches from the URL in the dependency descriptor and verifies
integrity per `[eos-verification-obligation]`.

Atom integrity verification is the atom protocol's responsibility at
ingestion time (see
[atom-transactions.md §ingest-preserves-identity](atom-transactions.md)).
Eos trusts atoms resolved from its `AtomSource` — re-verification is
redundant since the atom protocol guarantees that nothing enters an
`AtomStore` unverified.

- **PRE**: The structured build request has been received and
  validated. The composer configuration has been interpreted.
- **POST**: For each dependency descriptor:
  (1) Atom deps: eos resolves the atom from its `AtomSource`
  composite (local store → registry mirrors → ion peer fallback).
  (2) Non-atom deps: eos fetches from the URL in the descriptor and
  verifies integrity against the descriptor's cryptographic fields
  (`hash` for content-hashed types, `rev` for git-pinned types).
  (3) Eos makes the resolved/verified artifact available to the
  backend evaluator and builder.
  Failed verification of non-atom deps MUST abort the build per
  `[eos-verification-obligation]`.
  `VERIFIED: unverified`

**[compose-evaluation]**: Eos evaluates the root atom using the
composer.

- **PRE**: All dependencies have been fetched and verified. The
  composer atom (if `[compose].use` references one) is available.
- **POST**: Eos constructs an `EvalRequest` containing:
  - The evaluation entrypoint from `[compose].entry`
  - Pre-resolved inputs, as an atom closure (all fetched, verified
    deps) — not store paths; a store path is an executor-specific
    detail, see `[result-reporting]`
  - The `ComposerConfig` derived from `[compose]`
  - The `eval_args` from `[compose.args]` (passed verbatim)
    Eos dispatches `build(atom closure, toolchain composition,
    action params)` through the executor trait
    ([ADR-0005](../adr/0005-hermetic-transactional-composition.md)
    §6, [htc-sad.md](../architecture/htc-sad.md) §3.5), producing an
    output tree — the primary FHS executor has
    no evaluator or `Plan` concept; the optional passthrough-snix
    legacy executor still produces a Nix derivation internally.
    Outputs are reported via the `BuildJob` capability.
    `VERIFIED: unverified`

**[query-discovery]**: Ion obtains the discovery capability and
issues read-only queries.

- **PRE**: Ion holds an `EosDaemon` capability (established via
  `[daemon-connect]`).
- **POST**: Ion invokes `EosDaemon.discover()` and receives an
  `AtomDiscovery` capability. Ion MAY then issue any combination of
  `resolve(id)`, `contains(id)`, and `search(query)` calls. These
  are read-only operations — they do not affect build state, daemon
  configuration, or the artifact store. The `AtomDiscovery`
  capability remains valid for the lifetime of the underlying
  `EosDaemon` connection.
  `VERIFIED: unverified`

### Content Delivery Negotiation

**[content-delivery-negotiation]**: After receiving a `BuildRequest`,
eos begins resolving the **top-level atoms** (those explicitly
requested for evaluation) into its atom store. Eos resolves atoms
from its configured `AtomSource` composite (local atom store, then
registry mirrors). For atoms that cannot be resolved from these
sources (e.g., local dev atoms that exist only on the developer's
machine), eos falls back to the ion peer — ion's `AtomStore`
exposed as an `AtomSource` — as a last-resort source.

The content transfer for peer-assisted resolution is a
**store-to-store transfer** through the atom protocol's
`AtomStore::ingest()` interface. Ion's store acts as an
`AtomSource` that eos's store ingests from. The transport
mechanism (git fetch, shared filesystem, or protocol-level
transfer over the existing RPC connection) is an implementation
detail of the atom protocol backend, not a concern of the
ion-eos contract. No ad-hoc data streaming channel exists
outside the atom protocol.

The negotiation follows a `FindMissing` pattern (analogous to
Bazel RE API's `FindMissingBlobs`):

1. Ion submits `BuildRequest` via `submitBuild()` → receives
   `BuildJob` capability
2. Eos begins concurrent resolution of all top-level atom
   references from its `AtomSource` composite (local atom
   store, then registry mirrors)
3. Ion calls `BuildJob.getMissing()` — blocks until eos has
   attempted all non-peer sources
4. Eos returns the list of `AtomId`s it could not resolve
   from its atom store or registries
5. If the missing list is non-empty, eos's composite source
   ingests the missing atoms from ion's store via the atom
   protocol. Ion makes its store available as an `AtomSource`
   through a mechanism determined by the atom protocol backend
   (e.g., git fetch for git-backed stores, or a protocol-level
   `AtomSource` capability over the RPC connection).
6. Eos ingests the received atoms into its atom store and
   dispatches evaluation

If `getMissing()` returns an empty list, step 5 is skipped — all
atoms were resolvable without ion's assistance. This is the common
case for CI/CD and warm-cache deployments.

**Atom access during evaluation:** Once the top-level atoms are
in the atom store, the executor operates on the top-level atom
from the atom store (e.g., via a git URI pointing to the store).
The atom's _transitive dependencies_ (locked in the atom's own
lock file) are resolved and verified by eos itself, from its
configured `AtomSource` composite, per `[fetch-verify-build]` —
the identical priority chain (local store → registry mirrors →
ion peer fallback) used for top-level atoms. Nothing is fetched by
backend-internal "Nix fetching semantics"; no executor performs its
own atom resolution. (This corrects a prior internal contradiction
against `[eos-verification-obligation]`/`[no-unverified-execution]`:
a transitive atom dependency fetched outside `AtomSource` would
never pass `AtomStore` ingestion verification.)

All ingestion invariants apply to peer-assisted transfers. The
atom protocol verifies integrity on ingestion — eos's store does
not accept unverified atoms regardless of their source.

`VERIFIED: unverified`

---

### Concurrent Builds

**[concurrent-builds]**: Multiple ion instances submitting build
requests derived from the same lock file content MUST receive
`BuildJob` capabilities referencing the same underlying build
execution. Deduplication is keyed on `JobId = hash(plan)`, where the
plan digest is computed from the lock file content (ensuring
consistency regardless of which ion instance performs the translation).
When a second submission matches an in-progress job, the daemon
returns the existing `BuildJob` — no duplicate work is performed.
Each ion instance independently attaches and detaches progress
callbacks via its own `BuildJob` capability reference. See
[eos-scheduler.md](eos-scheduler.md) for the full deduplication and
scheduling semantics.
`VERIFIED: unverified`

---

### Forbidden States

**[no-manifest-leakage]**: Eos MUST NOT read or depend on the
`ion.toml` manifest of the root atom. All information eos needs is
derived from the lock file and transmitted as structured `eos-core`
types. If eos needs data not present in the structured build request,
that is a signal that either the lock schema or the `eos-core`
contract types are incomplete — the fix is to extend them (see
[lock-file-schema.md](lock-file-schema.md) and
[layer-boundaries.md](layer-boundaries.md)), not to leak the manifest
across the layer boundary.
`VERIFIED: unverified`

**[no-unverified-execution]**: Eos MUST NOT evaluate, import, or
execute any artifact that has not passed integrity verification.
For atom dependencies, verification is the atom protocol's
responsibility at ingestion time — eos trusts atoms resolved from its
`AtomSource` (see `[eos-verification-obligation]`). For non-atom
dependencies, eos MUST verify integrity against the descriptor's
cryptographic fields: content-hashed types (verified via `hash`) and
git-pinned types (verified via `rev` and git's content-addressing).
`VERIFIED: unverified`

**[no-daemon-bypass]**: Ion MUST NOT circumvent the daemon by directly
invoking backend tools (e.g., `nix-build`, `snix eval`), reading the
eos artifact store, or manipulating eos-internal state. The `EosDaemon`
capability is the sole interface. Bypassing it violates the security
boundary and breaks the deduplication, progress tracking, and
attestation guarantees provided by the daemon.
`VERIFIED: unverified`

---

### Behavioral Properties

**[backend-substitutability]**: If the same lock file is processable
by two different eos backends (e.g., Snix and a future alternative),
switching backends MUST NOT require changes to the lock file. The lock
file is backend-neutral by construction — backend selection is an eos
daemon configuration concern, not a lock file concern.

- **Type**: Safety
  `VERIFIED: unverified`

**[plugin-type-extensibility]**: Adding a new plugin type tag (e.g.,
`guix+fetch`) MUST NOT require changes to existing eos backends that
do not support it. Unsupported types are rejected per
`[capability-mismatch-handling]`, not silently processed. The type
namespace convention (`prefix+suffix`) is defined in
[lock-file-schema.md §lock-type-namespace](lock-file-schema.md).

- **Type**: Safety
  `VERIFIED: unverified`

**[idempotent-submission]**: Submitting build requests derived from
the same lock file content to eos multiple times MUST be idempotent
with respect to build execution. The `JobId = hash(plan)`
deduplication ensures that repeated submissions from the same or
different ion instances do not trigger redundant builds. If the build
has already completed, the `BuildJob` capability returns the cached
result immediately.

- **Type**: Safety
  `VERIFIED: unverified`

**[progress-liveness]**: Once ion attaches a `ProgressStream` callback
to a `BuildJob`, eos MUST deliver `BuildStatus` updates reflecting
every state transition of the underlying build. Ion MUST NOT be
required to poll for status — the daemon pushes updates proactively.
The `-> stream` annotation provides backpressure so the daemon does
not overwhelm a slow client.

- **Type**: Liveness
  `VERIFIED: unverified`

---

## Lock File as Build Request

The lock file is the information-complete, human-readable artifact
that ion produces on disk. However, the lock file format is ion's
concern (see [layer-boundaries.md §4.2](layer-boundaries.md)). Ion
parses the lock file and translates its contents into structured
`eos-core` types before transmitting them over the Cap'n Proto
protocol. Eos receives structured build request types, not raw TOML.

### Serialization

Ion MUST translate the complete lock file content — `version`,
`[sets]`, `[compose]` (including `[compose.args]`), and `[[deps]]` —
into structured `eos-core` dependency descriptors and composer
configuration, then serialize those types into the Cap'n Proto
`submitBuild` invocation. The `planDigest` parameter carries the
BLAKE3 digest of the lock file content (ensuring deduplication
consistency with the on-disk artifact). The remaining parameters
carry the structured dependency and composer data.

### Required Build Request Content

For a `submitBuild` invocation to succeed, the build request MUST
include the translated equivalents of all lock file sections:

| Information            | Requirement                                                  | Lock file source | Reference                                                       |
| :--------------------- | :----------------------------------------------------------- | :--------------- | :-------------------------------------------------------------- |
| Schema version         | MUST be `0` (current schema version)                         | `version`        | [lock-file-schema.md §lock-version-field](lock-file-schema.md)  |
| Atom-set mirrors       | MUST include entries for all anchors referenced by atom deps | `[sets]`         | [lock-file-schema.md §lock-set-referenced](lock-file-schema.md) |
| Composer config        | MUST be present; determines evaluation strategy              | `[compose]`      | [lock-file-schema.md §[compose]](lock-file-schema.md)           |
| Dependency descriptors | MUST describe all transitive dependencies                    | `[[deps]]`       | [lock-file-schema.md §[[deps]]](lock-file-schema.md)            |

For `type = "atom"` dependency descriptors, the required fields are the
minimal pointer `(set, label, version, publish_czd)` per
`[handoff-atom-fields]`. No additional fields (`rev`, `id`, `dig`) are
required in the build request; eos reads those from the atom's
self-describing git object.

### Structural Validation

Before beginning fetch-verify-build, eos MUST validate the
structured build request:

1. **Closure integrity**: All atom-ids in dependency requirement
   lists, owner fields, and the composer reference resolve to
   exactly one atom-type dependency descriptor (per
   `lock-requires-closure`, `lock-owner-closure`,
   `lock-compose-closure` in
   [lock-file-schema.md](lock-file-schema.md)).
2. **Set reference integrity**: All atom-set references on atom-type
   descriptors match a provided atom-set entry (per
   `lock-set-referenced`).
3. **DAG acyclicity**: The dependency requirement graph is a directed
   acyclic graph (per `lock-dag-acyclicity`).
4. **Type recognition**: All dependency type tags are recognized by
   at least one available backend (per
   `[capability-mismatch-handling]`).

Validation failures MUST abort the build with structured errors per
`[error-reporting-format]`.

> [!NOTE]
> Ion SHOULD perform these same validations during lock file parsing
> (in `ion-lock`) as a fail-fast measure. Eos performs them
> redundantly because the daemon is a trust boundary — it cannot
> assume ion's validations were correct.

---

## Compose.args Flow

The `[compose.args]` table flows from the manifest through the lock
file and protocol to the backend evaluator:

```
ion.toml              atom.lock             ion-eos (bridge)            EosDaemon.submitBuild()     EvalRequest
─────────             ─────────             ────────────────            ───────────────────────     ───────────
[compose]             [compose.args]        ComposerConfig +            evalArgs: List(KeyValue)    eval_args: Vec<(String,String)>
  args.system =  ──▸  system = "x86_64" ──▸ eval_args translation  ──▸ {key:"system",           ──▸ [("system","x86_64-linux")]
  "x86_64-linux"                                                       value:"x86_64-linux"}
```

1. Ion resolves `[compose.args]` from the manifest and writes it into
   the lock file as a TOML sub-table.
2. `ion-eos` parses the lock file (via `ion-lock`) and extracts the
   args into structured types.
3. `ion-eos` serializes the args as `List(KeyValue)` in the Cap'n
   Proto `submitBuild` invocation.
4. Eos populates `EvalRequest.eval_args` from the received data.
5. The backend evaluator receives the args verbatim and applies them
   to the evaluation context (e.g., as Nix attrset overlays).

At no point does eos interpret, validate, or transform the argument
values. They are opaque to the protocol layer.

---

## Result and Error Reporting

### Build Results

Upon successful completion, eos reports results via the
`BuildStatus.completed` variant, which MUST include:

| Field          | Type         | Description                                |
| :------------- | :----------- | :------------------------------------------ |
| `outputDigest` | `Data`       | BLAKE3 digest of the combined build output — the primary, executor-agnostic artifact identity |
| `outputPaths`  | `List(Text)` | Executor-specific store paths, when the active executor produces them (e.g. the passthrough-snix legacy executor); OPTIONAL, not primary identity |

Ion receives these via the attached `ProgressStream` callback. The
`ArtifactInfo` for each output — output tree digest and size, plus an
optional executor-specific store path — is available through the
daemon's store query interface (see
[eos-build-engine.md §ArtifactStore](eos-build-engine.md)).

### Build Errors

Upon failure, eos reports errors via the `BuildStatus.failed` variant:

| Field      | Type    | Description                                           |
| :--------- | :------ | :---------------------------------------------------- |
| `error`    | `Text`  | Human-readable error message with structured metadata |
| `exitCode` | `Int32` | Process exit code (0 if not applicable)               |

The `error` field carries sufficient context for ion to present
diagnostics. Error categories and their typical causes:

| Category             | Phase        | Typical Cause                                         |
| :------------------- | :----------- | :---------------------------------------------------- |
| Evaluation failure   | `evaluating` | Nix expression error, missing import, type error      |
| Build failure        | `building`   | Builder process exited non-zero, timeout              |
| Fetch failure        | `fetching`   | Mirror unreachable, URL 404, network timeout          |
| Verification failure | `verifying`  | Content digest mismatch, tampered artifact            |
| Sandbox violation    | `building`   | Attempted network access, disallowed filesystem write |

### Job Status Query

Ion MAY query the status of a previously submitted job via
`EosDaemon.queryStatus(jobId)` without attaching a progress stream.
This returns the current `BuildStatus` as a snapshot value.

---

## Verification

| Constraint                     | Method                 | Result     | Detail                                                                       |
| :----------------------------- | :--------------------- | :--------- | :--------------------------------------------------------------------------- |
| `handoff-lock-sufficiency`     | Integration test       | UNVERIFIED | Submit lock-only; verify eos never reads manifest                            |
| `handoff-atom-fields`          | Schema conformance     | UNVERIFIED | Validate `set`, `label`, `version`, `publish_czd` presence (minimal pointer) |
| `handoff-plugin-fields`        | Schema conformance     | UNVERIFIED | Validate `type`, `name`, and fetch coordinates per type                      |
| `eos-verification-obligation`  | Fault injection        | UNVERIFIED | Corrupt artifact, verify build abort                                         |
| `eos-backend-agnosticism`      | Cross-backend test     | UNVERIFIED | Same lock succeeds on different backend configs                              |
| `compose-handoff`              | Integration test       | UNVERIFIED | Lock with composer → eos fetches composer first                              |
| `compose-args-passthrough`     | Unit test              | UNVERIFIED | Verify args reach evaluator verbatim                                         |
| `daemon-connection-required`   | Architecture audit     | UNVERIFIED | No library-mode code paths exist                                             |
| `result-reporting`             | Integration test       | UNVERIFIED | Attach callback, verify `completed` with `ArtifactInfo`                      |
| `error-reporting-format`       | Fault injection        | UNVERIFIED | Trigger each error category, verify structured output                        |
| `daemon-discovery-v1`          | Unit test              | UNVERIFIED | Test socket resolution order                                                 |
| `daemon-discovery-vN`          | Design review          | UNVERIFIED | Architectural property (future)                                              |
| `capability-negotiation`       | Handshake test         | UNVERIFIED | Query capabilities, verify response structure                                |
| `capability-mismatch-handling` | Integration test       | UNVERIFIED | Submit unsupported type, verify rejection message                            |
| `daemon-connect`               | Integration test       | UNVERIFIED | Verify handshake and bootstrap cap                                           |
| `build-request`                | Integration test       | UNVERIFIED | Submit lock, receive `BuildJob`                                              |
| `attach-progress`              | Integration test       | UNVERIFIED | Attach callback, verify status delivery                                      |
| `fetch-verify-build`           | Integration test       | UNVERIFIED | End-to-end: fetch, verify, build                                             |
| `compose-evaluation`           | Integration test       | UNVERIFIED | Full composer with args → successful eval                                    |
| `concurrent-builds`            | Dedup test             | UNVERIFIED | Two clients, same lock, one execution                                        |
| `no-manifest-leakage`          | Code audit             | UNVERIFIED | No manifest-reading code in eos                                              |
| `no-unverified-execution`      | Fault injection        | UNVERIFIED | Bypass verification, verify rejection                                        |
| `no-daemon-bypass`             | Architecture audit     | UNVERIFIED | No direct backend invocation in ion                                          |
| `backend-substitutability`     | Cross-backend test     | UNVERIFIED | Same lock, different backend, no lock changes                                |
| `plugin-type-extensibility`    | Extension test         | UNVERIFIED | Add unknown type, verify clean rejection                                     |
| `idempotent-submission`        | Repeat submission test | UNVERIFIED | Same lock twice → same `JobId`, no duplicate work                            |
| `progress-liveness`            | Integration test       | UNVERIFIED | Attach → verify all state transitions delivered                              |
| `discovery-read-only`          | Code audit             | UNVERIFIED | No mutating code paths reachable via `AtomDiscovery`                         |
| `query-discovery`              | Integration test       | UNVERIFIED | Obtain `AtomDiscovery`, issue resolve/contains/search                        |
| `content-delivery-negotiation` | Integration test       | UNVERIFIED | getMissing flow; store-to-store ingest for missing atoms                     |

---

## Implications

1. **Daemon Lifecycle Is External to This Contract.** This spec
   governs what happens _after_ ion connects to eos, not how the
   daemon is started, stopped, or supervised. Daemon lifecycle is
   defined in
   [eos-network-protocol.md §Daemon Architecture](eos-network-protocol.md).
   Ion assumes a running daemon; operational tooling (systemd units,
   `eos daemon` subcommand) is out of scope.

2. **Lock Schema Is the Sole Extensibility Surface.** To make new
   information available to eos, extend the lock schema in
   [lock-file-schema.md](lock-file-schema.md). Never leak
   manifest internals, ion plugin state, or resolution metadata
   across the boundary. If eos needs it and it's not in the lock,
   the lock is incomplete.

3. **Backend Addition Is a Daemon Concern.** Adding a new backend
   (e.g., a Guix backend) requires: (a) a new ion plugin producing
   `guix+*` type tags in `[[deps]]`, (b) a new eos backend crate
   implementing `BuildEngine`, (c) daemon configuration to enable the
   backend. No changes to this contract or the lock schema are
   required — the type namespace is extensible by construction.

4. **v1 Transport Is Deliberately Minimal.** Unix socket + filesystem
   permissions provide implicit authentication for local single-user
   operation. Signature-based authentication (Cyphr Principal Roots)
   is deferred to vN's TCP transport. This spec does not constrain
   authentication beyond referencing
   [eos-network-protocol.md](eos-network-protocol.md).

5. **Compose.args Determinism.** Because `[compose.args]` values
   influence evaluation, they MUST be included in the plan digest
   that computes `JobId`. Changing args invalidates cached
   evaluations. This is an eos-internal concern, not an ion concern —
   ion simply passes the args; eos ensures cache correctness.

6. **Cross-Language Client Path.** This contract is specified in terms
   of Cap'n Proto capabilities, not Rust types. A Go or Python
   frontend could implement the same contract by generating client
   stubs from the Cap'n Proto schema. This is a vN concern — all
   foreseeable frontends are Rust. A protocol translation proxy
   (Cap'n Proto ↔ gRPC) is an option if cross-language support
   becomes critical (see
   [eos-network-protocol.md §Implications](eos-network-protocol.md)).
