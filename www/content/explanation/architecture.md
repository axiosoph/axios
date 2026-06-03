+++
title = "Axios Stack Architecture & Implementation"
description = "High-level technical overview of the Axios monorepo workspaces, crate organization, trait boundaries, and evaluation data flows"
quadrant = "Explanation"
audience = "Axios stack developers, codebase contributors, and architects tracking integration patterns"
+++

The Axios stack is designed as a layered, content-addressed publishing and build system. To maintain clean separation of concerns and prevent dependency bloat, the codebase is split into three independent Cargo workspaces, representing a downward-only dependency chain: **Ion (L3)** $\to$ **Eos (L2)** $\to$ **Atom (L1)**.

## Workspace Organization

- **L1: Atom** (`atom/`) — The protocol layer. It defines package identity (`AtomId`), cryptographic signing (utilizing Coz), and transport mapping. It is completely unaware of build environments or dependency resolution.
- **L2: Eos** (`eos/`) — The runtime scheduler. It construct build plans and coordinates cache-skipping, delegating sandboxed build execution to backends (such as Snix). It is unaware of user-facing CLI commands or manifest syntax.
- **L3: Ion** (`ion/`) — The user-facing CLI and dependency resolver. It parses manifests (`ion.toml`), resolves the dependency graph using a SAT solver, and writes lockfiles (`ion.lock`).

To enforce this layer discipline, L2 crates never import L3 porcelain, and L1 crates never import L2 runtime schedulers.

---

## Core Trait Boundaries

The stack decouples implementation details from core logic by defining abstract traits as interfaces. This design allows changing storage backends or build schedulers without modifying porcelain crates.

### L1: Protocol Interface (atom-core)

The protocol defines traits for reading and writing atoms:

- **`AtomSource`**: The read-only observer interface. It provides methods to resolve package labels, retrieve versions, and fetch deterministic content trees.

  ```rust
  pub trait AtomSource {
      type Error: std::error::Error + Send + Sync + 'static;

      async fn resolve_id(&self, label: &Label) -> Result<AtomId, Self::Error>;
      async fn get_versions(&self, id: &AtomId) -> Result<Vec<Version>, Self::Error>;
      async fn fetch_snapshot(&self, id: &AtomId, version: &Version) -> Result<Tree, Self::Error>;
  }
  ```

- **`AtomRegistry`**: The write-only publisher interface. It manages signing and publishing claim and publish transactions to the backend.
  ```rust
  pub trait AtomRegistry: AtomSource {
      async fn publish_claim(&self, claim: &Claim, key: &PrivateKey) -> Result<(), Self::Error>;
      async fn publish_version(&self, publish: &Publish, key: &PrivateKey) -> Result<(), Self::Error>;
  }
  ```

_The default implementation of these traits is provided by the `atom-git` bridge, translating the operations into Git references and objects._

### L2: Scheduler Interface (eos-core)

The build scheduler separates evaluation orchestration from build execution:

- **`BuildEngine`**: The execution interface. It takes a build plan (such as a nix/snix derivation) and schedules sandboxed build execution, yielding a content-addressed output path.

  ```rust
  pub trait BuildEngine {
      type Plan: PlanSpec;
      type Output: OutputSpec;
      type Error: std::error::Error + Send + Sync + 'static;

      async fn execute(&self, plan: &Self::Plan) -> Result<Self::Output, Self::Error>;
  }
  ```

- **`ArtifactStore`**: The cache and storage interface. It stores build artifacts and links them to the source atom digests, enabling cache-skipping optimizations.

_The default build engine is implemented by `eos-snix`, which delegates the actual build work to the Snix engine while Eos manages scheduling and caching._
