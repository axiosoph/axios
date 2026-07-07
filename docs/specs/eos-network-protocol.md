# SPEC: Eos Network Protocol

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

**Problem Domain:** The Eos Network Protocol defines the daemon architecture,
Cap'n Proto wire format, capability-based RPC semantics, session lifecycle,
streaming progress model, binary substitution protocol, and cryptographic
trust verification for all communication between Ion frontends, the Eos
daemon, worker nodes, and remote artifact caches.

Eos is a **network-first daemon**. It exposes a message-based API over Cap'n
Proto RPC — not a Rust library that callers link against. The `eos-core`
trait surface defines the _behavioral contract_ (what the daemon does); this
specification defines the _calling convention_ (how clients invoke it over
the wire). The daemon listens on a transport endpoint (Unix domain socket in
v1, authenticated TCP in vN), accepts multiplexed client connections, and
manages concurrent build sessions through an object-capability model.

Because Eos operates in a decentralized and potentially adversarial
environment, it treats every network boundary as a trust boundary. Worker
nodes cannot blindly execute build commands, nor can client machines blindly
import artifacts from caches. This specification establishes the
cryptographic guarantees that ensure reproducibility verification and origin
validation without reliance on central Certificate Authorities.

**Model Reference:**

- [ion-eos-contract.md](ion-eos-contract.md) — Handoff boundaries and lock
  file translation
- [atom-transactions.md](atom-transactions.md) — Cryptographic claims and
  verify operations
- [eos-build-engine.md](eos-build-engine.md) — `BuildEngine` trait contracts,
  `ActionParams`, and cache model
- [eos-scheduler.md](eos-scheduler.md) — Job queue, deduplication, and
  dispatch semantics
- [htc-sad.md](../architecture/htc-sad.md) — §2 (core object taxonomy), §2.3
  (`BuildRecord`), §6.5 (action identity)
- [eos-sad.md](../architecture/eos-sad.md) — §8.3 (worker-facing interface
  summary), atom-DAG scheduling model

**Criticality Tier:** High — correctness preserves the security boundary of
the publishing stack, protects hosts from executing unverified binaries, and
ensures the integrity of the capability-based session model.

---

## Wire Format: Cap'n Proto

Eos uses [Cap'n Proto](https://capnproto.org/) as its wire format and RPC
framework. This is a deliberate architectural choice, not a default:

1. **Capability model is native semantics.** The `submitBuild → get BuildJob
capability → attachProgress → drop to detach` lifecycle is first-class in
   Cap'n Proto's object-capability RPC — not simulated over streaming RPCs.
2. **Transport-agnostic.** Cap'n Proto operates over any `AsyncRead +
AsyncWrite` stream, enabling clean layering of Cyphr authentication
   without fighting HTTP/2 framing.
3. **Zero-copy for hot-path types.** Digests, plan hashes, and store paths
   transfer without deserialization allocation.
4. **Dependency budget.** The `capnp` runtime carries zero non-core
   dependencies, contrasted with gRPC's transitive closure (hyper, h2,
   tower, prost, http, etc.).
5. **Schema evolution.** Append-only field numbering (`@N`) provides forward
   and backward compatibility identical to protobuf's model.

### Protocol Schema

#### Implemented Schema

The following schema is quoted verbatim from
`eos/eos-proto/schema/eos.capnp` (ground truth):

```capnp
@0xb8d8f0d996dfe9b0;

struct PlanDigest {
  bytes @0 :Data;  # 32-byte Blake3 digest
}

struct BuildStatus {
  union {
    queued @0 :Void;
    evaluating :group { message @1 :Text; }
    building :group { phase @2 :Text; progress @3 :Float32; }
    completed :group { outputPaths @4 :List(Text); outputDigest @5 :Data; }
    failed :group { error @6 :Text; exitCode @7 :Int32; }
    cancelled @8 :Void;
  }
}

interface ProgressStream {
  update @0 (status :BuildStatus) -> stream;
  done @1 () -> ();
}

struct AtomSetEntry {
  anchor @0 :Text;
  tag @1 :Text;
  mirrors @2 :List(Text);
}

struct DepDescriptor {
  union {
    atom :group {
      id @0 :AtomId;
      label @1 :Text;
      version @2 :Text;
      set @3 :Text;
      rev @4 :Text;
      requires @5 :List(AtomId);
      direct @6 :Bool;
    }
    nix :group {
      name @7 :Text;
      url @8 :Text;
      hash @9 :Text;
      owner @10 :AtomId;
    }
    nixGit :group {
      name @11 :Text;
      url @12 :Text;
      rev @13 :Text;
      version @14 :Text;
      owner @15 :AtomId;
    }
    nixTar :group {
      name @16 :Text;
      url @17 :Text;
      hash @18 :Text;
      owner @19 :AtomId;
    }
    nixSrc :group {
      name @20 :Text;
      url @21 :Text;
      hash @22 :Text;
      owner @23 :AtomId;
    }
  }
}

struct ComposerSpec {
  union {
    atom :group {
      id @0 :AtomId;
      entry @1 :Text;
      args @2 :List(KeyValue);
    }
    nixTrivial :group {
      expression @3 :Text;
      args @4 :List(KeyValue);
    }
    static @5 :Void;
  }
}

struct BuildRequest {
  planDigest @0 :Data;
  sets @1 :List(AtomSetEntry);
  deps @2 :List(DepDescriptor);
  composer @3 :ComposerSpec;
  evalArgs @4 :List(KeyValue);
}

interface EosDaemon {
  submitBuild @0 (request :BuildRequest) -> (job :BuildJob);
  queryStatus @1 (jobId :Data) -> (status :BuildStatus);
  getCapabilities @2 () -> (
    supportedBackends :List(Text),
    apiVersion :UInt32
  );
  discover @3 () -> (discovery :AtomDiscovery);
}

interface BuildJob {
  attachProgress @0 (callback :ProgressStream) -> ();
  cancel @1 () -> ();
  getJobId @2 () -> (jobId :Data);
  getMissing @3 () -> (missingAtoms :List(AtomId));
}

interface AtomDiscovery {
  resolve @0 (id :AtomId) -> (meta :AtomMeta);
  contains @1 (id :AtomId) -> (exists :Bool);
  search @2 (query :AtomQuery) -> (results :List(AtomMeta));
}

struct AtomId {
  digest @0 :Data;
}

struct AtomMeta {
  id @0 :AtomId;
  label @1 :Text;
  versions @2 :List(VersionInfo);
  sets @3 :List(Text);  # anchor hashes of sets containing this atom
}

struct VersionInfo {
  version @0 :Text;
  rev @1 :Text;
  set @2 :Text;
}

struct AtomQuery {
  labelPattern @0 :Text;    # glob or substring match
  setFilter @1 :Text;       # optional: restrict to specific set
  limit @2 :UInt32;         # max results
}

struct KeyValue {
  key @0 :Text;
  value @1 :Text;
}
```

#### Deprecated Fields (Post-ADR-0005)

The Implemented Schema above is quoted verbatim from ground truth and is
**not** edited by this pass (spec text only; the actual `eos.capnp` migration
is separate implementation work). ADR-0005's atom-DAG re-scope
(`[htc-atom-dag-executor-trait]`, §6) and the deletion of the evaluation
stage retire the following implemented-schema fields/structs in *meaning*,
not on the wire — per Cap'n Proto's append-only evolution model (§Wire
Format, item 5, ~line 73), they remain present for backward compatibility
with historical job records until an actual schema-evolution commit appends
their replacements and marks them formally deprecated:

- **`BuildStatus.evaluating`** — no eos build past this ADR ever produces
  this status. See §Streaming Protocol below.
- **`DepDescriptor`'s `nix`/`nixGit`/`nixTar`/`nixSrc` groups** — these
  described dependencies fetched by eos itself "using normal Nix fetching
  semantics." Under HTC, non-atom fetch dependencies are lock-side
  `[[deps]]` entries of `type = "fetch"` (ADR-0005 §7,
  `[htc-fetch-set-lock-plugin]`), executed by HTC's record/replay proxy
  inside the executor's sandbox — not fetched by eos, and not resolved by
  an evaluator. The `atom` group is unaffected.
- **`ComposerSpec`** and **`BuildRequest.composer`** — composer
  configuration was an evaluation-era construct (selecting and configuring
  a Nix composer atom for `snix-eval`). Superseded by `ActionParams` (see
  §Atom-DAG Intake Surface below), which is opaque to the daemon.
- **`BuildRequest.evalArgs`** — superseded by `ActionParams.variantFlags`
  (same successor as `ComposerSpec`).
- **`AtomId.digest`** — this shipped shape names a hash of the
  `(anchor, label)` pair, which CONTRADICTS the keystone identity decision
  that `AtomId` is the abstract pair itself, not a digest of it (atom-sad
  §6.1, `[identity-content-addressed]`). See the PROPOSED successor shape
  below.

#### Atom-DAG Intake Surface — PROPOSED (Post-ADR-0005)

**Status: PROPOSED** — append-only additions to the client-facing
`BuildRequest`; not yet in the implemented schema. Cap'n Proto's schema
evolution is strictly append-only (§Wire Format, item 5): the fields named
"Deprecated" above are never removed or renumbered, only superseded by new,
higher-numbered fields, exactly as this section proposes.

Per [htc-atom-dag-executor-trait] (ADR-0005 §6) and eos-sad §1.1/§4.1: there
is no evaluation stage. The atom-DAG derives entirely from the lock (the
ion → eos handoff, ion-sad §6.6); no evaluation-derived information is
needed or accepted on this surface — the exact inversion of the
pre-ADR-0005 `EvalResult`/`PlanDag` design this section replaces (that
PROPOSED surface, where an eval worker reported a plan sub-DAG *back* to
the scheduler after evaluation, is retired wholesale along with the
evaluation stage itself).

```capnp
# PROPOSED additions to BuildRequest (append-only; illustrative field
# numbers — the actual appending happens in eos.capnp at implementation
# time). ActionParams is defined ONCE in eos-build-engine.md; these are
# the SAME fields (target_system, variant_flags), capnp-cased.
struct ActionParams {
  targetSystem @0 :Text;            # Target system triple (from [compose.args].system)
  variantFlags @1 :List(KeyValue);  # Opaque variant flags (from [compose.args])
}

# A node in the submitted atom-DAG. Carries the atom reference plus
# optional advisory weight annotations — re-scoped from the pre-ADR-0005
# eval-worker PlanNode report (deleted along with the evaluation stage)
# onto the submission surface instead. Annotations are predictions only —
# the scheduler gates them through confidence weighting and they never
# alter correctness guarantees (Theorem 3 decay applies, ADR-0004 §2).
struct AtomDagNode {
  atomId @0 :AtomId;                 # publish_czd / (set, label) reference — see below
  predictedDurationMs @1 :UInt64;    # Predicted build duration (0 = unknown)
  predictedMemPeakKb @2 :UInt64;     # Predicted peak memory in KiB (0 = unknown)
  predictedOutputKb @3 :UInt64;      # Predicted output size in KiB (0 = unknown)
}

# Directed edge in the submitted atom-DAG (predecessor → successor),
# read directly off the lock — not computed by an evaluator.
struct AtomDagEdge {
  from @0 :UInt32;  # index into BuildRequest's node list: predecessor
  to @1 :UInt32;    # index into BuildRequest's node list: successor
}

# extend BuildRequest (illustrative — fields @0–@4 above are the
# deprecated evaluation-era surface; new intake fields are appended,
# never renumbered):
#   dagNodes     @5 :List(AtomDagNode); # atom-DAG nodes, by publish_czd
#   dagEdges     @6 :List(AtomDagEdge); # dependency edges, read off the lock
#   actionParams @7 :ActionParams;      # successor of composer/evalArgs

# PROPOSED successor to AtomId (append-only; keystone re-spec — ADR-0005,
# atom-sad §6.1 `[identity-content-addressed]`). The shipped `AtomId.digest`
# field (Implemented Schema, above) is deprecated per the note above; the
# target shape references the atom by the abstract pair, not a digest of it:
struct AtomId {
  set   @0 :Text;   # anchor hash of the atom-set (see AtomSetEntry.anchor)
  label @1 :Text;   # human-readable atom label within the set
}
```

Non-atom fetch dependencies continue to use the existing (shipped)
`deps: List(DepDescriptor)` field on `BuildRequest` — no new field is
needed for fetch-set references. Each non-atom `DepDescriptor` variant now
names a lock-side fetch-set entry that HTC's record/replay proxy serves
inside the executor's sandbox (htc-sad §4.2), rather than a URL eos itself
fetches.

The scheduler's `G∪` merge and `MergeRequest` handling (eos-scheduler.md)
are unaffected by this re-scope: `MergeRequest` is source-agnostic to where
a DAG fragment came from, and now merges fragments read off the lock rather
than reported by an eval worker.

#### Worker Protocol — PROPOSED (Post-ADR-0005)

The following interfaces are L3-internal (eos scheduler-to-worker and
worker-to-scheduler; ADR-0005 §9 layer renumbering). They are **NOT**
present in the implemented schema. **Status: PROPOSED**. There is exactly
one worker kind — the executor
worker (eos-sad §2.2, §8.3) — dispatching through HTC's executor trait
regardless of which implementation (primary FHS, or optional legacy
the removed passthrough executor's shape is retired, ADR-0006 §3) backs a given worker process.

```capnp
# Worker-facing types and interfaces — PROPOSED (Post-ADR-0005)
# These types are NOT in the implemented schema.

# Distinct from the client-facing BuildRequest (Implemented Schema, above)
# to avoid a name collision between the whole-DAG client submission and a
# single-action worker dispatch — two structurally different payloads.
struct WorkerBuildRequest {
  atomClosureRoot @0 :Data;           # atom_czd_closure_root (htc-sad §6.5)
  toolchainCompositionRoot @1 :Data;  # toolchain_composition_root (htc-sad §6.5)
  actionParams @2 :ActionParams;      # target system, variant flags
  jobId @3 :Data;                     # Scheduler-assigned job ID (= action_id)
  leaseId @4 :Data;                   # Lease token for health monitoring
}

struct WorkerBuildResult {
  union {
    success :group {
      outputTreeDigest @0 :Data;       # htc-sad §2.3 BuildRecord.output_tree_digest
      buildCompositionRoot @1 :Data;   # htc-sad §2.3 BuildRecord.build_composition_root
      observedReadSetDigest @2 :Data;  # htc-sad §2.3 BuildRecord.observed_read_set_digest
    }
    failure :group {
      error @3 :Text;                 # Build error message
      exitCode @4 :Int32;              # Builder exit code
    }
  }
}

interface WorkerRegistry {
  registerWorker @0 (worker :ExecutorWorker,
    caps :WorkerCapabilities)
    -> (registration :Registration);
}

# Returned to worker at registration time.
# Worker holds this capability and calls heartbeat() periodically
# (keepalive model, per [eos-scheduler-heartbeat-liveness]).
# Dropping this capability = deregistration.
interface Registration {
  heartbeat @0 () -> ();
  updateMeta @1 (meta :WorkerMeta) -> ();
}

# Held by the scheduler. Methods invoked by scheduler. Wraps whichever
# executor implementation (primary FHS, or optional legacy
# retired shapes) the worker process runs — opaque to the scheduler
# beyond the capability metadata advertised at registration (eos-sad §7.2).
interface ExecutorWorker {
  build @0 (request :WorkerBuildRequest) -> (result :WorkerBuildResult);
  cancel @1 (jobId :Data) -> ();
  attachProgress @2 (jobId :Data, callback :ProgressStream) -> ();
}
```

The bidirectional capability exchange — worker passes `ExecutorWorker` →
scheduler holds it; scheduler returns `Registration` → worker holds it;
connection break invalidates both — is unchanged from the pre-ADR-0005
design (eos-sad §7.1, §8.3).

### Schema–Type Correspondence

The Cap'n Proto schema defines the wire representation. The `eos-core` Rust
types define the behavioral contract. Both MUST remain synchronized.

**Implemented** (`eos/eos-proto/schema/eos.capnp` ↔ `eos/eos-core/src/request.rs`):

| Cap'n Proto Type     | `eos-core` Rust Type              | Role                                                            |
| :------------------- | :-------------------------------- | :-------------------------------------------------------------- |
| `PlanDigest`         | `Digest`                          | Content-addressed plan identifier                               |
| `BuildStatus`        | `JobStatus`                       | Job lifecycle state                                             |
| `ProgressStream`     | `ProgressEvent`                   | Streaming status callback                                       |
| `AtomSetEntry`       | `AtomSetInfo`                     | Atom-set declaration (anchor → tag + mirrors)                   |
| `DepDescriptor`      | `FetchDescriptor`                 | Pre-fetch dependency descriptor (union of atom/nix variants; `nix`/`nixGit`/`nixTar`/`nixSrc` deprecated — now HTC fetch-set entries, see §Deprecated Fields) |
| `ComposerSpec`       | `ComposerSpec`                    | **Deprecated** — superseded by `ActionParams` (see §Atom-DAG Intake Surface) |
| `BuildRequest`       | `BuildRequest<D: Digest>`         | Structured build request (plan digest + sets + deps + composer; `composer`/`evalArgs` deprecated, see §Atom-DAG Intake Surface for the PROPOSED successor fields) |
| `VersionInfo`        | `(version, rev, set)` fields      | Per-version atom metadata                                       |
| `EosDaemon`          | Daemon entry point                | Top-level client-facing RPC surface                             |
| `BuildJob`           | Job handle                        | Per-build capability (client-facing)                             |
| `AtomDiscovery`      | `AtomSource` (read-only)          | Atom resolution and search                                      |
| `AtomMeta`           | Atom metadata                     | Per-atom identity and version info                               |
| `AtomQuery`          | Search parameters                 | Discovery query constraints                                     |
| `KeyValue`           | `(String, String)`                | Generic key-value pair (deprecated `evalArgs`/`ComposerSpec.args` role; reused by PROPOSED `ActionParams.variantFlags`) |

The atom identifier struct (`atom-id` crate) appears throughout the schema
as the canonical atom reference type. It is an L1 contract type — not an
`eos-core` type — and is omitted from this table.

**PROPOSED** (Post-ADR-0005 — not yet in implemented schema):

| Cap'n Proto Type      | `eos-core` Rust Type       | Role                                                            |
| :--------------------- | :--------------------------- | :------------------------------------------------------------------ |
| `ActionParams`        | `ActionParams`              | Target system + variant flags (defined once, eos-build-engine.md) |
| `AtomDagNode`         | `AtomRef` + advisory weights | Atom-DAG submission node (re-scoped `PlanNode`)                  |
| `AtomDagEdge`         | DAG edge                    | Atom-DAG submission edge, read off the lock                     |
| `ExecutorWorker`      | Executor worker interface   | Scheduler-to-executor-worker RPC (internal)                      |
| `WorkerBuildRequest`  | Build job specification     | Atom closure root + toolchain composition root + action params + lease |
| `WorkerBuildResult`   | Build result                | `BuildRecord` fields (htc-sad §2.3) or error, from executor worker |

---

## Constraints

### Type Declarations

Network-level types expressed as Cap'n Proto schemas. The following
supplementary types constrain authentication and substitution:

```capnp
struct NodeIdentity {
  principalRoot @0 :Data;          # Cyphr sovereign principal (signing key anchor)
  timestamp @1 :UInt64;            # Unix epoch seconds
  signature @2 :Data;              # Signature over (principalRoot, timestamp, nonce)
  nonce @3 :Data;                  # Anti-replay nonce
}

struct HandshakeRequest {
  identity @0 :NodeIdentity;
  supportedBackends @1 :List(Text);
  apiVersion @2 :UInt32;
}

struct HandshakeResponse {
  accepted @0 :Bool;
  identity @1 :NodeIdentity;       # Server's sovereign identity
  reason @2 :Text;                 # Rejection reason (if !accepted)
}

struct OriginAttestation {
  builderId @0 :Data;              # NodeId (sovereign principal) of the builder
  planHash @1 :Data;               # Blake3 digest of the EnginePlan
  outputDigest @2 :Data;           # Blake3 digest of the build output
  signature @3 :Data;              # Builder's signature over (planHash, outputDigest)
  timestamp @4 :UInt64;            # Build completion time
}

struct SubstitutionQuery {
  planHash @0 :Data;               # Digest of the plan to substitute
  expectedOutputs @1 :List(Text);  # Expected store paths
}

struct SubstitutionResult {
  outputs @0 :List(OutputMapping);
  attestations @1 :List(OriginAttestation);
}

struct OutputMapping {
  storePath @0 :Text;
  contentDigest @1 :Data;          # Blake3 digest of the artifact content
}

interface SubstitutionService {
  query @0 (request :SubstitutionQuery) -> (result :SubstitutionResult);
  fetchArtifact @1 (contentDigest :Data) -> (stream :ArtifactStream);
  # PROPOSED: presence-only batch query for the scheduler's two-level
  # cache filter (one logical round trip; REVIEW.md §4).
  queryPresenceBatch @2 (request :BatchPresenceQuery) -> (result :BatchPresenceResult);
}

interface ArtifactStream {
  read @0 (maxBytes :UInt32) -> (data :Data, done :Bool);
}

# PROPOSED: types for the batched presence query.
# List of plan digests in, per-digest presence partition out.
struct BatchPresenceQuery {
  planDigests @0 :List(Data);   # Plan digests to check (Blake3)
}

struct BatchPresenceResult {
  present @0 :List(Data);       # Plan digests with available artifacts
  missing @1 :List(Data);       # Plan digests without cached artifacts
}
```

---

### Invariants

**[eos-network-sovereign-auth]**: All daemon connections and inter-node wire
sessions MUST authenticate using sovereign identities at Layer 1 (Cyphr
sovereign principals). Authentication proceeds via signed challenge-response
over `NodeIdentity` payloads. Eos MUST NOT accept connections authenticated
solely by web-PKI TLS certificates. The signing algorithm is determined by the
Cyphr cryptographic suite — implementations MUST NOT hardcode a specific curve
or scheme.
`VERIFIED: unverified`

**[eos-trustless-substitution]**: When fetching a pre-built artifact from a
remote substituter at a given store path, Eos MUST verify that the content
digest of the fetched artifact matches the expected digest derivable from the
verified `Plan`. Eos MUST NOT import substituted artifacts that fail this
content-address verification.
`VERIFIED: unverified`

**[eos-origin-attestation]**: A build artifact committed to a shared cache
MUST be accompanied by an `OriginAttestation`: a signature from the worker
node's `NodeIdentity` over the tuple `(PlanHash, OutputDigest)`. The
attestation MUST include a timestamp for freshness verification.
`VERIFIED: unverified`

**[eos-protocol-capability-matching]**: During the connection handshake, the
client and daemon MUST exchange `HandshakeRequest`/`HandshakeResponse`
payloads declaring supported backends (`supportedBackends`) and protocol
version (`apiVersion`). If no common backend exists, or if the API versions
are incompatible, the connection MUST be terminated with a rejection reason.
`VERIFIED: unverified`

**[eos-signature-freshness]**: `NodeIdentity` payloads and
`OriginAttestation` records MUST carry a timestamp and nonce. The receiving
node MUST reject payloads whose timestamp deviates from the receiver's system
clock by more than a configurable freshness window (default: 5 minutes).
Nonces MUST NOT be reused within the freshness window to prevent replay.
`VERIFIED: unverified`

**[eos-capability-lifecycle]**: A `BuildJob` capability returned by
`submitBuild` MUST remain valid for the duration of the job. Dropping the
capability reference (client disconnect or explicit release) MUST detach the
client from progress streaming but MUST NOT cancel or terminate the
underlying build. Cancellation MUST only occur via an explicit `cancel()`
invocation on the `BuildJob` capability.
`VERIFIED: unverified`

**[eos-progress-multiplexing]**: Multiple clients MUST be able to attach
`ProgressStream` callbacks to the same `BuildJob` concurrently. Each
attached callback MUST receive the same sequence of `BuildStatus` updates.
When a client drops its `ProgressStream` capability, the daemon MUST clean up
that callback's resources without disturbing other attached clients.
`VERIFIED: unverified`

**[eos-transport-agnosticism]**: The protocol layer MUST be decoupled from
the transport layer. All protocol operations MUST function identically over
any transport satisfying `AsyncRead + AsyncWrite`. Transport-specific
concerns (socket paths, TLS handshakes, Cyphr authentication) MUST be
resolved before the Cap'n Proto `TwoPartyVatNetwork` is instantiated.
`VERIFIED: unverified`

**[eos-wot-substitution-threshold]**: When strict substitution policy is
enabled, Eos MUST require that a substituted artifact carry attestations from
at least _M_ of _N_ configured trusted builders (Web of Trust threshold)
before accepting the artifact. The threshold values _M_ and _N_ are
deployment-configurable.
`VERIFIED: unverified`

**[eos-discovery-read-only]**: The `AtomDiscovery` capability MUST NOT expose
mutation operations. Discovery is strictly observation-only: `resolve`,
`contains`, and `search` are pure reads with no side effects on daemon state.
This is consistent with Eos consuming `AtomSource` (a read-only trait) per
the formal model. Any future method added to `AtomDiscovery` MUST preserve
this read-only invariant.
`VERIFIED: unverified`

---

### Transitions

**[daemon-startup]**: Initialize the Eos daemon and begin accepting
connections.

- **PRE**: A valid configuration exists specifying the transport endpoint
  (socket path or bind address), backend selection, and trust policy.
- **POST**: The daemon binds the transport endpoint, initializes the
  `BuildEngine` backend, starts the RPC event loop on a `LocalSet` thread,
  spawns the worker pool, and enters the listening state. The daemon MUST
  create the socket file (UDS) or bind the TCP port before signaling
  readiness.
  `VERIFIED: unverified`

**[client-connect]**: Establish an authenticated session between a client and
the daemon.

- **PRE**: The daemon is in the listening state. A client opens a transport
  connection (Unix stream or TCP stream).
- **POST**: The client and daemon exchange `HandshakeRequest` /
  `HandshakeResponse` payloads. If authentication succeeds and capabilities
  match, the connection is promoted to an authenticated Cap'n Proto RPC
  session. The client receives an `EosDaemon` bootstrap capability. If
  authentication fails, the connection is closed with a rejection reason.
  `VERIFIED: unverified`

**[submit-build]**: Submit a build request and receive a job capability.

- **PRE**: An authenticated client holds an `EosDaemon` capability.
- **POST**: The client invokes `submitBuild(request: BuildRequest)`. The
  `BuildRequest` carries the plan digest (a session-scoped deduplication key,
  not a semantic plan identifier), atom-set declarations, dependency
  descriptors, composer spec, and eval args. The daemon computes
  `JobId = hash(planDigest)` for deduplication. If a job with the same
  `JobId` already exists, the existing `BuildJob` capability is returned
  (deduplication). Otherwise, a new job is enqueued and a fresh `BuildJob`
  capability is returned. The build proceeds asynchronously.
  `VERIFIED: unverified`

**[attach-progress]**: Attach a progress callback to a running build.

- **PRE**: A client holds a `BuildJob` capability.
- **POST**: The client invokes `attachProgress(callback)`, passing a
  client-implemented `ProgressStream` capability. The daemon begins pushing
  `BuildStatus` updates via `callback.update()`. The `-> stream` annotation
  provides built-in backpressure. When the build completes, the daemon
  invokes `callback.done()`.
  `VERIFIED: unverified`

**[detach-progress]**: Detach from progress streaming without cancelling the
build.

- **PRE**: A client has an attached `ProgressStream` callback.
- **POST**: The client drops its `ProgressStream` capability. The Cap'n
  Proto runtime notifies the daemon of the dropped reference. The daemon
  removes that callback from the job's subscriber list and reclaims
  associated resources. The build continues unaffected.
  `VERIFIED: unverified`

**[cancel-build]**: Cancel a running build.

- **PRE**: A client holds a `BuildJob` capability for a job in `Queued` or
  `Building` state.
- **POST**: The client invokes `cancel()` on the `BuildJob`. The daemon
  transitions the job to `Cancelled` state and notifies all attached
  `ProgressStream` callbacks. In-flight build work is terminated. The
  `BuildJob` capability remains valid but subsequent operations return the
  `Cancelled` status.
  `VERIFIED: unverified`

**[request-substitute]**: Query remote caches for pre-built artifacts.

- **PRE**: An Eos daemon has a plan in the `NeedsBuild` state and at least
  one configured remote substituter.
- **POST**: The daemon sends a `SubstitutionQuery` to each configured
  `SubstitutionService`. If a valid `SubstitutionResult` is returned
  containing verified `OriginAttestation`s that satisfy the Web of Trust
  threshold, Eos fetches the artifact via `fetchArtifact()`, verifies the
  content digest, and bypasses local build execution.
  `VERIFIED: unverified`

**[client-disconnect]**: Graceful or abrupt session termination.

- **PRE**: A client has an active RPC session.
- **POST**: All capability references held by the client are dropped. The
  Cap'n Proto runtime cleans up associated server-side state (progress
  callbacks, pending responses). No running builds are cancelled — only
  explicit `cancel()` terminates builds.
  `VERIFIED: unverified`

**[daemon-shutdown]**: Graceful daemon termination.

- **PRE**: A shutdown signal is received (SIGTERM, explicit command).
- **POST**: The daemon stops accepting new connections, drains in-flight
  builds to completion (or cancels them per policy), notifies connected
  clients, closes all transport endpoints, and removes the socket file (UDS).
  `VERIFIED: unverified`

---

### Forbidden States

**[no-unattested-substitution]**: Eos MUST NOT accept artifacts from
substituters that lack valid `OriginAttestation`s when strict substitution
policy is enabled. Artifacts without attestations meeting the configured
Web of Trust threshold MUST be rejected.
`VERIFIED: unverified`

**[no-unencrypted-secrets]**: Worker nodes MUST NOT transmit private keys or
plaintext credentials over the network during build execution or session
establishment.
`VERIFIED: unverified`

**[no-unauthorized-handshake]**: A connection MUST NOT be promoted to an
authenticated RPC session if the `HandshakeRequest` signature does not
validate against the declared `NodeIdentity` (sovereign principal).
`VERIFIED: unverified`

**[no-cancel-on-drop]**: Dropping a `BuildJob` or `ProgressStream` capability
MUST NOT implicitly cancel the associated build. Only an explicit `cancel()`
invocation MAY terminate a build.
`VERIFIED: unverified`

**[no-unauthenticated-capability]**: The `EosDaemon` bootstrap capability
MUST NOT be issued to a connection that has not completed the authenticated
handshake. Unauthenticated transports MUST NOT expose any RPC surface.
`VERIFIED: unverified`

---

### Behavioral Properties

**[eventual-cache-consistency]**: If a build artifact is successfully pushed
to a remote binary cache, subsequent `SubstitutionQuery` requests for that
artifact's plan hash MUST return the artifact within a bounded propagation
delay.

- **Type**: Liveness
  `VERIFIED: unverified`

**[reproducible-build-consensus]**: For high-security environments, Eos MAY
schedule the same `Plan` on _N_ independent, distrusted worker nodes and
verify that the resulting output digests are identical (majority consensus)
before committing the output. This follows the Trustix model: builders
publish signed `PlanHash → OutputDigest` mappings, and clients enforce an
_M_-of-_N_ agreement threshold.

- **Type**: Safety
  `VERIFIED: unverified`

**[capability-cleanup-on-disconnect]**: When a client disconnects (gracefully
or abruptly), all server-side resources associated with that client's
capabilities MUST be reclaimed within a bounded interval. No resource leak
MAY persist after the Cap'n Proto runtime processes the disconnection.

- **Type**: Liveness
  `VERIFIED: unverified`

---

## Capability-Based Security Model

Cap'n Proto's object-capability model provides the security and lifecycle
semantics for Eos sessions. Capabilities are unforgeable references to
server-side objects — possession of a capability is both necessary and
sufficient for invoking the operations it exposes.

### Capability Hierarchy

```
EosDaemon (bootstrap)
  │
  ├── submitBuild(request: BuildRequest) ──→ BuildJob (per-job capability)
  │                        ├── attachProgress(callback) ──→ server holds ProgressStream ref
  │                        ├── cancel()
  │                        ├── getJobId()
  │                        └── getMissing() ──→ List(AtomId)
  │
  ├── discover() ──→ AtomDiscovery (read-only capability)
  │                     ├── resolve(id) ──→ AtomMeta
  │                     ├── contains(id) ──→ Bool
  │                     └── search(query) ──→ List(AtomMeta)
  │
  ├── queryStatus(jobId) ──→ BuildStatus (value, not capability)
  │
  └── getCapabilities() ──→ capability metadata (value)
```

### Lifecycle Semantics

1. **Submit.** Client invokes `EosDaemon.submitBuild(request: BuildRequest)`.
   The daemon returns a `BuildJob` capability — an opaque, unforgeable
   reference to the running job.

2. **Attach.** Client invokes `BuildJob.attachProgress(callback)`, passing a
   client-side `ProgressStream` implementation. The daemon holds a reference
   to this callback and pushes `BuildStatus` updates via
   `callback.update()`. The `-> stream` return annotation provides built-in
   flow control (backpressure).

3. **Detach.** Client drops its `ProgressStream` capability (or the client
   object goes out of scope). The Cap'n Proto runtime detects the dropped
   reference and notifies the daemon, which cleans up the callback. The
   build continues.

4. **Cancel.** Client invokes `BuildJob.cancel()`. The daemon transitions the
   job to `Cancelled` and notifies all attached `ProgressStream` callbacks
   via a final `update(cancelled)` followed by `done()`.

5. **Disconnect.** Client disconnects (network failure, process exit). All
   capabilities held by that client are implicitly dropped. Progress
   callbacks are cleaned up. Builds persist.

### Multi-Client Attach

Multiple clients MAY hold references to the same `BuildJob`. This arises
naturally from `JobId`-based deduplication: if two clients submit identical
plans, both receive capabilities referencing the same underlying job. Each
client independently attaches and detaches progress callbacks.

---

## Transport Layer

### Transport Evolution

The protocol is transport-agnostic by design. Cap'n Proto's
`TwoPartyVatNetwork` operates over any byte stream satisfying `AsyncRead +
AsyncWrite`.

| Version | Transport                                     | Authentication                          |
| :------ | :-------------------------------------------- | :-------------------------------------- |
| v1      | Unix domain socket (`tokio::net::UnixStream`) | Implicit (filesystem permissions)       |
| vN      | TCP socket (`tokio::net::TcpStream`)          | Cyphr authentication layer over raw TCP |

**v1: Unix Domain Socket.** The daemon creates a socket file at a
well-known path (configurable, default: `$XDG_RUNTIME_DIR/eos/eos.sock`).
Clients connect via `UnixStream`. Authentication in v1 relies on filesystem
permissions — only users with read/write access to the socket file can
connect. The `HandshakeRequest`/`HandshakeResponse` exchange still occurs,
establishing capability matching and API version agreement, but signature
verification MAY be relaxed for local UDS connections.

**vN: Authenticated TCP.** The daemon binds a TCP port. Before
instantiating the Cap'n Proto `TwoPartyVatNetwork`, both endpoints perform a
Cyphr authentication handshake directly on the raw `TcpStream`. This
handshake establishes mutual authentication via Principal Roots and
negotiates session keys. Once the Cyphr layer is established, the
authenticated stream is passed to `TwoPartyVatNetwork` as an opaque
`AsyncRead + AsyncWrite` transport.

### Transport Setup Sequence

```
Client                              Daemon
  │                                   │
  │── open transport ───────────────▸ │  (UnixStream::connect or TcpStream::connect)
  │                                   │
  │── [Cyphr auth handshake] ───────▸ │  (vN only: mutual authentication)
  │◂── [Cyphr auth response] ────── │
  │                                   │
  │═══ TwoPartyVatNetwork established ═══│
  │                                   │
  │── HandshakeRequest ────────────▸ │  (Cap'n Proto RPC: capability negotiation)
  │◂── HandshakeResponse ────────── │
  │                                   │
  │── [EosDaemon bootstrap cap] ───▸ │  (client receives bootstrap capability)
  │                                   │
```

---

## Daemon Architecture

### Daemon Lifecycle

1. **Configuration.** Parse daemon configuration: transport endpoint, backend
   selection, worker pool size, trust policy, substituter list.

2. **Service Connection.** Connect to HTC's shared CAS via gRPC URIs
   (configured blob, directory, and path-info service addresses; reused
   `snix-castore` services, htc-sad §2.4). Register with the executor
   worker pool via Cap'n Proto handshake. The scheduler holds no
   executor-implementation dependencies — all worker communication is via
   Cap'n Proto RPC.

3. **Transport Binding.** Create the transport endpoint (bind UDS or TCP).
   For UDS, create the socket file and set permissions. Signal readiness
   (e.g., `sd_notify` for systemd integration).

4. **Event Loop.** Enter the main accept loop. For each incoming connection:
   - Perform transport-level authentication (Cyphr for TCP, filesystem
     permissions for UDS).
   - Instantiate a `TwoPartyVatNetwork` over the authenticated stream.
   - Bootstrap the `EosDaemon` capability to the client.
   - Service RPC calls until the client disconnects.

5. **Shutdown.** On receiving a shutdown signal, stop accepting new
   connections, drain or cancel in-flight jobs per policy, close all
   sessions, remove the socket file, and exit.

### `!Send` Threading Model

The Cap'n Proto Rust RPC system (`capnp-rpc`) uses `Rc`-based internals and
is `!Send`. This is an **architectural constraint**, not a deficiency — it
enables zero-cost reference counting on the RPC event loop without atomic
operations.

The daemon accommodates this via a dedicated threading model:

```
┌─────────────────────────────────────────────┐
│  RPC Thread (tokio LocalSet)                │
│                                             │
│  ┌─────────────────────────────────┐        │
│  │ TwoPartyVatNetwork (per client) │        │
│  │   EosDaemon capability impl     │        │
│  │   BuildJob capability impls     │        │
│  │   ProgressStream dispatching    │        │
│  └───────────────┬─────────────────┘        │
│                  │ mpsc channels             │
└──────────────────┼──────────────────────────┘
                   │
    ┌──────────────┼──────────────────┐
    │              ▼                  │
    │  ┌───────────────────────┐     │
    │  │  Scheduler (Send)     │     │
    │  │  ┌─────────────────┐  │     │
    │  │  │ Executor Worker │  │     │
    │  │  │ Pool (Cap'n     │  │     │
    │  │  │ Proto)          │  │     │
    │  │  └─────────────────┘  │     │
    │  └───────────────────────┘     │
    │    tokio multi-thread runtime  │
    └────────────────────────────────┘
```

**RPC event loop:** Runs on a dedicated thread using `tokio::task::LocalSet`.
All `!Send` Cap'n Proto state (capability tables, `Rc`-based references,
`TwoPartyVatNetwork` instances) lives exclusively on this thread.

**Scheduler:** Runs on the standard `tokio` multi-threaded runtime. The
scheduler manages one Cap'n Proto worker pool (executor workers). All
worker communication is via Cap'n Proto RPC — the scheduler has no
executor-implementation dependencies and no in-process build state. Any
`!Send` constraint specific to a given executor implementation (e.g. the
retired executor state) is handled
internally within that worker's own process.

**Communication:** The RPC thread dispatches job requests to the scheduler
via `tokio::sync::mpsc` channels. The scheduler communicates with external
workers via Cap'n Proto RPC. Workers send status updates back via Cap'n
Proto capabilities, which the scheduler relays to the RPC thread for
forwarding to attached `ProgressStream` callbacks.

---

## Substitution Protocol

### Trustless Substitution Model

Eos supports a decentralized binary substitution network modeled after
[Trustix](https://github.com/nix-community/trustix): builders publish signed
`PlanHash → OutputDigest` mappings, and clients apply a configurable Web of
Trust threshold to decide whether to accept a substituted artifact.

### Substitution Flow

```
Eos Daemon                          SubstitutionService (remote)
  │                                           │
  │── SubstitutionQuery(planHash, outputs) ──▸│
  │◂── SubstitutionResult ──────────────────│
  │    { outputs: [...], attestations: [...] }│
  │                                           │
  │  [verify attestation signatures]          │
  │  [check WoT threshold: M-of-N]           │
  │  [verify content digests match plan]      │
  │                                           │
  │── fetchArtifact(contentDigest) ─────────▸│
  │◂── ArtifactStream.read() ──────────────│  (chunked transfer)
  │                                           │
  │  [verify fetched content matches digest]  │
  │  [import into ArtifactStore]              │
```

### Web of Trust Policy

The substitution trust model is deployment-configurable:

| Policy                  | Behavior                                                                                                                                |
| :---------------------- | :-------------------------------------------------------------------------------------------------------------------------------------- |
| **Trust-on-first-use**  | Accept any attested artifact. Suitable for single-builder local deployments.                                                            |
| **Named-builder trust** | Accept only attestations from an explicit set of trusted `NodeIdentity` Principal Roots.                                                |
| **M-of-N threshold**    | Require agreement from at least _M_ independent builders out of _N_ configured trust anchors. Catches non-deterministic builds.         |
| **N=2 double-build**    | Schedule the same plan on 2 independent workers; accept only if output digests agree. First hardening step beyond single-builder trust. |

### Attestation Chain

Each `OriginAttestation` binds:

- The builder's sovereign identity (`builderId`: Cyphr sovereign principal)
- The plan that was executed (`planHash`: Blake3 digest of the `Plan`)
- The output that was produced (`outputDigest`: Blake3 digest of the artifact)
- A timestamp for freshness verification
- The builder's signature over `(planHash, outputDigest)`

This structure follows the [in-toto](https://in-toto.io/) attestation model,
adapted for sovereign (Cyphr/Coz) signing rather than Sigstore/OIDC.

### Batched Presence Query — PROPOSED

**Status: PROPOSED** — `queryPresenceBatch` is not in the implemented schema;
`BatchPresenceQuery` and `BatchPresenceResult` are specified under Constraints
§ Type Declarations above.

The scheduler's two-level cache filter requires a single logical round trip
to determine which plans in a ready execution-point's scope are already
available as substitutable artifacts. The per-plan `query` method cannot
satisfy this in one round trip; the PROPOSED `queryPresenceBatch` method
provides a presence-only batch interface.

The scheduler sends a `BatchPresenceQuery` carrying the list of plan digests
to check. The `SubstitutionService` returns `present` and `missing`
partitions. The scheduler uses `missing` to identify plans requiring local
build execution; `present` plans can be fetched via the per-plan `query` +
`fetchArtifact` path (which provides the full `SubstitutionResult` with
`OriginAttestation` for trust verification).

This directly addresses finding F3 (REVIEW.md §4): the single-round-trip
intent of ADR-0004's batch cache filter is preserved without requiring a
batch RPC on the snix `PathInfoService` wire. A `SubstitutionService` shim
translates to bounded-concurrency per-digest `PathInfoService.Get` calls
internally; the scheduler's contract is "one logical batch round trip."

---

## Streaming Protocol

### Progress Streaming

Progress events flow from the daemon to attached clients via the
`ProgressStream` callback capability. The `-> stream` return annotation on
`ProgressStream.update()` provides Cap'n Proto's native backpressure
semantics — the daemon suspends sending if the client cannot consume updates
fast enough.

The `BuildStatus` union covers the complete job lifecycle:

| Variant      | Semantics                                                                |
| :----------- | :----------------------------------------------------------------------- |
| `queued`     | Job is waiting in the scheduler queue                                    |
| `evaluating` (deprecated) | No eos build past ADR-0005 produces this status; retained on the wire per append-only evolution (§Wire Format, item 5) for historical job records |
| `building`   | The action is being built; `phase` and `progress` carry build phase info |
| `completed`  | Build succeeded; `outputPaths` and `outputDigest` carry results          |
| `failed`     | Build failed; `error` and `exitCode` carry diagnostics                   |
| `cancelled`  | Build was explicitly cancelled via `BuildJob.cancel()`                   |

### Artifact Streaming

Artifact transfer (for substitution and cache distribution) uses the
`ArtifactStream` capability:

- `read(maxBytes)` returns a chunk of `data` and a `done` flag.
- The client reads in a loop until `done` is `true`.
- On error, the capability is dropped, and the transfer is aborted.
- Content integrity is verified after transfer completes by comparing the
  full content's Blake3 digest against the expected `contentDigest`.

---

## Verification

| Constraint                         | Method           | Result     | Detail                                                               |
| :--------------------------------- | :--------------- | :--------- | :------------------------------------------------------------------- |
| `eos-network-sovereign-auth`       | Unit tests       | UNVERIFIED | Challenge-response verification with Cyphr Principal Roots           |
| `eos-trustless-substitution`       | Integration test | UNVERIFIED | Inject corrupted artifact into cache, verify rejection               |
| `eos-origin-attestation`           | Signature check  | UNVERIFIED | Verify `OriginAttestation` signature validation                      |
| `eos-protocol-capability-matching` | Handshake test   | UNVERIFIED | Capability mismatch → connection rejection                           |
| `eos-signature-freshness`          | Replay test      | UNVERIFIED | Replay expired `NodeIdentity` payload, verify rejection              |
| `eos-capability-lifecycle`         | Integration test | UNVERIFIED | Drop `BuildJob` cap, verify build continues                          |
| `eos-progress-multiplexing`        | Integration test | UNVERIFIED | Attach two clients to same job, verify both receive events           |
| `eos-transport-agnosticism`        | Integration test | UNVERIFIED | Run identical test suite over UDS and TCP transports                 |
| `eos-wot-substitution-threshold`   | Policy test      | UNVERIFIED | Configure M-of-N, inject insufficient attestations, verify rejection |
| `daemon-startup`                   | Integration test | UNVERIFIED | Verify socket creation and readiness signaling                       |
| `client-connect`                   | Unit test        | UNVERIFIED | Verify handshake transitions (success and rejection)                 |
| `submit-build`                     | Integration test | UNVERIFIED | Submit identical plans, verify deduplication                         |
| `attach-progress`                  | Integration test | UNVERIFIED | Attach callback, verify `BuildStatus` delivery                       |
| `detach-progress`                  | Integration test | UNVERIFIED | Drop `ProgressStream`, verify cleanup without build interruption     |
| `cancel-build`                     | Integration test | UNVERIFIED | Cancel via capability, verify job transitions to `Cancelled`         |
| `request-substitute`               | Integration test | UNVERIFIED | Mock substituter, verify attestation and digest checks               |
| `client-disconnect`                | Integration test | UNVERIFIED | Abrupt disconnect, verify resource cleanup                           |
| `daemon-shutdown`                  | Integration test | UNVERIFIED | SIGTERM, verify graceful drain and socket removal                    |
| `no-unattested-substitution`       | Policy audit     | UNVERIFIED | Verify rejection when attestations are missing                       |
| `no-unencrypted-secrets`           | Code audit       | UNVERIFIED | Static analysis for credential exposure in wire payloads             |
| `no-unauthorized-handshake`        | Signature check  | UNVERIFIED | Invalid signature → connection rejection                             |
| `no-cancel-on-drop`                | Integration test | UNVERIFIED | Drop all client capabilities, verify build completes                 |
| `no-unauthenticated-capability`    | Integration test | UNVERIFIED | Skip handshake, verify RPC calls are rejected                        |
| `eventual-cache-consistency`       | Integration test | UNVERIFIED | Cache push → propagation delay → query verification                  |
| `reproducible-build-consensus`     | Consensus test   | UNVERIFIED | Dual-build with injected non-determinism, verify detection           |
| `capability-cleanup-on-disconnect` | Stress test      | UNVERIFIED | Rapid connect/disconnect cycles, verify no resource leaks            |
| `eos-discovery-read-only`          | API audit        | UNVERIFIED | Verify `AtomDiscovery` exposes no mutation operations                |

---

## Implications

1. **Sovereign Cryptography Integration.**
   The entire authentication surface relies on Cyphr/Coz cryptography. The
   signing algorithm, key types, and identity model are determined by the
   Cyphr suite — this spec deliberately avoids hardcoding `ed25519` or any
   specific curve. Implementations MUST use the `Digest` trait seam for
   algorithm agility, consistent with the Cyphr transition plan.

2. **Cap'n Proto Constraints Shape Daemon Architecture.**
   The `!Send` nature of `capnp-rpc` is not a limitation to work around but
   an architectural driver. The dedicated RPC thread + channel-based
   scheduler dispatch pattern is the canonical design for Cap'n Proto
   daemons. Under the executor-trait architecture, all executor-
   implementation state (including any `!Send` constraints specific to a
   given executor) is
   fully encapsulated within executor worker processes — the daemon itself
   holds no executor-implementation state at all.

3. **Decentralized Substitution Networks.**
   Because caches are content-addressed and verified via plan-to-output
   attestation chains, binary distribution can be entirely peer-to-peer.
   Worker nodes can serve as substituters for one another without central
   registration. The Trustix-style Web of Trust threshold provides
   configurable security guarantees without requiring a global consensus
   protocol.

4. **Reproducibility Audit Trail.**
   The `OriginAttestation` chain creates a cryptographic provenance record.
   If a compromised artifact is detected (e.g., by rebuilding the plan
   locally and observing a digest mismatch), the attestation signature
   identifies the responsible builder node (`NodeIdentity`), enabling
   immediate revocation from the trust group.

5. **Transport Evolution Path.**
   The v1 UDS transport is deliberately minimal — filesystem permissions
   suffice for local single-user operation. The vN TCP transport with Cyphr
   authentication extends the same protocol to multi-machine deployments
   without protocol-level changes. The transport-agnostic design means a
   future transport (e.g., QUIC with Cyphr auth) requires only a new
   connection setup function, not a protocol revision.

6. **Cross-Language Client Path.**
   Cap'n Proto has variable cross-language support. If Go or Python frontends
   become necessary, a protocol translation proxy (Cap'n Proto ↔ gRPC)
   provides a clean migration path without altering the daemon's internal
   protocol. This is a vN concern — all foreseeable frontends are Rust.
