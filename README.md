# axios

The decentralized publishing stack, organized as three independent
Cargo workspaces inside a shared monorepo.

## Layer model

```
Cyphrpass (L0)  →  Atom (L1)  →  Eos (L2)  →  Ion (L3)  →  Plugins (L4)
identity/signing   protocol       runtime       frontend     adapters
```

Each layer depends only on the layers below it. See
[ADR-0001](docs/adr/0001-monorepo-workspace-architecture.md) for the
full architectural rationale, and the
[formal model](docs/models/publishing-stack-layers.md) for validated
trait boundary properties.

## Workspaces

**[atom/](atom/)** — The protocol library. Identity, addressing,
publishing, and the abstract trait surface. Ecosystem-agnostic.

**[eos/](eos/)** — The runtime engine. Build planning, execution,
artifact storage, and scheduling. Receives locked dependencies from
ion; does not perform resolution.

**[ion/](ion/)** — The reference frontend. CLI, dependency resolution,
the concrete `ion.toml` manifest, and dev workspace management.

## Crates

| Crate          | Workspace | Responsibility                                          |
| :------------- | :-------- | :------------------------------------------------------ |
| `atom-id`      | atom      | Identity primitives: Label, Tag, AtomDigest, AtomId     |
| `atom-uri`     | atom      | URI parsing, version trait abstraction                  |
| `atom-core`    | atom      | Protocol traits: AtomSource, AtomStore, Manifest, etc.  |
| `atom-git`     | atom      | Git backend: implements AtomRegistry + AtomStore        |
| `eos-core`     | eos       | BuildEngine trait with plan/apply + associated types    |
| `eos-store`    | eos       | ArtifactStore trait for content-addressed build outputs |
| `eos`          | eos       | Runtime: evaluation, building, cache, scheduling        |
| `ion-manifest` | ion       | Concrete ion.toml format, Compose system                |
| `ion-resolve`  | ion       | SAT resolver, unified lock file                         |
| `ion-cli`      | ion       | CLI, build dispatch, dev workspace management           |

## License

This project is currently under an interim restrictive license while
the final open licensing terms are being determined. See [LICENSE](LICENSE)
for details.
