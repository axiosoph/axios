# SPEC: Fact Exchange

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  New 2026-07-30, landing [ADR-0009](../adr/0009-atom-composition-plane.md)
  Decision 7 (RATIFIED by nrd, design-of-record §6): the channel by which
  the cloud's facts propagate peer-to-peer. Kept minimal:
  enough to host the claims below coherently, not a full protocol spec.
-->

## Domain

**Problem Domain:** The cloud ([Atom Store Format](atom-store-format.md),
[Atom Model](../models/atom-model.md) §11) is write-anywhere: any anchored
signer holding the right role may append a fact, with no push access to
the subject's own project. This spec is the pull protocol a consumer uses
to learn which facts exist and fetch them, and the obligation on
ingestion that keeps a populated store from silently missing the
metadata a composed operation needs.

**Model Reference:** [Composition Model](../models/composition-model.md)
§6 (the fact-set); [ADR-0009](../adr/0009-atom-composition-plane.md)
Decision 7 (the fact channel) and Decision 10 (interface manifests as
facts).

**Criticality Tier:** Medium — a gap here degrades to staleness, not
tamper: nothing served through this channel is trusted without the
verification path ([Atom Store Format](atom-store-format.md)) resolving
it independently.

## Constraints

### The channel

```
facts.roots(subjects: [czd]) → [(subject, root)]   -- over the ADVERTISED set
facts.diff(subject, my_root) → [fact_czd]
facts.get(czds)              → [bytes]
```

Bounded by a set the consumer already holds — its own closure — never by
a peer's total fact count; merge is union, so no coordination, no
ordering, and no conflict resolution is needed.

#### [fact-exchange-subject-scoped]

Fact exchange is `roots(subjects)` / `diff(subject, root)` / `get(czds)`,
bounded by the consumer's own closure. An unscoped call has no method to
invoke.

```claim
kind: constraint
evaluator: none
```

Facts are **advertised** under a gating policy, distinct from being
**held**: a source may hold more than it advertises, and the channel
answers strictly over what a queried source chose to advertise.

#### [fact-exchange-advertised-only]

The exchange, bounded the same way as `fact-exchange-subject-scoped`,
answers over the **advertised** set, not the held set.

```claim
kind: constraint
evaluator: none
```

### Ingestion

A store populated with content but empty of facts fails to compose while
appearing complete: interface resolution cannot run without the
manifests it needs, the same gap that leaves a Nix store holding
artifacts while the metadata to evaluate them lives elsewhere.

#### [ingest-closure-complete]

Ingestion, over the same `fact-exchange-subject-scoped` channel, is
closure-complete over records, facts, **and** content. A store full of
artifacts but empty of facts MUST NOT be treated as a populated store.

```claim
kind: constraint
evaluator: none
```
