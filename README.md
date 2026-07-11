# axios

Axios builds software the way upstream already builds it — upstream's own
build, inside a cryptographic closure. An **atom** is a signed,
content-addressed snapshot of sources, manifest, and lock: build _intent_.
A **composition** binds conventional names to content digests, Merkle-rooted
and signed — the closure object, successor to a derivation's output closure.
A **view** is a composition mounted at runtime. The one function is
`build(atom closure, toolchain composition, action params) → output tree`,
executed by upstream's own, unmodified build process inside a materialized
FHS view; its result is analyzed into an **interface manifest**
(provides/requires) rather than trusted by convention. There is no
interpreted expression language, no `nixpkgs`-equivalent package corpus, and
no world-rebuild distro.

> [!IMPORTANT]
> **Early-stage, pre-1.0, spec-first work.** Specifications and formal
> models (TLA+, Alloy, Lean) lead the implementation, not the other way
> around. Expect churn. Concretely, as of this writing: the atom protocol
> workspace is substantially real but not yet conformance-tested against its
> own specification; the composition substrate (HTC) is architecture and
> formal models with a skeleton implementation; the current `eos` Rust
> implementation is a throwaway scaffold pending re-scope to the atom-DAG
> architecture; the `ion` frontend has not been extracted from prototype
> code yet. See [ROADMAP.md](ROADMAP.md) for what's done, what's in
> progress, and what's planned.

## Architecture

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

Each layer depends only on the layers below it; a higher layer never imports
a lower layer's implementation. L2 (HTC) replaces the traditional
evaluator-and-derivation pipeline entirely: there is no evaluation stage,
and eos schedules a DAG of atoms read directly off dependency locks, not a
DAG of expressions an evaluator produced. Execution itself — not build
specifically — is the substrate's dynamic primitive: build, test,
fetch-discovery, and runtime-closure capture are policy variants of one
underlying execute operation, detailed in
[ADR-0006](docs/adr/0006-execution-as-the-primitive.md).

See [ADR-0001](docs/adr/0001-monorepo-workspace-architecture.md) for the
original workspace rationale, [ADR-0005](docs/adr/0005-hermetic-transactional-composition.md)
and [ADR-0006](docs/adr/0006-execution-as-the-primitive.md) for the current
composition-substrate and execution-model decisions, and the
[formal layer model](docs/models/publishing-stack-layers.md) for validated
trait-boundary properties. [ROADMAP.md](ROADMAP.md) is the authoritative
source for what each layer's current implementation status is.

## Where to go deeper

- **[ROADMAP.md](ROADMAP.md)** — the plan to a working MVP, milestone by
  milestone, with status and dependencies.
- **[docs/adr/](docs/adr/)** — Architecture Decision Records: the "why"
  behind each major design turn.
- **[docs/architecture/](docs/architecture/)** — Software Architecture
  Documents (SADs) per layer: the full elaboration of each ADR's decisions.
- **[docs/specs/](docs/specs/)** — normative specifications (BCP 14
  constraint language) for the atom protocol, eos scheduler, ion resolution,
  and related surfaces.
- **[docs/models/](docs/models/)** — formal models: TLA+ (temporal safety),
  Alloy (structural assertions), and Lean (mechanically verified scheduling
  theorems).

## Repository layout

Three independent Cargo workspaces, one skeleton workspace, one standalone
utility crate, and development tooling under `tools/`, inside a shared
monorepo:

- **[atom/](atom/)** — the protocol library (L1). Identity, addressing,
  publishing, and the abstract trait surface. Ecosystem-agnostic.
- **[eos/](eos/)** — the build-scheduling engine (L3). Reads a
  pre-coarsened atom-DAG off locks and dispatches build actions to executor
  workers implementing HTC's build-execution contract. Receives locked
  dependencies from ion; does not perform resolution itself.
- **[htc/](htc/)** — the composition substrate (L2), currently a skeleton
  workspace carrying model-derived types. Its build-execution contract is
  implemented by eos's executor trait today; the substrate itself (hermetic
  builder, analyzers, composer) is planned work — see
  [ROADMAP.md](ROADMAP.md).
- **[ion/](ion/)** — the reference frontend (L4, not yet extracted from
  prototype code). CLI, dependency resolution, the concrete `ion.toml`
  manifest, and dev workspace management.
- **`alurl`** (standalone crate) — structure-preserving URL alias detection
  and expansion.

Crates and dependency layouts evolve; use `cargo metadata` or the root
`Cargo.toml` to see the live set rather than relying on a static list here.

## License

This project is currently under an interim restrictive license while the
final open licensing terms are being determined. See [LICENSE](LICENSE) for
details.
