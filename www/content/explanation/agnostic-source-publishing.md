+++
title = "Ecosystem Agnosticism: Augmenting Traditional Package Managers"
description = "How the Atom protocol serves as a general-purpose, decentralized source publishing overlay for Cargo, npm, PyPI, and other ecosystems"
quadrant = "Explanation"
audience = "Systems architects, package manager maintainers, and developers seeking to use Atom with traditional toolchains"
+++

Although the Atom protocol was originally motivated by the unique code scaling and purity challenges of the Nix ecosystem, its production architecture is decoupled from Nix. The core protocol is an ecosystem-agnostic publishing and integrity layer that can augment traditional package managers (e.g., Cargo, npm, PyPI) to provide decentralized, cryptographically secure source publishing.

## Why Traditional Package Managers Need Atom

Traditional package managers rely on centralized registries to resolve dependencies and verify package integrity. While this provides a convenient workflow, it couples the ecosystem to a single host and administrative authority.

By integrating the Atom protocol as a publishing overlay, traditional toolchains gain three main benefits:

1. **Decentralized Publishing**: Developers publish package versions directly from their Git repositories using Git references (`refs/atoms/...`). There is no need to upload packages to a centralized registry or configure private registry servers.
2. **Surety of Source**: Downstream consumers verify that the source code they compile came from the original repository owner (verified via signed claims and commit DAG ancestry).
3. **Pluggable Mirrors**: Since package integrity is verified mathematically, packages can be retrieved from any mirror, CDN, or local store without compromising security.

## The Custom URI Scheme (`atom-uri` & `alurl`)

To address packages within the decentralized space, the system uses a custom URI scheme implemented in the `atom-uri` crate. The URI format is designed for both human readability and machine resolution:

```text
[source::] label [@version]
```

- **`source`** — A URL, SCP-style path, directory, or a `+`-prefixed alias. It is separated from the package label by the rightmost `::` delimiter.
- **`label`** — A validated Unicode identifier (following UAX #31 rules with a custom hyphen exception) naming the package within the repository set.
- **`version`** — An unparsed raw version string following semantic or ecosystem-specific versioning.

### URL Aliasing via `alurl`

To avoid typing long hostnames in sources, the stack integrates the `alurl` crate. `alurl` is a structure-preserving URL alias detection and expansion library.

It scans host positions within source strings for `+`-prefixed identifiers (e.g. `+gh/owner/repo`) and expands them using a locally defined `AliasMap` (e.g. mapping `gh` to `github.com`). It performs recursive resolution with cycle detection, translating raw URIs like `+org:project::my-atom@^1` into fully qualified URLs without modifying the underlying path structure.

## Adapter Dispatch and PURL Types

Atom does not implement the full Package URL (PURL) specification, as PURL makes hardcoded layout assumptions that do not map to Atom's unique `(anchor, label)` identity.

Instead, the protocol borrows only the **ecosystem type identifiers** from PURL (e.g. `"cargo"`, `"npm"`, `"pypi"`) to populate the `pkg` field in the claim transaction. 

This type identifier allows generic package resolution clients to dispatch manifest parsing and version resolution to the appropriate ecosystem adapter:

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

For example, when resolving a Cargo project dependency:
1. The resolution tool reads the dependency's Atom ID.
2. It fetches the corresponding publish transactions from the Git repository.
3. The **Cargo Adapter** extracts the `Cargo.toml` manifest from the deterministic content snapshot (`dig`).
4. Cargo's native resolver processes the manifest's crate dependencies.

The core Atom protocol remains completely unaware of Cargo's TOML format, semantic version resolving logic, or build requirements. It only provides the transport, verification, and file extraction capabilities.

## Augmenting Existing Toolchains

Integrating Atom does not require rewriting Cargo or npm from scratch. Instead, it can be implemented as a client-side wrapper or plugin:

- **Publishing Overlay**: A developer runs `cargo atom-publish` which creates the deterministic snapshot of the crate subdirectory, signs the transaction, and pushes the references to their public Git repository.
- **Dependency Resolution**: When compiling, `cargo atom-fetch` resolves the pinned atom versions in the project lock file, verifies their signatures, and extracts the crates to Cargo's local cache directory before compiling.

This architecture enables developers to incrementally adopt decentralized, secure workflows while keeping their existing build tools and language conventions intact.
