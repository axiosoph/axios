+++
title = "Frequently asked questions"
description = "The whole Axios project in one place: what it is, why it matters, how each layer works, and how to get involved"
weight = 1
quadrant = "Explanation"
tags = ["general"]
audience = "Anyone seeking to understand Axios, from the curious to prospective contributors"
+++

## Part 0 — Orientation

### What is Axios?

A decentralized publishing and build system built on one primitive: the
signed, content-addressed binding of names to content, applied
recursively from published sources all the way to running systems. We
call the paradigm **composition-addressing**. The goal is software you
can verify instead of trust: anyone can check what was published, who
says so, what it depends on, what built it, and what it runs against,
with no central registry, build farm, or naming authority vouching for
any of it.

The stack has four working layers, each answering one question:

| Layer         | Answers                                                | In short                                                                                                     |
| :------------ | :----------------------------------------------------- | :----------------------------------------------------------------------------------------------------------- |
| **Atom** (L1) | _What is this, and who says so?_                       | Identity, publishing, and version integrity for source code                                                  |
| **HTC** (L2)  | _How does it get built, and what can stand in for it?_ | _Hermetic Transactional Composition_: builds of unmodified upstream software, composed into runnable systems |
| **Eos** (L3)  | _When and where does the work run?_                    | A build scheduler over the atom dependency graph                                                             |
| **Ion** (L4)  | _How do humans drive it?_                              | Manifests, dependency resolution, locks, the CLI                                                             |

Axios is early-stage and specification-first: the design is much further
along than the code, and the documents are honest about which is which.
Two things cut across all four layers: a verification-and-evidence
discipline (Part V holds the receipts) and a per-layer map of what
actually runs today (Part VIII).

### Why should anyone care?

Because the problem is central to computing, not niche. Everything you
run arrived through a chain of publishes, resolutions, and builds that
you cannot currently verify. You trust registries, build farms, and
maintainers' machines wholesale, and supply-chain attacks have become
one of the most effective ways to compromise an entire ecosystem
precisely because that trust is unexamined: poison one link and
everything downstream inherits it silently. Put bluntly, if we can't
trust our own runtimes, what level of sanity do we really have?

The field's history shows both the hunger and the gap. Nix, for all its
difficulty and opacity, is adored to this day because it got closer to
the goal than anything else, and a long line of build systems since has
come along trying to solve one or two of these problems each. Our bet
is that they are one problem: verifiable, signed naming applied
recursively, from published source to running system. Solve it at the
root and the fixes stop being partial. The system also gets to be
approachable, because there is one primitive to understand instead of a
dozen patches to memorize.

That one thread, _checking replaces trusting_, is what ties the other
virtues together. Decentralization stops being a separate ideal: when
anyone can verify a claim, there is nothing left for a registry or
naming authority to vouch for. And industry compatibility stops being
a compromise, which is a point worth dwelling on. Nix ended up at odds
with the industry standard; Axios is built to integrate cleanly with
it _and_ to extend it in ways it currently struggles to do for
itself. Giving OCI layers a coherent meaning is no small thing. The
layering becomes essentially a Merkle tree instead of ad hoc blob
hashes, the entire distribution model becomes self-verifying exactly
the way Axios's own model is, and it composes cleanly with OCI's
existing implementation rather than replacing it (entry 15). That is
the posture toward the industry across the board: extend the standard
where it struggles, never fight it. Trustable runtimes are the payoff
at the end of that chain.

So for all the negatives above, the vision is overwhelmingly positive:
every one of them is a removable property, not a fact of life. The
world this is aimed at has three facets of one picture. Verification
goes ambient: nobody "does" it anymore, it's simply a property the
runtime has, and compromise becomes loud instead of silent. The
lockfile moment generalizes: lockfiles made dependency drift boring,
and this makes the entire system boring in the same way, built and
composed and run against exactly what was pinned and proven. And
upstream becomes the distro: systems composed directly from what
upstream publishes, with no repackaging middle layer between authors
and the machines that run their work.

One thread in that picture deserves explicit weight: deployment. Years
of running Nix in production made its deployment model's awkwardness a
personally felt limit. A system verified all the way to _where it runs_
is not the only end goal here, but it is a major one.

---

## Part I — Atoms: identity you can verify

### 1. What is an atom?

Signed, versioned build intent: a project's sources, manifest, and lock
under one snapshot identity, published as the unit of a
version-integrity system. An atom composes at three layers:

1. **The record closure** — the fixed authorization bundle for one
   specific version: the project's _charter_, the name's _claim_, and
   the version's _publish_ record, verified together, plus the content
   they authorize. Each of those is a **record**: one signed statement,
   complete and verifiable entirely on its own.
2. **The verified content tree** — the exact bytes the publish committed
   to, identified by content digest no matter which store holds a copy.
3. **The built artifact** — what the build substrate produces from the
   verified tree.

The closure settles permanently, at publish time, what a version is and
whether it was genuinely authorized. Everything that happens afterward
(a yank, a deprecation, a security advisory, a build record, an
ownership change) is a **fact**: a new signed statement layered onto
the atom's chain, never a mutation of what's already there. Whether to
_trust_ a version is a separate judgment, computed per consumer over
those facts. "What is it" and "should I currently trust it" never
collapse into each other.

Said differently, every publish carries two verifiable record closures:
the _permanent_ one, the initial immutable bundle that maps cleanly
from input to final output, and the _temporal_ one, the same closure
with the accumulated facts layered on. Keeping them distinct is what
lets the permanent record stay meaningful forever, no matter what facts
later accrue.

The model's word for this whole shape is the **composite**, and it
names more than the atom's three layers. The output side carves the
same way (package, environment, system, each a distinct boundary with
its own meaning and mechanism), so one concise term covers a single
piece or the whole thing. Think of it as an approachable analog of
"closure": closures, too, are made of smaller closures.

For readers who know Nix, this is a direct formal iteration on the
derivation, and the correspondence is nearly 1:1. A record is the
analog of a single `.drv` file: one complete, self-describing unit. An
atom, a distinct closure of records describing one cohesive unit, is
the analog of a distinct derivation closure sufficient to describe a
package (or an environment, or a system). What changed is not the
shape but the discipline: records are signed and identity-bearing
where derivations are anonymous store artifacts, and the atom's
closure settles authorization, not just build inputs.

(Historically the atom began as an experiment: making git deliver a
package's source detached from its repository's history, as an
orphaned, reproducible commit cryptographically tied back to the
history it came from. That mechanism survives as the content layer and
the git backend's snapshot format. The model around it, the records and
claims and facts, is what grew since, and the current specifications,
not the early write-ups, are authoritative for it.)

### 2. What question started all of this?

Two, in sequence. First: Nix's derivation is too granular. There is no
precise way to track a particular package across time over its
versions, nor to know everything that derives from it. Can you define a
package identifier that is both unique enough and meaningful enough to
systematize? Then, the pivot: _we have no coherent identity for a
repository either._ Solve the repository's identity and stable
per-package identity follows.

The seed insight: a git repository's genesis commit is a meaningfully
stable identifier. No later commit can replace it without invalidating
the cryptographic identity of every commit in the history. Mix that
genesis with an unambiguous, Unicode-normalized name, cryptographically,
and you get a stable identity for a package across time.

### 3. What is an atom's identity?

**The signed claim is the center of the system.** For any claimed atom,
identity is the claim record's own content-address (its czd), a digest
that incorporates the signature itself. Identity therefore commits to
an authorization event, not merely to public content. The same rule
holds at every scope: a project's identity is the czd of its charter, a
named atom's is the czd of its claim, a version's is the czd of its
publish.

Identity also degrades gracefully, so atoms are useful before any
infrastructure exists, and it _changes_ at each boundary, by design:

- **Outside any repository** (no history at all): the anchor is a
  well-known constant. The atom still works locally.
- **Inside a repository, unclaimed** (a development atom): the anchor
  becomes the repository's genesis commit, and identity is a digest
  computed from the genesis and the atom's name, the only stable
  content that exists before a signature.
- **Claimed**: identity changes again, to the claim record's czd, and
  the anchor's _meaning_ shifts with it. It is no longer just the
  genesis: it is the point in the repository's history at which the
  claim was made, linked through ancestry all the way back to the
  genesis, but with more entropy and with temporal meaning ("this claim
  exists as of this point in this history").

What about the `(anchor, label)` pair you'll see in older documents?
Nothing, by itself. Each component has real, separate significance: a
structural anchor into the repository, and a human-readable name within
a project's scope. But the fused pair has no role anywhere except the
unclaimed-development case above, where the two values combine exactly
once, as the preimage of one digest. Inside a claim record they persist
as fields among others: metadata, not structure.

### 4. Why does signing matter so much?

Before signing, an identity could only commit to publicly computable
content. Anyone able to present the same repository and name could
assert the same identity: there was authenticity of _content_ but no
exclusivity and no owner. A signed claim changes the game. Exclusive
names within a project, real ownership, transfers recorded as facts,
and provenance that needs no registry to vouch for it. Signing "gives
us something we never had before," and the whole current model is built
around that pivot.

### 5. What are the anchor and the charter?

A project's standing identity. In its primordial form, the genesis
commit (entry 2). In the current model, the project's **charter**: a
signed founding record whose own content-address is the project's
anchor (`anchor = czd(charter₀)`, a property of the signed record
rather than of any git object, hence backend-agnostic). The charter
strengthens the primordial anchor rather than replacing what it means:
more entropy, pinned to a point in time, still linked back to the
origin commit, and carrying meaningful metadata (canonical source
domain and address) under a signature.

Governance after the founding moment is deliberately boring. The
charter is written exactly once, at genesis; everything that changes
later (key rotation, ownership, policy) is an ordinary signed fact on
the chain, never a "successor charter." This isn't a style choice:
publishes must stay immutable so that each version's permanent record
closure (entry 1) stays immutable, independent of whatever facts
accumulate afterward.

### 6. Is there a general principle behind the identity design?

Yes, worked backwards from the charter. An identity needs two things: a
meaningful structural connection (the cryptographic identity of the
anchor, or its temporal genesis point), and metadata distinguishing
each thing tied to that anchor from every other thing on the same
anchor. The principle generalizes downward: a version is linked to a
particular charter _and_ is itself a claim-shaped record of similar
purpose.

### 7. What does "surety of source" mean?

An atom's legitimacy is always verifiable by consulting the source. The
claim lives at the source, and provenance verification traces content
back to the source revision. What you trust is the chain, not whichever
store happens to hold a copy. Mirrors are interchangeable because
divergence between them is tamper _evidence_, not a tie to break.

### 8. What makes it a version-integrity system?

Not the atom alone. The other critical component is the append-only,
tamper-evident log that accompanies it: the chain all of an atom's
records and facts live on. Every charter, claim, publish, and fact is
appended, never mutated and never deleted, and the chain of temporal
commitments makes tampering _evident_, not merely detectable
(entry 18 explains why git's primitives happen to host exactly such a
log). What a version is, who authorized it, and everything that has
happened to it since are all readable from one place, and all
verifiable.

This was the latest of the realizations behind the design (entry 32):
the log, together with the formalization of records, is what turned
the atom from a clever pointer hack — the git detachment trick it
began as — into a disciplined, end-to-end system in its own right. The
two record closures of entry 1 are simply views over this log: the
permanent closure is what the log held at publish, and the temporal
closure is the same thing with every fact appended since.

---

## Part II — HTC: hermetic builds without an evaluator

First, the name, since it's one of ours: HTC stands for **Hermetic
Transactional Composition**. Builds run _hermetically_, inside a sealed
world containing exactly what was declared (entry 10). Changes land
_transactionally_: a composition or view swaps whole, all-or-nothing,
never mutated in place (entries 11 and 15). And _composition_ is how
everything above a single build is formed: systems are combinations of
signed name-to-content bindings, not installations into shared state
(entry 11).

### 9. Why build a new substrate instead of fixing Nix?

Because the blocker is structural: a collision-resistant hash has no
accessible fixed point. An artifact cannot contain its own hash, so any
system that embeds hash-pointers _inside_ artifacts, as Nix's store
paths do, is structurally cut off from ever being purely
content-addressed. Nix's own content-addressing effort has fought that
obstruction for years with hash-rewriting machinery that breaks
signatures. Move the pointers _beside_ the artifact, into a separate
signed object that maps names to hashes, and the obstruction doesn't
get solved — it evaporates.

That's the killer reason, but not the only one. The other is
_complection_, in Rich Hickey's sense. The store path is one string
doing five jobs at once (storage key, build-time binding, runtime
binding, closure discovery, co-installation), braided together such
that you cannot touch one strand without pulling the other four. That
braid is what keeps Nix from doing what the rest of the world does:
FHS compatibility, `/usr/lib` instead of `/nix/store/<hash>-…`. And it
forces composition at the wrong layer. Nix discovers runtime
dependencies by scanning build outputs for embedded hash strings,
which is error-prone guesswork, when the right layer to compose at is
the runtime interface itself: what a binary imports, what a library
provides (entries 10 and 11).

The full argument is the blog post [_Nix Is Right. Its Cost Is
Not._](https://nrd.sh/blog/store-was-never-the-point.html) That post is
the significant answer to this question; this entry is the compressed
version.

### 10. So what does a build look like here?

Upstream's own, unmodified build process (`./configure && make`, cargo,
CMake, whatever the project already does), run inside a hermetic FHS
view: a composed filesystem that _is_ the process's entire visible
world, materialized from the declared dependency closure and toolchain.
No expression language, no patching, no wrapper scripts. The sandbox is
deny-by-default; the build can read exactly what was declared and
nothing else. It leans on only two conventions that are already
ecosystem norms: fetching is separable from building, and installs are
staged (`DESTDIR`-style).

Network is handled by a record/replay proxy. The first build may
discover fetches (explicitly impure, like a lockfile update); every
response is content-addressed and pinned; every later build replays
exactly those bytes with the network otherwise sealed. For ecosystems
that already ship a lockfile (`Cargo.lock`, `go.sum`, …), the pins are
adopted directly from it, with no translation layer and no
re-declaration.

### 11. What is a composition?

The closure object: a signed, content-addressed binding of names to
content digests — a description of a filesystem as a _value_. Outputs
are analyzed into **interface manifests** (what a tree provides and
requires: ELF sonames and symbols, Python modules, and so on), and a
runtime closure is _computed_ as a satisfaction fixpoint over those
interfaces. Every entry is present because a named requirement binds to
it, not because a hash-scan guessed. Two consequences follow that
package managers historically can't offer:

- **Fix without rebuilding.** A missing runtime dependency is repaired
  by a one-line declaration on the package's contract; every
  environment containing it relinks on re-formation. Nothing rebuilds,
  because no content changed.
- **Swap with a bounded blast radius.** An ABI-compatible security
  patch is a checked edit: consumers whose interfaces still match are
  rebound (a metadata change); only the consumers the check fails for
  rebuild. The satisfaction proof is recorded.

At runtime, a composition mounts as a **view** (composefs + fs-verity):
the kernel refuses to read tampered content, a strictly stronger
guarantee than verifying at download time. There is also an export tier
(plain copy, OCI image, tarball) for deploying onto systems that don't
run this substrate.

### 12. What ties the atom to what actually runs?

The atom declares the _build_ closure: the sealed world the build is
allowed to see (entry 10). But the _runtime_ closure, everything that
must be present for the artifact to actually work, is a different
thing, and Axios neither guesses it nor asks you to hand-maintain it.
It measures it. After a build, analyzers read the artifacts and
extract each tree's **runtime interface**: what it provides and what
it requires, per namespace (ELF sonames and symbols, Python modules,
and so on). A requirement binds to a provider by name, with a
satisfaction check on the match, and the runtime closure is computed
from those bindings out to a fixpoint. In effect it is a module
system: interface manifests are the signatures, and binding is
linking.

Analysis can't see everything, and the model is honest about that. A
plugin loaded at runtime, a tool invoked by name, a data file: these
are invisible to a binary reader, so the author declares them
explicitly in the manifest, and declared requirements enter the same
binding machinery as measured ones. Being wrong in the fat direction
is safe here. An unused declaration is just closure bloat, prunable
later by evidence, never a breakage.

The same mechanism scales across every output boundary: a package is
linked internally with its external requirements left open, an
environment links many packages into a single layer, and a system
composes both in arbitrary layers (entry 13).
This is the critical link in the chain from atom to running artifact:
build closure by declaration, runtime closure by measurement plus
explicit declaration, every member of what runs justified by a named
requirement. It is the exact opposite of scanning artifacts for hash
strings (entry 9), and it is why fixes and swaps don't cascade into
rebuilds (entry 11).

### 13. What exactly are the three output composites?

Everything HTC produces is one of three artifacts, and the containment
law between them is strict: packages contain only packages,
environments compose environments and packages, and a system declares
the boundaries between environments.

- **A package** is already a linked module in its own right: its
  internal requirements are bound among its own members, while its
  _external_ requirements stay open, listed in its runtime interface
  (entry 12). Open externals are normal and healthy here. A library
  that needs a libc it doesn't carry is a package doing its job.
- **An environment** is an atom that composes many packages into a
  single linked layer: one scope, every requirement bound, exactly one
  provider chosen per name (no diamonds), the choices recorded
  (defaults plus your overrides), and whatever remains open (the
  kernel ABI, the loader) declared explicitly as the ambient base
  rather than left silent.
- **A system** composes those two kinds of pieces in arbitrary layers.
  It is not a bigger environment; it is a declaration of boundaries.
  The filesystem namespace is partitioned into disjoint domains, each
  pinned to its own composition root: a base environment at `/usr`, a
  config layer at `/etc` that swaps without touching the base, a user
  layer at `/home`, a quirk scope for one individually patched
  program. Its certificate proves boundary coherence, meaning every
  layer's leftover requirements are discharged by a sibling layer or
  the ambient base. And because the domains are disjoint, there is no
  ordered shadowing and no "later wins": each layer swaps
  transactionally, whole, on its own cadence.

If you come from Nix, the mapping is direct. A package composite is
just as precise, formally, as a derivation describing a package. An
environment composite is just as rigorous as a dev shell. And a system
composite is nothing more exotic than vertical layering of arbitrary
numbers of those two pieces; the kind earns its name because that is
enough to compose an entire operating system, the role a NixOS system
plays. The difference is that Nix's derivation is totally generic: one
untyped mechanism plays every role, and only convention tells a
package from a shell from an OS. Here each kind is a type with its own
contract (the interface for a package, the coherence certificate for
an environment, the boundary declaration for a system), so what Nix
maintains by discipline and wrapper scripts, this model checks.

If you know how a binary linker works, you already know this shape,
because it is nearly `ld`'s own job at a larger radius. A package is a
partial link (`ld -r`): internally resolved, its undefined externals
expected and listed, the way an object file lists undefined symbols.
Forming an environment is the final link: every undefined reference
must resolve to exactly one definition, or it fails loudly at link
time instead of quietly at runtime. A system is the loader's world:
independently linked artifacts arranged in one namespace, with the
kernel as the ambient base. The interface manifest is, in essence, a
generalized symbol table, extended across languages and file kinds,
canonicalized, hashed, and signed. Distro tooling has scanned sonames
for dependency metadata for decades; the new step is making linking
the _composition semantics itself_, with every resolution recorded and
checkable.

Flat is the normal form, and a scope boundary must earn its existence:
either a genuine conflict (two members needing divergent providers,
which is exactly how two versions co-install, through forced, explicit
namespace separation) or a genuine intent boundary (a project dev
shell recorded as a delta over the OS scope, inheriting every choice
it doesn't override). A layer here is a certified linking boundary
with a stated reason, expressed inside the composition algebra itself.
That is the precise difference from OCI layering, where a layer is an
accident of build-script ordering (entry 15).

### 14. How do swaps, overrides, and injection work?

The runtime interface (entry 12) is a clean swap point. For providers
with well-known semantics, versioned ABIs above all, anything that
satisfies the interface can be swapped in: the substitution is checked
against every consumer's requirements, the satisfaction proof is
recorded, and consumers rebind without rebuilding (entry 11). And
because every atom is a signed, trusted claim (entry 3), what you swap
in carries its provenance with it. Composing trusted components is
trivial rather than heroic, and it works at both levels: injection
into an environment through its choice function, or into a system
through its layering, as a quirk scope that patches one program
without touching anything beside it.

Overrides are deltas, not forks. An override is a recorded change to
the choice function ("use this provider instead"), consumed as
defaults plus your delta, so a child scope inherits every choice it
does not explicitly override. Reusable edit sequences (swap, extend,
prune, override) can be bundled as an **overlay**: plain data,
content-addressed and signed like everything else. The expressive
power Nix gets from an untyped language of functions, this model gets
from a handful of typed operators whose blast radius is computed, not
discovered.

The checking is honest about its strength. Patch-level swaps of
ABI-disciplined libraries are the measured, well-evidenced case (the
OpenSSL study, entry 29); anything beyond that defaults to strict, and
per-edge declarations can tighten or relax the default in either
direction.

### 15. How does this relate to OCI and containers?

It integrates, and even augments. Nix and OCI have always been at odds;
Axios treats OCI as a first-class export target instead. An environment
exports as an OCI image with one layer per composition, each layer
carrying its coherence certificate. And since a composition is a
signed, Merkle-rooted binding of names to digests, the image's layering
becomes essentially a Merkle tree. Every layer boundary _means_
something: a certified linking boundary, present for a stated reason,
where today an OCI layer is an ad hoc blob hash whose boundary is an
accident of Dockerfile ordering. (Inside the substrate there is no
"later wins" shadowing at all. Layers compose by a conflict-checked
merge, so what OCI approximates by ordering, a composition guarantees
by construction.)

Near-term, your runtimes and deployment pipelines keep working
unchanged, and images can still flow through OCI registries; they just
start carrying images whose internal structure can be verified rather
than trusted. But the registry itself is a target, not a fixture. The
longer aim is to retire it outright: layers are _composed_, lazily,
straight from the content-addressed storage interface, and that
interface is highly optimized, transferring only the chunks of files
you don't already have. That is a much more natural network story than
pulling giant blobs from a single point of failure. So the goal is not
only decentralized publishing of source. It is the decentralization of
OCI image composition itself. That sits outside the MVP's scope, and
honestly so, but it's worth naming: once the foundational pieces are in
place, what this stack can offer OCI is significant.

### 16. Where did the evaluator go?

Gone entirely, with no compatibility fallback. An evaluation layer was
central to the design until the final insight of the years-long search:
Nix isn't needed at all, and removing it has real structural benefits.
The build DAG is read directly off dependency locks. There is no
interpreted language in the trusted core, so there is nothing whose
evaluation could become the bottleneck or the trust problem.

### 17. What does "cacheable at all levels" mean?

The constraint that bounded the whole research arc, predating even the
atom: no evaluation that has run should ever run again, and no build
that has run should ever build again. It began as an observation about
Nix. Full caching was already technically _possible_ there, but for
lack of proper encapsulation it was never fully expressed. Bound an
encapsulated format representing one package version, and that source
itself becomes a cache key, for the derivation it produces and
ultimately for the final build. In Nix that is a natural consequence of
the compositional model, enforced by purely functional semantics.

Axios keeps the principle: caching _should_ fall out of stable
content-derived keys over deterministic, hermetic computation. But
there is one honest difference. With no purely functional language here
to enforce it automatically, it is carried as an explicit invariant the
design stays cognizant of at every layer, not assumed as a free
theorem.

### 18. Why is git the storage substrate?

Because the tree pointer was the first thing that worked. The founding
experiments made git deliver an isolated commit, with no history
attached, containing the precise tree object of the commit it derived
from. The property was discovered empirically and formalized later; the
backend contract now states what _any_ content-addressed versioned
store must provide, making git the reference backend rather than a hard
dependency.

There's a deeper way to say it. Git describes itself in two layers: a
content-addressable filesystem (layer one) with a version-control
system built on top (layer two). Axios is essentially building an
alternate layer two, and gently reframing layer one, which at its core
is just a blob store with first-class tree structures and temporal
commitments. Those primitives happen to be general enough to host
something git's makers never described: a fully verifiable, append-only
Merkle log and its chain of temporal commitments. Git is more general
than even its own documentation says, and the atom store is a second
index over that generality, beside the one git already maintains.

### 19. What is the relationship with Snix?

Two pieces of the implementation are deliberately not ours. HTC's
content-addressed store is slated to ride on `snix-castore`, and the
hermetic build environment reuses `snix-build`, the sandbox layer
underneath the FHS view. Credit where due, and gladly: without those
two pieces existing, this would be a lot more work. The Snix folks
decomplected their storage layer when they didn't strictly have to,
and in doing so built components more general than the system they
were reimplementing.

The boundary is precise: those two components and nothing else. None
of the Nix-evaluation machinery Snix also carries is used, which is
consistent with the evaluator being gone from this design entirely
(entry 16). And the intent toward upstream is cooperative, not
extractive. Where Axios needs something the components don't yet have,
say performance work or production-grade logging, the plan is to
contribute and help make their product more whole, rather than diverge
or fork.

---

## Part III — Eos: the build engine

### 20. What is Eos?

The build scheduler (L3). It takes the atom dependency DAG, read
directly off locks and never produced by evaluation, and dispatches
build actions to workers through an executor trait, from a single
machine up to a federated cluster. Eos schedules; it does not build.
Building is HTC's contract, and Eos neither knows nor cares which
executor implementation backs a worker.

### 21. What's distinctive about the scheduling?

Stable identity makes builds _predictable_. Because an atom's identity
doesn't change across versions and builds, historical build profiles
have a reliable key: build #1 and build #1000 of the same atom share
it, so cheap statistics over history (durations, resource shapes) give
the scheduler a high-quality prediction oracle. The atom protocol
itself is what makes learning-augmented scheduling work without heavy
machinery.

And the guarantee is more than a hope: scheduling is near-optimal when
predictions are good _and_ stays within proven bounds when predictions
are arbitrarily wrong. That property is mechanically proven
(entry 29). The theory is node-agnostic and carries over to
atom-granularity scheduling unchanged; the current Rust implementation
predates the re-scope and is treated as a scaffold, which the roadmap
says plainly.

The design didn't start from a favorite algorithm. It started with
extensive research passes over the scheduling-theory literature: get
current with the state of the art first, then tailor it to Nix
derivations (and now atoms). The thread that survey produced: a
scheduler with no head-of-line blocking and no starvation is the
field's holy grail, so that became the aim, and it took many iterations
to achieve both at once with the right balance. The learning-augmented
literature was the entry point (Graham's classic list scheduling came
with it as the standard provable baseline), and the real aha was the
identity–oracle link above. Stable atom identity is what turns build
history into a key you can predict from.

### 22. What happens when builders disagree?

Records accumulate. When several builders run the same action, each
result is a signed record; there is no canonical winner and nothing is
silently reconciled. Your builds use whichever witness _your_ trust
policy accepts: your own builds, named builders, M-of-N agreement.
Agreement among independent builders surfaces as reproducibility
evidence, and disagreement surfaces as information. For atoms that
_declare_ reproducibility, a conflicting record from a trusted builder
is a standing alarm that policies can refuse to cache-serve until it's
adjudicated. Substituted artifacts are always digest-verified
regardless of who served them.

---

## Part IV — Ion: the human interface

### 23. What is Ion?

The layer people actually touch (L4). You declare direct dependencies
with version constraints in a **manifest**; Ion resolves the transitive
graph to a single coherent version per atom, deterministically, and
writes a **lock**, the sole reproducible input the build engine needs.
Resolution failures produce actionable "because X and Y, therefore Z"
diagnostics rather than solver dumps.

The resolution problem was the proof-of-concept's hardest validation
and the reason stable identity was needed in the first place:
transitive, decentralized version resolution across independent
repositories, with no prior art. It was proven end-to-end in the PoC
with a SAT-solver-backed engine handling diamond dependencies and
conflicts. SAT was the hunch from the start, and for a principled
reason: the system's formality is what carries the guarantees it
wants. Less formal resolvers (PubGrub and kin) were considered, but in
a system with formal discipline everywhere else, modeling resolution
inside that same discipline was the consistent call.

### 24. What's in the lock — and what deliberately isn't?

The lock is small on purpose: set anchors with discovery snapshots,
ground dependency pins with the requires graph, promoted fetch pins,
and a schema version. Everything in it is _ground_, meaning exact
versions and content identities, never ranges. Everything else lives
where it belongs: constraints and parameters in the manifest (inside
the atom), build records and interface manifests on the atom's fact
chain, adopted ecosystem lockfiles inside the atom's sources. The
governing rule: **lock = intent (before the build); metadata = fact
(after the build).** Locks are canonical bytes, so the same inputs
yield a byte-identical lock, a lock diff is meaningful, and a lock
digest is never a hidden identity.

### 25. How does Axios relate to cargo, npm, and friends?

It works a layer above them, not in their place. A claim declares which
ecosystem an atom wraps (PURL-style: `cargo`, `npm`, `pypi`, …), and
thin ecosystem adapters know how to find and read that ecosystem's
manifest and lockfile. A `Cargo.lock` is still useful, and is adopted
directly as the pinned fetch set. It just isn't complete: the atom
carries the _system-level_ closure, the toolchains and native libraries
that no language lockfile sees.

That's why ion is a **system compositor**, not a package manager. A
package manager cares about its own ecosystem; a compositor cares about
the entire formal closure a build needs. Package is only the first
boundary, then environments, then whole systems, so the tool is named
by its upper bound.

### 26. How will ion actually feel, compared to Nix?

In spirit, the system compositor works much more like a classic
package manager than like Nix, and that is a feature. When you ask for
something, ion traverses metadata and fetches actual, trusted
artifacts. Nothing stands between you and your package. Nix interposes
a Turing-complete, unknowable computation (the language) between every
request and its result, and the complexity of nixpkgs compounds it.
Removing the evaluator (entry 16) didn't just shrink the trusted core;
it removed the wait and the mystery from the everyday path. This is a
benefit of the evaluator's death worth naming on its own: the old
principle that true genius makes things simpler is being followed
quite literally.

The shorthand: Nix, but more disciplined, and with much broader
abilities that are first class rather than ad hoc. The endgame for ion
is UX, and that is a major reason the whole model was fleshed out
beforehand. When the components the interface must handle are clear,
the UX can fall out of them instead of being invented against
ambiguity. A scoped UX design pass precedes ion's implementation on
the roadmap for exactly this reason.

### 27. What are plugins for?

The plugin boundary is first class, not an afterthought; the stack
reserves its top layer (L5) for it. The ecosystem adapters of
entry 25 are one family. Deployment is the sharpest example of what
the boundary makes possible: picture a deployment plugin that manages
deploys as a git history whose every point is a precise, recorded
deployment of an environment, with policies attached per environment,
like "no unsigned, untrusted atoms on prod." Machinery like that has
to compose with locks, facts, and trust policy, but it does not need
to live in the core. Keeping it a plugin keeps the core small and the
ability first class at the same time.

### 28. As a Nix veteran, don't I give up declarative system configuration?

No, and this objection deserves to be named, because it will likely be
a veteran's first: Nix users rightly prize cryptographically bundled
system, service, and user configuration. That has always been an
explicit goal here too, even before it was articulated anywhere. Two
things actually improve. First, the ability is well bounded: because
configuration composition is not entangled with a package evaluator,
it inserts cleanly, without rebuilding the world the way each NixOS
rebuild does. Second, generative configuration can be composed back in
where it earns its keep: an atom may use a configuration language like
Nickel to compose its entire configuration environment. The evaluator
is removed from the trusted core, then re-admitted scoped, per atom,
for exactly the jobs it is good at.

---

## Part V — Proof and evidence

### 29. Most of this is design documents. How do you know any of it is right?

The design is checked, not just asserted. The scheduler's dispatch
theory is mechanically proven and model-checked, so "near-optimal when
predictions are good, bounded even when they're wrong" is a theorem
rather than a hope. The charter protocol is model-checked against
explicit attack scenarios (hostile takeovers, forged successions,
forks). And the code that exists is fuzzed and tested against
adversarial cases the model must reject.

The empirical claims were tested against reality too, not benchmarks.
The scheduling heuristics run against real build graphs extracted from
nixpkgs' CI, packages like the Linux kernel and Chromium. The "swap a
patched library without rebuilding" claim was exercised on a real
Debian OpenSSL security update, where every consumer the analysis
cleared kept passing its own test suite on the swapped library.

That's as deep as this FAQ goes. The proofs, models, invariants, and
study reports all live in the repository (`docs/models/`,
`docs/specs/`, `tools/`) for anyone who wants the deep end.

### 30. Why take a spec-driven approach at all?

Several reasons converge here. Nix never had a spec, and that absence
contributed in no small part to its pain: behavior defined by whatever
one implementation happened to do, corner cases discovered instead of
decided. This project is also not a first draft. The Nix model has
been turning over in one head for more than a decade, which means the
desired system and its particular constraints can be fully articulated
up front, and a spec holds the code to what is actually expected
rather than what happened to compile. And we live in a different world
now: most people code with an LLM somewhere in the loop, and precise
specifications are what make contributions, human or machine,
checkable. The end state is simple to say. Unlike Nix, this system
will never _not_ have a spec.

### 31. Beyond the proofs, what experience backs the design?

Moving deliberately cuts against the industry's grain, but it is
squarely in Nix's own lineage: Nix began as research, and the
literature has seen no significant iteration since its inception, only
a handful of obscure mentions, several of them expansions by Nix's
original author. Work like this no longer fits the standard university
flow, so pushing the research forward falls to practitioners. The atom
is offered as exactly that missing iteration: a formal successor to
the derivation, with a nearly 1:1 correspondence at its core
(entry 1).

The practice behind the bet: years of professional-grade Nix at scale,
from early flakes tooling built before flakes stabilized, to
production use close enough to see precisely where Nix cracked,
repeatedly, inside large organizations. Around it, deep study of the
surrounding systems: git and OSTree internals, deployment
infrastructure both standard (Kubernetes) and alternative (Nomad), and
a long-standing bent for functional discipline and stateless
infrastructure. Hindsight suggests why problems this painful were
never tackled whole: not because the pain wasn't real, but because the
abstractions needed to envision a whole solution, the atom first among
them, had not yet been formulated.

That industry experience also shapes the method. Specs and proofs lead
(entry 30) precisely to avoid the pitfall Nix never escaped: a
perpetual research implementation running in production. The first
whitepaper, introducing Atom and HTC together since they explain each
other's motivation, will accompany the MVP; a follow-up on Eos comes
when that layer is substantively implemented, which sits outside the
MVP's scope.

---

## Part VI — The journey

### 32. What's the actual sequence of events?

1. A decade inside Nix, half professionally, including years as an SRE
   running Nix in production, where the deployment model was always
   where things got difficult and awkward. Fixing that is a personal
   impetus that runs through the whole stack. The lingering question:
   _could you build a better Nix?_
2. Banned from the Nix community over political drama. Continuing the
   work meant working outside the project, and wanting a real answer
   rather than a fork for its own sake.
3. Entry point: the ecosystem's most pragmatic pain, build-pipeline
   efficiency. A first-principles scheduler was clearly possible, and
   just as clearly, package identity had to be solved first.
4. The bounding constraint, early: cacheable at all levels (entry 17).
5. Eos vision → the atom → experiments proving it → the PoC (eka).
6. PoC goal achieved: full transitive version resolution via SAT. The
   feasibility proof, deliberately not the product.
7. Then a genuine pause. None of this work is paid, so there were real
   gaps, and after eka came a long stretch of thinking and writing to
   figure out what should even happen next.
8. Out of that came the **Hermetic Transactional Composition
   realization**: the store model's obstruction dissolves if pointers
   move beside artifacts, and no evaluator is needed at all. The three
   primitives (fetch, execute, compose) were named at the same moment,
   in the blog post _"Nix is right; its cost is not"_. Late, though
   they had been implicit in Nix's own model and quietly guiding the
   process all along; they couldn't be named until the complexity
   obscuring them was picked apart. This came first, and led to the
   final piece.
9. The **version-integrity realization**: what atom actually is, a
   version-integrity system in its own right, completed by the
   charter, the signed-claim identity model, and the append-only
   record log (entry 8). This is the current design of record.

(Calendar anchors, where they matter, live in the public record: the
blog's git history and the repositories themselves. The work happened
part-time, with real gaps, and the sequence above is its honest shape.)

### 33. What was eka, and why was it retired?

The proof of concept: atoms in pure Rust via gitoxide, isolated refs, a
URI format, the module system, and the SAT resolution engine. It ended
because it had proven what it needed to prove and its code was too
messy to be the foundation, a lesson taken directly from watching Nix
ship a research prototype into twenty years of production burden.
Retiring it opened the licensing question alongside the rebuild: _if I
go to all the trouble of building a better Nix, can I actually protect
it?_

### 34. Where do the names come from?

In true FAQ tradition, everyone asks.

- **Atom** began as nothing deeper than a name that wasn't "flake":
  something physically smaller than even a snowflake, with the right
  connotation. It ended up more descriptive than originally imagined.
  By now the name is technically apt, since the atom really is the
  system's smallest indivisible unit of publishable intent (entry 1).
- **Eos** is the Greek goddess of dawn, answering Nyx, goddess of
  night, whose name Nix echoes. Night to dawn, and the alliteration
  doesn't hurt.
- **Ion** keeps the chemistry going: ions are what give atoms of the
  same element a distinct character of their own, a fitting connotation
  for the tool that turns published atoms into _your_ environments and
  systems.
- **HTC** is the least poetic and the most literal. Hermetic
  Transactional Composition is what you get by decomplecting what Nix
  was doing with its store-based model and naming the abstract
  principle that made it work (entries 9 and 13).

---

## Part VII — Governance and licensing

### 35. Why a new license?

Mostly commercialization, with dilution of community values
contributing. A career in open source made the capture patterns
legible: under every existing license, a project like this could be
taken and enclosed by actors who return nothing, and with AI entering
serious industrial use, existing licenses would not survive the coming
environment. Short of going proprietary, which was refused (this is a
FOSS project in conviction), no existing answer worked. The legal
insight behind the answer was earned the hard way, firsthand: invert
contract law the way copyleft inverted copyright law, to protect
rather than enclose the commons.

### 36. What is copyback?

"If you profit from the commons, you owe value back: code, compute, or
cash." Below a scale threshold, the framework is classic copyleft with
no copyback, no registration, and no fees. Above it, a proportional
return, dischargeable in kind, enforced through three legal layers
(copyright conditions, contract, patents) with cure and reinstatement.
Above the threshold it is deliberately _not_ OSD-open-source, and says
so plainly rather than stretch the word. The full treatment is the Open
Commons FAQ and license; this entry is a pointer, not the authority.

---

## Part VIII — Status and contributing

### 37. What actually runs today?

The honest per-layer map, the README's own framing verified against the
workspaces:

- **`atom/`** — the most real code: `atom-id` (Coz digests, charter
  construction and verification, name normalization), the `atom-git`
  reference backend (ingest, charter chain, snapshot storage),
  `atom-uri`, and the conformance harness. Substantially real, not yet
  fully conformance-tested against its own spec.
- **`htc/`** — a skeleton workspace. The substance at L2 is currently
  the architecture, the formal models, and the empirical studies
  (entry 29); the code is intentionally last.
- **`eos/`** — a scaffold that predates the atom-DAG re-scope, treated
  as throwaway; the durable Eos assets are the proven scheduling theory
  and the simulator. The evaluator-era `eos-snix` bridge is slated for
  removal; do not build on it (the kept Snix components are entry 19's
  story).
- **`ion/`** — the lock (v2, with its violation corpus), manifest, and
  resolution crates are real; the CLI has not yet been extracted from
  the proof-of-concept.
- **`alurl/`** — shared URL-aliasing support (the aliased-URL
  resolution spec's implementation).
- **`tools/`** — the evidence tooling behind entry 29: the scheduling
  simulator and corpus extractors, the real-world trace corpus, and the
  interface-proxy study.

The design deliberately leads the code. That's the eka lesson
(entry 33) applied: specs and proofs first, so the implementation is
poured into a settled mold instead of shipping a research prototype
into twenty years of production burden.

### 38. How do I explore or contribute?

The development environment is pinned with Nix: `nix-shell` (or
`direnv allow`) gives you the toolchain, `just test` runs every
workspace's unit and property tests, `just fuzz` runs the bolero fuzz
harnesses, and `treefmt` formats everything. Reading order for the
design: `docs/adr/` for the why, `docs/architecture/` for the shape,
`docs/specs/` for the letter, `docs/models/` for the proofs. The
website under `www/` renders the explanation-level material. Commits
follow Conventional Commits.

Any contribution is welcome, but there's a natural order to what helps
most right now: adversarial spec reading first (read a spec hostilely
and report what breaks or contradicts), documentation and site clarity
next, then extending the conformance battery and test coverage toward
the specs, with new code deliberately last. That's not a door closed on
coding. Fleshing out the specs and the conformance battery first is
exactly what makes code contribution easier and safer afterward.
