+++
title = "Hermetic Transactional Composition: the post-Nix build substrate"
description = "How HTC builds unmodified upstream software inside a cryptographic closure, with no expression language and no store-path lore — the four new nouns, the one function, and why it replaces Nix's model rather than patching it"
quadrant = "Explanation"
tags = ["htc"]
audience = "Architects and engineers evaluating the Axios build substrate who want to understand what changed after Nix and why, without reading the full ADR"
+++

HTC (Hermetic Transactional Composition) runs upstream's own build, inside a cryptographic closure. There is no expression language to learn, no evaluator to run first, and no store-path lore (`RPATH` rewriting, `patchelf`, `cc-wrapper`) to work around — a package's own `./configure && make && make install`, unmodified, executed inside a sandboxed view that contains exactly its declared dependencies and nothing else, with every input and every output addressed by content, never by a mutable path.

## The one-paragraph model

An **atom** is signed intent — sources plus lock, already defined one layer down and unchanged here. Building an atom against a chosen toolchain produces a **tree**, a content-addressed Merkle output. Analyzing a tree produces an **interface manifest** — the facts (`provides`/`requires`) about what it offers and needs, derived once and memoized. A **composition** binds names to digests — a signed, content-addressed closure, the direct successor to a Nix derivation's output closure — and a **view** is a composition mounted at runtime, materialized on demand rather than persisted. Nothing in this model is interpreted: compositions are pure data, and the only function that operates over any of it is `build`.

## The four nouns and one function

HTC introduces four new nouns on top of the atom that Atom (L1) already defines:

- **Tree** — a castore Merkle output: the result of running a build, hashed and chunked for storage.
- **Interface manifest** — a derived, static fact about one tree (`{subject, provides, requires}`), keyed by _(analyzer, subject)_ so a newer analyzer version gets its own key rather than overwriting an older one's facts. Dynamically observed facts (what a check-phase run actually touched) are a separate, run-scoped record — not part of this object, because they depend on which composition was mounted for that run, not on the tree alone.
- **Composition** — a signed, content-addressed binding of conventional names to content digests: the closure object, and the successor to the drv-closure. Most entries pin an exact digest; an entry may instead carry an ABI-satisfaction constraint (a provider whose interface manifest satisfies what's bound to it, with the proof recorded alongside). Constraint strength is a per-entry attribute, not a toggle on the composition as a whole.
- **View** — a composition mounted at runtime. How it is mounted and whether file access is observed are two independent choices, not tiers: materialization ranges from composefs/EROFS + fs-verity (the production default — kernel-enforced tamper evidence) to plain export copies, OCI images, or tarballs for interop elsewhere; observation is an execution-policy flag on any of them, implemented by syscall-level tracing (ptrace + seccomp), and paid only by the executions that request it.

The **one function** is `build: (atom closure, toolchain composition, action params) → output tree`, executed by upstream's own, unmodified build process inside a materialized FHS view. It is hermetic — nothing outside those three inputs can influence the result, so the same inputs always mean the same cache identity — while byte-for-byte reproducibility is evidence accumulated across independent runs, never an assumption. And it is not total: an unmodified upstream build can still fail for the reasons it would fail anywhere else. In the substrate's execution model, `build` is the fully-closed instance of the one general operation, `execute` — see [the three primitives](the-three-primitives.md). An **action** is one invocation of `build`, identified by

```
action_id = H(atom_czd_closure_root, toolchain_composition_root, action_params)
```

— the cache-key primitive that replaces the drv hash everywhere it was used as one. Same three inputs, same `action_id`, same cache slot.

## Why leave Nix's model rather than patch it

The motivation is the payoff, not any single defect. A coherent compositional model — bindings carried _beside_ artifacts rather than paths baked _inside_ them — is simultaneously cheaper and more flexible than Nix's. Cheaper: Nix's storage cost is explosive compared to traditional build systems, because every input change produces a new store path and re-materializes entire closures of near-identical content; content-addressing with external bindings dedupes at the chunk level and _rebinds_ instead of re-materializing. More flexible: swapping a provider becomes a checked relink with a bounded rebuild frontier, instead of a mass rebuild of everything downstream for no reason. Those benefits are what the implementation effort buys.

There is also a technical reason patching Nix cannot get there — why the move is necessary, not merely attractive. A collision-resistant hash has no accessible fixed point: an artifact cannot embed a pointer to its own content hash, so any system that embeds hash-pointers _inside_ artifacts — as Nix's store paths do, via `RPATH`, shebangs, baked-in dependency paths — is structurally obstructed from being purely content-addressed. Nix's own fix for this (content-addressed derivations, RFC 62) has been unstabilized since 2019, fighting exactly this obstruction with hash-rewriting that breaks signatures. HTC moves the pointers _beside_ the artifact instead of inside it, into the composition — a separate, signed, content-addressed binding object — and the obstruction dissolves by construction rather than by patching.

## The concept-count argument

What a newcomer has to hold in their head, across the whole substrate, is five nouns and one function — atom (already familiar from L1) plus the four new ones above, and `build` — no lazy functional language, no `stdenv`/`cc-wrapper`/patchelf lore, no fixed-output exceptions, no `nixpkgs`. Interface manifests are the only genuinely new _concept_ among the four HTC introduces, and they make _explicit_ what Nix left as implicit lore — ABI compatibility via mass rebuilds, `outputs.dev` splitting conventions. Everything else this substrate needs already exists in production, separately: `snix-castore`'s Merkle trees, the OCI/bwrap sandbox in `snix-build`, composefs+fs-verity, and 25 years of distro-grade interface extraction (rpm `elfdeps`, `dpkg-shlibdeps`, `libabigail`). HTC is the composition of existing, production-proven parts, not a new million-line corpus.

## The ingestion wedge

Existing ecosystem artifacts — distro packages, upstream release tarballs, PyPI wheels — are ingestible on day one with zero rebuilds. Adopting HTC does not require anyone to build natively on the substrate first: a composition can bind names to digests of artifacts nobody built with HTC at all, and interface analysis still derives real provides/requires facts from them. The substrate earns its way in at the edges before it has to win every build.

## Where this fits

HTC is the new L2 layer of the Axios stack, between Atom (L1, identity and signing) and Eos (L3, scheduling — moved up from L2 to make room). Eos's scheduler dispatches build actions through HTC's **executor trait**; the implementation is the FHS/composefs builder described above. There is no evaluator anywhere in the stack — not even as an interop on-ramp ([ADR-0006](../architecture/0006-execution-as-the-primitive.md) removed it from the design entirely); Nix-ecosystem content enters the same way any ecosystem's artifacts do, by ingestion at the edges.

For the full decision record, forces, and consequences, see [ADR-0005](../architecture/0005-hermetic-transactional-composition.md). For the complete schemas, algorithms, and component design, see the [HTC Software Architecture Document](../architecture/htc-sad.md).
