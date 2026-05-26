# SPEC: Ion–Eos Contract

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

**Problem Domain:** Ion (the atom-native frontend) communicates with
eos (the store-based backend) to build, evaluate, and execute resolved
atoms. This spec constrains the communication contract between the
two: what ion sends, what eos expects, what each is responsible for,
and how capability negotiation works.

This spec is intentionally minimal — eos is still taking shape, and
over-constraining the contract before implementation experience would
be premature. It captures the invariants we know must hold and leaves
the API surface (HTTP, gRPC, IPC, etc.) as an open question.

**Related Specs:**

- [ion-manifest.md](ion-manifest.md) — manifest schema, plugin model
- [ion-resolution.md](ion-resolution.md) — resolution, lock production
- [atom-sourcing.md](atom-sourcing.md) — sourcing pipeline invariants

**Criticality Tier:** Medium — the handoff is a trust boundary. Ion
trusts the lock; eos trusts the artifacts it fetches and verifies.
Miscommunication at this boundary could lead to building the wrong
atoms or bypassing integrity checks.

## Concepts

**Handoff**: The point at which ion transfers a resolved dependency
graph (lock file) to eos for execution. After the handoff, ion's
job is done. Eos fetches, verifies, and builds.

**Backend**: A specific build system that eos delegates to (Nix, Guix,
Tvix, etc.). Each backend has its own evaluation semantics, store
format, and dependency model.

**Capability**: A declaration by an eos instance of what it can do:
which backends it supports, which atom `pkg` types it can build.

## Constraints

### Invariants

**[handoff-lock-sufficiency]**: The lock file produced by ion MUST be
the sole input to eos for dependency resolution. Eos MUST NOT require
access to ion's manifest, plugin state, or resolution history. The
lock file is the complete contract.
`VERIFIED: unverified`

**[handoff-atom-fields]**: For each locked atom dependency, eos MUST
receive at minimum: `anchor`, `label`, `version`, `czd` (publish
transaction digest), `dig` (atom snapshot digest), and the set-level
mirror mapping (anchor → mirrors). The `czd` identifies the publish
`CozMessage`, from which the claim chain is derivable. These fields
are sufficient for eos to: (1) locate a mirror via the anchor,
(2) fetch and verify the atom snapshot against `dig`, (3) validate
the ownership chain via the publish transaction at `czd`.
`VERIFIED: unverified`

**[handoff-plugin-fields]**: For each locked plugin dependency, eos
MUST receive at minimum: `type` (type tag), `name`, `url`, and `hash`.
Eos dispatches on the type tag to select the correct fetch and
verification strategy. Eos does NOT need to know which ion plugin
produced the entry.
`VERIFIED: unverified`

**[eos-verification-obligation]**: After fetching an atom, eos MUST
verify the artifact's integrity against the lock entry's cryptographic
fields: `dig` (atom snapshot digest) for content, and `czd` (publish
transaction digest) for ownership chain verification. For plugin
dependencies, eos MUST verify the fetched content against the `hash`
field using the hash algorithm implied by the type tag. Eos MUST NOT
execute unverified artifacts.
`VERIFIED: unverified`

**[eos-backend-agnosticism]**: Ion MUST NOT assume a specific eos
backend. The lock file format is backend-neutral — it does not contain
Nix derivation paths, Guix store paths, or any backend-specific build
instructions. Backend selection is eos's responsibility.
`VERIFIED: unverified`

**[compose-handoff]**: The lock file's `[compose]` section tells eos
which atom provides the import/evaluation logic for the root atom.
Eos MUST fetch and prepare the composer atom before evaluating the
root atom.
`VERIFIED: unverified`

### Capability Model

**[capability-advertisement]**: An eos instance SHOULD advertise its
capabilities to ion clients. At minimum, the capability set includes:

- **Supported backends**: Which build systems are available (e.g.,
  `nix`, `guix`, `tvix`).
- **Supported plugin types**: Which lock entry type tags eos can
  process (e.g., `nix`, `nix+git`, `nix+tar`, `nix+build`).

The advertisement mechanism (config file, API endpoint, out-of-band
documentation) is NOT constrained by this spec.
`VERIFIED: unverified`

**[capability-mismatch-handling]**: If the lock file contains a
plugin dependency type tag that eos does not recognize, eos MUST
reject the build with a clear error identifying the unsupported type.
Eos MUST NOT silently skip dependencies.
`VERIFIED: unverified`

### Transitions

**[build-request]**: Ion (or any frontend) submits a build request
to eos.

- **PRE**: A reconciled lock file exists. All entries have been
  validated by ion's resolution pipeline.
- **POST**: Eos has received the lock file and begins processing.
  The transport mechanism (file handoff, API call, etc.) is not
  constrained.
  `VERIFIED: unverified`

**[fetch-verify-build]**: Eos processes each lock entry.

- **PRE**: The lock file has been received and parsed.
- **POST**: For each dependency:
  (1) Eos selects a mirror (for atoms) or uses the URL (for plugin
  deps).
  (2) Eos fetches the artifact.
  (3) Eos verifies integrity against the lock entry's hash fields.
  (4) Eos makes the verified artifact available to the build.
  Failed verification MUST abort the build.
  `VERIFIED: unverified`

### Forbidden States

**[no-manifest-leakage]**: Eos MUST NOT read or depend on the
`atom.toml` manifest of the root atom. All information eos needs
is in the lock file. If eos needs data not in the lock, that is a
signal that the lock schema is incomplete — the fix is to extend the
lock, not to leak the manifest.
`VERIFIED: unverified`

**[no-unverified-execution]**: Eos MUST NOT execute, evaluate, or
import any artifact that has not passed integrity verification
against the lock entry's cryptographic fields.
`VERIFIED: unverified`

### Behavioral Properties

**[backend-substitutability]**: If the same atom is buildable by
two different eos backends (e.g., Nix and Guix), switching backends
MUST NOT require changes to the lock file. The lock file is backend-
neutral.

- **Type**: Safety
  `VERIFIED: unverified`

**[plugin-type-extensibility]**: Adding a new plugin type tag (e.g.,
`guix+fetch`) MUST NOT require changes to existing eos backends
that do not support it. Unsupported types are rejected per
`[capability-mismatch-handling]`, not silently processed.

- **Type**: Safety
  `VERIFIED: unverified`

## Verification

| Constraint                  | Method      | Result | Detail                                               |
| :-------------------------- | :---------- | :----- | :--------------------------------------------------- |
| handoff-lock-sufficiency    | agent-check | pass   | Lock is self-contained by construction                |
| handoff-atom-fields         | agent-check | pass   | Subset of lock-atom-entry-fields from ion-resolution  |
| handoff-plugin-fields       | agent-check | pass   | Subset of plugin-lock-contract from ion-manifest      |
| eos-verification-obligation | agent-check | pass   | Integrity before execution is non-negotiable          |
| eos-backend-agnosticism     | agent-check | pass   | Lock format has no backend-specific fields            |
| compose-handoff             | agent-check | pass   | Composer is in lock; eos processes it as a dep        |
| capability-advertisement    | agent-check | pass   | SHOULD; flexible mechanism                            |
| capability-mismatch-handling| agent-check | pass   | Fail-loud on unknown types                            |
| no-manifest-leakage         | agent-check | pass   | Clean separation; lock is the interface               |
| no-unverified-execution     | agent-check | pass   | Fundamental security property                          |
| backend-substitutability    | agent-check | pass   | Lock is backend-neutral by construction                |
| plugin-type-extensibility   | agent-check | pass   | Rejection is the safe default for unknown types        |

All constraints are internally consistent. No contradictions with
ion-manifest.md, ion-resolution.md, or atom-sourcing.md.

## Implications

1. **Transport mechanism**: This spec deliberately avoids constraining
   the transport (file path, HTTP, gRPC, Unix socket). Early
   implementations will likely use direct file system handoff (ion
   writes `atom.lock`, eos reads it). Future versions may define an
   API spec.

2. **Capability discovery**: The simplest capability model is: eos
   reads `atom.lock`, tries to process it, and fails loudly on
   unsupported type tags. Explicit capability advertisement can be
   added later without breaking this contract.

3. **Backend-specific deps in lock**: The lock already carries plugin
   deps with type tags. Eos dispatches on the tag. Adding a new
   backend (Guix) means: (a) write a new ion plugin that produces
   `guix+*` type tags, (b) teach eos to handle `guix+*` fetches.
   No changes to the contract itself.

4. **Open questions**:
   - Should eos report build results back to ion, or is it fire-and-
     forget? The current model assumes eos is invoked directly, not
     through ion. But if ion orchestrates builds, a result channel
     is needed.
   - Should the spec define an error reporting format for eos failures?
   - How does eos handle concurrent builds from the same lock file?
