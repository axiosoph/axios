+++
title = "Axios Documentation"
description = "Overview of the Axios monorepo projects, including the Atom protocol, Eos build scheduler, and Ion CLI"
quadrant = "Explanation"
audience = "Developers, architects, and users seeking to understand the Axios monorepo projects"
+++

# Axios Project Umbrella

> [!WARNING]
> Axios is in **early-stage active development**. The specifications, protocols, and APIs are pre-1.0 and subject to change.

Welcome to the Axios static documentation website. Axios is the umbrella name for a collection of independent projects related to decentralized source publishing, builds, and package management.

The stack consists of three distinct layers:

1. **L1: Atom** — The decentralized, content-addressed source code publishing protocol. _(Specification active; `atom-git` backend and `atom-uri` resolver are implemented)._
2. **L2: Eos** — The idempotent build scheduler and backend coordinator. _(Scheduler traits and `eos-snix` backend are active; daemon/remote protocol is under development)._
3. **L3: Ion** — The user-facing CLI, manifest schema, and dependency resolver. _(Under Construction — SAT resolver and TOML parsing logic are prototyped; CLI commands are pending implementation)._

## Key Sections

Explore the documentation sections to learn more:

- [Reference Documentation](reference/index.html) — Normative behavioral contracts, schemas, and spec compliance.
- [Architecture Decision Records](adr/index.html) — Context and design rationale for architectural changes.
