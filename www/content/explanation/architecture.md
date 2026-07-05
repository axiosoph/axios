+++
title = "Stack architecture and implementation"
description = "Technical overview of the Axios monorepo workspaces, crate organization, and trait boundaries"
quadrant = "Explanation"
tags = ["general"]
audience = "Axios stack developers, codebase contributors, and architects"
+++

The Axios stack is a layered, content-addressed publishing and build system. The codebase is split into three independent Cargo workspaces with a downward-only dependency chain: **Ion (L3)** $\to$ **Eos (L2)** $\to$ **Atom (L1)**.

A standalone utility crate, **alurl**, lives at the repository root. It handles URL alias resolution for `atom-uri`.

## Workspace organization

- **L1: Atom** (`atom/`) — The protocol layer. Defines package identity (`AtomId`, the abstract `(anchor, label)` pair — not a hash of it), cryptographic signing (via [Coz](https://github.com/Cyphrme/Coz)), content-addressed store references (`publish_czd`, with the store keyed `blake3(publish_czd)`), and transport mapping. Has no knowledge of build environments or dependency resolution. Four crates: `atom-id`, `atom-uri`, `atom-core`, `atom-git`.
- **L2: Eos** (`eos/`) — The runtime scheduler. Constructs build plans and coordinates cache-skipping, dispatching sandboxed execution to workers behind an executor trait — `eos-snix` today, re-scoping toward [HTC](hermetic-transactional-composition.md)'s executor trait per ADR-0005. Five crates: `eos-core`, `eos`, `eos-snix`, `eos-daemon`, `eos-proto`.
- **L3: Ion** (`ion/`) — CLI and dependency resolver. Parses manifests (`ion.toml`), resolves the dependency graph with a SAT solver, and writes lockfiles (`ion.lock`). Five crates: `ion-cli`, `ion-manifest`, `ion-lock`, `ion-resolve`, `ion-eos`.
- **alurl** (`alurl/`) — URL alias detection and expansion. Resolves `+`-prefixed identifiers (e.g. `+gh/owner/repo`) through configurable alias maps.

L2 crates never import L3. L1 crates never import L2.

---

## Trait boundaries

The stack uses abstract traits as interfaces between layers. Swapping a storage backend or build scheduler does not require changes to porcelain crates.

### L1: Protocol interface (atom-core)

The protocol defines a hierarchy of four traits:

- `AtomSource` — Read-only observation. Looks up atoms by identity or search query.

  ```rust
  pub trait AtomSource: Send + Sync + 'static {
      type Entry: AtomEntry;
      type Error: std::error::Error + Send + Sync + 'static;

      async fn resolve(&self, id: &AtomId) -> Result<Option<Self::Entry>, Self::Error>;
      async fn discover(&self, query: &str) -> Result<Vec<AtomId>, Self::Error>;
  }
  ```

- `AtomContent` — Extends `AtomSource` with content tree extraction. This is how consumers recover the actual file tree that `AtomSource` deliberately omits.

  ```rust
  pub trait AtomContent: AtomSource {
      async fn content(
          &self,
          id: &AtomId,
          dig: &[u8],
      ) -> Result<Option<Vec<ContentEntry>>, Self::Error>;
  }
  ```

- `AtomRegistry` — Write-only publisher interface. `claim` establishes ownership (returning a `Czd` that authorizes subsequent publishes). `publish` creates version releases.

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

- `AtomStore` — Consumer-side accumulation. Extends `AtomContent` with `ingest` to import atoms from any source. The store only grows; it never loses atoms through ingestion.

  ```rust
  pub trait AtomStore: AtomContent {
      async fn ingest<S: AtomContent>(&self, source: &S) -> Result<(), Self::Error>;
      async fn contains(&self, id: &AtomId) -> Result<bool, Self::Error>;
  }
  ```

The default implementation of these traits lives in `atom-git`, which maps the operations to Git references and objects.

### L2: Scheduler interface (eos-core)

The build scheduler separates evaluation from execution:

- `BuildEngine` — Evaluates atom references into build plans, checks the cache, runs sandboxed builds, and extracts artifact metadata.

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

- `ArtifactStore` — Cache and storage interface. Links build artifacts to source atom digests for cache-skipping.

`eos-snix` is the current build engine. Per [ADR-0005](../architecture/0005-hermetic-transactional-composition.md), Eos is re-scoping around [HTC](hermetic-transactional-composition.md)'s executor trait: the `evaluate`/`plan`/`build` split above reflects `BuildEngine`'s pre-substrate design, and the associated `Plan` type is exactly the escape hatch that substitution exercises — `eos-snix` becomes one executor implementation behind that trait rather than the assumed default, and the evaluation step `evaluate` names is being deleted from the design, not deferred.
