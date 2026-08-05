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
  ADR-0006 and the 2026-07-08 lock redesign, incorporating the findings
  of a zero-context adversarial review (F1–F13, recorded on PR #36).
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

The lock is deliberately small. Five concepts: set anchors with
discovery snapshots, ground dependency pins with a requires graph,
promoted fetch pins, and a schema version. Everything else that once
seemed lock-shaped lives elsewhere by decision: constraints, params,
overrides, and toolchain-role defaults in the manifest (which ships
inside the atom snapshot); interface manifests and build records on the
atom metadata chain; adopted ecosystem lockfiles inside the atom's
sources.

**Model Reference:**

- [Composition Model](../models/composition-model.md) — §4
  (reconciliation; single choice per name per scope), §5 (two strata of
  intent; the lock/certificate symmetry), §6 (fact-set; snapshot
  pinning; recorded choice)
- [Execution Model](../models/execution-model.md) — §2.4 (`action_id`;
  identity discipline), §3.3 (promotion; adopted lockfiles), §8 (P5–P7)
- [ADR-0006](../adr/0006-execution-as-the-primitive.md) — §3 (no
  evaluator: no composer, ever)
- [ion-eos-contract.md](ion-eos-contract.md) — handoff boundary
  (predates this spec; see Supersessions)

**Criticality Tier:** High — the lock is a trust boundary. Every value
plays exactly one of three roles: a **cryptographic commitment**
(`anchor`, `charter_head`, `snapshot`, `publish`, `digest`), a
**non-authoritative transport hint** (`mirrors`, `url`), or a
**structural annotation** (`schema`, `version`, `requires`). Annotations
are never the basis of a fetch or trust decision and MUST be consistent
with the commitments ([lock-annotation-consistency]).

## Formal requirements

These bind the schema as a whole; every section constraint below serves
one of them.

#### [lock-sufficiency]

The lock MUST pin every worldly discovery needed
to form execution requests for the atom: the transitive dependency
closure by content identity, and every fetch payload not adopted from an
ecosystem lockfile in the atom's sources. Build-time resolution or
discovery of any kind MUST NOT be required by a consumer holding the
atom snapshot and its lock.

Whole-closure sufficiency is a resolution-time property; no resolver
exists yet -- `ion-resolve/src/lib.rs`'s own docstring: "A full
SAT-based resolver and lock file generator are planned but not yet
implemented."

```claim
kind: constraint
evaluator: none
```

#### [lock-recomputability]

The lock MUST be the output of deterministic
resolution: the same manifest and the same discovery snapshot (as pinned
by [lock-set-snapshot]) MUST re-derive a byte-identical lock.
Serialization MUST be canonical (see [lock-canonical-form]).

This depends on both the canonical encoder and resolution
determinism, neither implemented: `LockFileV2::to_canonical` is an
explicit `unimplemented!()` stub, and no resolver exists.

```claim
kind: constraint
evaluator: none
depends: [lock-set-snapshot, lock-canonical-form]
```

#### [lock-choice-policy]

The choice function is fixed and canonical:
among candidates satisfying the manifest's constraints, resolution MUST
select the highest version under the atom's declared scheme order,
deviating only through declared, manifest-side overrides. Determinism in
[lock-recomputability] is therefore two-place — (manifest, snapshot) —
with the choice policy a constant of this specification (Composition
Model §6: the recorded choice).

The choice function is part of the not-yet-implemented SAT-based
resolver (`ion-resolve/src/lib.rs`).

```claim
kind: constraint
evaluator: none
because: [docs/models/composition-model#6]
```

#### [lock-groundness]

Every lock value MUST be ground: names bound to
content identities and exact version strings. Version constraints,
ranges, override declarations, and any other unresolved intent MUST NOT
appear in the lock; they are manifest-side. The only equality relation a
lock consumer needs is syntactic.

This is a whole-schema meta-invariant; no check beyond the type shape
exists, and the type shape alone does not prevent, e.g., a version
range being stuffed into the version `String` field.

```claim
kind: constraint
evaluator: none
```

#### [lock-closure-completeness]

The `[deps]` section MUST contain the
full transitive closure of the atom's dependencies as resolved. Which
entries are direct is the manifest's knowledge and MUST NOT be
duplicated in the lock (no root marker, no `[self]`).

This requires a manifest-driven resolution pipeline to compute the
closure and know which entries are direct; no such pipeline exists.

```claim
kind: constraint
evaluator: none
```

#### [lock-action-totality]

The lock, together with the manifest in the
same atom snapshot, MUST determine — by pure elaboration, with no
discovery — all three inputs of the atom's `action_id` (Execution Model
§2.4): `atom_czd_closure_root` from the dep pins; `params` from the
manifest's declarations; and the effective `toolchain_composition_root`
from the manifest's toolchain-role declarations as resolved in the lock,
after role-keyed override propagation. For closure members, the lock
MUST let a consumer identify and fetch each member's snapshot, from
which that member's `action_id` is computed the same way — governed by
*this* lock, the top-level one under computation, never by re-reading
the member's own nested lock inside its fetched snapshot:
[lock-closure-completeness] already guarantees this lock carries every
member's fully-resolved entry, so a member's own lock matters only when
that atom is itself the top level. This discharges P7's totality gate
(Execution Model §8): the toolchain pin is an ordinary dep entry; no
dedicated entry type exists or is needed.

The `action_id` computation combining lock, manifest, and
toolchain-override propagation is not implemented anywhere in
atom-core or eos.

```claim
kind: constraint
evaluator: none
depends: [docs/models/execution-model#2.4, lock-closure-completeness]
because: [docs/models/execution-model#8]
```

#### [lock-in-snapshot-locality]

The lock is carried inside the atom
snapshot it serves — it is reviewed, signed intent. An edit to an atom's
lock therefore legitimately changes that atom's _own_ action identity
(its pinned inputs changed). What MUST NOT happen is cross-action
leakage: the identity of any _other_ action — a dependency's build,
another atom's job — MUST NOT be derived from this atom's lock; each
action's identity derives only from its own snapshot's slice (see
[lock-no-plan-digest]). Canonical form exists partly for this reason:
with exactly one legal serialization and no comments, no
semantically-inert lock edit exists to spuriously shift identity.

Cross-action leakage prevention depends on the same unimplemented
`action_id` computation as `[lock-action-totality]`.

```claim
kind: constraint
evaluator: none
because: [lock-no-plan-digest]
```

## Top level

#### [lock-schema-version]

The lock MUST contain a top-level integer
field `version`, in minimal decimal form. This document specifies
`version = 0`, denoting a pre-MVP schema: the shape is unstable and no
compatibility is promised across changes until an MVP fixes it.
Consumers MUST refuse locks whose version they do not implement.

The field exists as a typed `u64` but under the superseded v2
framing (named `schema`, valued `2`; `ion/ion-lock/src/v2.rs`); no
code path rejects an unrecognized version, and no such check or test
exists.

```claim
kind: constraint
evaluator: none
```

#### [lock-tool-owned]

The lock is tool-authored; humans review lock
diffs and never hand-edit them in supported workflows. The canonical
form contains no comments: generator provenance belongs in VCS metadata
and tool output, never in lock bytes.

"No comments in canonical form" is tied to the unimplemented
`to_canonical` encoder; "humans never hand-edit" is a workflow
discipline, not machine-checkable by this repo's tooling.

```claim
kind: constraint
evaluator: none
```

## `[sets]` — set anchors and discovery snapshots

#### [lock-set-anchor]

Each key under `[sets]` is a local set alias; its
`anchor` field MUST be the algorithm-prefixed coz digest of the
atom-set's founding charter transaction: `anchor = czd(charter₀)`
(atom-transactions.md's `[charter-anchor]`). The anchor IS the set's
identity, and this derivation is backend-agnostic: the value is a coz
digest regardless of which VCS backend hosts the set's source. This
supersedes the prior genesis-commit derivation. The founding commit is
not lost — `charter.src` is the chartering-point revision, and because
a revision hash commits to its entire ancestry, `charter.src`
transitively pins the genesis commit (atom-transactions.md's
`[charter-ancestry]`) without itself being the genesis commit or
serving as the set's identity; that role now belongs one layer up, to
`anchor`. A weak source-repository hash — SHA-1 is not
collision-resistant (Storage Model §5) — therefore no longer bears on
the lock's own identity value directly: it bears only on the ancestry
`charter.src` transitively pins, not on `anchor` itself. Continuity
across a hash-migrating
history rewrite is a successor charter's explicit concern
(atom-transactions.md's `[charter-succession]`, `[anchor-hash-agile]`),
never this field's: `anchor` is immutable for the set's lifetime
regardless of what happens to the source repository underneath it.

`golden_digest_fields_are_byte_exact_canonical_form`
(`ion/ion-lock/tests/v2_corpus.rs`) confirms the field decodes and
encodes as a well-formed `AtomDigest`, but not that the value
genuinely equals `czd(charter₀)`; no code computes a lock's anchor
from charter machinery yet.

```claim
kind: constraint
evaluator: none
depends: [charter-anchor]
because: [charter-ancestry]
```

#### [lock-set-charter-head]

Each `[sets]` entry MUST record a
`charter_head` field: the algorithm-prefixed coz digest of the charter
transaction — the founding charter, or a later successor — that this
lock's resolution observed as the **effective charter**
(atom-transactions.md's `[charter-succession-linear]`: the head of the
unique valid succession chain, ordered by chain position, never by the
untrusted `now` field). This is the lock's contribution to
atom-transactions.md's `[chain-monotonicity]`, which explicitly
requires it: a consumer holding this lock MUST refuse to accept a
served succession chain that regresses below the recorded
`charter_head` — reproducing a prefix of previously observed chain
state is a detected rollback, never an alternative history.
`charter_head` MUST equal `anchor` exactly while the set has undergone
no succession (the founding charter is still effective); it diverges
from `anchor` — both remaining valid digests of real charter
transactions — precisely when a successor charter (key rotation or
ownership transfer, atom-transactions.md's `[charter-succession]`) is
the effective charter at resolution time. Unlike `anchor`, which is
immutable for the set's lifetime, `charter_head` MAY advance on
re-resolution as succession proceeds; it MUST NOT regress.

`charter_head_is_distinct_from_anchor` (`ion/ion-lock/src/v2.rs`)
only shows the two fields CAN hold different values; the substantive
non-regression/succession-derivation invariant depends on the
chain-monotonicity machinery, which is Phase 1 unimplemented
(`atom/atom-id/tests/charter/chain_monotonicity.rs`).
`SUPERSEDED 2026-07-30: charter succession (atom-transactions.md [charter-succession-linear]) no longer exists as a distinct concept under genesis-once; this field's non-regression job is renamed spine_head, per [lock-sets-three-fields](#sets-under-the-spinecloud-split-adr-0009) below.`

```claim
kind: constraint
evaluator: none
```

#### [lock-set-snapshot]

Each `[sets]` entry MUST record a `snapshot`
field: the algorithm-prefixed object id of the set repository's tip
commit at discovery time. This pins the discovery snapshot (Composition
Model §6): re-resolving the same manifest against the same set
snapshots MUST re-derive the byte-identical lock, making
[lock-recomputability] auditable rather than aspirational.

Same digest-format-only reasoning as `[lock-set-anchor]`:
re-derivation determinism depends on the not-yet-implemented
resolver.
`SUPERSEDED 2026-07-30: a whole-repository discovery snapshot has no place under the spine/cloud split — the cloud is a history-independent set with no single tip to snapshot, and a fixed cloud root is not a reproducible pin (composition-model.md #the-cloud-a-snapshot-name-for-the-fact-set). Retired outright, not renamed; see [lock-pins-gating-facts](#sets-under-the-spinecloud-split-adr-0009) below.`

```claim
kind: constraint
evaluator: none
```

#### [lock-set-mirrors]

The `mirrors` field MUST be an array of
transport hints: URLs, or the `"::"` sentinel denoting the local
workspace source (no remote). Mirrors are NEVER identity and NEVER
trusted: content fetched from any mirror MUST verify against the
content identities in this lock. Consumers MAY consult mirrors from any
other source.

`SetEntry.mirrors` is a raw `Vec<String>`; no validation of URL shape
or the `"::"` sentinel exists.

```claim
kind: constraint
evaluator: none
```

#### [lock-set-referenced]

Every set alias appearing in a `[deps]` key
path MUST have an entry under `[sets]`, and every `[sets]` entry MUST be
referenced by at least one dep entry.

```claim
kind: constraint
evaluator: test
```

## `[sets]` under the spine/cloud split (ADR-0009)

_Added 2026-07-30._ [ADR-0009](../adr/0009-atom-composition-plane.md)
retires two premises `[lock-set-charter-head]` and `[lock-set-snapshot]`
above were written against. **`charter_head` is superseded**:
`atom-transactions.md`'s `[charter-succession-linear]` — the "effective
charter" the field tracked — no longer exists as a distinct concept once
genesis is written once and owner rotation is an ordinary spine
amendment; the field's job (refuse a served chain that regresses) is now
tracked as the spine's own amendment-chain head, renamed `spine_head`
below. **`snapshot` is retired outright**, not renamed: it pinned a
discovery-time repository tip, which was this design's only notion of
"the world as of resolution." `[lock-pins-gating-facts]` below replaces
it with something narrower and reproducible — the specific fact czds
that gated the decision, never a whole-store root, which
[cloud-history-independent](../models/composition-model.md#the-cloud-a-snapshot-name-for-the-fact-set)
already rules out as a stable pin: a cloud root is `(subject, store,
moment)`-relative, so pinning one would make re-resolution
irreproducible against a store that has since received more facts for
the same subject.

#### [lock-sets-three-fields]

A `[sets]` entry carries `anchor`, `spine_head`, `mirrors`. Nothing else.

```claim
kind: constraint
evaluator: none
```

#### [lock-pins-gating-facts]

The lock pins the czds of facts that gated a resolution decision. It
MUST NOT pin a cloud root — a root is `(subject, store, moment)`-relative
and irreproducible.

```claim
kind: constraint
evaluator: none
```

The manifest side of this same reconciliation: test and check parameters
already live outside the lock (`[lock-no-params]` above), but nothing yet
says they live in a section of their own, separate from build params —
which is what makes editing a test unable to invalidate a build cache
entry by construction rather than by scheduler discipline (Execution
Model §5.2's obligation on the manifest/lock pair).

#### [manifest-test-params-separate]

Test and check parameters live in a manifest section feeding test
requests only — what makes no-rebuild-on-test-edit structural rather
than a scheduler courtesy.

```claim
kind: constraint
evaluator: none
```

## `[deps.<set>.<label>]` — the ground pins

Dependency entries are nested tables keyed by set alias, then atom
label. This two-level keying is the `(set, label)` name anchor; there is
no per-entry type dispatch and no `set` field.

#### [lock-single-version]

A lock MUST bind at most one version per
`(set, label)` — the nested keying makes this structural. Within one
atom's closure, resolution reconciles to a single shared choice per
name (Composition Model §4); diamond requirements that cannot reconcile
are a resolution failure at this layer. Divergent-version coexistence
is scope territory — environment certificates and co-installation —
never package-lock state.

The nested `(set, label)` keying structurally prevents two versions
under one key, but the substantive claim (diamond requirements
reconcile to a single shared choice) is SAT-resolution logic, not
implemented (`ion-resolve/src/lib.rs`).

```claim
kind: constraint
evaluator: none
because: [docs/models/composition-model#4]
```

#### [lock-dep-version]

The `version` field MUST be the exact, non-empty
version string of the resolved publish, recorded byte-verbatim as
published (raw scheme — no normalization of any kind; scheme
interpretation is a manifest/resolution concern, never a lock concern).

`DepEntry.version` is a plain `String`; non-emptiness and
byte-verbatim-as-published are untested and unenforced
(`ion/ion-lock/src/v2.rs`).

```claim
kind: constraint
evaluator: none
```

#### [lock-dep-publish]

The `publish` field MUST be the content digest
of the resolved publish transaction (the bare publish czd), prefixed
with its hash algorithm. This is the entry's identity.

`golden_digest_fields_are_byte_exact_canonical_form`
(`ion/ion-lock/tests/v2_corpus.rs`) verifies `AtomDigest` format
round-trip only, not that the value is genuinely the resolved publish
transaction's digest.

```claim
kind: constraint
evaluator: none
```

#### [lock-annotation-consistency]

`version` and the `(set, label)` key
path MUST equal the values derivable from the entry's `publish`
transaction. A mismatch is a hard validation error: annotations exist
for humans and indexing, never as independent authority.

No code cross-references `version`/`(set, label)` against the
publish transaction's own payload; that would require decoding the
signed publish `CozMessage` during lock validation, not implemented.

```claim
kind: constraint
evaluator: none
```

#### [lock-dep-requires]

The `requires` field MUST be an array listing
the entry's direct dependencies as dotted key paths (`"<set>.<label>"`
for dep entries, `"fetch.<name>"` for fetch entries), sorted bytewise.
Requires edges are the closure's graph structure. Provider-side owner
back-pointers MUST NOT exist.

Resolvability and acyclicity of the requires graph are separately
verified (`[lock-requires-resolvable]`, `[lock-requires-acyclic]`),
but bytewise-sortedness of the array is a canonical-serialization
concern tied to the unimplemented `to_canonical` encoder.

```claim
kind: constraint
evaluator: none
```

#### [lock-requires-resolvable]

Every `requires` edge MUST name an entry
that exists in this lock. Dangling edges are a hard validation error.

```claim
kind: constraint
evaluator: test
```

#### [lock-requires-acyclic]

The requires graph MUST be acyclic — it is
the skeleton of the action DAG, and a cyclic build dependency is
unrealizable.

```claim
kind: constraint
evaluator: test
```

#### [lock-dep-liveness]

Dep-entry liveness is reachability from the
manifest's direct dependencies — the graph's roots, which live
manifest-side per [lock-closure-completeness]. The lock alone is
deliberately not a self-contained GC domain: sanitization MUST take its
roots from the manifest.

No manifest-linkage or sanitization/GC pass exists yet; this depends
on the same unimplemented resolution pipeline as
`[lock-closure-completeness]`.

```claim
kind: constraint
evaluator: none
depends: [lock-closure-completeness]
```

#### [lock-dep-ordering]

Dep entries MUST be serialized in bytewise
lexicographic order of set alias, then label. (Fetch entries sort
independently within `[fetch]`; no cross-section ordering relation
exists or is needed.)

Serialization ordering is part of the canonical encoder, an explicit
`unimplemented!()` stub (`LockFileV2::to_canonical`,
`ion/ion-lock/src/v2.rs`).

```claim
kind: constraint
evaluator: none
```

## `[fetch.<name>]` — promoted fetch pins

Fetch entries record **promoted** discoveries (record-mode trial →
reviewed, signed intent; Execution Model §3.3). Origin coincides with
section: everything under `[fetch]` is promotion-authored and NOT
regenerable by resolution.

#### [lock-fetch-digest]

Each fetch entry MUST contain a `digest` field:
the algorithm-prefixed content digest of the fetched payload. The digest
is the identity; the `url` field is a transport hint and MUST NOT be
treated as authoritative.

`golden_digest_fields_are_byte_exact_canonical_form`
(`ion/ion-lock/tests/v2_corpus.rs`) verifies `AtomDigest` format
round-trip only, not that `url` is never treated as authoritative by
any consumer (no fetch/verify path exists yet).

```claim
kind: constraint
evaluator: none
```

#### [lock-fetch-naming]

Fetch names are lock-local labels with no
cross-lock meaning. The promoting tool MUST derive names
deterministically from the discovery context, and a promotion whose
name collides with an existing entry carrying a different digest MUST
fail loudly for user resolution. (TOML itself rejects duplicate keys;
this constraint binds the promoting tool, not the parser.)

No promotion/promoting-tool implementation exists yet (the
record-mode fetch proxy is unimplemented; see ion-manifest.md's P4
flag).

```claim
kind: constraint
evaluator: none
```

#### [lock-fetch-liveness]

A fetch entry is live while at least one
`requires` edge references it. Automated sanitization MUST NOT remove a
live fetch entry, and MUST NOT remove a dead one except under an
explicit user-invoked purge — promoted knowledge is not regenerable and
its removal is a user decision.

No sanitization/purge implementation exists; ion-lock is currently
parse/validate-only.

```claim
kind: constraint
evaluator: none
```

#### [lock-fetch-adopted-absent]

Dependencies pinned by an **adopted**
ecosystem lockfile (e.g. `Cargo.lock` shipped inside the atom's sources)
MUST NOT be re-declared as fetch entries. The adopted lockfile is the
pin payload, already inside the atom snapshot.

Cross-referencing adopted ecosystem lockfiles against fetch entries
requires atom-sourcing pipeline integration, not implemented
anywhere.

```claim
kind: constraint
evaluator: none
```

## Canonical form and write discipline

#### [lock-canonical-form]

Serialization MUST be canonical, defined
concretely as: UTF-8, LF newlines, exactly one terminating newline; no
comments; fixed section order (`schema`, `[sets]`, `[deps]`, `[fetch]`);
keys sorted bytewise at every nesting level; exactly one blank line
between tables and none elsewhere; bare keys wherever TOML permits,
otherwise basic (double-quoted) keys; all string values as basic
strings with TOML's minimal escaping; exactly one space on each side of
`=`, no alignment padding; arrays inline with elements separated by
`", "`; integers in minimal decimal form; version strings byte-verbatim
as published (no Unicode normalization). Two locks with equal content
MUST be byte-identical.

`LockFileV2::to_canonical` is an explicit `unimplemented!()` stub by
design (`ion/ion-lock/src/v2.rs`); `canonical_form_is_byte_exact` is
a red test (`#[ignore]`, "canonical encoder is unimplemented by
design").

```claim
kind: constraint
evaluator: none
```

#### [lock-atomic-write]

Every lock mutation MUST be a whole-file
atomic write: after any crash or interruption, either the old lock is
intact or the new lock has completely replaced it, with no intermediate
state observable. Implementations SHOULD write to a temporary file and
rename. Write phases (resolution; later promotions) land in their own
sections; independent promotions MUST be reorder-invariant — running
two independent promotions in either order MUST yield the same bytes
(Execution Model §8, P6).

No lock-writing code exists at all; ion-lock is currently a
parse/validate-only crate (no `LockFileV2` writer, temp-then-rename,
or promotion-merge path).

```claim
kind: constraint
evaluator: none
because: [docs/models/execution-model#8]
```

## Deliberate absences

Each absence is a decision with a source; adding any of these is a
regression, not an extension.

#### [lock-no-compose]

The lock MUST NOT contain a `[compose]` section,
composer selection, or evaluator arguments of any kind (ADR-0006 §3).

```claim
kind: constraint
evaluator: test
```

#### [lock-no-params]

Action parameters MUST NOT appear in the lock.
They are declared in the manifest, which ships inside the atom snapshot
(they are declared, not resolved).

```claim
kind: constraint
evaluator: test
```

#### [lock-no-toolchain-section]

The lock MUST NOT contain a toolchain
section or toolchain entry type. Toolchain-role defaults are ordinary
dependencies declared in the manifest and pinned as ordinary dep
entries; the _effective_ toolchain after role-keyed override
propagation is computed per [lock-action-totality], never stored.

```claim
kind: constraint
evaluator: test
because: [lock-action-totality]
```

#### [lock-no-interfaces]

Interface manifests MUST NOT appear in the
lock. They are facts, not choices; they live on the atom metadata chain
beside build records.

```claim
kind: constraint
evaluator: test
```

#### [lock-no-override-state]

Override declarations (target-keyed or
role-keyed, forced or bounded) MUST NOT appear in the lock. The lock
records their ground _results_; the pin-diff is the audit trail.

```claim
kind: constraint
evaluator: test
```

#### [lock-no-foreign-metadata]

The lock MUST NOT contain registry
metadata, timestamps, resolution history, environment markers, or
conditional entries of any kind. (The `snapshot` field is a content
identity, not a timestamp.)

```claim
kind: constraint
evaluator: test
```

## Whole-lock hashing is not an identity

#### [lock-no-plan-digest]

Consumers MUST NOT use a digest of the whole
lock file as a cache key, plan identity, job identity, or build
identity for any action other than the owning atom's own (whose
snapshot legitimately contains the lock, per
[lock-in-snapshot-locality]). Identity is per-action (`action_id`,
Execution Model §2.4): an edit in one atom's lock MUST NOT shift the
identity of dependency actions or other atoms' actions. (This retires
the v1-era `plan_digest` and the job-identity scheme built on it.)

```claim
kind: constraint
evaluator: test
because: [docs/models/execution-model#2.4]
```

## Example

_The `[sets.core]` table below shows the [lock-sets-three-fields](#sets-under-the-spinecloud-split-adr-0009)
shape (`anchor`, `spine_head`, `mirrors`); the pre-ADR-0009 `charter_head`
and `snapshot` fields shown superseded above do not appear._

```toml
schema = 2

[sets.core]
anchor = "sha256:3fa1c9e0…"
spine_head = "sha256:3fa1c9e0…"
mirrors = ["::", "https://mirror.example.org/core"]

[deps.core.gcc]
publish = "sha256:57de9a02…"
requires = []
version = "13.3.0"

[deps.core.libfoo]
publish = "sha256:7be13c55…"
requires = ["core.openssl", "core.zlib-ng", "fetch.libfoo-vendor-models"]
version = "2.1.4"

[deps.core.openssl]
publish = "sha256:c2104e88…"
requires = []
version = "3.0.16"

[deps.core.zlib-ng]
publish = "sha256:e9973b19…"
requires = []
version = "2.2.1"

[fetch.libfoo-vendor-models]
digest = "blake3:aa31f6c0…"
url = "https://files.example.com/models-4.2.tar.zst"
```

## Supersessions and open items

- This spec supersedes the v1 lock schema wholesale. The older
  boundary specs ([ion-eos-contract.md](ion-eos-contract.md),
  [ion-resolution.md](ion-resolution.md),
  [ion-manifest.md](ion-manifest.md)) carry v1-era constraints —
  `plan_digest`-keyed job identity, plugin type dispatch, `owner`
  tracking, `[[deps]]` arrays, semver mandates — that this spec
  invalidates; their reconciliation is tracked follow-up work, with the
  job-identity re-key (`plan_digest` → `action_id`) sequenced first as
  a live contract boundary.
- The publish czd's digest algorithm is whatever coz produces — the
  concrete prefix string MUST be pinned from `atom-core` before
  implementation (placeholder above: `sha256`). `anchor` and
  `charter_head` share this open item rather than a separate one: both
  are now coz czds ([lock-set-anchor], [lock-set-charter-head]), not
  git object ids, so both carry the same pending prefix pin.
- The manifest schema (constraints, overrides, toolchain roles,
  ecosystem declaration, params) is a separate specification; this spec
  constrains only what crosses into the lock.
