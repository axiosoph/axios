# SPEC: Atom Store Format

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  New 2026-07-30, landing [ADR-0009](../adr/0009-atom-composition-plane.md)
  Decisions 5 and 6: the store on-disk format is a public contract
  (design-of-record §9a), independent of ion or eos — Story 4's consumer
  holds neither and reads a store directly. Kept minimal: enough to host
  the claims below coherently, not a full store specification.
-->

## Domain

**Problem Domain:** The atom plane's own store — the keyspace holding
charters, claims, publishes, amendments, and facts — is a different store
from the artifact CAS ([Storage Model](../models/storage-model.md) §5
disclaims it explicitly: "Atoms... have their own storage primitive").
This spec is that primitive's on-disk contract: what a publish's
committed object contains, how a consumer verifies it against the spine,
and how the store itself is keyed. A consumer with neither ion nor eos —
holding only a populated store — MUST be able to do everything this spec
covers with no other tooling.

**Model Reference:** [Atom Model](../models/atom-model.md) §5 (the
signature-anchoring law), §11 (the spine/cloud split); [ADR-0009](../adr/0009-atom-composition-plane.md)
Decisions 5 (the published object) and 6 (the store).

**Criticality Tier:** High — this is the trust boundary a zero-context
verifier crosses to accept a publish at all.

## Constraints

### The published object

A publish's committed object is reached in two steps, neither of which
carries any security weight of its own: git is transport, never
authority ([ADR-0007's Alt 1](../adr/0007-atom-version-integrity-system.md)).
The publish ref targets the tip of the publish's own materialized view
chain — a chain of checkpoint commits over the append-only record,
starting at the publish and extended by one commit per fact that later
lands about it. A consumer MUST read and verify the record carried by
that chain before content is reachable at all: the ref names the
record, never a bare content pointer, so there is no path that
bypasses reading and verifying the record first.

The atom's content lives in a second, parentless commit whose `tree`
**is** the source tree — the same OID `position:path` already resolves
to — with fixed, inert fields (blank author/committer, zero timestamp,
empty message, no signature) and no other content. It hangs off the
FIRST commit of the publish chain as a second parent, contributing
exactly itself, its tree, and its blobs; nothing else is reachable
from it. A consumer reaches it only by walking that second-parent edge
from the chain's first commit — never from the publish ref directly,
and never from a later commit the chain has since grown to.

#### [publish-ref-transport-bundle]

The publish ref targets the tip of the publish's materialized view
chain, never a content commit directly. The chain's first commit
carries a second parent: a parentless commit whose `tree` is the
source tree (same OID) and whose `message` is empty. That second
parent is the only path to the atom's content. Both commits are
transport, never authority.

```claim
kind: constraint
evaluator: none
```

A record known only from its own carrier is self-declaration: under the
[record realization](../models/surety-of-source.md#4-total-and-the-genesis-seeds),
a free-floating record classifies as a seed, and `Total` would be
trivially — and meaninglessly — satisfied. Verification therefore MUST
walk past the carrier to the spine itself.

#### [publish-inclusion-required]

Verification MUST prove the publish record's czd is included in the
[spine-chain-complete](../models/atom-model.md#spine-chain-complete)
spine, against a root the consumer pinned or TOFU'd. Without this proof a
record is self-certifying and, under the
[record realization](../models/surety-of-source.md#4-total-and-the-genesis-seeds)'s
seed rule, classifies as a **seed**.

```claim
kind: constraint
evaluator: none
depends: [spine-chain-complete]
```

Verification against the spine proceeds without ever querying what else
exists — it walks a closure the consumer already holds or is given, never
"is there something newer."

#### [verification-no-discover]

No operation on the verification path — signature check, czd
recomputation, inclusion proof, content-digest recomputation, governance
fold — takes a discovery capability
([discover-one-question](../models/atom-model.md#discover-one-question)).
Verification is offline-correct over a store already holding the
publish's closure.

```claim
kind: constraint
evaluator: none
depends: [discover-one-question]
```

### The store

`store : czd → object`. Every object — charter, claim, publish, fact,
amendment — is a signed envelope whose identity is its own czd: one
keyspace, one lookup, no reduction to a uniform hash space and no
double-keying by subject, since a fact's own czd already covers a payload
naming its subject
([ADR-0009](../adr/0009-atom-composition-plane.md#6-the-store-is-a-flat-content-addressed-keyspace-with-every-index-derived-acp-store)).

#### [store-keyed-by-czd]

The store is a flat map `czd → object`. No algorithm-reducing key, no
compound key.

```claim
kind: constraint
evaluator: none
```

#### [fact-keyed-by-own-czd]

A fact is keyed by its own czd, the same
[store-keyed-by-czd](#store-keyed-by-czd) map every other object lives
in; the subject index is derived, local, and never fetched or trusted.

```claim
kind: constraint
evaluator: none
because: [store-keyed-by-czd]
```
