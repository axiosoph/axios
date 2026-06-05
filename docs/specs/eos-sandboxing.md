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

**Problem Domain:** Hermetic evaluation and build execution represent core security boundaries in Eos (L2). The stack must prevent host system contamination, arbitrary filesystem access, and network leaks during build execution.

Under the gRPC-first integration architecture ([ADR-0002](../adr/0002-decoupling-snix-backend.md)), sandboxing responsibilities are narrowly scoped:

1. **Evaluation Isolation**: Snix's pure evaluation model confines the evaluator to the atom's encapsulation boundary. The evaluator MUST NOT import code or data external to the atom being evaluated (with the exception of content-addressed fetches, which are safe by construction — they fail if the content does not match the pre-declared hash). This language-level confinement eliminates the need for OS-level process sandboxing during evaluation.
2. **Build Sandboxing**: Build execution is sandboxed by the snix builder process itself (OCI runtime, Bubblewrap, or remote delegation). Eos build worker shims forward derivations to snix builders via gRPC; sandboxing is the builder's concern.

The Eos daemon (scheduler) has zero snix dependencies and performs no sandboxing itself. It dispatches jobs to eval workers and build workers via Cap'n Proto RPC.

**Model Reference:**

- [eos-build-engine.md](eos-build-engine.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore)
- [eos-snix-backend.md](eos-snix-backend.md) — Eval worker threading and store access
- [ADR-0002](../adr/0002-decoupling-snix-backend.md) — gRPC-first snix integration architecture

**Criticality Tier:** High — build hermeticity governs the reproducibility of the entire stack. Failure to isolate build execution permits malicious actors to execute arbitrary code with host-level privileges.

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
|   - Runs snix-eval in pure eval mode                  |
|   - Confined to atom encapsulation boundary           |
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

### Evaluation Isolation Invariants

**[eos-eval-worker-isolation]**: Eos MUST NOT execute evaluations within the daemon process. All evaluations MUST be dispatched to registered eval workers via the `EvalWorker` Cap'n Proto interface. Eval workers are separate, long-lived processes managed by external orchestrators (systemd, process-compose, Kubernetes).
`VERIFIED: unverified`

**[eos-eval-worker-lifecycle]**: The Eos daemon MUST NOT manage eval worker lifecycles (starting, stopping, restarting). Workers register with the scheduler via Cap'n Proto handshake at startup. Worker health is monitored via heartbeats and lease expiry (see [eos-scheduler.md](eos-scheduler.md)).
`VERIFIED: unverified`

**[eos-eval-pure-eval]**: Eval workers MUST run snix in pure evaluation mode. Pure evaluation confines the evaluator to the atom's encapsulation boundary — the evaluator MUST NOT import code or data external to the atom being evaluated. Content-addressed fetches (where a hash is pre-declared) are permitted because they are safe by construction: the fetch fails if the content does not match the declared hash, guaranteeing reproducibility. This language-level confinement eliminates the need for OS-level process sandboxing (Bubblewrap, Birdcage) during evaluation.
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
- **POST**: The scheduler selects an eval worker via Rendezvous hashing and sends the request via Cap'n Proto `EvalWorker.evaluate()`. The eval worker processes the request in pure eval mode and returns a `Derivation` (or error) via Cap'n Proto.
  `VERIFIED: unverified`

---

## Forbidden States

**[no-eval-daemon-process]**: Evaluations MUST NOT execute within the Eos daemon process. The daemon dispatches all evaluation work to external eval workers via Cap'n Proto.
`VERIFIED: unverified`

**[no-eval-external-imports]**: During pure evaluation, the evaluator MUST NOT import code or data external to the atom's encapsulation boundary. Any attempt to access paths outside the atom MUST result in an evaluation error, not silent fallback to host filesystem access.
`VERIFIED: unverified`

---

## Verification

| Constraint                              | Method             | Result     | Detail                                                                          |
| :-------------------------------------- | :----------------- | :--------- | :------------------------------------------------------------------------------ |
| `eos-eval-worker-isolation`             | Process inspection | UNVERIFIED | Verify daemon dispatches to external worker, no in-process eval                 |
| `eos-eval-worker-lifecycle`             | Integration test   | UNVERIFIED | Verify daemon does not spawn/stop workers; workers register via Cap'n Proto     |
| `eos-eval-pure-eval`                    | Eval boundary test | UNVERIFIED | Attempt external imports during pure eval, verify rejection                     |
| `eos-build-sandbox-delegation`          | Architecture audit | UNVERIFIED | Verify Eos delegates build sandboxing to snix builder, no in-process sandboxing |
| `eos-build-sandbox-network-containment` | Build socket test  | UNVERIFIED | Verify snix builder enforces network containment for non-FOD builds             |

---

## Implications

1. **No OS-Level Eval Sandboxing Required**:
   Snix's pure evaluation model provides language-level confinement. The evaluator cannot access files outside the atom's encapsulation boundary. Content-addressed fetches are safe by construction. This eliminates the need for Bubblewrap or Birdcage sandboxing of eval workers, reducing operational complexity (no bwrap dependency, no sandbox configuration, no namespace management).

2. **FUSE Mounts in Build Sandboxes**:
   Build sandboxes (managed by the snix builder process) rely on FUSE to mount input directory trees from the content-addressed store. This requires the host kernel to support FUSE and the current user to have access to `/dev/fuse`. These requirements apply to the machine running the snix builder, not the Eos daemon.

3. **Long-Lived Eval Workers**:
   Eval workers are persistent processes that handle many evaluations over their lifetime. There is no per-evaluation process spawn overhead. The evaluation cache is consulted by the scheduler before dispatching to a worker, avoiding unnecessary round-trips.

4. **Eval Cache and Dispatch Interaction**:
   The evaluation cache MUST be consulted by the scheduler _before_ dispatching to an eval worker. If the cache hits, no worker dispatch occurs. The cache is trusted data in the daemon's process space.
