# SPEC: Eos Build and Evaluation Sandboxing

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

**Problem Domain:** Hermetic evaluation and build execution represent core security boundaries in Eos (L2). Because evaluating code (Nix/Snix expressions) or building outputs (executing plan builders) requires interacting with arbitrary execution parameters, we must prevent host system contamination, arbitrary filesystem access, and network leaks. Eos enforces strict containment boundaries through isolated worker processes.

Under the gRPC-first integration architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), sandboxing responsibilities are distributed:

1. **Evaluation Sandboxing**: Eval worker processes MAY self-sandbox (using Bubblewrap on Linux or Birdcage on macOS) before accepting Cap'n Proto connections. Eval workers are long-lived processes managed by external orchestrators — the Eos daemon does NOT spawn them.
2. **Build Sandboxing**: Build execution is sandboxed by the snix builder process itself (OCI runtime, Bubblewrap, or remote delegation). Eos build worker shims forward derivations to snix builders via gRPC; sandboxing is the builder's concern.

The Eos daemon (scheduler) has zero snix dependencies and performs no sandboxing itself. It dispatches jobs to eval workers and build workers via Cap'n Proto RPC.

**Model Reference:**

- [eos-build-engine.md](eos-build-engine.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore)
- [eos-snix-backend.md](eos-snix-backend.md) — Eval worker threading and store access
- [ADR-0002](../adr/0002-decoupling-snix-backend.md) — gRPC-first snix integration architecture

**Criticality Tier:** High — security and hermeticity govern the reproducibility of the entire stack. Failure to isolate execution permits malicious actors to execute arbitrary code with host-level privileges.

---

## Constraints

### Worker Execution Model

Eos delegates evaluation and build execution to external worker processes. The daemon does not spawn workers — it discovers them via Cap'n Proto handshake.

```
+-------------------------------------------------------+
| Eos Daemon (scheduler)                                |
|   1. Receives EvalRequest from client (Cap'n Proto)   |
|   2. Checks eval cache (plan already computed?)       |
|   3. Dispatches to eval worker pool (Cap'n Proto)     |
+-----------------------+-------------------------------+
                        |
          Cap'n Proto   | EvalWorker.evaluate(request)
                        v
+-------------------------------------------------------+
| Eval Worker Process (long-lived)                      |
|   - Started by external orchestrator                  |
|   - MAY self-sandbox on startup (bwrap / birdcage)    |
|   - Runs snix-eval on dedicated OS thread             |
|   - Connects to snix store daemons via gRPC           |
+-------------------------------------------------------+
                        |
          Cap'n Proto   | EvalResult { derivation, ... }
                        v
+-----------------------+-------------------------------+
| Eos Daemon (scheduler)                                |
|   4. Caches computed Plan (Derivation)                |
|   5. Dispatches to build worker pool (Cap'n Proto)    |
+-------------------------------------------------------+
```

### Cap'n Proto Worker Interfaces

Eval workers and build workers communicate with the scheduler via Cap'n Proto RPC:

```capnp
interface EvalWorker {
  evaluate @0 (request :EvalRequest) -> (result :EvalResult);
  heartbeat @1 () -> ();
}

interface BuildWorker {
  build @0 (request :BuildRequest) -> (result :BuildResult);
  cancel @1 (jobId :Data) -> ();
  attachProgress @2 (jobId :Data, callback :ProgressStream) -> ();
  heartbeat @3 () -> ();
}
```

Eval workers return a computed `Derivation` (ATerm bytes). Build workers wrap snix's gRPC `BuildService.DoBuild()` in the Cap'n Proto interface, adding cancellation, progress streaming, and lease management.

---

## Invariants

### Evaluation Sandboxing Invariants

**[eos-eval-worker-isolation]**: Eos MUST NOT execute evaluations within the daemon process. All evaluations MUST be dispatched to registered eval workers via the `EvalWorker` Cap'n Proto interface. Eval workers are separate, long-lived processes managed by external orchestrators (systemd, process-compose, Kubernetes).
`VERIFIED: unverified`

**[eos-eval-worker-lifecycle]**: The Eos daemon MUST NOT manage eval worker lifecycles (starting, stopping, restarting). Workers register with the scheduler via Cap'n Proto handshake at startup. Worker health is monitored via heartbeats and lease expiry (see [eos-scheduler.md](eos-scheduler.md)).
`VERIFIED: unverified`

**[eos-eval-sandbox-network-containment]**: Eval workers SHOULD restrict external network access to the minimum required for operation. The eval worker MUST have access to snix store daemon gRPC endpoints and the scheduler's Cap'n Proto endpoint, but SHOULD NOT have general internet access unless evaluating fixed-output derivations.
`VERIFIED: unverified`

**[eos-eval-sandbox-host-isolation]**: Eval workers SHOULD NOT have write permissions to the host filesystem, except for designated temporary directories used for evaluation state. When self-sandboxing is enabled, the worker process applies sandbox restrictions before accepting Cap'n Proto connections.
`VERIFIED: unverified`

**[eos-eval-sandbox-linux-bwrap]**: On Linux, eval workers MAY self-sandbox using Bubblewrap (`bwrap`) at startup. When enabled, the Bubblewrap jail MUST enforce:

- Bind read-only views of system libraries (`/usr`, `/bin`, `/lib`, `/lib64`).
- Bind writable paths to the eval worker's temporary workdir.
- Allow network access to configured gRPC endpoints (snix store daemon) and the scheduler's Cap'n Proto endpoint.
- Unshare PID, IPC, and UTS namespaces. Network namespace is NOT unshared (gRPC/Cap'n Proto connectivity is required).
  `VERIFIED: unverified`

**[eos-eval-sandbox-macos-birdcage]**: On macOS, eval workers MAY self-sandbox using Birdcage (`birdcage`) at startup. When enabled, the Birdcage jail MUST enforce:

- Grant read-only access to system libraries and the eval worker executable.
- Grant read/write access to the eval worker's temporary workdir.
- Allow network access to configured gRPC and Cap'n Proto endpoints.
- Deny write access to all other host directories.
  `VERIFIED: unverified`

### Build Sandboxing Invariants

**[eos-build-sandbox-delegation]**: Under the gRPC-first architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), build sandboxing is the responsibility of the snix builder process, NOT the Eos daemon or build worker shim. The Eos scheduler dispatches derivations to build workers via Cap'n Proto; the build worker shim forwards to snix builders via gRPC. The snix builder applies platform-appropriate sandboxing (OCI runtime, Bubblewrap, or future alternatives).
`VERIFIED: unverified`

**[eos-build-sandbox-network-containment]**: Build execution MUST NOT have access to the external network, unless the plan is explicitly declared as a fixed-output derivation containing a pre-computed hash of the expected artifact. This invariant is enforced by the snix builder's sandbox backend, not by the Eos daemon.
`VERIFIED: unverified`

**[eos-build-sandbox-host-isolation]**: Build execution MUST NOT write outside the temporary directory allocated for the build task. Inputs MUST be mounted read-only. This invariant is enforced by the snix builder's sandbox backend.
`VERIFIED: unverified`

---

## Transitions

### Evaluation Dispatch Transition

**[eval-worker-dispatch]**: Dispatches an evaluation request to an eval worker.

- **PRE**: The scheduler holds an `EvalRequest`. The eval cache has been consulted and no cached plan exists. At least one healthy eval worker is registered.
- **POST**: The scheduler selects an eval worker via Rendezvous hashing and sends the request via Cap'n Proto `EvalWorker.evaluate()`. The eval worker processes the request and returns a `Derivation` (or error) via Cap'n Proto.
  `VERIFIED: unverified`

### Eval Worker Self-Sandbox Transition

**[eval-worker-self-sandbox]**: An eval worker MAY apply sandbox restrictions at startup.

- **PRE**: The eval worker process has started (launched by external orchestrator). Platform-specific sandbox tools are available.
- **POST**: If sandboxing is enabled, the eval worker applies bwrap (Linux) or birdcage (macOS) restrictions to its own process. The worker then connects to the scheduler via Cap'n Proto and begins accepting evaluation requests.
  `VERIFIED: unverified`

---

## Forbidden States

**[no-unbounded-eval-io]**: When self-sandboxing is enabled, the eval worker process MUST NOT access `/etc`, `/home`, or host system configuration directories not explicitly whitelisted. The allowed access is:

- `/usr`, `/bin`, `/lib`, `/lib64` (read-only) — system binaries and libraries
- The eval worker's temporary workdir (read-write) — evaluation state
- gRPC endpoints to snix store daemons (network) — store access
- Cap'n Proto endpoint to scheduler (network) — job dispatch

All other host filesystem paths MUST be denied.
`VERIFIED: unverified`

**[no-eval-daemon-process]**: Evaluations MUST NOT execute within the Eos daemon process. The daemon dispatches all evaluation work to external eval workers via Cap'n Proto.
`VERIFIED: unverified`

---

## Verification

| Constraint                              | Method                  | Result     | Detail                                                                          |
| :-------------------------------------- | :---------------------- | :--------- | :------------------------------------------------------------------------------ |
| `eos-eval-worker-isolation`             | Process inspection      | UNVERIFIED | Verify daemon dispatches to external worker, no in-process eval                 |
| `eos-eval-worker-lifecycle`             | Integration test        | UNVERIFIED | Verify daemon does not spawn/stop workers; workers register via Cap'n Proto     |
| `eos-eval-sandbox-network-containment`  | Network socket test     | UNVERIFIED | Verify eval worker restricts network to gRPC/Cap'n Proto endpoints              |
| `eos-eval-sandbox-host-isolation`       | Write restriction check | UNVERIFIED | Attempt writes to `/var` or `/tmp` from self-sandboxed worker, verify error     |
| `eos-eval-sandbox-linux-bwrap`          | Sandbox mount check     | UNVERIFIED | Audit Bubblewrap argument vectors when self-sandboxing is enabled               |
| `eos-eval-sandbox-macos-birdcage`       | Sandbox profile audit   | UNVERIFIED | Audit Birdcage exception rules when self-sandboxing is enabled                  |
| `eos-build-sandbox-delegation`          | Architecture audit      | UNVERIFIED | Verify Eos delegates build sandboxing to snix builder, no in-process sandboxing |
| `eos-build-sandbox-network-containment` | Build socket test       | UNVERIFIED | Verify snix builder enforces network containment for non-FOD builds             |

---

## Implications

1. **Host Dependency Requirements**:
   If eval workers enable self-sandboxing on Linux, the host machine must have Bubblewrap (`bwrap`) installed. On macOS, the eval worker binary links against native Apple sandbox libraries through `birdcage`. Build sandboxing dependencies (FUSE, OCI runtimes) are the snix builder's concern.

2. **FUSE Mounts in Build Sandboxes**:
   Build sandboxes (managed by the snix builder process) rely on FUSE to mount input directory trees from the content-addressed store. This requires the host kernel to support FUSE and the current user to have access to `/dev/fuse`. These requirements apply to the machine running the snix builder, not the Eos daemon.

3. **Long-Lived Eval Workers**:
   Eval workers are persistent processes that handle many evaluations over their lifetime. There is no per-evaluation process spawn overhead. Self-sandboxing (bwrap/birdcage) is applied once at worker startup, not per evaluation. The evaluation cache is consulted by the scheduler before dispatching to a worker, avoiding unnecessary round-trips.

4. **Eval Cache and Dispatch Interaction**:
   The evaluation cache MUST be consulted by the scheduler _before_ dispatching to an eval worker. If the cache hits, no worker dispatch occurs. The cache is trusted data in the daemon's process space.
