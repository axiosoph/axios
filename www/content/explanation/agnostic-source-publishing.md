+++
title = "Ecosystem agnosticism"
description = "How Atom publishes any ecosystem's sources without a registry — and composes hermetic, whole-world builds a level above Cargo, npm, PyPI, and other language package managers"
quadrant = "Explanation"
tags = ["general"]
audience = "Systems architects, package manager maintainers, and developers interested in using Atom with traditional toolchains"
+++

The Atom protocol started as a response to Nix-specific code scaling problems, but the production architecture has no dependency on Nix. Its ecosystem agnosticism runs in two directions. As a publishing and integrity layer, it gives any ecosystem's sources decentralized, cryptographically verified publishing. And as a consumer of those sources, Atom is not a plugin that teaches Cargo or npm new tricks — it is a **hermetic package compositor operating a level above the language package managers**, consuming what they already produce (sources and lockfiles) while tracking everything a real build needs that they cannot see.

## Why this matters for traditional package managers

Traditional package managers depend on centralized registries for dependency resolution and integrity verification. That couples the entire ecosystem to a single host and administrative authority.

Atom, used as a publishing overlay, gives traditional toolchains:

1. **Decentralized publishing** — Developers publish versions directly from their Git repositories using Git references (`refs/atoms/...`). No registry upload, no private registry server.
2. **Surety of source** — Consumers verify that source code came from the original repository owner through signed claims and commit DAG ancestry.
3. **Pluggable mirrors** — Because integrity is verified cryptographically, packages can be fetched from any mirror, CDN, or local store without trusting the transport.

## The custom URI scheme (atom-uri and alurl)

Packages in the decentralized space are addressed using a custom URI scheme from the `atom-uri` crate:

```text
[source::] label [@version]
```

- `source` — A URL, SCP-style path, directory, or `+`-prefixed alias. Separated from the label by the rightmost `::`.
- `label` — A validated Unicode identifier (UAX #31 rules with a custom hyphen exception) naming the package within the repository.
- `version` — An unparsed raw version string (semantic or ecosystem-specific).

### Examples

- Full remote URL: `git.snix.dev/snix/snix::snix-core@^1.2`
- Aliased shorthand: `+gh/axiosoph/axios::ion-cli@1.0.0`
- Local path: `/home/user/src/project::lib-common@0.5.1`
- Bare (current repo context): `atom-uri@1.0`

### URL aliasing with alurl

The `alurl` crate saves typing by expanding `+`-prefixed identifiers in source strings. For example, `+gh/owner/repo` expands to `github.com/owner/repo` using a locally defined `AliasMap`. Resolution is recursive with cycle detection, so aliases can reference other aliases.

## Adapter dispatch and PURL types

Atom does not implement the full Package URL (PURL) specification. PURL assumes hardcoded layout conventions that don't map to Atom's `(anchor, label)` identity model.

Instead, the protocol borrows only the ecosystem type identifiers from PURL (`"cargo"`, `"npm"`, `"pypi"`) to populate the `pkg` field in the claim transaction. This lets clients dispatch lockfile handling to the right ecosystem adapter:

```
                       ┌──────────────────────┐
                       │  Generic Atom Core   │
                       └──────────┬───────────
                                  │ (pkg string dispatch)
                  ┌───────────────┼───────────────┐
                  ▼               ▼               ▼
           ┌─────────────┐ ┌──────────────┐ ┌─────────────┐
           │Cargo Adapter│ │ npm Adapter  │ │ PyPI Adapter│
           │(Cargo.lock) │ │(package-     │ │(uv.lock)    │
           │             │ │ lock.json)   │ │             │
           └─────────────┘ └──────────────┘ └─────────────┘
```

A Cargo integration works roughly like this:

1. The resolution tool reads the dependency's lock entry — a `(set, label)` pair pointing at a `publish_czd`.
2. It fetches the publish transactions from the Git repository and verifies them.
3. The Cargo adapter reads the `Cargo.lock` already inside the verified snapshot and enumerates its `(url, checksum)` pairs as the build's pinned fetch set.
4. The build runs `cargo` itself, unmodified, hermetically sealed against exactly that fetch set — the crates it downloads are precisely the ones its own lockfile declares, and nothing else.

The Atom protocol never re-implements Cargo's resolution and knows nothing about its semver rules — the language lockfile is already reviewed, pinned intent, and the adapter's entire job is to enumerate it. This is deliberately the smallest possible integration surface: not \*2nix-style translation of manifests into build instructions, just fetch-set enumeration through a small compatibility interface.

## Above, not beside: the whole-world closure

Language package managers are excellent inside their own world and blind outside it. Cargo resolves crates — but an actual Rust build also needs a specific C compiler for that `-sys` crate, a linker, system headers, `pkg-config`, none of which Cargo has any vocabulary for. Every language ecosystem has this boundary, and everything beyond it is traditionally "whatever happens to be installed."

Atoms are where the rest of the world gets tracked. An atom's dependencies are not limited to its language ecosystem: the toolchain, system libraries, and cross-language tools are first-class, versioned, signed dependencies, and the build runs inside a composed hermetic view containing exactly that closure and nothing else ([the three primitives](the-three-primitives.md)). The language lockfile pins the crates; the atom pins _everything_ — including the language lockfile itself, which ships inside the atom's sources.

Two consequences worth stating plainly:

- **Lockfiles are consumed, never required.** Ecosystems that ship good lockfiles get them adopted directly as the pinned fetch set; ecosystems without them are covered by recorded fetch discovery, promoted into the atom's own lock as signed, reviewed intent. Either way the closure is exhaustive — exhaustiveness comes from the atom, not from the language tool.
- **No per-ecosystem re-invention.** Cargo keeps resolving crates; npm keeps resolving npm packages. Atom composes the world they run in and tracks what they cannot. That is the whole division of labor — one compositor above many package managers, not one plugin per package manager.

> [!NOTE]
> The Ion CLI owns this consumer surface; per-ecosystem adapters are small enumeration shims, not integrations that modify upstream tools.

This approach lets developers adopt decentralized publishing incrementally without abandoning their existing build tools. The same incremental posture holds one layer down at build time: [HTC](hermetic-transactional-composition.md) can ingest existing ecosystem artifacts — distro packages, upstream tarballs, PyPI wheels — with zero rebuilds, so adopting the substrate doesn't require anyone to build natively on it first either.
