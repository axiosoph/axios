+++
title = "Axios Documentation"
description = "Overview of the Axios monorepo projects, including the Atom protocol, Eos build scheduler, and Ion CLI"
quadrant = "Explanation"
audience = "Developers, architects, and users seeking to understand the Axios monorepo projects"
+++

> [!WARNING]
> Axios is in **early-stage active development**. The specifications, protocols, and APIs are pre-1.0 and subject to change.

Axios is a collection of independent projects for decentralized source publishing, builds, and package management. The stack consists of three layers and a utility crate:

1. **L1: Atom** (`atom/`) — The decentralized, content-addressed source publishing protocol. Core identity (`atom-id`), protocol traits (`atom-core`), URI parsing (`atom-uri`), and the Git backend (`atom-git`) are implemented with tests and fuzz harnesses.
2. **L2: Eos** (`eos/`) — The idempotent build scheduler. Scheduler traits (`eos-core`) and the Snix backend (`eos-snix`) are implemented. The daemon (`eos-daemon`) and gRPC protocol (`eos-proto`) are under development.
3. **L3: Ion** (`ion/`) — The user-facing CLI, manifest schema, and dependency resolver. Manifest parsing (`ion-manifest`), lockfile schema (`ion-lock`), and the Eos bridge (`ion-eos`) are prototyped with fuzz harnesses. The resolver (`ion-resolve`) and CLI (`ion-cli`) are under construction.
4. **alurl** — Structure-preserving URL alias resolution, used by `atom-uri` to expand shorthand source identifiers (e.g. `+gh/owner/repo`).

## Documentation Sections

- [Explanations](explanation/index.html) — Conceptual overviews of the architecture, security model, and ecosystem integration.
- [How-To Guides](how-to/index.html) — Step-by-step instructions for contributors and operators.
- [Reference](reference/index.html) — Normative behavioral contracts, schemas, and spec compliance.
- [Architecture Decision Records](adr/index.html) — Context and design rationale for architectural changes.
