# ADR-0003: Composable Deployment Modes

- **Status**: PROPOSED
- **Date**: 2026-06-05
- **Deciders**: nrd
- **Source**: [ADR-0001](0001-monorepo-workspace-architecture.md)
  §Embedded default |
  [ADR-0002](0002-decoupling-snix-backend.md) §Alternative 3 |
  [Eos SAD](../architecture/eos-sad.md) §1.5
- **Supersedes**: ADR-0001 §Embedded default
  ("embedded default, daemon opt-in")

---

**Document Classification**: Architecture Decision Record
**Audience**: Architects, Core Developers, Operators

---

## Context

ADR-0001 established "embedded default, daemon opt-in" as a
design principle. ADR-0002 then designed a network-first
service-boundary architecture with separate processes for the
daemon, eval workers, build workers, snix store daemons, and
snix builders — five long-running processes for a single
developer workstation.

This creates a tension. The service-boundary architecture is
correct for cluster deployments where independent scaling is
needed. But five services for a solo developer is a significant
ergonomic regression from every comparable build tool:

| Tool    | Daemon processes   | CLI processes |
| :------ | :----------------- | :------------ |
| Cargo   | 0                  | 1             |
| Go      | 0                  | 1             |
| Nix     | 1 (`nix-daemon`)   | 1             |
| Bazel   | 1 (`bazel server`) | 1             |
| Buck2   | 1 (`buckd`)        | 1             |
| **Eos** | **5**              | 1             |

Nix required a daemon because the Nix store needs root
privileges. Snix does not share this requirement — the snix
store can be written to without root. This means there is no
architectural reason for a mandatory daemon in single-user
mode. Architecturally, the system is closer to Cargo or Go
than to Nix: a tool that can operate as a single process.

### The Trait Abstraction Enables Composability

The service-boundary architecture from ADR-0002 uses two
interface layers, both of which are transport-agnostic:

**Cap'n Proto (eos daemon ↔ workers)**: `VatNetwork` operates
on any `AsyncRead + AsyncWrite` pair. Swapping a UDS/TCP socket
for `tokio::io::duplex` (in-memory pipe) requires changing
only the transport initialization. The RPC semantics, capability
passing, and serialization are identical.

**Snix store/build services (workers ↔ snix)**: These are
defined as Rust traits (`BlobService`, `DirectoryService`,
`PathInfoService`, `BuildService`) with multiple pluggable
implementations:

- `GRpcBlobService` — gRPC client (remote store daemon)
- `RedbBlobService` — embedded redb database (in-process)
- `MemoryBlobService` — in-memory (testing)
- `ObjectStoreBlobService` — S3/GCS/Azure
- Cache combinators (layered caching)

An eval worker consumes `Arc<dyn BlobService>`. Whether that
Arc wraps a gRPC client to a remote store or an in-process
redb instance is a construction-time decision. The worker code
is identical in both cases. This is dependency injection, not
codepath divergence.

This trait-based design means **no sockets are needed for fully
in-process operation**. A single binary can construct in-process
store implementations and wire them directly into workers via
trait objects. No gRPC, no sockets, no listening processes.

---

## Decision

### Three Composable Deployment Modes

The system supports three deployment modes. All modes use the
same business logic, the same trait interfaces, and the same
RPC protocol semantics. They differ only in how components are
wired at construction time (dependency injection).

#### Mode 1: Monolithic Ion (single binary, zero daemons)

```
ion build   # single process, single binary, no services

ion (single process)
  ├── scheduler (in-process)
  ├── eval worker (thread, Cap'n Proto over memory pipe)
  ├── build worker (thread, Cap'n Proto over memory pipe)
  ├── snix store (in-process redb, direct trait injection)
  └── snix builder (in-process, direct trait injection)
```

**For**: Solo developer on a single machine. Install the
binary, run `ion build`. No daemon, no services, no process
management. Analogous to `cargo build` or single-user
`nix build`.

**How it works**: The `ion` binary is compiled with a
`monolithic` feature flag that pulls in `eos` with its own
`monolithic` flag enabled (see §Feature Flag Composition).
At startup, it constructs in-process store backends (redb for
persistence), spawns eval/build workers as threads connected
via in-memory Cap'n Proto pipes, and runs the scheduler
in-process. Everything communicates via the same trait
interfaces and Cap'n Proto protocol — just over in-process
transports instead of network sockets.

#### Mode 2: Monolithic Eos (one daemon, many clients)

```
eosd        # single daemon process, contains all eos + snix

ion build   # client, connects to eosd via Cap'n Proto RPC
ion build   # another client on another machine
```

**For**: Small team, homelab, single powerful server with
multiple developer workstations. One machine runs `eosd`,
all developers point their `ion` clients at it.

**How it works**: The `eosd` binary is compiled with the
`monolithic` feature flag on the eos workspace. Internally
identical to monolithic ion's eos subsystem — in-process
stores, in-process workers, memory pipes. The difference is
that it exposes a Cap'n Proto RPC surface for remote `ion`
clients instead of being invoked directly.

#### Mode 3: Distributed Eos (microservices)

```
snix-store-daemon   # independent store process
snix-builder        # independent builder process(es)
eos-eval-worker     # independent eval worker(s)
eos-build-worker    # independent build worker(s)
eosd                # scheduler/dispatcher only

ion build           # client
```

**For**: Large clusters, CI farms, organizations needing
independent scaling of evaluation, build, and storage
capacity.

**How it works**: Each component is a separate process.
Workers connect to stores via gRPC. Workers register with the
daemon via Cap'n Proto. Ion connects to the daemon via Cap'n
Proto. This is the architecture described in ADR-0002 and the
Eos SAD.

### Feature Flag Composition

The `monolithic` feature flag is layered to separate concerns:

```toml
# eos/Cargo.toml (or eos-daemon/Cargo.toml)
[features]
monolithic = [
  "dep:snix-store",      # in-process store backends
  "dep:snix-build",      # in-process builder
  "dep:snix-eval",       # in-process evaluator
]

# ion/ion-cli/Cargo.toml
[features]
monolithic = [
  "dep:eos",             # pulls in eos with monolithic
  "eos/monolithic",      # triggers eos's monolithic flag
]
```

This preserves separation of concerns: ion's `monolithic`
flag triggers eos's `monolithic` flag, but ion does not need
to know how eos pulls in and composes snix components. Each
layer manages its own internal composition.

In distributed mode (no `monolithic` flag), eos compiles
without snix dependencies and communicates with snix services
via gRPC clients. In monolithic mode, eos compiles with snix
and uses in-process trait implementations directly.

### What Changes Between Modes

| Concern          | Monolithic                          | Distributed                 |
| :--------------- | :---------------------------------- | :-------------------------- |
| Store backend    | In-process (redb)                   | gRPC client → remote daemon |
| Worker transport | In-memory Cap'n Proto pipe          | TCP/UDS Cap'n Proto socket  |
| Client transport | In-process (mode 1) or RPC (mode 2) | RPC                         |
| Scaling          | Single machine                      | Horizontal                  |
| Business logic   | **Identical**                       | **Identical**               |

The only conditional compilation is in transport
initialization and DI wiring. No `#[cfg(monolithic)]` on
any business logic, scheduling algorithm, evaluation
codepath, or build pipeline.

### Invariants

**[deployment-no-codepath-divergence]**: All deployment modes
MUST use the same trait interfaces, the same scheduling
algorithms, and the same evaluation/build pipelines. The
difference between modes is which implementation of
`Arc<dyn BlobService>` (gRPC client vs. redb) and which
transport backing for Cap'n Proto (socket vs. memory pipe)
is injected at construction time. No business logic may be
conditionally compiled based on deployment mode.

**[deployment-composable-monolith]**: Each component boundary
(store, eval worker, build worker, scheduler) is independently
wireable as in-process or remote. A deployment can mix modes:
e.g., in-process store with remote build workers. The
composition is a deployment configuration choice, not a
compilation choice.

**[deployment-mode-bisimilarity]**: All three deployment modes
MUST produce identical outputs given identical inputs. This
is validated by the formal model's bisimulation property:
deployment mode is an observation-preserving morphism.

### Supersede ADR-0001 §Embedded Default

ADR-0001's "embedded default, daemon opt-in" is superseded
by this composable model. The concept is preserved (monolithic
ion IS the embedded default) but the framing changes:

- ~~Embedded mode uses a different `Engine` impl~~ →
  All modes use the same traits, different wiring
- ~~Daemon is a separate architectural path~~ →
  Daemon is the same code with a network listener added
- ~~`RemoteEngine` vs `SnixEngine`~~ →
  Same `SnixEngine`, different store backend injection

### Implementation Priority

1. **Distributed mode first** (ADR-0002): Build and stabilize
   the service-boundary architecture with separate processes,
   gRPC store access, and Cap'n Proto worker protocol. This is
   the hard part — everything else falls out of it.

2. **Monolithic eos second**: Once the trait abstractions and
   Cap'n Proto interfaces are stable, wire them in-process
   within a single `eosd` binary. This is primarily DI wiring.

3. **Monolithic ion third**: Extend monolithic eos by embedding
   the scheduler into the `ion` binary. Simplest possible
   single-user experience.

### Single-User Deployment (Interim)

Until monolithic modes are implemented, single-user deployment
uses process composition:

- **Development**: `process-compose` with a canonical
  composition file. Single command to start all services.
- **Production**: Systemd units or NixOS module. Long-term,
  one should derive from the other for consistency.

---

## Consequences

### Positive

- **Zero-to-build UX**: Monolithic ion gives the Cargo/Go
  experience — install a binary, run `ion build`. No services,
  no configuration, no root.
- **Composable scaling**: Same codebase serves solo developers,
  homelab operators, and large organizations. No separate
  "lite" and "enterprise" editions.
- **No codepath divergence**: The distributed mode tests also
  validate the monolithic mode (same business logic). Only
  transport/wiring integration tests differ.
- **Snix store flexibility**: Users can choose store backends
  (redb for local, S3 for cloud, gRPC for shared) without
  changing any eos code.

### Negative

- **Binary size**: Monolithic ion includes all of snix-eval,
  snix-store (all backends), snix-build, eos-daemon, and
  ion-cli. Expected >100MB.
- **Feature flag complexity**: Cargo feature flags for
  `monolithic` must be carefully gated to avoid pulling in
  unwanted dependencies in distributed mode builds.
- **Testing matrix**: Three deployment modes require at minimum
  three integration test configurations, though the shared
  business logic means unit tests cover all modes.

### Risks Accepted

- **In-process redb concurrency**: A monolithic binary running
  eval and build workers as threads all sharing one redb store
  must handle concurrent access correctly. Redb supports
  concurrent readers with single writer; this may need a
  write-serialization layer.
- **Memory footprint**: All components in one process may use
  more memory than idle separate processes that could be paged
  out. Acceptable for single-user workloads.

---

## Alternatives Considered

### Alt 1: Embedded Library (ADR-0001 Original)

A `SnixEngine` compiled into ion-cli with a separate
`RemoteEngine` for daemon mode.

- **Why superseded**: Creates two engine implementations with
  different codepaths. The composable monolith achieves the
  same UX (single binary, no daemon) without divergent code.

### Alt 2: No Monolithic Mode

Multi-process only. Single-user uses systemd/process-compose.

- **Why not chosen**: Five services for a solo developer is a
  significant ergonomic regression from Cargo, Nix, Bazel,
  and Buck2. The trait-based architecture makes monolithic
  wiring genuinely low-cost (dependency injection only).

### Alt 3: Monolithic Eos Only (No Monolithic Ion)

Support monolithic eos (one daemon) but not monolithic ion
(zero daemons).

- **Why not chosen**: Since snix's store doesn't need root,
  there's no architectural reason for a mandatory daemon in
  single-user mode. Monolithic ion is strictly simpler for
  the solo developer. Monolithic eos remains useful for the
  homelab/small team case as a separate deployment option.

---

## Related Documents

- [ADR-0001](0001-monorepo-workspace-architecture.md) —
  original "embedded default" (superseded §Embedded default)
- [ADR-0002](0002-decoupling-snix-backend.md) — service
  boundary architecture (Mode 3 of this ADR)
- [Eos SAD](../architecture/eos-sad.md) — system architecture
  (§1.5 to be updated per this ADR)
