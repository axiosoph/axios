+++
title = "Axios Stack Architecture & Implementation"
description = "High-level technical overview of the Axios monorepo workspaces, crate organization, trait boundaries, and data flows"
quadrant = "Explanation"
audience = "Axios stack developers, codebase contributors, and architects tracking integration patterns"
+++

The Axios stack is a layered, content-addressed publishing and build system. To maintain clean separation of concerns and prevent dependency bloat, the codebase is split into three independent Cargo workspaces representing a downward-only dependency chain: **Ion (L3)** $\to$ **Eos (L2)** $\to$ **Atom (L1)**.

A standalone utility crate, **alurl**, lives at the repository root and provides structure-preserving URL alias resolution used by `atom-uri`.

## Workspace Organization

- **L1: Atom** (`atom/`) — The protocol layer. Defines package identity (`AtomId`), cryptographic signing (via [Coz](https://github.com/Cyphrme/Coz)), content-addressed digests (`AtomDigest`), and transport mapping. Completely unaware of build environments or dependency resolution. Contains four crates: `atom-id`, `atom-uri`, `atom-core`, and `atom-git`.
- **L2: Eos** (`eos/`) — The runtime scheduler. Constructs build plans and coordinates cache-skipping, delegating sandboxed build execution to backends (such as Snix). Contains five crates: `eos-core`, `eos`, `eos-snix`, `eos-daemon`, and `eos-proto`.
- **L3: Ion** (`ion/`) — The user-facing CLI and dependency resolver. Parses manifests (`ion.toml`), resolves the dependency graph using a SAT solver, and writes lockfiles (`ion.lock`). Contains five crates: `ion-cli`, `ion-manifest`, `ion-lock`, `ion-resolve`, and `ion-eos`.
- **alurl** (`alurl/`) — Structure-preserving URL alias detection and expansion. Resolves `+`-prefixed identifiers (e.g. `+gh/owner/repo`) via configurable alias maps.

To enforce layer discipline, L2 crates never import L3 porcelain, and L1 crates never import L2 runtime schedulers.

---

## Core Trait Boundaries

The stack decouples implementation details from core logic by defining abstract traits as interfaces. This design permits swapping storage backends or build schedulers without modifying porcelain crates.

### L1: Protocol Interface (atom-core)

The protocol defines a hierarchy of traits for reading, writing, and accumulating atoms:

- **`AtomSource`**: The read-only observation interface. Returns entries by identity or by search query.

  ```rust
  pub trait AtomSource: Send + Sync + 'static {
      type Entry: AtomEntry;
      type Error: std::error::Error + Send + Sync + 'static;

      async fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error>;
      async fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error>;
  }
  ```

- **`AtomContent`**: Extends `AtomSource` with the ability to yield the content tree for a specific atom version. This is the content recovery interface — it extracts the full file tree that `AtomSource` deliberately omits.

  ```rust
  pub trait AtomContent: AtomSource {
      async fn content(
          &self,
          id: &AtomId,
          dig: &[u8],
      ) -> Result<Option<Vec<ContentEntry>>, Self::Error>;
  }
  ```

- **`AtomRegistry`**: The write-only publisher interface. Establishes ownership via `claim` (returning a `Czd` used to authorize subsequent publishes) and creates version releases via `publish`.

  ```rust
  pub trait AtomRegistry: AtomSource {
      fn claim(&self, id: &AtomId, owner: &[u8]) -> Result<Czd, Self::Error>;
      fn publish(
          &self,
          id: &AtomId,
          claim: &Czd,
          version: &RawVersion,
          dig: &[u8],
          src: &[u8],
          path: &str,
      ) -> Result<(), Self::Error>;
  }
  ```

- **`AtomStore`**: The consumer-side accumulation interface. Extends `AtomContent` with `ingest` to import atoms from any source, preserving the monotonic accumulation guarantee.

  ```rust
  pub trait AtomStore: AtomContent {
      async fn ingest<S: AtomContent>(&self, source: &S) -> Result<(), Self::Error>;
      async fn contains(&self, id: &AtomId) -> Result<bool, Self::Error>;
  }
  ```

_The default implementation of these traits is provided by the `atom-git` bridge, translating the operations into Git references and objects._

### L2: Scheduler Interface (eos-core)

The build scheduler separates evaluation orchestration from build execution:

- **`BuildEngine`**: The execution interface. It evaluates atom references into build plans, checks cache state, executes sandboxed builds, and extracts artifact metadata.

  ```rust
  pub trait BuildEngine: Send + Sync + 'static {
      type Digest: Digest;
      type Plan: Clone + Send + Sync + 'static;
      type Output: Send + Sync + 'static;
      type Error: std::error::Error + Send + Sync + 'static;

      async fn evaluate(&self, request: EvalRequest<Self::Digest>)
          -> Result<Self::Plan, Self::Error>;
      async fn plan(&self, request: EvalRequest<Self::Digest>)
          -> Result<BuildPlan<Self::Digest, Self::Plan>, Self::Error>;
      async fn build(&self, plan: &Self::Plan) -> Result<Self::Output, Self::Error>;
      async fn lookup_cached(&self, plan: &Self::Plan)
          -> Result<Option<Self::Output>, Self::Error>;
      fn plan_digest(&self, plan: &Self::Plan) -> Self::Digest;
      fn output_artifacts(&self, output: &Self::Output, plan: &Self::Plan)
          -> Vec<ArtifactInfo<Self::Digest>>;
  }
  ```

- **`ArtifactStore`**: The cache and storage interface. Stores build artifacts and links them to source atom digests, enabling cache-skipping optimizations.

_The default build engine is implemented by `eos-snix`, which delegates build execution to the Snix engine while Eos manages scheduling and caching._
