<!--
  Ion Software Architecture Document (SAD) — L4 (renumbered 2026-07-05 per
  ADR-0005 §9, [htc-layer-designation]; was L3 before HTC/L2 was inserted).

  Authoritative source of truth for the Ion layer. The specs under docs/specs/
  (ion-manifest.md, ion-resolution.md, lock-file-schema.md, ion-eos-contract.md)
  derive from this document; on conflict, this document takes precedence and the
  spec is realigned. ADRs record architectural changes.

  Maintained as Architecture-as-Code. Diagrams are Mermaid.js, inline.

  Consumes the atom (L1) identity contract — see atom-sad.md. Key inputs:
  identity is the claim record's czd, not the (anchor,label) pair (atom-sad
  §6.1); the lock's `AtomId`-digest dep key, nested `set`/`label`; the
  2-value atom lock contribution {version, publish_czd}; the self-describing
  atom git object.
-->

# Ion Software Architecture Document (SAD)

## 1. Context

### 1.1 System Purpose

Ion is the **dependency resolution and lock** layer (L4) of the Axios stack. It
takes a project's **manifest** (what it depends on, as ranges), resolves a
**single coherent version per atom** across the transitive graph, and writes a
**lock** that is the sole, reproducible input handed to eos (L3) for building.

Ion owns:

- **The manifest** — the human-authored declaration of direct dependencies and
  version constraints, extensible by plugins for non-atom dependency types.
- **Resolution** — solving the transitive constraint graph to one version per
  label within its anchor, deterministically, with actionable diagnostics on
  failure.
- **The lock** — the reproducible, verifiable pin of the resolved graph: the
  atom-core entry (from L1) plus ion's graph extensions, keyed by `AtomId`.
- **The eos handoff** — turning the lock into a build request eos can execute.

Ion does **not** own: atom identity, publishing, or storage (L1, atom); build
execution or scheduling (L3, eos). It **consumes** atom's identity contract and
**produces** eos's build input. By layer discipline, ion depends on atom and eos
contracts; nothing depends on ion.

### 1.2 External Actors

```mermaid
graph TB
  DEV["Developer (ion-cli)"]
  subgraph L4["Ion (L4)"]
    MAN["Manifest"]
    RES["Resolver"]
    LOCK["Lock"]
    BR["Build Request"]
  end
  subgraph L1["Atom (L1)"]
    SRC["AtomSource / AtomContent\n(discovery + self-describing objects)"]
  end
  subgraph L3["Eos (L3)"]
    EOS["Scheduler"]
  end

  DEV -->|"edit deps"| MAN
  MAN -->|"constraints"| RES
  RES -->|"discover versions; read manifests"| SRC
  RES -->|"one version per label within its anchor"| LOCK
  LOCK -->|"serialize"| BR
  BR -->|"build request (atom pointers)"| EOS
  EOS -->|"read content"| SRC
```

### 1.3 System Boundaries

| Boundary       | Inside Ion                                                   | Outside Ion                                                          |
| :------------- | :----------------------------------------------------------- | :------------------------------------------------------------------- |
| **Manifest**   | direct-dep declaration, ranges, plugin-extended types        | atom identity, version _semantics_ (L1 `VersionScheme`)              |
| **Resolution** | the constraint solver, single-version selection, diagnostics | atom discovery/fetch primitives (L1 `AtomSource`)                    |
| **Lock**       | the lock schema, ion graph extensions, cross-references      | the atom-core entry fields (defined by L1 `[lock-entry-sufficient]`) |
| **Handoff**    | lock→build-request serialization, the eos contract           | build execution, scheduling, the artifact store (L3)                 |

### 1.4 Layer Discipline

`ion-*` crates depend on `atom-*` (the identity/lock-core contract) and on the
eos contract types; the reverse is forbidden (`layer-boundaries.md`
`[boundary-downward-only]`). Ion reads atoms via L1 `AtomSource`/`AtomContent`
and never reaches below the published contract (no direct git-object surgery —
that is L1's `atom-git`).

### 1.5 Two Clients, One Store (ADR-0009)

_Added 2026-07-30._ Ion is not the only reader of the atom store: eos is
a second client, on a different axis. Both read and manage the same
`czd → object` keyspace ([Atom Store Format](../specs/atom-store-format.md)),
but ion is the **local, single-user** client — tracking few sources per
subject, over its own closure — while eos **aggregates over many
sources** for the subjects it serves. Being the aggregator does not
imply whole-store sync: eos calls `facts.roots` ([Fact
Exchange](../specs/fact-exchange.md)) against N peers with one subject
list, the same subject-scoped shape ion uses at one-source scale.
Divergent roots for one subject from two sources is, for free, an
equivocation signal — the store is the only place records from multiple
sources meet.

#### [two-clients-one-store]

Two clients read and manage one store: a local single-user client, and
an aggregate over many **sources**. They differ on the source axis, not
the subject axis.

```claim
kind: requirement
evaluator: none
```

## 2. Container View

```mermaid
graph TB
  CLI["ion-cli (entry)"]
  MAN["ion-manifest\n(parse + plugin types)"]
  RES["ion-resolve\n(solver)"]
  LOCK["ion-lock\n(schema + (de)serialize)"]
  EOS["ion-eos\n(build-request bridge)"]
  ATOM["atom (L1)\nAtomSource/AtomContent"]
  SCHED["eos daemon (L3)"]

  CLI --> MAN
  MAN --> RES
  RES -->|"discover/fetch"| ATOM
  RES --> LOCK
  LOCK --> EOS
  EOS -->|"BuildRequest"| SCHED
```

### 2.1 Manifest

The human-authored entrypoint (`ion.toml`): direct dependencies with version
ranges, the `[compose]` declaration, and plugin-typed non-atom deps. Parsed by
`ion-manifest`; it is the input to resolution, never transmitted to eos.

### 2.2 Resolver

Drives `ion-resolve`: discovers candidate versions (object-free `ls-refs` over L1
`AtomSource`), lazily fetches each reached atom's manifest, and solves to one
version per label within its anchor. Output is the resolved graph.

### 2.3 Lock

`ion-lock` serializes the resolved graph deterministically. It is the **only**
artifact that crosses the L4→L3 boundary, and the sole input eos needs to fetch,
verify, and build the closure.

### 2.4 Eos Bridge

`ion-eos` translates the lock into an eos `BuildRequest` (atom pointers +
`compose.args` → `action_params`) and connects to the eos daemon over the wire (§6.9).
It is the only ion container that speaks the eos protocol.

### 2.5 Atom (L1) / Eos Daemon (L3)

External to ion: the L1 `AtomSource`/`AtomContent` that resolution discovers and
fetches from, and the L3 daemon that executes the build request. Ion depends on
both contracts; neither depends on ion.

### 2.6 Ion's Operation Set (ADR-0009)

_Added 2026-07-30._ The premise organizing ion's whole surface (nrd):
**decouple doing from building**. Nix's derivation is too general —
entering a devshell may unexpectedly invoke a build, because everything
routes through one undifferentiated evaluation. Ion's surface is instead
seven named operations, each classified against the trichotomy's
`Fetch`/`Discover` capability split
(`discover-one-question`):
declare, **resolve** (the only one that coordinates), form, materialize,
do, publish, inspect. A user-visible promise follows directly: on a
plane, everything works except adding a new dependency.

#### [seven-operations]

declare, resolve, form, materialize, do, publish, inspect.

```claim
kind: requirement
evaluator: none
```

## 3. Component View

```mermaid
graph TB
  CLI["ion-cli"] --> MAN["ion-manifest"]
  CLI --> RES["ion-resolve"]
  CLI --> EOSB["ion-eos"]
  MAN --> RES
  RES --> LK["ion-lock"]
  EOSB --> LK
  RES -. discover/fetch .-> ATOM["atom (L1)\nAtomSource/AtomContent"]
  EOSB -. Cap'n Proto .-> DAEMON["eos daemon (L3)"]
```

| Crate          | Role                                                                             |
| :------------- | :------------------------------------------------------------------------------- |
| `ion-manifest` | Parse the manifest; dispatch direct-dep types (atom + plugin-registered)         |
| `ion-resolve`  | The constraint solver; transitive resolution to one version per label within its anchor |
| `ion-lock`     | The lock schema and (de)serialization; the `DepMap` keyed by `AtomId`            |
| `ion-eos`      | Translate the lock into an eos `BuildRequest`; the L3 handoff                    |
| `ion-cli`      | Developer entry point (resolve, lock, build orchestration)                       |

## 4. Core Lifecycles

### 4.1 Resolve → Lock

```mermaid
sequenceDiagram
  participant CLI as ion-cli
  participant Res as Resolver
  participant Src as AtomSource (L1)
  CLI->>Res: resolve(manifest)
  loop per atom reached
    Res->>Src: get_candidates(label) [object-free ls-refs]
    Src-->>Res: advertised versions
    Res->>Src: get_dependencies(version) [fetch manifest — lazy]
    Src-->>Res: that atom's deps
  end
  Note over Res: solve to ONE version per label within its anchor\n([resolution-complete], deterministic)
  Res-->>CLI: solution
  CLI->>CLI: lock-capture → DepMap{set: {label: {version, publish_czd, requires, direct}}}
```

### 4.2 Lock → Build Request (eos handoff)

```mermaid
sequenceDiagram
  participant CLI as ion-cli
  participant Eos as ion-eos
  participant Sched as eos daemon (L3)
  CLI->>Eos: build(lock)
  Eos->>Eos: serialize lock → BuildRequest (atom pointers: set,label,version,publish_czd)
  Eos->>Sched: BuildRequest
  Note over Sched: eos resolves each atom via L1 AtomSource and\nreads dig/rev/content from the self-describing git object
```

## 5. State Machine Models

### 5.1 Resolution

```mermaid
stateDiagram-v2
  [*] --> Solving: resolve(manifest)
  Solving --> Solving: propagate / decide / backtrack (solver-internal; not spec-mandated)
  Solving --> Solved: one version per label within its anchor
  Solving --> Unsolvable: no valid assignment
  Solved --> [*]: write lock
  Unsolvable --> [*]: actionable diagnostic ([resolution-unsolvable-diagnostic])
```

### 5.2 Lock Entry

```mermaid
stateDiagram-v2
  [*] --> Captured: lock-capture (resolved version + publish_czd)
  Captured --> Reused: consume — fetch by publish_czd, verify peeled sha == payload.dig
  Captured --> Bumped: upstream metadata advanced (new publish_czd in chain) — warn + optional bump
```

## 6. Cross-Cutting Concerns and System Invariants

### 6.1 Resolution Properties

Ion's contract is five behavioral properties, solver-agnostic (any complete
resolver — PubGrub or a SAT solver such as resolvo — MAY implement them):

- **`[resolution-complete]`** — if a valid assignment exists, the resolver MUST
  find one.
- **`[resolution-deterministic]`** — same manifest + same available versions ⇒
  byte-identical lock. **This requires every discovery/provider output to be
  consumed in a stable order** (git-ref and map iteration MUST be normalized);
  ordering nondeterminism is the determinism footgun and is forbidden.
- **`[resolution-highest-match]`** (SHOULD) — prefer the highest version
  satisfying all constraints.
- **`[resolution-unsolvable-diagnostic]`** — on failure, produce a human-readable
  "because X and Y, Z" explanation.
- **`[resolution-transitive]`** — resolve the full transitive closure.

### 6.2 Single Version per Identity

Exactly one version is locked per label within its anchor. This is structural:
the lock's `DepMap` nests by `set` then `label`, so uniqueness holds by
construction. (Multi-version coexistence is a roadmap concern, not the
current contract.)

### 6.3 The Lock Model (atom-core + ion extensions)

The lock entry composes two layers (see §7 for the schema):

- **Atom core** (defined by L1 `[lock-entry-sufficient]`): keyed by nesting on
  `set` then `label`, value `{version, publish_czd}`. Ion does **not**
  redefine these — it inherits them.
- **Ion extensions**: `requires` (the resolved direct-dep edges, for offline graph
  reconstruction without re-resolving) and `direct` (declared-vs-transitive). These
  are ion's concern, layered over the atom core.

### 6.4 Cross-Reference Encoding

With no hashed atom `id`, intra-lock references — `requires`, `owner`, and
`[compose].use` — name an atom by its **`publish_czd`** (the bare Coz digest already
carried by every atom entry; cryptographically unique to one publish). Each
reference MUST equal the `publish_czd` of exactly one `type = "atom"` entry. This
pins the **specific publish** of the referenced dependency — not just its name — and
inherits the same durability as the entry's own pin (if the dependency's metadata
later advances, the edge still names the old, persisted `publish_czd`). The closures
(`[lock-requires-closure]`, `[lock-owner-closure]`, `[lock-compose-closure]`) match
on `publish_czd`.

### 6.5 Plugin Extension Point

Non-atom dependency types are **plugin extensions**, not core. Each `[[deps]]`
entry carries a `type` discriminant (`[lock-dep-type-dispatch]`); `type = "atom"`
is the ion core, and plugins register additional namespaced types via
`[lock-type-extension-mechanism]`. Ion core does not interpret plugin-defined
fields; it dispatches by `type` and preserves them. The manifest mirrors this
(`ion-manifest.md` §Direct Dependency Plugin Extension).

### 6.6 The Eos Handoff (minimal pointer)

The build request carries, per atom dep, the **minimal pointer**
`(set, label, version, publish_czd)`. Eos resolves the atom via L1 `AtomSource`
and reads `dig`, the source revision, and content **from the self-describing atom
git object** (the signed publish payload + the peeled commit) — nothing else is
copied. `[handoff-atom-fields]`'s prior mandate to transmit `rev`/`id` is
**over-specification** and is removed: those are git-object-readable. (Whether eos
reads the object directly or ion materializes wire fields from it is an
implementation choice that does not change the lock or the atom contract.)

### 6.7 Determinism and Reproducibility

`[resolution-deterministic]` + the lock's deterministic serialization
(`BTreeMap` ordering on the nested `set`/`label` key) make the lock a
reproducible artifact. A run-twice property test is the standing guard against provider-order
leaks (§6.1).

### 6.8 Composition

The `[compose]` section determines how the root atom's build is **composed** —
which atom (or trivial nix expression, or static config) provides the
import/composition logic that wires the dependency graph into the
inputs eos's executor builds from. It is the third pillar of the lock
alongside `[sets]` and `[[deps]]`, and it flows the full length of the
pipeline:

- **`compose.use`** selects the composer. For the atom variant it names the composer
  atom by its **`publish_czd`** (§6.4) — the composer is itself a locked dependency,
  pinned like any other. `at` + `entry` give its version and composition entrypoint.
  The non-atom variants are `"nix"` (trivial import) and `"static"` (config).
- **`compose.args`** are action parameters (e.g. `system`) that flow
  manifest → lock → `ion-eos` → the eos `BuildRequest`'s `action_params`
  (`ion-eos-contract.md §Compose.args Flow`). Ion captures them in the lock; eos's
  executor consumes them. This is the L4 side of eos-sad §6.5's `action_params`.

> **Note (2026-07-05; amended 2026-07-07):** This section's terminology
> matches the substrate model — no evaluation stage; `action_params`,
> not `eval_args` (eos-sad.md §6.5). The compose _object model_
> (`compose.use`/`compose.as.nix`) described an evaluator-shaped
> composer and is **dead** per
> [ADR-0006](../adr/0006-execution-as-the-primitive.md) §3 (the
> evaluator is removed, no legacy path); its successor is the
> manifest/lock redesign. See
> [ADR-0005](../adr/0005-hermetic-transactional-composition.md) and
> [htc-sad.md](htc-sad.md).

### 6.9 Transport and Security

Ion is a client of the eos daemon, not a networked service itself. `ion-eos`
connects over **Cap'n Proto RPC** — in v1 a **Unix domain socket** at a well-known
path; the daemon returns an `EosDaemon` bootstrap capability on connect. The build
request is submitted through that capability, and progress is streamed back over
**capability-based** references (`BuildJob`), which are exercised and then released.
The wire schema, capability model, session lifecycle, and transport evolution are
defined normatively in `eos-network-protocol.md`; the semantic contract (what ion
transmits, what eos expects) is `ion-eos-contract.md`. Authentication is the
transport layer's concern (the daemon's auth middleware), not ion's — ion holds
only the capabilities the daemon grants it, and capability possession _is_ the
authorization (drop the capability, drop the access).

### 6.10 The Composite Plugin Seam (render/apply, ADR-0009)

_Added 2026-07-30. A different plugin axis from §6.5's lock dependency-type
plugins — this one governs how a composite crosses out of the formal
core, not what a manifest may depend on._ Below the seam, everything is
content-addressed and verified; above it, a plugin acts in an open world
that cannot be closed. The seam is trust-safe by construction because a
plugin can only be one of two things: a **rendering** (`Composite →
Rendering`, pure, no trust weight — producing files or a description) or
an **apply** (a trial, yielding a signed record — ADR-0006's `execute`,
of which build is already the built-in `do`; deploy-apply is one more
variant). There is no third thing a plugin can do, and in particular it
can never produce intent.

#### [plugin-seam-at-composite]

The plugin boundary is the composite. A plugin is either a **rendering**
(pure, no trust weight) or an **apply** (a trial, yielding a signed
record). There is no third thing it can be, so it can never produce
intent.

```claim
kind: requirement
evaluator: none
```

### 6.11 Blast Radius Preview (ADR-0009)

_Added 2026-07-30._ Config is what users spend their attention on, and
composition-model.md §7's override algebra already computes the readout
this requirement asks ion-cli to surface: the substitution operator's
validity is checked per consumer before it is applied, so the set of
affected consumers — closure size, how much substitutes, how much
rebuilds — is knowable before the user commits to a change, not
discovered after a build runs. Without this readout, *decouple doing
from building* (§2.6) is true of the architecture but invisible to the
user, who is still surprised by a build, just later than before.

#### [blast-radius-visibility]

Before committing an override or a dependency add, ion-cli MUST report
its blast radius: closure size, how much substitutes, how much builds.

```claim
kind: requirement
evaluator: none
```

### 6.12 Closure Fault Loop (ADR-0009)

_Added 2026-07-30._ Execution Model §4.4's graceful-degradation loop
already gives closure faults a policy (denied and logged with the name
that missed, growing `J`); what has not been stated is that ion-cli owns
turning that log entry into a user-visible repair path rather than a
bare error. A closure fault is an observation, not an exception —
Run → fault → repair → refine is the pathway a user walks, symmetric with
§6.11's blast-radius preview on the other side of the same commit.

#### [closure-faults-user-loop]

A closure fault is denied and logged with the name that missed, and
ion-cli MUST present it as a repair step (run → fault → repair →
refine), never a bare error.

```claim
kind: requirement
evaluator: none
```

## 7. The Lock Schema

Keyed by `Either<AtomId, Name>` where `AtomId` nests `set` then `label`; atom
entries sort lexicographically on `set` then `label`. A representative atom
entry:

```toml
[sets.<anchor>]
tag     = "auth-set"            # human-readable alias (required)
mirrors = ["git@…/atoms", "https://mirror/…"]

[[deps]]
type        = "atom"            # core; plugins register other types
set         = "<anchor>"        # AtomId component; → [sets.<anchor>]
label       = "auth-service"    # AtomId component
version     = "1.4.2"           # atom-core (L1)
publish_czd = "12207a…"         # atom-core (L1) — the pin; store-keyed via blake3
requires    = ["1220b3…", "1220c9…"]   # ion extension — publish_czd of each direct dep
direct      = true              # ion extension — declared vs transitive
```

- **Atom core** (`version`, `publish_czd`) is L1's `[lock-entry-sufficient]`; ion
  inherits it unchanged. No `id`, no `rev`, no `dig` (`dig` lives in the signed
  payload, verified by peel — atom-sad §6.5).
- **Ion extensions** (`requires`, `direct`) record the graph; `requires` entries are
  the `publish_czd` of each direct dependency.
- **Cross-references** (`requires`, `owner`, `compose.use`) name an atom by its
  `publish_czd` (§6.4).
- **`[sets]` → mirrors** (with the required `tag`) is shared with L1 (atom-sad §1.5).

## 8. Failure Modes

| #   | Failure                         | Behavior                                                                |
| :-- | :------------------------------ | :---------------------------------------------------------------------- |
| 8.1 | Unsatisfiable constraints       | `[resolution-unsolvable-diagnostic]` — "because…" explanation, no lock  |
| 8.2 | Provider returns unstable order | determinism violation — forbidden (§6.1); caught by run-twice test      |
| 8.3 | Cross-reference dangling        | a `requires`/`owner` pair with no matching `set`/`label` entry → reject |
| 8.4 | Unknown plugin `type`           | core dispatches by `type`; unregistered type → error, not silent drop   |
| 8.5 | Upstream metadata advanced      | non-fatal — warn + optional `publish_czd` bump (atom-sad §6.4)          |

## 9. Known Gaps and Future Explorations

| #   | Gap                                                                 | Notes                                                                 |
| :-- | :------------------------------------------------------------------ | :-------------------------------------------------------------------- |
| 1   | Multi-version coexistence + MaxSAT optimality                       | roadmap; would justify a full SAT solver over PubGrub                 |
| 2   | Plugin extension API surface (how a plugin registers a type/fields) | mechanism named (`[lock-type-extension-mechanism]`); concrete API TBD |

## 10. Scope Boundaries

Out of scope for ion:

- **Atom identity, publishing, storage** (L1) — ion consumes the contract.
- **Build execution, scheduling, the artifact store** (L3).
- **Version semantics** — `VersionScheme` is an L1 trait; concrete ecosystem
  adapters live above ion.
- **The atom-core lock fields** — defined by L1; ion inherits, never redefines.

## Appendix A: Terminology

| Term            | Definition                                                        |
| :-------------- | :---------------------------------------------------------------- |
| Manifest        | Human-authored direct-dependency declaration with version ranges  |
| Lock            | Reproducible pin of the fully resolved transitive graph           |
| AtomId          | Digest of the pre-signature `(anchor, label)` preimage (from L1); the lock nests its dep key by `set` then `label` |
| Atom core       | The L1-defined lock entry fields `{version, publish_czd}`         |
| Ion extension   | Ion-layer lock fields (`requires`, `direct`) over the atom core   |
| Cross-reference | An intra-lock atom reference, named by the target's `publish_czd` |
| Plugin type     | A non-atom dep `type`, registered via the extension mechanism     |
| Build request   | The serialized lock handed to eos (atom pointers)                 |

## Appendix B: Crate Map

| Layer | Crate          | Kind           | Purpose                                                     |
| :---- | :------------- | :------------- | :---------------------------------------------------------- |
| L4    | `ion-manifest` | Implementation | Parse manifest; direct-dep type dispatch (atom + plugins)   |
| L4    | `ion-resolve`  | Implementation | Constraint solver; transitive single-version resolution     |
| L4    | `ion-lock`     | Implementation | Lock schema + (de)serialization; `DepMap` keyed by `AtomId` |
| L4    | `ion-eos`      | Bridge         | Lock → eos `BuildRequest` handoff                           |
| L4    | `ion-cli`      | Implementation | Developer entry point                                       |

## Appendix C: Specification Cross-Reference

| SAD Section                | Governing Specification                                                                                                                              |
| :------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------------------- |
| §4.1 Resolve               | [ion-resolution.md](../specs/ion-resolution.md) §Resolution Properties                                                                               |
| §4.2 Handoff               | [ion-eos-contract.md](../specs/ion-eos-contract.md)                                                                                                  |
| §6.1 Resolution Properties | [ion-resolution.md](../specs/ion-resolution.md) `[resolution-*]`                                                                                     |
| §6.3 Lock Model            | [lock-file-schema.md](../specs/lock-file-schema.md); [atom-sourcing.md](../specs/atom-sourcing.md) `[lock-entry-sufficient]`                         |
| §6.4 Cross-Reference       | [lock-file-schema.md](../specs/lock-file-schema.md) `[lock-requires-closure]`, `[lock-owner-closure]`, `[lock-compose-closure]`                      |
| §6.5 Plugin Extension      | [lock-file-schema.md](../specs/lock-file-schema.md) `[lock-type-extension-mechanism]`; [ion-manifest.md](../specs/ion-manifest.md) §Plugin Extension |
| §6.6 Eos Handoff           | [ion-eos-contract.md](../specs/ion-eos-contract.md) `[handoff-atom-fields]`                                                                          |
| §7 Lock Schema             | [lock-file-schema.md](../specs/lock-file-schema.md)                                                                                                  |

## Appendix D: Known Specification Drift

None outstanding. The lock-layer realignment to this SAD has been applied across
`lock-file-schema.md` (no hashed `id`/`rev`; `publish_czd` cross-references; the
nested `set`/`label` key), `ion-resolution.md` (`AtomDep` without `cad`; `requires` over
`publish_czd`; determinism ordering), `ion-manifest.md` (`owner` over `publish_czd`),
`ion-eos-contract.md` (minimal-pointer handoff), and `layer-boundaries.md`
(`blake3(publish_czd)` addressing). The schema `version` is `0` and the anchor lock
field is named `set`, consistently across the lock and resolution specs.

## Appendix E: Stale Documentation

None outstanding. (The row previously here — claiming `ion-resolution.md`
§Lock File Schema still showed a `cad`/`AtomDigest` example — was itself
stale: `ion-resolution.md`'s informative schema already shows `publish_czd`,
no `cad`, consistent with SAD §7. Removed 2026-07-05.)
