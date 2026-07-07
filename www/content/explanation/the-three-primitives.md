+++
title = "The three primitives: store, compose, execute"
description = "A plain-language companion to the formal substrate models — why the whole build system reduces to three operations, and what that buys you"
quadrant = "Explanation"
tags = ["htc"]
audience = "Developers who want to understand how the Axios substrate works without reading the formal models"
+++

Underneath everything Axios does — building packages, running tests, assembling environments, installing a system — there are exactly three operations. Everything else is one of these three wearing a costume:

1. **Store** — put bytes somewhere and name them by their own content.
2. **Compose** — arrange stored content into a world a program could run in.
3. **Execute** — run a program in that world and record what happened.

The formal models pin each of these down with laws and proof obligations. This page explains the same design in ordinary terms: what each primitive does, why the split is exactly here, and what falls out of it for free.

## Storage: identity

The store answers one question: _what is this?_ Every blob and directory tree is named by the hash of its own content. That single idea does a surprising amount of work: if you know the name, you can verify the bytes yourself, so **it doesn't matter who hands them to you**. A mirror, a CDN, a stranger's laptop — the content either matches its digest or gets rejected. Distribution becomes trustless without any protocol cleverness, and "garbage collection" and "rollback" reduce to keeping or dropping references.

## Composition: structure

A composition is a **description of a filesystem as a value** — a map from paths to stored content, itself content-addressed and signed. When a build runs "against an environment," that environment is not the ambient machine; it is one of these values, materialized as the process's entire visible world.

Compositions behave like modules in a programming language. Each piece declares what it **provides** and what it **requires**; an _environment_ is a composition where every requirement has been wired to a provider and the wiring is recorded in a **certificate** — a receipt anyone can recheck without running anything. That receipt is what makes two things possible that package managers historically can't do:

- **Fix without rebuilding.** If a package turns out to need a runtime dependency no detector found, the fix is a one-line _declaration_ added to that package's contract — and every environment containing it relinks automatically on re-formation. Nothing is rebuilt, because no package's content changed. (Environments themselves are never patched: they're for composing packages, not fixing them, so a repair never gets stranded in one environment while others stay broken.)
- **Swap with a bounded blast radius.** Replacing a library (say, a patched OpenSSL) is a checked edit: anything whose declared interface still matches is simply relinked; only the consumers the check _fails_ for actually rebuild. Nix, by contrast, must rebuild everything downstream, because it has no way to check that anything survived.

Crucially, composition is **not a programming language**. It is a small algebra of data operations that always terminates. Anything that needs real computation is, by definition, the third primitive's job.

## Execution: dynamics

Execution is one operation: run an opaque command in a materialized composition, under a **policy** that says, for each channel to the outside world — network, clock, randomness — whether it is _closed_, _pinned to a declared value_, or _open_.

That policy, not the kind of workload, is what distinguishes everything:

- If **no channel is open**, the run is an **action**: the world can't influence it, so its result is a durable, cacheable fact. A build is an action. So is a fully sandboxed test.
- If **any channel is open**, the run is a **trial**: its result is signed **evidence** about one moment in the world — a networked test run, a dependency-discovery fetch — never a cache entry.

Several long-standing build-system pain points just disappear as consequences of this split, rather than being features anyone implemented:

- **Editing test configuration never triggers a rebuild** — test parameters simply aren't part of any build's identity.
- **A failing test never poisons a good build.** The build fact exists the moment the build finishes; a test failure only gates whether it gets _advertised_. (In nixpkgs, build and test share one derivation, so a flaky sandbox test forces a rebuild of a perfectly good artifact.)
- **Caching a hermetic test is sound** — it's an action like any other.

## The loop that ties them together

The three primitives form a cycle, not a stack. Composition assembles a world; execution runs in it and produces records; and **promotion** carries results back into composition's inputs — as signed, reviewed intent, never silently.

The everyday example: discovering a dependency's download URL and hash requires the network (a trial), but once the discovered pins are written into the lock file — reviewed and signed like any other change — every future build replays them with the network otherwise sealed. It's the same shape as `cargo update` writing `Cargo.lock`. And for ecosystems that already ship a lockfile, Axios adopts it directly as the pin set rather than translating it.

That signature requirement is the system's most important door. Nix's import-from-derivation lets evaluation quietly depend on build results, which is why Nix evaluation can be slow, unanalyzable, and impossible to cache well. Here, the only way a computed result becomes an input is through an explicit, signed promotion — you can always see where the dynamic step happened and who vouched for it.

## Why there is no evaluator

Nix gets its purity from a functional language that generates the build graph — which means trusting the graph requires trusting (or re-running) the generator, and it's where Nix's costs concentrate: evaluation runs before any real work can start and scales with the size of the world rather than the size of your change; artifacts bake their dependencies' paths inside themselves, so storage grows explosively as near-identical closures pile up; and any change re-materializes and rebuilds everything downstream, whether or not anything meaningful changed. Axios inverts the premise: **purity is a property of the objects, not of whatever produced them**. Compositions carry checkable laws, certificates carry recheckable wiring, records carry signatures. Any tool — a CLI, a script, eventually a DSL if someone wants one — may emit compositions, because nothing about the system's soundness depends on how they were made.

The payoff is the flexible, checkable composition described above — rebind instead of rebuild, dedupe instead of duplicate — and it is why the substrate's trusted core contains **no interpreted language at all**: the evaluator is not deprecated or wrapped, it is gone from the design.

## Many answers, honestly

One more inversion worth knowing about: when several builders run the same action, they may produce byte-different results (parallelism, linkers, luck). Instead of pretending a canonical answer exists, the substrate lets **witnesses accumulate** — each result is a signed record, and _your_ builds use whichever witness your trust settings accept. Agreement among independent builders is surfaced as evidence of reproducibility; disagreement is surfaced as information. Nothing is ever silently reconciled.

## Going deeper

The precise versions of everything above, with the laws and proof obligations:

- [Storage Model](../models/storage-model.md) — the axioms everything else borrows.
- [Composition Model](../models/composition-model.md) — the merge algebra, interfaces, certificates, and overrides.
- [Execution Model](../models/execution-model.md) — requests, policies, actions/trials, records, and the promotion laws.
- [ADR-0006](../architecture/0006-execution-as-the-primitive.md) — the decision record that ratified this design.
