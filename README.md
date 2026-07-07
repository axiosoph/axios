# axios

Axios builds software the way upstream already builds it — upstream's own
build, inside a cryptographic closure. An **atom** is a signed,
content-addressed snapshot of sources, manifest, and lock: build _intent_.
A **composition** binds conventional names to content digests, Merkle-rooted
and signed — the closure object, and the successor to a derivation's output
closure. A **view** is a composition mounted at runtime. The one function is
`build(atom closure, toolchain composition, action params) → output tree`,
executed by upstream's own, unmodified build process inside a materialized
FHS view; its result is analyzed into an **interface manifest**
(provides/requires) rather than trusted by convention. There is no
interpreted expression language and no world-rebuild distro. See
[ADR-0005](docs/adr/0005-hermetic-transactional-composition.md) and
[htc-sad.md](docs/architecture/htc-sad.md) for the full architecture — this
is the target design; the crate inventory below reflects what is actually
implemented today.

Concretely, today's implementation is organized as three independent Cargo
workspaces (plus one small standalone utility crate) inside a shared
monorepo.

## Layer model

```
L5  Plugins    Plugin crates extending ion (future)
L4  ion/       Frontend: CLI, manifests, resolution
L3  eos/       Engine: builds, stores, scheduling
L2  HTC        Build-execution & composition substrate: CAS, compositions,
                interface manifests, build records, fetch-proxy execution,
                closure computation, materialization (skeleton workspace: htc/)
L1  atom/      Protocol: identity, addressing, publishing
L0  Cyphr      Cryptographic substrate (external; future)
```

Each layer depends only on the layers below it. See
[ADR-0001](docs/adr/0001-monorepo-workspace-architecture.md) for the
original architectural rationale, [ADR-0005](docs/adr/0005-hermetic-transactional-composition.md)
for the L2/HTC layer insertion and renumbering, and the
[formal model](docs/models/publishing-stack-layers.md) for validated
trait boundary properties.

## Workspaces

**[atom/](atom/)** — The protocol library. Identity, addressing,
publishing, and the abstract trait surface. Ecosystem-agnostic.

**[eos/](eos/)** — The build-scheduling engine. Reads a pre-coarsened
atom-DAG off locks and dispatches build actions to executor workers
implementing HTC's build-execution contract; maintains the action-id
cache and the shared artifact store. Receives locked dependencies from
ion; does not perform resolution.

**[ion/](ion/)** — The reference frontend. CLI, dependency resolution,
the concrete `ion.toml` manifest, and dev workspace management.

## Crates

| Crate          | Workspace    | Responsibility                                                          |
| :------------- | :----------- | :---------------------------------------------------------------------- |
| `atom-id`      | atom         | Identity primitives: Anchor, Label, AtomId (the `(anchor, label)` pair) |
| `atom-uri`     | atom         | URI parsing, alias-aware resolution                                     |
| `atom-core`    | atom         | Protocol traits: AtomSource, AtomStore, Manifest, etc.                  |
| `atom-git`     | atom         | Git backend: implements AtomRegistry + AtomStore                        |
| `eos-core`     | eos          | BuildEngine trait with plan/apply + associated types                    |
| `eos-proto`    | eos          | Cap'n Proto wire schema and generated bindings                          |
| `eos-snix`     | eos          | Slated for removal (evaluator eradicated, ADR-0006 §3)                  |
| `eos-daemon`   | eos          | Scheduler, executor worker pool, RPC server                             |
| `eos`          | eos          | Orchestration: scheduling, action-id cache, artifact store              |
| `ion-manifest` | ion          | Concrete ion.toml format, Compose system                                |
| `ion-resolve`  | ion          | SAT resolver, dependency graph                                          |
| `ion-lock`     | ion          | Lock schema and (de)serialization; `DepMap` keyed by `AtomId`           |
| `ion-eos`      | ion          | Bridge: client interface to the eos daemon over Cap'n Proto             |
| `htc-comp`     | htc          | Composition primitive types + law-tested merge monoid (skeleton)        |
| `htc-exec`     | htc          | Execution primitive types + executor trait (skeleton)                   |
| `ion-cli`      | ion          | CLI, build dispatch, dev workspace management                           |
| `alurl`        | (standalone) | Structure-preserving URL alias detection and expansion                  |

L2/HTC has a landed ADR, SAD, and formal models, plus a skeleton
workspace (`htc/`) carrying the model-derived types (see the layer
model above); its build-execution contract is implemented by `eos`'s
executor trait today.

## License

This project is currently under an interim restrictive license while
the final open licensing terms are being determined. See [LICENSE](LICENSE)
for details.
