+++
title = "Encapsulation and Evaluation-Time Purity"
description = "How Atom Nix solves Nix scaling and cross-contamination issues using scoped evaluations and strict dependency separation"
quadrant = "Explanation"
audience = "Nix developers, system configuration engineers, and architects looking to understand evaluation-time performance optimization"
+++

# Encapsulation and Evaluation-Time Purity

The Nix package manager is celebrated for its ability to deliver reproducible, content-addressed build artifacts. However, Nix does not enforce the same discipline on its own source code (Nix expressions). As Nix repositories (such as `nixpkgs`) scale to millions of lines, this lack of code boundaries leads to a scaling crisis.

Atom Nix provides the core language-level tooling to solve this, enforcing true code encapsulation and evaluation-time isolation.

## The Nix Scaling Crisis: Unbounded Evaluation

In standard Nix, there are no enforceable boundaries between files or expressions:
- **Global Import Scope**: Any Nix expression can use `import` to read and evaluate any file in the repository (or even on the local filesystem in impure modes).
- **Implicit Coupling**: If package `A` imports a utility from package `B`, it implicitly couples `A` to `B`'s files. The evaluator must parse both subtrees, and changes to `B` force re-evaluation of `A`.
- **Monolithic Evaluation**: When using a repository like `nixpkgs`, evaluating a single package derivation requires checking out and parsing the entire repository. This results in heavy memory usage and slow evaluation times.

### Why Flakes Fail to Solve the Scaling Crisis

Nix Flakes were introduced to standardize package inputs and outputs. While flakes lock input versions, they do not solve the scaling crisis:
- **Monolithic Checkouts**: To evaluate a sub-flake or a single package from a flake, Nix must still check out the entire git repository containing the flake.
- **No Evaluation Isolation**: Flakes do not restrict the use of `import` within their expressions. A flake can still perform arbitrary, un-tracked imports across its source tree, maintaining implicit coupling.

## True Encapsulation via Scoped Imports

Atom Nix addresses this by establishing strict, enforceable module boundaries using a little-known primitive: `builtins.scopedImport`.

Instead of letting modules import files directly, Atom Nix wraps module evaluation inside a controlled context. It overrides the default Nix prelude so that calling `import` or `scopedImport` from within a module triggers a hard evaluation error. 

```
┌──────────────────────────────────────────────┐
│             Atom Nix Evaluator               │
│  (Custom context injected via scopedImport)  │
└──────────────────────┬───────────────────────┘
                       │
         ┌─────────────┴─────────────┐
         ▼                           ▼
  [from] Scope                [get] Scope
  - Eval-time code            - Build-time sources
  - Resolved dynamically      - Deferred to build-time
  - Absolute boundaries       - No eval-time fetch
```

All external dependencies must be explicitly declared in the module manifest and are injected into the evaluation context through two specific namespaces:
- **The `from` Scope**: Houses evaluation-time dependencies (Nix libraries or configurations). These are resolved and evaluated dynamically.
- **The `get` Scope**: Houses build-time dependencies (source trees, patches, binary assets). These are deferred to the build phase and are not fetched during evaluation.

## Benefits of Evaluation-Time Separation

By separating evaluation-time concerns from build-time concerns, Atom Nix achieves:

1. **Lazy Purity**: The evaluator only fetches and parses the code needed to construct the derivation. It does not download source trees or build tools during evaluation, drastically reducing startup latency.
2. **Evaluation Caching**: Because the evaluation graph is explicitly declared and isolated, evaluation outputs can be cached reliably. If the dependencies in the `from` scope do not change, the evaluation output is guaranteed to be identical, allowing the client to skip evaluation entirely.
3. **Static Introspection**: Tooling (like Language Servers and static analyzers) can inspect module interfaces and types without running the Nix evaluator, as imports cannot be executed dynamically.

Through scoped evaluation, Atom Nix brings the modularity and performance characteristics of modern compilation units to Nix expressions.
