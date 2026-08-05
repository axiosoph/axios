# SPEC: Ion Resolution

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
-->

## Domain

**Problem Domain:** Ion resolves atom dependencies declared in
`atom.toml` to concrete versions pinned in `atom.lock`. This spec
constrains the version semantics, resolution algorithm properties,
lock file schema, and the lifecycle of lock production and consumption.

Ion is the atom-native adapter. Its `[version-total-order]`
implementation (per atom-sourcing.md) is **semantic versioning**. This
spec makes that concrete.

**Related Specs:**

- [ion-manifest.md](ion-manifest.md) — manifest schema, plugin model
- [atom-sourcing.md](atom-sourcing.md) — sourcing pipeline, mirror
  validation, `LockEntry` requirements
- [atom-transactions.md](atom-transactions.md) — claim/publish, atom
  identity

**Criticality Tier:** High — resolution correctness directly affects
supply chain integrity. An incorrect resolution could select a version
with known vulnerabilities or break reproducibility.

## Concepts

**Version resolution**: The process of selecting a specific version for
each declared atom dependency such that all version constraints are
simultaneously satisfied.

**Lock file** (`atom.lock`): A TOML file capturing the exact resolved
versions of all direct and transitive dependencies. The lock file is
the canonical input for reproducible fetches.

**Reconciliation**: The process of ensuring the lock file is consistent
with the current manifest. This involves sanitizing stale entries,
resolving new dependencies, and updating changed constraints.

**SAT resolution**: An approach to dependency resolution that reduces
the version selection problem to Boolean satisfiability. The resolver
finds an assignment of versions to dependencies that satisfies all
constraints simultaneously, or reports that no valid assignment exists.

## Constraints

### Type Declarations

```
TYPE  LockVersion = u16                       -- lock file schema version

TYPE  SetDetails = {
        tag:      Tag,                        -- user-facing set name
        mirrors:  Set<SetMirror>,             -- mirror URLs
      }

TYPE  AtomDep = {
        set:         Anchor,                  -- atom-set identity (type: Anchor digest)
        label:       Label,                   -- atom label
        version:     Version,                 -- resolved version (exact)
        publish_czd: Czd,                     -- publish transaction digest (the lock pin)
        requires:    Vec<Czd>,               -- publish_czd of each direct atom dep
        direct:      bool,                    -- true if from root manifest
      }
      -- publish_czd is the digest of the publish CozMessage. From it,
      -- the claim czd, anchor, label, version, and provenance (src, path)
      -- are all derivable. dig lives in the signed payload, verified by
      -- peel — it is NOT lock-stored (atom-sad §6.5).
      -- atom-core (L1 [lock-entry-sufficient]): set, label, version,
      -- publish_czd. ion extensions: requires, direct.

TYPE  Lockfile = {
        version:  LockVersion,                -- REQUIRED, currently 0
        sets:     Map<Anchor, SetDetails>,    -- anchor → mirrors
        compose:  Using,                      -- resolved composer
        deps:     DepMap,                     -- all resolved deps
      }
```

### Invariants

#### [version-semantics-semver]

Ion MUST use [Semantic Versioning
2.0.0](https://semver.org) for atom version ordering and constraint
matching. This is ion's implementation of atom-sourcing.md's
`[version-total-order]`. The version comparison semantics are those
of the `semver` crate: precedence is determined by major, minor,
patch, then pre-release identifiers. Build metadata is ignored for
precedence.

```claim
kind: constraint
evaluator: test
because: [version-total-order]
```

The lock's top-level `version` field is specified by
[lock-file-schema.md](lock-file-schema.md) `[lock-schema-version]`.
Resolution does not constrain it further.

#### [lock-sets-capture]

The lock file MUST capture the mapping from
anchors to mirror sets. This is the set-level information required by
atom-sourcing.md `[lock-entry-sufficient]`. Each entry in the `[sets]`
table MUST contain the set's `tag` (for human readability) and its
`mirrors` (for fetch targets).

```claim
kind: constraint
evaluator: none
because: [lock-entry-sufficient]
```

#### [lock-atom-entry-fields]

Each locked atom dependency MUST contain:
`set`, `label`, `version`, and `publish_czd` (publish transaction
digest). The `publish_czd` is the digest of the publish `CozMessage` —
from it, the claim czd, anchor, label, version, and provenance (src,
path) are all derivable; `dig` lives in the signed payload and is
verified by peel, not lock-stored (atom-sad §6.5). These are the
REQUIRED fields from atom-sourcing.md's `[lock-entry-sufficient]`. Ion
extends these with `requires` (dep graph edges, as `publish_czd` values
of direct atom deps) and `direct` (provenance flag). The `set` field
(type: Anchor digest) is the lookup key into the lock's `[sets]` table
for mirror resolution.

```claim
kind: constraint
evaluator: none
because: [lock-entry-sufficient]
```

#### [lock-requires-graph]

Each locked atom dependency SHOULD include
a `requires` field listing the **`publish_czd`** values of its
direct atom dependencies. This enables dependency graph reconstruction
from the lock file without re-resolving and supports targeted updates
when a transitive dependency changes.

```claim
kind: constraint
evaluator: none
```

#### [lock-direct-flag]

Each locked atom dependency SHOULD include a
`direct` field indicating whether it is a direct dependency (from the
root manifest) or transitive. Direct dependencies default to `true`;
the field MAY be omitted for direct deps. Transitive deps MUST set
`direct = false`.

```claim
kind: constraint
evaluator: none
```

#### [lock-compose-capture]

The lock file MUST capture the resolved
composer configuration. This includes the `publish_czd` and version
of the composer atom (for `with` variant), or the evaluation
mode (for `as` variants). The composer is resolved through the same
pipeline as regular dependencies.

```claim
kind: constraint
evaluator: none
```

#### [lock-plugin-entries]

Lock entries produced by direct dependency
plugins (per ion-manifest.md `[plugin-lock-contract]`) MUST be
stored in the same `[[deps]]` array as atom entries, distinguished
by their type tag. This ensures a single lock file contains the
complete dependency graph.

```claim
kind: constraint
evaluator: none
because: [plugin-lock-contract]
```

### Resolution Properties

#### [resolution-complete]

If a valid assignment of versions to all
declared dependencies exists, the resolver MUST find one. The resolver
MUST NOT fail for solvable instances. (Completeness.)

```claim
kind: constraint
evaluator: none
```

#### [resolution-deterministic]

Given the same manifest and the same
set of available atoms, the resolver MUST produce the same lock file.
Non-deterministic resolution (e.g., random tie-breaking) is forbidden.
ALL provider and discovery outputs (candidate enumeration, dependency
lists, git-ref and map iteration) MUST be consumed in a stable,
normalized order; ordering nondeterminism is the primary determinism
footgun and is forbidden (SAD §6.1, §6.7).

```claim
kind: constraint
evaluator: none
because: [docs/architecture/atom-sad#6.1, docs/architecture/atom-sad#6.7]
```

#### [resolution-highest-match]

When multiple versions of an atom
satisfy a constraint, the resolver SHOULD prefer the highest
compatible version. This is a SHOULD, not MUST — the resolver MAY
select a lower version if it is necessary to satisfy other constraints
in the dependency graph.

```claim
kind: constraint
evaluator: none
```

#### [resolution-unsolvable-diagnostic]

If no valid assignment exists,
the resolver MUST report the conflict clearly. The error SHOULD
identify the conflicting constraints and the atoms involved. A bare
"resolution failed" message is insufficient.

```claim
kind: constraint
evaluator: none
```

#### [resolution-transitive]

The resolver MUST resolve the transitive
closure of atom dependencies. If atom A depends on atom B, and atom B
depends on atom C, then C MUST appear in the lock file even though
the root manifest does not mention it.

```claim
kind: constraint
evaluator: none
```

### Transitions

#### [reconcile]

Before any operation that reads the lock file, ion
MUST reconcile the lock with the current manifest.

- **PRE**: A valid manifest and a lock file (possibly empty, stale,
  or absent) exist.
- **POST**: The lock file contains entries for exactly the current
  dependency graph. Stale entries (deps removed from manifest) are
  purged. New entries (deps added to manifest) are resolved and
  locked. Changed constraints are re-resolved. The lock file is
  written atomically.

No reconciliation pipeline exists; `ion-resolve/src/lib.rs`'s own
docstring states: "A full SAT-based resolver and lock file generator
are planned but not yet implemented."

```claim
kind: constraint
evaluator: none
```

#### [lock-freeze]

After reconciliation, the lock file is frozen
for the current operation. Subsequent reads within the same operation
(e.g., publish, build) MUST use the reconciled lock, not re-resolve.

- **PRE**: Reconciliation has completed.
- **POST**: The lock file is immutable for the duration of the
  operation. Any further manifest changes require a new reconciliation
  cycle.

This depends on the same unimplemented reconciliation pipeline as
`[reconcile]`; no operation-scoped lock-freeze mechanism exists.

```claim
kind: constraint
evaluator: none
depends: [reconcile]
```

Lock write atomicity — including the crash-safety guarantee and the
write-to-temporary-then-rename discipline — is specified by
[lock-file-schema.md](lock-file-schema.md) `[lock-atomic-write]`.
Resolution computes the new lock state; it does not specify how that
state is written.

### Forbidden States

#### [no-partial-lock]

A lock file MUST NOT contain a partial
resolution. Either all dependencies are resolved and captured, or
the lock file is not written. A lock file with some deps resolved
and others missing is invalid.

```claim
kind: constraint
evaluator: none
```

#### [no-stale-lock-entry]

After reconciliation, the lock file MUST
NOT contain entries for dependencies that are no longer declared in
the manifest (directly or transitively). Stale entries MUST be purged.

```claim
kind: constraint
evaluator: none
```

> **Note (2026-07-05, P4 flag):** A literal reading purges
> tool-recorded fetch entries (the substrate's record-mode fetch proxy
> writes discovered `type = "fetch"` entries back into the lock, HTC/L2,
> `htc-sad.md` §4.2) that carry no manifest declaration. This needs
> either owner-derived liveness (the entry lives while its owning atom
> lives) or a tool-authored-entry class exempt from this purge; tracked
> as design campaign **P4**, per
> [ADR-0005](../adr/0005-hermetic-transactional-composition.md) §Open
> Items. No semantic change to this invariant in this pass.

#### [no-constraint-violation]

The lock file MUST NOT contain a
version that violates a declared constraint. If the manifest says
`foo = "^1.0"` and the lock contains `foo` at version `2.0.0`, the
lock is invalid.

```claim
kind: constraint
evaluator: none
```

### Behavioral Properties

#### [lock-reproducibility]

Given the same lock file and the same
mirror state, fetching all dependencies MUST produce the same set of
artifacts. This is a consequence of atom-sourcing.md's
`[resolution-reproducible]`, restated in ion's context. Content
binding flows through `publish_czd`: each locked entry's `publish_czd`
pins the publish `CozMessage`; on fetch, the peeled content sha MUST
equal `payload.dig`, which is verified by peel — `dig` is NOT stored
in the lock (atom-sad §6.5).

- **Type**: Safety

```claim
kind: constraint
evaluator: none
because: [resolution-reproducible]
```

#### [reconcile-idempotent]

Reconciling an already-reconciled lock
file against an unchanged manifest MUST produce no changes. The
reconcile operation is idempotent.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

#### [plugin-dep-sanitization]

During reconciliation, plugin
dependencies MUST be sanitized against the current manifest's
`[deps.direct]` section, just as atom dependencies are sanitized
against `[deps.from]`. Plugin entries whose names no longer appear
in the manifest MUST be purged from the lock.

- **Type**: Safety

```claim
kind: constraint
evaluator: none
```

> **Note (2026-07-05, P4 flag):** Same tension as `[no-stale-lock-
entry]` above: this would purge tool-recorded fetch entries the
> substrate's record-mode fetch proxy writes back with no manifest
> declaration. Design campaign **P4** resolves both together, per
> [ADR-0005](../adr/0005-hermetic-transactional-composition.md) §Open
> Items. No semantic change to this invariant in this pass.

#### [git-tag-version-inference]

For direct dependencies referencing
git repositories (via plugins), implementations MAY infer version
ordering from semver-compatible git tags. This is a convenience for
backends whose storage format uses semver-tagged refs. This capability
is NOT REQUIRED — it depends on the specific atom storage backend.

- **Type**: Liveness (convenience)

```claim
kind: constraint
evaluator: none
```

## Lock File Schema (Informative)

```toml
version = 0

[sets.<anchor-hex>]
tag = "company-atoms"
mirrors = ["git@github.com:our-company/atoms", "https://mirror.example.com/atoms"]

[compose]
use = "<publish-czd-of-composer-atom>"
at = "2.0.0"
entry = "default.nix"

[[deps]]
type        = "atom"
set         = "<anchor-hex>"
label       = "auth-service"
version     = "1.5.2"
publish_czd = "<bare-czd>"
requires    = ["<publish-czd-of-dep>"]

[[deps]]
type        = "atom"
set         = "<anchor-hex>"
label       = "shared-config"
version     = "0.1.0"
publish_czd = "<bare-czd>"
direct      = false

[[deps]]
type = "nix+git"
name = "nixpkgs"
url = "https://github.com/NixOS/nixpkgs"
rev = "<commit-hex>"

[[deps]]
type = "nix+tar"
name = "openssl"
url = "https://www.openssl.org/source/openssl-3.1.0.tar.gz"
hash = "sha256:..."
owner = "<publish-czd-of-owning-atom>"
```

## Verification

All constraints are internally consistent. No contradictions with
ion-manifest.md or atom-sourcing.md. Agent-level verification (Tier 1).

## Implications

1. **SAT solver choice**: The `[resolution-complete]` and
   `[resolution-deterministic]` constraints are satisfiable by any
   complete SAT-based resolver with deterministic variable ordering.
   The PoC uses `resolvo`. The spec does not mandate a specific solver.

2. **Lock migration**: The lock's `version` field
   ([lock-file-schema.md](lock-file-schema.md) `[lock-schema-version]`)
   enables format evolution once an MVP fixes the schema. At
   `version = 0` no migration path is promised.

3. **Plugin deps in lock**: Plugin lock entries share the `[[deps]]`
   array with atom entries, discriminated by type tag. This is exactly
   what the PoC already does with `Dep::Nix*` variants.

4. **Testing strategy**: Property-based tests should:
   - Generate dependency graphs and verify resolution completeness
   - Verify determinism by running the same resolution twice
   - Verify reconciliation idempotency on stable manifests
   - Generate partial locks and verify they are rejected
