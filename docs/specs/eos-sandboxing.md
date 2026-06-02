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

**Problem Domain:** Hermetic evaluation and build execution represent core security boundaries in Eos (L2). Because evaluating code (Nix/Snix expressions) or building outputs (executing plan builders) requires interacting with arbitrary execution parameters, we must prevent host system contamination, arbitrary filesystem access, and network leaks. Eos enforces strict containment boundaries by executing evaluations and builds inside isolated sandboxes. 

We distinguish between two sandboxing boundaries:
1. **Evaluation Sandboxing**: Isolates the language evaluator during expression-to-plan translation to prevent host file disclosure and impurity leaks.
2. **Build Sandboxing**: Isolates the build process when applying a plan to produce concrete artifacts.

To reconcile the thread-locality (`!Send`) of the language evaluator with the concurrent host daemon (`eosd`), Eos launches a sandboxed sub-process executing a hidden `--eval-worker` subcommand. This worker operates inside Bubblewrap on Linux and Birdcage on macOS, exchanging serialized request and result payloads via standard I/O streams.

**Model Reference:**
- [eos-build-engine.md](eos-build-engine.md) — §2.4 (BuildEngine), §2.5 (ArtifactStore)
- [eos-snix-backend.md](eos-snix-backend.md) — Snix-specific execution bounds and store mappings

**Criticality Tier:** High — security and hermeticity govern the reproducibility of the entire stack. Failure to isolate execution permits malicious actors to execute arbitrary code with host-level privileges.

---

## Constraints

### Subprocess Execution Model

Eos executes language evaluation out-of-process to isolate thread-local memory structures and sandbox permissions.

```
+-------------------------------------------------------+
| Eos Daemon (eosd)                                     |
|   1. Receives EvalRequest                             |
|   2. Spawns sandboxed worker                          |
|   3. Passes EvalRequestDto via Stdin                  |
+---------------------------+---------------------------+
                            |
                     Stdin  | JSON payload
                            v
+-------------------------------------------------------+
| Sandboxed Worker Subprocess (std::env::current_exe()) |
|   Arg: --eval-worker                                  |
|                                                       |
|   [ Linux Bubblewrap ]     [ macOS Birdcage ]         |
|   - Namespace isolation    - Seatbelt sandbox policy  |
|   - Mount namespaces       - Path exception whitelist |
|   - Network disabled       - Network socket block     |
+---------------------------+---------------------------+
                            |
                    Stdout  | ATerm derivation bytes
                            v
+-------------------------------------------------------+
| Eos Daemon (eosd)                                     |
|   4. Parses stdout into Plan (Derivation)             |
|   5. Registers computed Plan in cache                 |
+-------------------------------------------------------+
```

### Standard I/O Protocol Schema

The daemon and worker subprocess communicate strictly via standard input/output channels:

```
-- Input JSON payload written to worker stdin
TYPE EvalRequestDto = {
    expression: EvalTargetDto,
    inputs: Map<String, ResolvedInputDto>,
    composer: Option<ComposerConfigDto>,
    eval_args: Vec<(String, String)>
}

TYPE EvalTargetDto =
    File(PathBuf)
  | Expression(String)

TYPE ResolvedInputDto = {
    digest: String,
    store_path: String
}

-- Output ATerm representation written to worker stdout
-- (Nix-compatible canonical derivation text format)
```

---

## Invariants

### Evaluation Sandboxing Invariants

**[eos-eval-worker-isolation]**: Eos MUST NOT execute evaluations within the primary daemon thread or process. All evaluations MUST be delegated to a dedicated worker subprocess invoked with the `--eval-worker` parameter.
`VERIFIED: unverified`

**[eos-eval-worker-executable]**: The worker subprocess MUST be launched using the exact executable path of the running daemon, retrieved via `std::env::current_exe()`.
`VERIFIED: unverified`

**[eos-eval-sandbox-network-containment]**: The evaluation worker subprocess MUST NOT have access to the external network. Network namespace sharing or socket creation MUST be disabled.
`VERIFIED: unverified`

**[eos-eval-sandbox-host-isolation]**: The evaluation worker subprocess MUST NOT have write permissions to the host filesystem, except for designated temporary directories explicitly allocated for evaluation state.
`VERIFIED: unverified`

**[eos-eval-sandbox-linux-bwrap]**: On Linux, evaluation sandboxing MUST employ Bubblewrap (`bwrap`). The Bubblewrap jail MUST enforce the following constraints:
- Bind a read-only view of the Axios workspace directory.
- Bind a read-only view of the parent directory containing the target evaluation file.
- Bind writable paths to the temporary sandbox workdir.
- Bind writable paths to the local database sockets and directories (e.g., Redb stores).
- Unshare all namespaces (network, IPC, user, PID, UTS) except for paths mapped to standard descriptors.
`VERIFIED: unverified`

**[eos-eval-sandbox-macos-birdcage]**: On macOS, evaluation sandboxing MUST employ Birdcage (`birdcage`). The Birdcage jail MUST enforce the following constraints:
- Grant read-only access to the Axios workspace directory and the daemon executable.
- Grant read/write access to local database sockets and directories.
- Deny network access (excluding Unix sockets).
- Deny write access to all other host directories.
- Execute within a `tokio::task::spawn_blocking` pool to isolate synchronous seatbelt profile interactions.
`VERIFIED: unverified`

### Build Sandboxing Invariants

**[eos-build-sandbox-platform-dispatch]**: Eos MUST dispatch build sandboxes based on platform availability:
- **Linux**: Select OCI runtimes (`crun`, `runc`) if present; fall back to Bubblewrap (`bwrap`).
- **macOS**: Delegate execution to a remote builder, or report an error if no remote builder is configured. Local build sandboxing via Birdcage is reserved as a future extension.
`VERIFIED: unverified`

**[eos-build-sandbox-network-containment]**: Build execution MUST NOT have access to the external network, unless the plan is explicitly declared as a fixed-output derivation containing a pre-computed hash of the expected artifact.
`VERIFIED: unverified`

**[eos-build-sandbox-host-isolation]**: Build builders MUST NOT write outside the temporary directory allocated for the build task. Inputs MUST be mounted read-only.
`VERIFIED: unverified`

---

## Transitions

### Evaluation Pipeline Transition

**[eval-sandbox-spawn]**: Spawns the sandboxed evaluator process.

- **PRE**: The `EvalRequest` is constructed. If Linux, `bwrap` is installed. If macOS, `birdcage` support is compiled. Local DB paths are resolved.
- **POST**: The child subprocess is spawned with the `--eval-worker` argument and sandbox-specific limits. Stdin and stdout pipes are successfully initialized.
  `VERIFIED: unverified`

**[eval-sandbox-exchange]**: Transmits the request and retrieves the computed plan.

- **PRE**: The child process is running. The daemon holds the serialized JSON payload.
- **POST**: The JSON payload is written to stdin. The daemon blocks on the worker's stdout stream. The worker executes the Nix/Snix evaluation, prints the ATerm serialization of the computed `Plan` to stdout, and exits with code 0.
  `VERIFIED: unverified`

---

## Forbidden States

**[no-unbounded-eval-io]**: The evaluation worker subprocess MUST NOT access `/etc`, `/home`, or host system configuration directories not explicitly whitelisted in the sandbox exceptions list.
`VERIFIED: unverified`

**[no-shared-network-sockets]**: The evaluation worker MUST NOT inherit network file descriptors or access the loopback interface (`127.0.0.1`), preventing IPC leak paths to host-level daemons.
`VERIFIED: unverified`

---

## Verification

| Constraint | Method | Result | Detail |
| :--- | :--- | :--- | :--- |
| `eos-eval-worker-isolation` | Process inspection | UNVERIFIED | Verify that evaluation spawns a separate OS process |
| `eos-eval-sandbox-network-containment` | Network socket test | UNVERIFIED | Attempt network requests inside the sandbox, verify abort |
| `eos-eval-sandbox-host-isolation` | Write restriction check | UNVERIFIED | Attempt writes to `/var` or `/tmp` from the worker, verify error |
| `eos-eval-sandbox-linux-bwrap` | Sandbox mount check | UNVERIFIED | Audit Bubblewrap argument vectors for compliance |
| `eos-eval-sandbox-macos-birdcage` | Sandbox profile audit | UNVERIFIED | Audit Birdcage exception rules and profile constraints |
| `eos-build-sandbox-network-containment` | Build socket test | UNVERIFIED | Attempt network access in builds without pre-declared outputs, verify abort |

---

## Implications

1. **Host Dependency Requirements**:
   To perform local evaluation on Linux, the host machine must have Bubblewrap (`bwrap`) installed and present in the system `$PATH`. On macOS, the compiled binary links against the native Apple sandbox libraries through the `birdcage` dependency, meaning no external tools are needed for macOS evaluations.

2. **FUSE Mounts in Build Sandboxes**:
   Build sandboxes (such as `OCIBuildService` and `BubblewrapBuildService`) rely on FUSE to mount input directory trees from the content-addressed store. This requires the host kernel to support FUSE and the current user to have access to `/dev/fuse`.

3. **Performance Overhead**:
   Spawning a fresh process for each evaluation introduces execution overhead. Eos mitigates this cost by maintaining a persistent evaluation cache. We bypass the worker subprocess entirely if the cache key (the snapshot digest combined with evaluation arguments) already maps to a pre-computed plan.
