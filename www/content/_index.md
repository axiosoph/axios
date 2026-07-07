+++
title = "Axios Documentation"
description = "Overview of the Axios monorepo projects, including the Atom protocol, Eos build scheduler, and Ion CLI"
quadrant = "Explanation"
audience = "Developers, architects, and users seeking to understand the Axios monorepo projects"
+++

> [!WARNING]
> Axios is in **early-stage active development**. Specifications, protocols, and APIs are pre-1.0 and will change.

Axios is a set of independent projects for decentralized source publishing, hermetic builds, and package management. The stack has six layers and a utility crate:

0. **L0: Cyphr** — Signing and message-digest primitives ([Coz](https://github.com/Cyphrme/Coz)), external to this repository; every layer above signs and verifies through it.
1. **L1: Atom** (`atom/`) — Content-addressed source publishing protocol. Core identity (`atom-id`), protocol traits (`atom-core`), URI parsing (`atom-uri`), and the Git backend (`atom-git`) are implemented with tests and fuzz harnesses.
2. **L2: HTC** (Hermetic Transactional Composition) — The post-Nix build substrate: upstream's own build, run inside a cryptographic closure, with no expression language and no store-path lore. Decided in [ADR-0005](architecture/0005-hermetic-transactional-composition.html) and elaborated in the [HTC SAD](architecture/htc-sad.html); no crates exist yet, implementation is future work.
3. **L3: Eos** (`eos/`) — The atom-DAG build scheduler. Scheduler traits (`eos-core`) exist; the current `eos-snix` build engine is pre-substrate code being re-scoped around HTC's executor trait, not a finished capability. The daemon (`eos-daemon`) and Cap'n Proto RPC protocol (`eos-proto`) are under development.
4. **L4: Ion** (`ion/`) — User-facing CLI, manifest schema, and dependency resolver. Manifest parsing (`ion-manifest`), lockfile schema (`ion-lock`), and the Eos bridge (`ion-eos`) are prototyped with fuzz harnesses. The resolver (`ion-resolve`) and CLI (`ion-cli`) are under construction.
5. **L5: Plugins** — Ecosystem adapters (Cargo, npm, PyPI, …) using Atom and Ion as a publishing overlay; not yet built.

- **alurl** — URL alias resolution, used by `atom-uri` to expand shorthand source identifiers like `+gh/owner/repo`.

## Sections

- [Explanations](explanation/index.html) — Architecture, security model, and ecosystem integration.
- [How-to guides](how-to/index.html) — Step-by-step instructions for contributors and operators.
- [Reference](reference/index.html) — Specifications, schemas, and spec compliance tracking.
- [Architecture](architecture/index.html) — System design, blueprints, and decision records.
