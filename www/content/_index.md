+++
title = "Axios Documentation"
description = "Overview of the Axios monorepo projects, including the Atom protocol, Eos build scheduler, and Ion CLI"
quadrant = "Explanation"
audience = "Developers, architects, and users seeking to understand the Axios monorepo projects"
+++

> [!WARNING]
> Axios is in **early-stage active development**. Specifications, protocols, and APIs are pre-1.0 and will change.

Axios is a set of independent projects for decentralized source publishing, builds, and package management. The stack has three layers and a utility crate:

1. **L1: Atom** (`atom/`) — Content-addressed source publishing protocol. Core identity (`atom-id`), protocol traits (`atom-core`), URI parsing (`atom-uri`), and the Git backend (`atom-git`) are implemented with tests and fuzz harnesses.
2. **L2: Eos** (`eos/`) — Idempotent build scheduler. Scheduler traits (`eos-core`) and the Snix backend (`eos-snix`) are implemented. The daemon (`eos-daemon`) and gRPC protocol (`eos-proto`) are under development.
3. **L3: Ion** (`ion/`) — User-facing CLI, manifest schema, and dependency resolver. Manifest parsing (`ion-manifest`), lockfile schema (`ion-lock`), and the Eos bridge (`ion-eos`) are prototyped with fuzz harnesses. The resolver (`ion-resolve`) and CLI (`ion-cli`) are under construction.
4. **alurl** — URL alias resolution, used by `atom-uri` to expand shorthand source identifiers like `+gh/owner/repo`.

## Sections

- [Explanations](explanation/index.html) — Architecture, security model, and ecosystem integration.
- [How-to guides](how-to/index.html) — Step-by-step instructions for contributors and operators.
- [Reference](reference/index.html) — Specifications, schemas, and spec compliance tracking.
- [Architecture decision records](adr/index.html) — Design rationale for structural changes.
