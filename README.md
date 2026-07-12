# axios

Axios is building a decentralized publishing and build substrate on a
single primitive: the signed, content-addressed binding of names to
content, applied recursively from published sources to runtime closures.
We call it **composition-addressing**. The goal is software you can verify
instead of trust: anyone can check what was published, what it depends on,
what built it, and what it runs against, with no central registry, build
farm, or naming convention vouching for any of it.

The reason this is a new substrate rather than an incremental improvement
is structural. Store-path systems like Nix embed hash-pointers _inside_
artifacts, and a collision-resistant hash has no accessible fixed point —
an artifact cannot point at its own digest, which is why fully
content-addressing that model has been stuck upstream for years. Axios
moves the pointers _beside_ the artifact, into signed binding objects, and
the obstruction dissolves by construction. The design's vocabulary is
small: an **atom** is signed build intent (sources, manifest, lock); a
**composition** is a signed, Merkle-rooted binding of names to content
digests — the closure object; a **view** is a composition mounted at
runtime. Build outputs are analyzed into **interface manifests**
(provides/requires) and verified against them, so a runtime closure is
_justified_ — every entry is present because a named requirement binds to
it, not because a hash-scan guessed.

The price of entry is deliberately low: builds run upstream's own,
unmodified build process inside a hermetic view. There is no interpreted
expression language to learn, no world-rebuild distribution to maintain,
and existing ecosystem artifacts can be ingested as they are.

The stack's day-to-day tool, ion, is a **system compositor**, not a
package manager, and not a rival to one. A package manager cares about its
own ecosystem; a compositor cares about the entire formal closure a build
needs. Package is only the first boundary, then environments, then whole
systems, so the tool is named by its upper bound. It works a layer above
the package managers it meets, not in their place: a `Cargo.lock` is still
useful, just not complete, and the compositor adopts it as one piece of
the whole.

> [!IMPORTANT]
> **Axios is early-stage and pre-1.0.** It is a specification-first
> project, and the design is much further along than the code: the
> scheduler's dispatch theory is mechanically proven in Lean, the atom
> charter transaction protocol is modeled in TLA+ and Alloy, the atom
> layer's semantics and its storage-backend obligations now have their
> own formal treatments ([docs/models/atom-model.md](docs/models/atom-model.md),
> [docs/specs/atom-backend-contract.md](docs/specs/atom-backend-contract.md)),
> and the execution-primitive choice
> ([ADR-0006](docs/adr/0006-execution-as-the-primitive.md)) was settled by
> measurement rather than preference. The implementation deliberately
> trails that design: the atom protocol workspace is substantially real
> but not yet conformance-tested against its own specification; the
> composition substrate (HTC) is architecture and formal models over a
> skeleton workspace; the current `eos` code is a scaffold pending
> re-scope to the atom-DAG architecture; the `ion` frontend has not been
> extracted from prototype code. [ROADMAP.md](ROADMAP.md) tracks what is
> done, what is in progress, and what is planned.

## Architecture

```
L5  Plugins    Plugin crates extending ion (future)
L4  ion/       System compositor: CLI, manifests, resolution
L3  eos/       Engine: builds, stores, scheduling
L2  HTC        Build-execution & composition substrate: CAS, compositions,
                interface manifests, build records, fetch-proxy execution,
                closure computation, materialization (skeleton workspace: htc/)
L1  atom/      Protocol: identity, addressing, publishing
L0  Cyphr      Cryptographic substrate (external; future)
```

Each layer depends only on the layers below it; a higher layer never imports
a lower layer's implementation. L2 — **Hermetic Transactional
Composition** (HTC) — replaces the traditional
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
source for each layer's current implementation status.

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
- **[ion/](ion/)** — the system compositor (L4, not yet extracted from
  prototype code): CLI, dependency resolution, the concrete `atom.toml`
  manifest, and dev workspace management.
- **`alurl`** (standalone crate) — structure-preserving URL alias detection
  and expansion.

Crates and dependency layouts evolve; use `cargo metadata` or the root
`Cargo.toml` to see the live set rather than relying on a static list here.

## License

This project is currently under an interim restrictive license while the
final open licensing terms are being determined. See [LICENSE](LICENSE) for
details.
