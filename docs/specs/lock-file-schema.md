# SPEC: Lock File Schema

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  Version 2 — wholesale supersession of the v1 (PoC-era) schema, per
  ADR-0006 and the 2026-07-08 lock redesign. The v1 spec's evaluator-
  shaped remnants (its `[compose]` tombstone, `[[deps]]` type dispatch,
  semver mandate, and owner back-pointers) are replaced, not amended.
-->

## Domain

**Problem Domain:** The lock file is the serialized record of an atom's
**resolution**: the recorded choice function over a discovery snapshot
that makes executable-intent elaboration pure in `(intent, fact-set)`
([Composition Model](../models/composition-model.md) §5–§6). Ion (L4)
writes it; Eos (L3) consumes it to fetch, verify, and form execution
requests. **Resolution of an atom is reconciliation over its
dependencies' locked worlds, plus the atom's own constraints, plus its
declared overrides** — the lock records the ground result and nothing
else.

The lock is deliberately small. Four concepts: set anchors, ground
dependency pins with a requires graph, promoted fetch pins, and a schema
version. Everything else that once seemed lock-shaped lives elsewhere by
decision: constraints, params, overrides, and toolchain-role defaults in
the manifest (which ships inside the atom snapshot and therefore inside
`action_id`); interface manifests and build records on the atom metadata
chain; adopted ecosystem lockfiles inside the atom's sources.

**Model Reference:**

- [Composition Model](../models/composition-model.md) — §5 (two strata of
  intent; the lock/certificate symmetry), §6 (fact-set; snapshot pinning)
- [Execution Model](../models/execution-model.md) — §2.4 (`action_id`;
  identity discipline), §3.3 (promotion; adopted lockfiles), §8 (P5–P7)
- [ADR-0006](../adr/0006-execution-as-the-primitive.md) — §3 (no
  evaluator: no composer, ever)
- [ion-eos-contract.md](ion-eos-contract.md) — handoff boundary

**Criticality Tier:** High — the lock is a trust boundary. Every value
is either a cryptographic commitment or an explicitly non-authoritative
transport hint.

## Formal requirements

These bind the schema as a whole; every section constraint below serves
one of them.

**[lock-sufficiency]**: The lock MUST pin every worldly discovery needed
to form execution requests for the atom: the transitive dependency
closure by content identity, and every fetch payload not adopted from an
ecosystem lockfile in the atom's sources. Build-time resolution or
discovery of any kind MUST NOT be required by a consumer holding the
atom snapshot and its lock.

**[lock-recomputability]**: Resolution MUST be deterministic: the same
manifest and the same discovery snapshot MUST re-derive a byte-identical
lock. Serialization MUST be canonical (see [lock-canonical-form]).

**[lock-groundness]**: Every lock value MUST be ground: names bound to
content identities and exact version strings. Version constraints,
ranges, override declarations, and any other unresolved intent MUST NOT
appear in the lock; they are manifest-side. The only equality relation a
lock consumer needs is syntactic.

**[lock-closure-completeness]**: The `[deps]` section MUST contain the
full transitive closure of the atom's dependencies as resolved. Which
entries are direct is the manifest's knowledge and MUST NOT be
duplicated in the lock (no root marker, no `[self]`).

**[lock-action-totality]**: The lock, together with the atom snapshot
containing it, MUST suffice to compute `action_id` for the atom and for
every entry in its closure (Execution Model §2.4) with no further input.

## Top level

**[lock-schema-version]**: The lock MUST contain a top-level integer
field `schema`. This document specifies `schema = 2`. Consumers MUST
refuse locks whose schema version they do not implement.

**[lock-tool-owned]**: The lock is tool-authored. Tools SHOULD emit a
leading comment identifying the generator; humans review lock diffs and
MUST NOT be required to hand-edit them for any supported workflow.

## `[sets]` — set anchors

**[lock-set-anchor]**: Each key under `[sets]` is a local set alias; its
`anchor` field MUST be the set repository's genesis commit object id,
prefixed with its hash algorithm (`"sha1:…"` for git's current default;
a future `"sha256:…"` MUST be representable). The anchor IS the set's
identity.

**[lock-set-mirrors]**: The `mirrors` field MUST be an array of
transport hints (URLs or the `"::"` local sentinel). Mirrors are NEVER
identity and NEVER trusted: content fetched from any mirror MUST verify
against the content identities in this lock. Consumers MAY consult
mirrors from any other source.

**[lock-set-referenced]**: Every set alias appearing in a `[deps]` key
path MUST have an entry under `[sets]`, and every `[sets]` entry MUST be
referenced by at least one dep entry.

## `[deps.<set>.<label>]` — the ground pins

Dependency entries are nested tables keyed by set alias, then atom
label. This two-level keying is the `(set, label)` name anchor; there is
no per-entry type dispatch and no `set` field.

**[lock-dep-version]**: The `version` field MUST be the exact, non-empty
UTF-8 version string of the resolved publish, recorded verbatim as
published (raw scheme — no normalization, no semver requirement; scheme
interpretation is a manifest/resolution concern, never a lock concern).

**[lock-dep-publish]**: The `publish` field MUST be the content digest
of the resolved publish transaction (the bare publish czd), prefixed
with its hash algorithm. This is the entry's identity; everything else
is annotation.

**[lock-dep-requires]**: The `requires` field MUST be an array listing
the entry's direct dependencies as dotted key paths (`"<set>.<label>"`
for dep entries, `"fetch.<name>"` for fetch entries), sorted bytewise.
Requires edges are the closure's graph structure and the reference-
counting basis for garbage collection. Provider-side owner
back-pointers MUST NOT exist.

**[lock-dep-ordering]**: Dep entries MUST be serialized in bytewise
lexicographic order of set alias, then label. (Fetch entries sort
independently within `[fetch]`; no cross-section ordering relation
exists or is needed.)

## `[fetch.<name>]` — promoted fetch pins

Fetch entries record **promoted** discoveries (record-mode trial →
reviewed, signed intent; Execution Model §3.3). Origin coincides with
section: everything under `[fetch]` is promotion-authored and NOT
regenerable by resolution.

**[lock-fetch-digest]**: Each fetch entry MUST contain a `digest` field:
the algorithm-prefixed content digest of the fetched payload. The digest
is the identity; the `url` field is a transport hint and MUST NOT be
treated as authoritative.

**[lock-fetch-liveness]**: A fetch entry is live while at least one
`requires` edge references it. Automated sanitization MUST NOT remove a
live fetch entry, and MUST NOT remove a dead one except under an
explicit user-invoked purge — promoted knowledge is not regenerable and
its removal is a user decision.

**[lock-fetch-adopted-absent]**: Dependencies pinned by an **adopted**
ecosystem lockfile (e.g. `Cargo.lock` shipped inside the atom's sources)
MUST NOT be re-declared as fetch entries. The adopted lockfile is the
pin payload, already inside the atom snapshot and therefore already
inside `action_id`.

## Canonical form and write discipline

**[lock-canonical-form]**: Serialization MUST be canonical: fixed
section order (`schema`, `[sets]`, `[deps]`, `[fetch]`); keys sorted
bytewise at every nesting level; a single canonical TOML formatting.
Two locks with equal content MUST be byte-identical.

**[lock-atomic-write]**: Every lock mutation MUST be a whole-file
atomic write. Write phases (resolution; later promotions) land in their
own sections; independent promotions MUST be reorder-invariant —
running two independent promotions in either order MUST yield the same
bytes (Execution Model §8, P6).

## Deliberate absences

Each absence is a decision with a source; adding any of these is a
regression, not an extension.

**[lock-no-compose]**: The lock MUST NOT contain a `[compose]` section,
composer selection, or evaluator arguments of any kind (ADR-0006 §3).

**[lock-no-params]**: Action parameters MUST NOT appear in the lock.
They are declared in the manifest, which ships inside the atom snapshot
and is therefore already inside `action_id` (they are declared, not
resolved).

**[lock-no-toolchain-section]**: The lock MUST NOT contain a toolchain
section or toolchain entry type. Toolchain-role defaults are ordinary
dependencies declared in the manifest; the _effective_ toolchain after
role-keyed override propagation enters `action_id` as the toolchain
composition root and is never lock state.

**[lock-no-interfaces]**: Interface manifests MUST NOT appear in the
lock. They are facts, not choices; they live on the atom metadata chain
beside build records.

**[lock-no-override-state]**: Override declarations (target-keyed or
role-keyed, forced or bounded) MUST NOT appear in the lock. The lock
records their ground _results_; the pin-diff is the audit trail.

**[lock-no-foreign-metadata]**: The lock MUST NOT contain registry
metadata, timestamps, resolution history, environment markers, or
conditional entries of any kind.

## Whole-lock hashing is not an identity

**[lock-no-plan-digest]**: Consumers MUST NOT use a digest of the whole
lock file as a cache key, plan identity, or build identity. Identity is
per-action (`action_id`, Execution Model §2.4): an edit anywhere in a
lock MUST NOT shift the identity of actions whose own closure slices
are unchanged. (This retires the v1-era `plan_digest`.)

## Example

```toml
# ion.lock — generated by ion; do not edit
schema = 2

[sets.core]
anchor  = "sha1:9f2c81d4…"
mirrors = ["::", "https://mirror.example.org/core"]

[deps.core.gcc]
version  = "13.3.0"
publish  = "sha256:57de9a02…"
requires = []

[deps.core.libfoo]
version  = "2.1.4"
publish  = "sha256:7be13c55…"
requires = ["core.openssl", "core.zlib-ng", "fetch.libfoo-vendor-models"]

[deps.core.openssl]
version  = "3.0.16"
publish  = "sha256:c2104e88…"
requires = []

[deps.core.zlib-ng]
version  = "2.2.1"
publish  = "sha256:e9973b19…"
requires = []

[fetch.libfoo-vendor-models]
url    = "https://files.example.com/models-4.2.tar.zst"
digest = "blake3:aa31f6c0…"
```

## Open items binding this spec

- The publish czd's digest algorithm is whatever coz produces — the
  concrete prefix string MUST be pinned from `atom-core` before
  implementation (placeholder above: `sha256`).
- The manifest schema (constraints, overrides, toolchain roles,
  ecosystem declaration, params) is a separate specification; this spec
  constrains only what crosses into the lock.
