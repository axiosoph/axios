+++
title = "Atom's version integrity: one append-only record log"
description = "How Atom's charter, claim, publish, and fact records work together as one signed, append-only log per project, built on a Merkle construction whose core soundness argument is machine-checked"
quadrant = "Explanation"
tags = ["general"]
audience = "Architects and engineers who want to understand why Atom's version-integrity model is sound, without reading the full ADR"
+++

> **Status.** This page describes the design specified in
> [ADR-0007](../architecture/0007-atom-version-integrity-system.html),
> currently under review. See that document for the full decision
> record, forces, and open items.

Atom isn't a packaging convenience layered on top of Git. It's a
**second index over Git's own content-addressed object store**,
parallel to and independent of Git's native version-control index.
Git's own index answers "how did this evolve" — refs and ancestry.
Atom's index answers a different question: "what is this, who says so,
in what succession, how recently." Every Git repository's commit
history was always a partial, accidental instance of that second
question — every commit is, in principle, a claim about content — but
nothing made the claim *itself* first-class, signed, and independently
verifiable at the granularity of one specific act of publication.
That's what Atom is: the missing index, not a plugin.

## Records: the quantum everything else is built from

Everything in this system — a charter, a claim, a publish, a fact — is
the same underlying thing wearing a different label: one **record**, a
single signed [Coz](https://github.com/Cyphrme/Coz) message, complete
and meaningful entirely on its own. A record is the *quantum* here —
the smallest unit that means anything by itself, needing nothing else
to be checked. (Coz's own README has the precise wire format, if you
want it; nothing on this page depends on detail it doesn't cover.)

The **atom** the whole system is named for is the opposite pole: not
the smallest unit, but the largest self-contained one — a single thing
that can be acted on as a whole regardless of how many smaller parts
compose it underneath. That's true in both the everyday sense and a
more precise one: something is atomic exactly when its own internal
composition stops mattering at the level whatever's above it operates
on. What this page calls a **closure** is that same idea, specifically
at the layer this page covers — content plus the authorization state
that produced it. (The same atom carries further, into what it's
actually built into — a different layer, covered elsewhere.)

## The model

A project has one standing identity — its **anchor** — established once
by a signed **charter** and never reissued; the only way to get a new
one is a genuine fork. A project publishes under human-chosen
**labels** — a package name, roughly ("quill", say) — each established
once by a signed **claim**, also never reissued. Each released version
is established once by a signed **publish**. Charter, claim, and
publish are the only three things in the whole system that are ever
"founding" records — and none of them changes after the fact.
**Everything that happens afterward — an owner rotating keys, a label
changing hands, a version getting yanked or flagged — is a `fact`: a
new signed statement layered on top, never a mutation of what's
already there.** One mechanism, reused at every scope, rather than a
different kind of machinery for anchors, labels, and versions each.

Every charter, claim, publish, and fact a project ever signs lands in
**one continuous, append-only log**, in the order it was actually
signed. A separate, permanent artifact called a **closure** captures
exactly what's needed to build one specific version — its content,
plus a cryptographic snapshot of the log's own authorization state at
the moment it was produced. The closure is
**truly immutable**: once written, it never changes, full stop. Facts
about that version accrue afterward, forever, and are never folded
into it — not because they'd make it go stale, but because a fact and
a closure answer two different questions at two different layers. The
closure settles, permanently, at publish time, what a version *is* and
whether it was genuinely authorized. A fact never touches that
question; it answers a separate one, indefinitely: given that the
closure already, unconditionally, is what it is, should you currently
*trust* it — has it been yanked, flagged, deprecated. One question
closes forever the moment a version is published; the other stays open
for as long as the project lives, and the two are never allowed to
collapse into each other.

## A closure of closures

Atom already has a word for "the complete, self-contained set that
proves something" — a project's charter, claim, and publish, verified
together, are a **record closure**: everything needed to prove a
version was genuinely authorized, nothing more. The files a version
distributes are their own **content closure**, composed from whichever
files and directories a publish declares as part of it. The closure
described above — the one artifact actually published for each version
— *is* the binding of these two, and that binding is literal, not a
detached pointer sitting beside them: the content closure's own
identity — its root, never its bytes — becomes an embedded node inside
the very structure the records make up, provable the same way any part
of a Merkle tree is provable, without ever having to carry the actual
weight of the content along for the ride. Same law, one level up — a
complete, bounded set with nothing missing and nothing extraneous —
applied to the relationship *between* the other two closures, rather
than invented fresh for this one purpose.

## Publishing history that already exists

A publish record doesn't have to be created inside the history it
describes. It names an existing commit and a path within it — the
record sits entirely outside that history, pointing into it, rather
than requiring the commit itself to be rewritten, tagged, or modified
to carry any of this. That's what makes it possible to publish a
project that's been around for years, with history nobody ever touched
for Atom's sake: the signed record is the only new thing, and
everything it points at can predate it by any amount. Identity and
authorization are never fused to a specific commit — they're a
separate, signed layer that references history without living inside
it.

## Why it's sound

Nothing about Git's structure is ever the trust boundary. A record's
signature and its own signed links to prior records are what get
verified — always. Git objects (commits, trees, tags) exist purely to
make storage, retrieval, and garbage collection cheap by reusing
machinery that already works, never to substitute for checking a
signature.

The append-only log's construction — how records fold into a tree, how
inclusion and consistency proofs work — is machine-checked in Lean 4,
with no unproven gaps in its own mathematical soundness argument.
Worth being precise about that claim: it's the *abstract construction*
that's formally verified, not a guarantee that every line of code
implementing it is bug-free — the same narrower kind of trust any
formally verified system carries between its model and its
implementation. [`eml`](https://github.com/Cyphrme/eml), an external,
independent library, is the concrete implementation currently used.
What the math gives, regardless of implementation: an inclusion proof
("this specific record really is in the log") costs logarithmic work,
never a full download, and a consistency proof ("nothing already
published was silently removed or rewritten") comes from the same
structure, not a bolted-on check. A **closure**
cryptographically embeds a snapshot of the log's own authorization
state at publish time — the same snapshot described above — so a
build's authorization can be independently reverified
later, not merely asserted once and trusted forever.

One property is worth calling out specifically, because it's easy to
take for granted: whether a consumer has *missed* something (a fact
that exists but wasn't surfaced to them) is provable with a cheap,
deterministic check against the log itself — no dependency on any kind
of live freshness infrastructure. *How current* a consumer's view is
still depends on that infrastructure when a project wants it; but
*whether anything was silently omitted* from a view a consumer already
has doesn't.

## A shape Git already almost has

This isn't a novel invention that needs the industry to learn to
trust — it's the same shape that's secured exactly this class of
problem at internet scale for over a decade. Certificate Transparency
logs are signed, append-only Merkle logs that publicly account for
every certificate ever issued; Atom's record log is that same pattern,
aimed at package versions instead of certificates.

What makes it land so naturally on Git specifically: a chain of Git
commits is already, in shape, a chain of checkpoints over history —
each commit's own identity depends on its content plus a link back to
whatever came before it, the same recursive structure a signature
log's periodic "here's the current state" checkpoints need. Git
repositories have quietly been doing something Merkle-shaped the whole
time; nobody had to invent that part.

What's genuinely new is the *tree* each checkpoint commits to. A Git
tree object is already just an ordered, content-addressed list of
named entries — which happens to be exactly the shape one node of an
append-only Merkle tree needs. It's technically possible to lay a
Merkle log's internal structure out using Git's own tree objects
directly, entry for entry, instead of Git's ordinary, freely-editable
directory semantics. Whether an implementation does exactly that, or
stores the same information some other way, is purely a storage-layout
choice — notably, never the thing that makes a record's inclusion
trustworthy; that's still the mathematics described above, doing the
actual work, regardless of how the bytes happen to be laid out on
disk. Either way, every object involved is still an ordinary commit,
tree, or blob, nothing exotic — so Git's own tooling, including its
garbage collector, already works on all of it without knowing anything
about Merkle logs at all. The elegant implementation and the
well-tested industry-standard shape turn out to be nearly the same
shape, which is a large part of why none of this needed inventing from
scratch.

## Where this fits

For the full decision record, the forces, and everything left open,
see [ADR-0007](../architecture/0007-atom-version-integrity-system.html).
For related context on Atom's trust model, see
[Supply chain security](supply-chain-security.md). For the record
envelope's exact wire format, see [Coz](https://github.com/Cyphrme/Coz).
For the Merkle library the log is built on, see
[`eml`](https://github.com/Cyphrme/eml).
