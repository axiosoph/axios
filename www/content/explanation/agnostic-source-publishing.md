+++
title = "Ecosystem agnosticism"
description = "How the Atom protocol works as a decentralized source publishing overlay for Cargo, npm, PyPI, and other ecosystems"
quadrant = "Explanation"
audience = "Systems architects, package manager maintainers, and developers interested in using Atom with traditional toolchains"
+++

The Atom protocol started as a response to Nix-specific code scaling problems, but the production architecture has no dependency on Nix. The core protocol is an ecosystem-agnostic publishing and integrity layer. It can sit on top of traditional package managers (Cargo, npm, PyPI, etc.) to add decentralized, cryptographically verified source publishing.

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

Instead, the protocol borrows only the ecosystem type identifiers from PURL (`"cargo"`, `"npm"`, `"pypi"`) to populate the `pkg` field in the claim transaction. This lets resolution clients dispatch manifest parsing to the right ecosystem adapter:

```
                       ┌──────────────────────┐
                       │  Generic Atom Core   │
                       └──────────┬───────────
                                  │ (pkg string dispatch)
                  ┌───────────────┼───────────────┐
                  ▼               ▼               ▼
           ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
           │Cargo Adapter│ │ npm Adapter │ │ PyPI Adapter│
           │(Cargo.toml) │ │(package.json│ │(setup.py)   │
           └─────────────┘ └─────────────┘ └─────────────┘
```

A Cargo integration would work roughly like this:

1. The resolution tool reads the dependency's Atom ID.
2. It fetches the publish transactions from the Git repository.
3. The Cargo adapter extracts `Cargo.toml` from the deterministic content snapshot (`dig`).
4. Cargo's native resolver handles the rest.

The Atom protocol knows nothing about Cargo's TOML format, semver rules, or build requirements. It only handles transport, verification, and file extraction.

## Augmenting existing toolchains

Adding Atom to an existing ecosystem doesn't mean rewriting Cargo or npm. A hypothetical integration would be a client-side wrapper or plugin:

- A developer runs `cargo atom-publish`, which snapshots the crate subdirectory, signs the transaction, and pushes refs to the public Git repository.
- At build time, `cargo atom-fetch` resolves pinned atom versions from the lockfile, verifies signatures, and extracts crates into Cargo's local cache before compilation.

> [!NOTE]
> These commands are illustrative. No Cargo or npm plugins exist yet. The Ion CLI will provide this functionality once L3 is complete.

This approach lets developers adopt decentralized publishing incrementally without abandoning their existing build tools.
