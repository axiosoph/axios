# SPEC: Atom Sourcing

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

**Problem Domain:** The atom protocol enables decentralized, ecosystem-
agnostic package publishing. Consumers need a well-defined framework for
sourcing atoms from remote and local mirrors: discovering available atoms,
validating mirror consistency, and capturing resolutions in lock files.
This spec constrains the behavioral contracts that all sourcing
implementations MUST satisfy, while remaining agnostic to specific
manifest formats, lock file schemas, and version resolution algorithms.

**Model Reference:** [publishing-stack-layers.md](../models/publishing-stack-layers.md)
— §2.3 (store coalgebra), §3 (transactions and identity).

**Related Specs:**

- [atom-transactions.md](atom-transactions.md) — claim/publish
  transactions, Coz verification, `ClaimPayload.pkg` (PURL type)
- [git-storage-format.md](git-storage-format.md) — ref layout, object
  format, claim chain model, store vs registry semantics

**Criticality Tier:** Medium — supply chain integrity. Mirror validation
invariants are safety-critical (skip them and you accept tampered atoms).
Resolution mechanics are ecosystem-adapter concerns and carry lower risk.

## Concepts

This spec uses the following concepts, defined here or in related specs:

**Mirror**: A network-accessible atom source. Typically a git repository
URL, but the protocol does not mandate a specific transport.

**Mirror set** (or simply **set**): A named group of mirrors that all
serve atoms from the same atom-set (i.e., sharing the same anchor). The
set is the unit of consistency checking.

**Local set**: A special-case mirror set referring to the current
repository. Identified by `::` in manifest declarations (by convention).
Support for local sets is OPTIONAL but enables development workflows.

**Anchor**: The genesis commit hash of an atom-set. It cryptographically
pins a set of atoms to a single source history. Defined in
atom-transactions.md §Anchor.

**Publish digest** (`publish_czd`): The canonical Coz digest (`czd`) of
the **publish `CozMessage`**. Stored **bare** (original algorithm) in the
lock — representing the actual cryptographic security the signature covers.
`blake3(publish_czd)` is used exclusively as the store-key (never stored
in the lock). See atom-sad.md §6.5–6.6.

**Claim digest** (`czd`): The canonical digest of a claim `CozMessage`.
Uniquely identifies a specific claim. Defined in atom-transactions.md
§ClaimPayload.

**Atom snapshot digest** (`dig`): The ObjectId of the atom commit. The
published artifact. Defined in atom-transactions.md §PublishPayload.

## Constraints

### Type Declarations

```
TYPE  MirrorURL   = String                    -- network-accessible atom source
TYPE  SetName     = String                    -- user-facing label for a mirror group
TYPE  Version     = String                    -- opaque to protocol; ordering is adapter-defined

TYPE  MirrorSet = {
        name:     SetName,
        mirrors:  Set<MirrorURL>,             -- one or more mirrors
        anchor:   Anchor                      -- all mirrors MUST share this anchor
      }

TYPE  AtomRef = {
        label:    Label,
        version:  Version,
        dig:      Vec<u8>,                    -- atom snapshot digest (atom commit hash)
        publish_czd: Czd                      -- bare publish digest (locked; store-keyed via blake3); claim reachable via the publish payload
      }

TYPE  LockEntry = {
        set:         Anchor,                  -- atom-set identity = the genesis anchor; keys into anchor → mirrors
        label:       Label,                   -- which atom
        version:     Version,                 -- which version (opaque)
        publish_czd: Czd,                     -- bare publish digest (actual crypto security)
      }
      -- Minimum per-entry fields. Mirror URLs are captured at the set
      -- level (anchor → mirrors), not per-entry. `dig` is NOT stored —
      -- it lives in the signed publish payload and MUST be verified by
      -- peeling the publish_czd tag chain on acquisition. Adapters MAY
      -- add src (provenance), requires (transitive deps), pkg (PURL
      -- type), or any ecosystem-specific fields.
```

### Invariants

**[set-anchor-bijection]**: The mapping between set names and anchors
MUST be bijective. Specifically:

- An anchor MUST NOT appear in more than one set.
- A set name MUST NOT map to more than one anchor.

Violations indicate misconfiguration or supply-chain attack.
Implementations MUST reject the entire resolution if a bijection
violation is detected.
`VERIFIED: unverified`

**[atom-version-identity]**: If two mirrors in the same set advertise
the same atom (same label) at the same version, their atom snapshot
digests (`dig`) MUST be identical. A mismatch MUST be treated as
evidence of tampering or misconfiguration. Implementations MUST reject
the atom and SHOULD surface the conflicting mirrors and digests in the
error message.
`VERIFIED: unverified`

**[mirror-staleness-tolerance]**: One mirror having MORE atoms or
versions than another mirror in the same set is NOT an error. Mirrors
MAY sync at different cadences. The invariant constrains conflicting
content, not missing content. Resolvers SHOULD aggregate availability
across all mirrors in a set and resolve from the union.
`VERIFIED: unverified`

**[version-total-order]**: The ecosystem adapter MUST define a total
order over version strings for each atom type. The protocol treats
versions as opaque strings; the adapter provides comparison semantics
(e.g., semver for cargo, PEP 440 for pypi). Resolvers rely on this
order for constraint satisfaction and highest-match selection.
`VERIFIED: unverified`

**[lock-entry-sufficient]**: A lock entry for a **published** atom
MUST capture at minimum: `set`, `label`, `version`, and
`publish_czd`. Mirror URLs are NOT per-entry — the lock MUST separately
capture the mapping from anchors to mirror sets (anchor → mirrors),
enabling any entry to derive its fetch targets by anchor lookup.
These four fields are sufficient to reproduce the exact fetch without
re-resolving: `set` + `label` + `version` locates the atom by name;
`publish_czd` pins the exact signed publish. The atom's `dig` is NOT
stored in the lock — it lives in the signed publish payload (the
`publish_czd`-pinned source of truth); on acquisition the peeled
content-addressed sha MUST equal `payload.dig`, or the fetch MUST be
rejected as tampered (SAD §6.5; SAD §8 failure mode 8.3). The atom
commit is peelable from the `publish_czd` tag chain; no `rev` field is
needed. Adapters MAY extend the lock entry with additional fields.

Local development atoms (see §Local Development Sets) are exempt from
the `publish_czd` requirement — they have no publish. A dev lock entry
MUST still capture `set`, `label`, and `version`, and SHOULD indicate
that the resolution is local-only.
`VERIFIED: unverified`

### Transitions

**[source-discovery]**: A resolver MAY discover atom sources from
manifest declarations.

- **PRE**: The manifest declares one or more mirror sets, each with at
  least one mirror URL. The manifest format is ecosystem-defined.
- **POST**: A validated set of `MirrorSet` values exists. Each set has
  a name, one or more mirror URLs, and a discovered anchor. All mirrors
  in a set have been queried (or a subset, per the resolver's fetch
  strategy). `[set-anchor-bijection]` holds across all sets.
  `VERIFIED: unverified`

**[mirror-validation]**: Before aggregating atoms from a set's mirrors,
the resolver MUST validate mirror consistency.

- **PRE**: At least one mirror in the set has been queried. The set's
  anchor has been discovered from at least one mirror.
- **POST**: All queried mirrors in the set share the same anchor
  (`[set-anchor-bijection]`). All atoms advertised by multiple mirrors
  have consistent digests (`[atom-version-identity]`). Atoms from all
  queried mirrors are aggregated into a single availability set
  (`[mirror-staleness-tolerance]`).
  `VERIFIED: unverified`

**[lock-capture]**: After version selection, the resolver MUST capture
the resolution in a lock entry.

- **PRE**: A specific version of a specific atom has been selected. The
  atom's `publish_czd` is known (located in the publish tag chain). At
  least one source set (anchor) is known.
- **POST**: A `LockEntry` exists with `set`, `label`, `version`, and
  `publish_czd` populated per `[lock-entry-sufficient]`. The `dig` is
  NOT stored in the lock; it is verified on acquisition by peeling the
  publish tag chain (`payload.dig` MUST equal the peeled atom commit
  sha). The lock entry is sufficient to reproduce the fetch and verify
  integrity without re-resolving.
  `VERIFIED: unverified`

### Forbidden States

**[no-cross-set-anchor]**: An anchor MUST NOT appear in more than one
named set within a single resolution context. If detected, the resolver
MUST reject the entire resolution — not just the conflicting set.
`VERIFIED: unverified`

**[no-conflicting-digest]**: Two mirrors in the same set MUST NOT
advertise the same atom at the same version with different digests. If
detected, the resolver MUST reject the atom and SHOULD identify the
conflicting mirrors.
`VERIFIED: unverified`

**[no-unpublished-dependency]**: An atom MUST NOT be publishable if any
of its **direct** atom dependencies exist only as local development
versions (no remote published version exists). All direct dependencies
MUST be published and locked to remote references before the depending
atom can be published. This ensures every published atom is evaluable
from any remote mirror.

Transitive enforcement is not required: if every publisher enforces
this gate on direct dependencies, then by induction no published atom
can transitively depend on an unpublished atom. The gate cascades
through the publishing chain without resolvers walking the transitive
closure.
`VERIFIED: unverified`

### Behavioral Properties

**[aggregation-monotonic]**: Adding a mirror to a set MUST NOT reduce
the set of available atoms. A resolver that aggregates from N mirrors
MUST make available at least the union of atoms from all N mirrors.
This is a consequence of `[mirror-staleness-tolerance]`.

- **Type**: Safety
  `VERIFIED: unverified`

**[resolution-reproducible]**: Given the same lock file, a resolver
MUST reproduce the same fetch targets. Lock entries contain sufficient
information (`set`, `label`, `version`, `publish_czd`, sources) to
deterministically identify the exact atom version without re-resolving.

- **Type**: Safety
  `VERIFIED: unverified`

**[concurrent-validation-safe]**: Implementations SHOULD support
concurrent validation of mirrors within a set for performance.
Concurrent validation MUST produce the same result as sequential
validation — the invariant checks are commutative over mirror ordering.

- **Type**: Safety
  `VERIFIED: unverified`

**[czd-divergence-handling]**: If two mirrors in the same set advertise
the same atom at the same version with identical `dig` but different
`publish_czd` values, the resolver MUST derive each publish's claim
(from the publish payload) and check whether the claims are in the
same claim chain (one is an ancestor of the other). If they are, the
resolver SHOULD prefer the newest claim and SHOULD surface the claim's
`meta` fields (see atom-transactions.md `[claim-payload-extensible]`)
to inform the user — particularly `meta.supersedes` and
`meta.announcement`. If the claims are NOT in the same chain (genuinely
distinct ownership), the resolver MUST treat this as a conflict and
reject the atom.

- **Type**: Safety
  `VERIFIED: unverified`

## Sourcing Pipeline

The sourcing pipeline is the normative sequence of operations a resolver
performs at **ingestion time** — when populating an `AtomStore` from
remote sources. It does NOT describe build-time resolution, which reads
from the local store through the `AtomSource` interface (see
atom-transactions.md §Source/Store Topology).

Steps 1–4 are generic and constrained by this spec. Step 5 is an
extension point for ecosystem adapters.

```
1. Source Discovery       — parse manifest, discover mirror sets
2. Mirror Validation      — enforce [set-anchor-bijection], [atom-version-identity]
3. Atom Aggregation       — merge availability across mirrors per [mirror-staleness-tolerance]
4. Lock Capture           — record resolution per [lock-entry-sufficient]
5. Version Selection      — ecosystem-specific (semver, PEP 440, etc.)
```

Steps 1–4 are ordered: each step's post-conditions are the next step's
pre-conditions. Step 5 (version selection) executes between steps 3 and
4 — after aggregation produces the availability set, the adapter
selects a version, and then the lock entry is captured.

### Manifest Requirements

A manifest format that declares atom dependencies MUST provide, in
whatever syntax the ecosystem uses:

1. **Source declaration**: One or more mirror URLs per set. Each mirror
   MUST be a network-accessible atom source or the local set sentinel
   (`::` by convention).

2. **Dependency declaration**: The atom label and a version constraint.
   The constraint syntax is ecosystem-defined (e.g., semver ranges for
   cargo, version specifiers for pypi).

3. **Set grouping**: Which mirrors belong to the same set. Sets are the
   unit of anchor bijection checking and mirror validation.

The manifest format itself (TOML, JSON, YAML, etc.) is NOT constrained.

### Lock Requirements

A lock format that captures atom resolutions MUST record, for each
resolved atom, the fields defined in the `LockEntry` type:

**Per-entry fields:**

| Field         | Purpose                                             | Required    |
| :------------ | :-------------------------------------------------- | :---------- |
| `set`         | Atom-set identity (genesis commit hash)             | MUST        |
| `label`       | Atom label within the set                           | MUST        |
| `version`     | Resolved version (opaque string)                    | MUST        |
| `publish_czd` | Bare publish digest (cryptographic security anchor) | MUST        |
| `src`         | Source revision hash (provenance audit)             | RECOMMENDED |
| `requires`    | Transitive atom dependencies                        | RECOMMENDED |
| `pkg`         | PURL type (tooling interop)                         | OPTIONAL    |

**Set-level fields:**

| Field              | Purpose                            | Required |
| :----------------- | :--------------------------------- | :------- |
| `anchor → mirrors` | Mapping from anchor to mirror URLs | MUST     |

Mirror URLs are captured at the set level, not duplicated per entry.
Each entry's `set` serves as the lookup key into the mirror set
mapping. This matches the PoC's lock format and avoids redundancy.

The lock format itself (TOML, JSON, etc.) is NOT constrained.

### Local Development Sets

Support for local development sets is OPTIONAL. If an adapter supports
local sets:

- The local repository MAY be referenced as a source using the `::` sentinel
  (or equivalent adapter-defined syntax).

- Local development atoms are atoms that exist in the local
  repository but have not been published to any remote mirror.

- **[no-unpublished-dependency]** MUST be enforced: atoms with
  dev-only dependencies MUST NOT be publishable. All dependencies
  MUST be published and locked to remote references first.

- Local atoms SHOULD be captured in the lock file when no remote
  version exists, to ensure dependency completeness. The lock entry
  SHOULD indicate that the resolution is local-only.

If an adapter does not support local sets, `[no-unpublished-dependency]`
is trivially satisfied (no local-only atoms exist).

### Peer-Assisted Resolution

A resolver or build system MAY accept atom content from a peer client as a fallback resolution mechanism. This enables two scenarios:

1. **Local development atoms**: Atoms that exist only in the developer's working tree have no remote mirror. The client (ion) is the only source for these atoms. See §Local Development Sets.

2. **Mirror failure recovery**: If all configured mirrors for an atom-set are unreachable, the client MAY provide cached atom content it previously resolved, allowing the build to proceed despite network issues.

Peer-assisted resolution is a transport-level concern: the atom content enters the store through the same `AtomStore::ingest()` path as any other source. The peer acts as an `AtomSource` that the store ingests from. All ingestion invariants apply — the store verifies atom integrity before accepting content from a peer, just as it would from a registry.

**[peer-source-last-resort]**: Peer-assisted resolution SHOULD be the lowest-priority source. The resolution priority order is:

1. Local store (previously ingested atoms)
2. Remote mirrors (authoritative sources)
3. Peer client (fallback)

This ordering ensures that authoritative sources are preferred and that the peer is only consulted when all other options are exhausted.
`VERIFIED: unverified`

## Verification

| Constraint                 | Method      | Result | Detail                                                                                                 |
| :------------------------- | :---------- | :----- | :----------------------------------------------------------------------------------------------------- |
| set-anchor-bijection       | agent-check | pass   | Bijective mapping is well-defined; no contradictions with other invariants                             |
| atom-version-identity      | agent-check | pass   | Consistent with cryptographic assumptions (collision-resistant digests)                                |
| mirror-staleness-tolerance | agent-check | pass   | Non-constraining (permits superset); no contradiction possible                                         |
| version-total-order        | agent-check | pass   | Requirement on adapter, not protocol; no internal contradiction                                        |
| lock-entry-sufficient      | agent-check | pass   | Fields are a subset of publish/claim payloads; all derivable                                           |
| no-cross-set-anchor        | agent-check | pass   | Corollary of set-anchor-bijection                                                                      |
| no-conflicting-digest      | agent-check | pass   | Corollary of atom-version-identity                                                                     |
| no-unpublished-dependency  | agent-check | pass   | Ensures remote evaluability; no contradiction with local dev                                           |
| aggregation-monotonic      | agent-check | pass   | Follows from staleness tolerance definition                                                            |
| resolution-reproducible    | agent-check | pass   | Lock entry fields are deterministic; no stochastic components                                          |
| concurrent-validation-safe | agent-check | pass   | Invariant checks are per-atom or per-set; commutative                                                  |
| czd-divergence-handling    | agent-check | pass   | Chain ancestry check is well-defined; distinct-chain rejection is consistent with set-anchor-bijection |

All constraints are internally consistent. No contradictions detected
between invariants. Verification is agent-level (Tier 1); formal
verification (Alloy/TLA+) is deferred pending implementation
experience.

## Implications

1. **Implementation guidance**: Resolvers MUST implement
   `[set-anchor-bijection]` and `[atom-version-identity]` as
   early-exit checks. Failing fast on mirror inconsistency prevents
   consumption of potentially tampered atoms.

2. **Testing strategy**: Property-based tests should generate mirror
   sets with controlled inconsistencies and verify rejection.
   Specifically:
   - Same atom + version with different digests → must reject
   - Same anchor in two sets → must reject
   - Mirrors with differing completeness → must accept union

3. **atom-core trait surface**: The `AtomStore::ingest` method in
   atom-core already accepts a source. The sourcing pipeline's
   validation steps SHOULD execute before `ingest` is called — the
   store should only see pre-validated atoms.

4. **Ecosystem adapter contract**: Adapters MUST provide:
   - A version comparator (total order over version strings)
   - Manifest parsing (source/dependency/set declarations)
   - Lock serialization (at minimum the `LockEntry` fields)

5. **Open questions**:
   - Should the spec define a canonical wire format for mirror
     advertisement, or is ref enumeration sufficient?
