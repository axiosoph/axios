# SPEC: Ion Manifest

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

**Problem Domain:** Ion is the atom-native adapter: a manifest format,
lock file schema, and resolution engine for consuming and publishing
atoms via eos backends. This spec constrains the manifest schema —
what a valid `atom.toml` file MUST contain, what it MAY contain, and
the behavioral contracts between manifest declarations and the
resolution/lock pipeline.

**Model Reference:**
[publishing-stack-layers.md](../models/publishing-stack-layers.md)

**Related Specs:**

- [atom-sourcing.md](atom-sourcing.md) — mirror validation, lock entry
  requirements, sourcing pipeline invariants
- [atom-transactions.md](atom-transactions.md) — claim/publish
  transactions, atom identity model
- [git-storage-format.md](git-storage-format.md) — ref layout, object
  format, store semantics
- [ion-resolution.md](ion-resolution.md) — version semantics, SAT
  resolution, lock production

**Criticality Tier:** Medium — manifest validity is the entry point for
all ion operations. Malformed manifests propagate errors throughout the
resolution and publish pipeline. The plugin extension point is
safety-critical in that untrusted plugins could inject malicious
references.

## Concepts

**Manifest**: A TOML file named `atom.toml` that declares an atom's
identity, dependencies, composer configuration, and optionally,
non-atom dependencies via plugins. It is the user-facing interface for
declaring what an atom is and what it needs.

**Package section** (`[package]`): Required. Declares the atom's label,
version, and optional metadata (description, sets).

**Set**: A named group of mirrors serving atoms from the same atom-set.
Declared under `[package.sets]`. Each set maps to one or more mirror
URLs (or the local sentinel `::`). Sets are the unit of mirror
validation per atom-sourcing.md `[set-anchor-bijection]`.

**Composer** (`[compose]`): Required. Declares how this atom is
evaluated. Either references another atom that provides import logic
(`compose.with`), indicates a trivial Nix expression (`compose.as.nix`),
or declares a static configuration atom (`compose.as.static`).

**Atom dependencies** (`[deps.from.<set>]`): Dependencies on other atoms,
grouped by the set they are sourced from. Each entry is a label with a
version constraint.

**Direct dependencies** (`[deps.direct.<plugin>]`): Non-atom dependencies
resolved by plugins. The manifest section schema is plugin-defined. Ion
core treats these as extension points with a minimal contract.

## Constraints

### Type Declarations

```
TYPE  Label       = String                    -- verified atom name (alphanumeric + hyphen)
TYPE  Tag         = String                    -- verified set name (same character rules as Label)
TYPE  Version     = semver::Version           -- semantic version (MAJOR.MINOR.PATCH[-pre][+build])
TYPE  VersionReq  = semver::VersionReq        -- semver range (e.g., "^1.0.0", ">=2,<3")

TYPE  SetMirror   = Local                     -- the "::" sentinel
              | Url(gix::Url)                 -- a network-accessible mirror

TYPE  AtomSet     = Singleton(SetMirror)      -- single mirror
              | Mirrors(Set<SetMirror>)       -- multiple mirrors for redundancy

TYPE  Atom = {
        label:       Label,                   -- REQUIRED
        version:     Version,                 -- REQUIRED
        description: Option<String>,          -- OPTIONAL
        sets:        Map<Tag, AtomSet>,       -- OPTIONAL (required if deps.from is non-empty)
      }

TYPE  AtomReq = {
        version:     VersionReq,              -- REQUIRED
      }

TYPE  Dependency = {
        from:        Map<Tag, Map<Label, VersionReq>>,  -- atom deps by set
        direct:      Map<PluginTag, PluginData>,        -- non-atom deps by plugin
      }

TYPE  Compose     = With(AtomComposer)        -- another atom provides import logic
              | As(TrivialAtom)               -- self-contained evaluation

TYPE  TrivialAtom = Nix(PathBuf)              -- nix expression at given path
              | Static(Config)                -- static configuration atom

TYPE  Manifest = {
        package:     Atom,                    -- REQUIRED
        compose:     Compose,                 -- REQUIRED
        deps:        Dependency,              -- OPTIONAL (defaults to empty)
      }
```

### Invariants

**[manifest-required-sections]**: A valid `atom.toml` MUST contain
both a `[package]` section and a `[compose]` section. The `[deps]`
section is OPTIONAL and defaults to empty (no dependencies).
`VERIFIED: unverified`

**[package-identity-required]**: The `[package]` section MUST contain
a `label` field (a valid `Label`) and a `version` field (a valid
semantic version as defined by the [Semantic Versioning 2.0.0
specification](https://semver.org)). The `description` field is
OPTIONAL.
`VERIFIED: unverified`

**[set-declaration-completeness]**: Every set name referenced in
`[deps.from.<set>]` MUST have a corresponding entry in
`[package.sets]`. A manifest that references an undeclared set MUST
be rejected during validation.
`VERIFIED: unverified`

**[set-mirror-minimum]**: Each set declared in `[package.sets]` MUST
contain at least one mirror (either a URL or the local sentinel `::`).
`VERIFIED: unverified`

**[composer-set-reference]**: If the composer uses the `with` variant,
the `from` field MUST reference a set declared in `[package.sets]`.
The referenced atom MUST be resolvable from that set.
`VERIFIED: unverified`

**[version-constraint-syntax]**: Version constraints in
`[deps.from.<set>]` MUST use semantic versioning range syntax as
defined by the `semver` crate (compatible with the node-semver range
grammar). Ion's `[version-total-order]` implementation (per
atom-sourcing.md) is semver. Examples: `"^1.0.0"`, `">=2.0,<3.0"`,
`"=1.2.3"`, `"*"`.
`VERIFIED: unverified`

**[deny-unknown-fields]**: A valid manifest MUST reject any fields not
defined by the manifest schema or by registered plugins. This prevents
silent misconfiguration from typos or deprecated fields.
`VERIFIED: unverified`

**[label-version-uniqueness]**: Within a single atom-set, the pair
(label, version) MUST uniquely identify an atom snapshot. This is a
consequence of atom-transactions.md atom identity, restated here for
manifest clarity: publishing a new snapshot under an existing
(label, version) pair is forbidden.
`VERIFIED: unverified`

### Direct Dependency Plugin Extension

**[plugin-extension-point]**: Ion MUST support an extension mechanism
for non-atom dependencies. Each plugin is identified by a tag (e.g.,
`nix`) and occupies its own namespace under `[deps.direct.<plugin>]`.
The schema within each plugin namespace is defined by the plugin, not
by ion core.
`VERIFIED: unverified`

**[plugin-lock-contract]**: A direct dependency plugin MUST produce
lock entries containing at minimum:

| Field  | Type   | Purpose                                |
| :----- | :----- | :------------------------------------- |
| `name` | String | Unique identifier within plugin scope  |
| `url`  | URL    | Fetch target for the resolved resource |
| `hash` | String | Integrity verification digest          |

The hash format is plugin-defined (e.g., NixHash for the nix plugin,
SHA256 for others). Ion core does not interpret the hash — it captures
it in the lock and passes it through.
`VERIFIED: unverified`

**[plugin-lock-type-tag]**: Each lock entry produced by a plugin MUST
carry a type tag that identifies both the plugin and the specific
fetch strategy (e.g., `"nix"`, `"nix+git"`, `"nix+tar"`,
`"nix+build"`). This tag enables eos to select the correct fetch and
verification strategy without understanding ion's plugin mechanism.
`VERIFIED: unverified`

**[plugin-owner-tracking]**: When a plugin dependency originates from
a transitive atom dependency (not the root manifest), the lock entry
SHOULD include an `owner` field containing the owning atom's
`publish_czd` (a Czd). This enables dependency
graph reconstruction and targeted re-resolution when the owning atom
updates.
`VERIFIED: unverified`

**[plugin-resolution-locality]**: Plugin resolution MUST execute
within ion (or the plugin itself). Eos MUST NOT be required to
understand the plugin's resolution logic. Eos consumes only the lock
output: type tag, name, URL, and hash.
`VERIFIED: unverified`

### Transitions

**[manifest-parse]**: Ion MUST parse and validate `atom.toml` before
any resolution or publish operation.

- **PRE**: An `atom.toml` file exists at the atom root.
- **POST**: A validated `Manifest` value exists. All invariants
  (`[manifest-required-sections]`, `[set-declaration-completeness]`,
  `[composer-set-reference]`, `[deny-unknown-fields]`) hold. Invalid
  manifests MUST be rejected with a diagnostic error.
  `VERIFIED: unverified`

**[manifest-add-dep]**: Adding a dependency via CLI (e.g., `eka add`)
MUST update the manifest and trigger re-resolution.

- **PRE**: A valid manifest exists. The user specifies a URI (label,
  optional URL, optional version constraint, optional set tag).
- **POST**: The `[deps.from.<set>]` section contains the new
  dependency. If the set was not previously declared, it is added to
  `[package.sets]`. The lock file is updated via the resolution
  pipeline. The manifest is re-validated after modification.
  `VERIFIED: unverified`

**[manifest-add-mirror]**: Adding a mirror to an existing set MUST
validate the mirror's anchor against the set's established anchor.

- **PRE**: The set exists in `[package.sets]`. The new mirror is a
  valid URL.
- **POST**: The mirror is appended to the set's mirror list. The
  mirror singleton is promoted to a mirror array if needed. The
  anchor of the new mirror has been verified consistent with existing
  mirrors per atom-sourcing.md `[set-anchor-bijection]`.
  `VERIFIED: unverified`

### Forbidden States

**[no-orphaned-set]**: A set declared in `[package.sets]` that is
referenced by NEITHER `[deps.from]` NOR `[compose.with]` is NOT an
error — sets MAY be declared for future use or for mirror management.
This is explicitly NOT forbidden.
`VERIFIED: unverified`

**[no-undeclared-set-reference]**: A `[deps.from.<set>]` section MUST
NOT reference a set not declared in `[package.sets]`. Corollary of
`[set-declaration-completeness]`.
`VERIFIED: unverified`

**[no-self-dependency]**: An atom MUST NOT declare a dependency on
itself (same label from the local set). Implementations MUST detect
and reject self-referential dependencies.
`VERIFIED: unverified`

### Behavioral Properties

**[manifest-roundtrip]**: Parsing a valid manifest to the `Manifest`
type and serializing it back to TOML MUST produce a semantically
equivalent document. Field ordering and formatting MAY differ (per
`toml_edit` formatting rules), but the deserialized result MUST be
identical.

- **Type**: Safety
  `VERIFIED: unverified`

**[manifest-validation-total]**: Manifest validation MUST catch all
constraint violations in a single pass and report all errors, not
just the first one. This enables users to fix multiple issues without
repeated parse-fix-parse cycles.

- **Type**: Liveness (usability)
  `VERIFIED: unverified`

**[plugin-isolation]**: Plugins MUST NOT modify ion core manifest
sections (`[package]`, `[compose]`, `[deps.from]`). A plugin's scope
is strictly limited to its own namespace under `[deps.direct.<tag>]`
and its corresponding lock entries.

- **Type**: Safety
  `VERIFIED: unverified`

## Manifest Schema (Informative)

This section provides a concrete example of a valid `atom.toml` for
reference. This is informative, not normative — the constraints above
are authoritative.

```toml
[package]
label = "my-atom"
version = "1.0.0"
description = "A sample atom for demonstration"

[package.sets]
company-atoms = "git@github.com:our-company/atoms"
local-atoms = "::"
nixpkgs = ["https://github.com/NixOS/nixpkgs", "https://mirror.example.com/nixpkgs"]

[compose.with.atom-nix]
from = "company-atoms"

[deps.from.company-atoms]
other-atom = "^1.0.0"
auth-service = ">=2.0,<3.0"

[deps.from.local-atoms]
shared-config = "*"

[deps.direct.nix]
# Plugin-defined schema — ion core does not interpret these fields
openssl.url = "https://www.openssl.org/source/openssl-3.1.0.tar.gz"
nixpkgs.git = "https://github.com/NixOS/nixpkgs"
nixpkgs.version = ">=24.05"
```

## Verification

| Constraint                   | Method      | Result | Detail                                                             |
| :--------------------------- | :---------- | :----- | :----------------------------------------------------------------- |
| manifest-required-sections   | agent-check | pass   | Two required sections, well-defined defaults for optional          |
| package-identity-required    | agent-check | pass   | label + version are minimal identity; consistent with AtomId model |
| set-declaration-completeness | agent-check | pass   | Forward reference check; no contradiction with optional sets       |
| set-mirror-minimum           | agent-check | pass   | Empty set is useless; at least one mirror is constructive          |
| composer-set-reference       | agent-check | pass   | Composer must resolve; unresolvable composer is a fatal error      |
| version-constraint-syntax    | agent-check | pass   | Uses semver crate grammar; well-defined and widely understood      |
| deny-unknown-fields          | agent-check | pass   | Prevents misconfiguration; compatible with plugin extension        |
| label-version-uniqueness     | agent-check | pass   | Restated from atom-transactions; no new constraint                 |
| plugin-extension-point       | agent-check | pass   | Namespaced under deps.direct; no collision with core sections      |
| plugin-lock-contract         | agent-check | pass   | Minimal (name, url, hash) is sufficient for fetch+verify           |
| plugin-lock-type-tag         | agent-check | pass   | Enables eos dispatch without plugin knowledge                      |
| plugin-owner-tracking        | agent-check | pass   | SHOULD, not MUST; backward compatible                              |
| plugin-resolution-locality   | agent-check | pass   | Keeps eos clean; consistent with ion-as-adapter model              |
| manifest-roundtrip           | agent-check | pass   | Standard serde/toml_edit property; no contradiction                |
| manifest-validation-total    | agent-check | pass   | Usability property; no safety contradiction                        |
| plugin-isolation             | agent-check | pass   | Prevents plugin from corrupting core state                         |
| no-self-dependency           | agent-check | pass   | Self-reference is always a cycle; trivially detectable             |

All constraints are internally consistent. No contradictions detected
with atom-sourcing.md, atom-transactions.md, or git-storage-format.md
invariants. Agent-level verification (Tier 1).

## Implications

1. **PoC migration**: The PoC's `[deps.direct.nix]` section maps
   directly to the plugin model. The existing `DirectDeps` struct
   becomes the nix-fetcher plugin. The `Dep::Nix*` lock variants
   already carry the correct type tags.

2. **New plugin development**: The minimal lock contract
   (name, url, hash, type tag) is intentionally low-barrier. A new
   backend (e.g., Guix) can implement a plugin by:
   - Defining its manifest schema under `[deps.direct.guix]`
   - Implementing resolution that produces (name, url, hash) lock entries
   - Registering a type tag (e.g., `"guix+fetch"`)

3. **Eos consumption**: Eos reads the lock, dispatches on type tag,
   fetches from URL, and verifies hash. No ion plugin knowledge needed.

4. **Testing strategy**: Property-based tests should:
   - Generate random valid manifests and verify roundtrip
   - Generate manifests with undeclared set references and verify rejection
   - Generate plugin lock entries and verify eos can consume them
     without plugin knowledge

5. **Open questions**:
   - Should the plugin registry be static (compile-time) or dynamic
     (runtime loadable)? The PoC is compile-time. Dynamic would enable
     third-party plugins but adds complexity.
   - Should `[compose]` support plugin-defined composers, or is
     `with`/`as` sufficient?
