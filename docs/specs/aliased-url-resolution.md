# SPEC: Aliased URL Resolution

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

**Problem Domain:** User-facing tools frequently need to resolve short alias
names (e.g., `+gh/owner/repo`) into full URL strings (e.g.,
`https://github.com/owner/repo`). This pattern recurs across package
managers, build systems, and source reference formats. Alurl is a
standalone library that provides alias detection and expansion as a
generic, reusable concern — decoupled from any specific URL parser,
transport layer, or application grammar.

**Model Reference:** None. Alurl's behavior is a pure function
(string → string) with no hidden state or protocol ordering. Formal
modeling is not warranted; normative constraints are sufficient.

**Criticality Tier:** Medium — alurl's output feeds into URL resolution
pipelines. Incorrect expansion could direct users to unintended sources.
Alias stacking introduces termination and cycle risks.

## Scope

Alurl owns **alias detection and expansion**. Nothing else.

**In scope:**

- The `+` sigil convention for marking aliases
- Alias name extraction and path splitting
- The `AliasResolver` trait (abstract configuration interface)
- Recursive resolution (alias values that are themselves aliases)
- Cycle detection and depth limiting
- Deterministic, side-effect-free expansion

**Out of scope:**

- URL classification (scheme, SCP, credentials, port) — downstream
  concern (e.g., gix-url)
- Path interpretation (absolute, relative) — consumer's concern
- Application-specific delimiters (e.g., atom's `::`) — consumer's
  concern
- Alias storage, configuration format, or persistence — consumer
  provides an `AliasResolver` implementation
- Network access or I/O — alurl is a pure computation library

## Constraints

### Type Declarations

```
TYPE  AliasName  = String { UAX #31 Identifier — XID_Start + XID_Continue }   (alurl)
  -- The alias key. Uses the same UAX #31 rules as atom-id's Identifier,
  -- but validated inline (no dependency on atom-id). Alurl uses
  -- `unicode-ident` directly for XID_Start / XID_Continue checks.
  -- Examples: gh, nixpkgs, work, myOrg
  -- Non-examples: my.alias (dots), my-alias (hyphens), 123 (digit start)

TYPE  AliasPath  = String { arbitrary, non-empty }                            (alurl)
  -- The path suffix following the alias name.
  -- Opaque to alurl — no validation, no normalization.
  -- Examples: owner/repo, path/to/thing

TYPE  AliasedUrl = Expanded { alias: AliasName, url: String }                 (alurl)
               | Raw(String)
  -- EITHER the input was aliased and has been expanded,
  -- OR the input was not aliased and is passed through as-is.

TYPE  AliasResolver = trait {                                                 (alurl)
        type Error: std::error::Error;
        fn resolve(&self, name: &str) -> Result<String, Self::Error>;
      }
  -- Abstract configuration interface. The resolver maps alias names
  -- to URL templates. How aliases are stored (TOML, JSON, env vars,
  -- hardcoded) is the implementor's concern.

TYPE  ResolveError = AliasNotFound(String)                                    (alurl)
                   | CycleDetected { chain: Vec<String> }
                   | ResolverError(Box<dyn Error>)
  -- Errors that can occur during alias resolution.
  -- Note: DepthExceeded is not a required variant. Cycle detection
  -- is the primary termination guarantee. A depth limit MAY be added
  -- as defense-in-depth but is not mandated by this spec.
```

### Grammar

The alias sigil grammar, expressed in ABNF (RFC 5234):

```abnf
aliased-input  = alias / raw-input

alias          = "+" alias-name ["/" alias-path]
alias-name     = XID_Start *XID_Continue        ; UAX #31 Identifier
alias-path     = 1*(%x01-FF)                    ; non-empty, opaque

raw-input      = *(%x01-FF)                     ; anything not starting with "+"
```

### Invariants

**[sigil-required]**: An input string MUST be treated as an alias if
and only if it starts with the `+` character (U+002B). All other
inputs MUST be passed through as `AliasedUrl::Raw` without
modification.
`VERIFIED: unverified`

**[sigil-unambiguous]**: The `+` sigil MUST NOT collide with any
valid URL scheme, SCP notation, absolute file path, or relative file
path. This is guaranteed by construction: URL schemes end with `://`,
SCP uses `user@host:`, absolute paths start with `/`, and relative
paths start with a path character — none start with `+`.
`VERIFIED: agent-check`

**[alias-name-validated]**: The alias name (the segment between `+`
and the first `/`, or the entire string after `+` if no `/` is
present) MUST be a valid UAX #31 Identifier (XID_Start followed by
zero or more XID_Continue characters). An invalid alias name MUST
produce an error, not a fallback to raw.
`VERIFIED: unverified`

**[path-opaque]**: The alias path (everything after the first `/`
following the alias name) MUST be treated as an opaque string. Alurl
MUST NOT validate, normalize, or interpret it. The path is
concatenated to the resolved alias value with a `/` separator.
`VERIFIED: unverified`

**[expansion-deterministic]**: Given the same input and the same
`AliasResolver` state, `resolve()` MUST produce the same output.
Alias resolution MUST be a pure function of its inputs.
`VERIFIED: unverified`

**[resolution-terminates]**: Recursive alias resolution MUST
terminate. Alurl MUST enforce termination through cycle detection:
if the same alias name is encountered twice in a resolution chain,
resolution MUST fail with `CycleDetected`. Because the alias
configuration is finite and cycles are detected, non-cyclic chains
are guaranteed to terminate. A depth limit MAY be implemented as
defense-in-depth but is not required.
`VERIFIED: unverified`

**[recursive-transparent]**: If alias expansion produces a string
that starts with `+`, alurl MUST re-resolve the result as a new alias.
The caller MUST NOT need to loop — recursive resolution is alurl's
responsibility, not the caller's.
`VERIFIED: unverified`

**[raw-preserves-input]**: When the input does not start with `+`,
the returned `AliasedUrl::Raw` MUST contain the exact input string
with no modifications — no trimming, no normalization, no encoding
changes.
`VERIFIED: unverified`

**[expanded-preserves-alias]**: When the input is an alias, the
returned `AliasedUrl::Expanded` MUST include the **original** alias
name (the first alias in the chain, before recursive resolution).
This enables downstream consumers to provide diagnostic messages
referencing what the user typed.
`VERIFIED: unverified`

**[zero-deps]**: Alurl MUST have zero non-std external dependencies.
Alias detection and expansion is pure string processing. Unicode
validation MAY use `unicode-ident` (the same crate atom-id uses) as
the sole permitted dependency.
`VERIFIED: unverified`

**[no-io]**: Alurl MUST NOT perform any I/O (filesystem, network,
environment variables). All external state is provided through the
`AliasResolver` trait.
`VERIFIED: unverified`

### Transitions

**[classify-transition]**: Given an input string, alurl MUST classify
it as either an alias or a raw input in O(1) — a single check of
the first character.

- **PRE**: Input MUST be a non-empty string.
- **POST**: If the input starts with `+`, parse the alias name and
  optional path. If the alias name fails validation, return an error.
  If the input does not start with `+`, return `Raw(input)`.
  `VERIFIED: unverified`

**[resolve-transition]**: Given an alias input and an `AliasResolver`,
alurl MUST expand the alias.

- **PRE**: The input MUST have been classified as an alias (starts
  with `+`). The alias name MUST be a valid Identifier.
- **POST**: The resolver is called with the alias name. If the
  resolver returns a URL template string, the alias path (if any)
  is appended with a `/` separator. If the result starts with `+`,
  recursive resolution is applied (subject to cycle and depth
  guards). The final result is returned as `AliasedUrl::Expanded`.
  `VERIFIED: unverified`

### Forbidden States

**[no-silent-fallback]**: If an input starts with `+` but the alias
name is invalid or the resolver returns an error, alurl MUST NOT
silently fall back to `Raw`. It MUST return an error. An explicit `+`
is an unambiguous declaration of alias intent — failure to resolve is
an error, not a suggestion.
`VERIFIED: unverified`

**[no-partial-expansion]**: Alurl MUST NOT return an `AliasedUrl::Expanded`
whose `url` field still contains an unresolved `+`-prefixed alias.
Recursive resolution MUST complete fully or fail entirely.
`VERIFIED: unverified`

**[no-alias-in-metadata]**: Alurl types (aliases, alias names)
MUST NOT appear in persisted protocol state, signed payloads, or
stored metadata. Aliases are a user convenience — all persistent
references MUST use fully expanded URLs. (This constraint is
enforced by consumers, but alurl's API design SHOULD make it natural
by separating `AliasedUrl::Expanded.url` as the resolved artifact.)
`VERIFIED: unverified`

### Behavioral Properties

**[resolution-complexity]**: Single-level alias resolution MUST be
O(1) for classification and O(1) for resolver lookup (assuming the
resolver is hash-based). Recursive resolution MUST be O(d) where d
is the chain depth, bounded by the alias configuration size.

- **Type**: Performance
  `VERIFIED: unverified`

**[error-diagnostic]**: All error types MUST carry sufficient
information for diagnostic messages. `CycleDetected` MUST include
the full chain of alias names that formed the cycle.
`AliasNotFound` MUST include the alias name that failed. If a
`DepthExceeded` variant is implemented (see `[resolution-terminates]`),
it SHOULD include both the depth reached and the maximum allowed.

- **Type**: Usability
  `VERIFIED: unverified`

**[expansion-concatenation]**: Alias expansion MUST follow the
concatenation rule: `resolve(name) + "/" + path`. The `/` separator
MUST be inserted between the resolved value and the path if and only
if a path is present. Alurl MUST NOT normalize the result — no
deduplication of double slashes, no trailing slash removal. The
resolver's output is treated as opaque.

- **Type**: Safety
  `VERIFIED: unverified`

**[trailing-slash-warning]**: If the resolved alias value ends with
`/`, alurl SHOULD emit a warning (via the return type or a callback)
indicating that the alias value may produce a double-slash in the
expanded URL. This is a non-normative diagnostic aid — the expansion
result is still valid.

- **Type**: Usability
  `VERIFIED: unverified`

## Verification Pipeline

### Resolution Examples

| Input                 | Alias Config              | Result                                                                  |
| :-------------------- | :------------------------ | :---------------------------------------------------------------------- |
| `+gh/owner/repo`      | `gh → https://github.com` | `Expanded { alias: "gh", url: "https://github.com/owner/repo" }`        |
| `+gh`                 | `gh → https://github.com` | `Expanded { alias: "gh", url: "https://github.com" }`                   |
| `+work/myproject`     | `work → +gh/myorg`        | `Expanded { alias: "work", url: "https://github.com/myorg/myproject" }` |
| `+a`                  | `a → +b`, `b → +a`        | `Err(CycleDetected { chain: ["a", "b", "a"] })`                         |
| `+unknown/repo`       | (empty)                   | `Err(AliasNotFound("unknown"))`                                         |
| `https://example.com` | (any)                     | `Raw("https://example.com")`                                            |
| `/tmp/local/repo`     | (any)                     | `Raw("/tmp/local/repo")`                                                |
| `foo/bar/baz`         | (any)                     | `Raw("foo/bar/baz")`                                                    |
| `git@host:path`       | (any)                     | `Raw("git@host:path")`                                                  |
| `` (empty)            | (any)                     | Error (empty input)                                                     |

### Recursive Resolution Trace

For `+work/myproject` with config `{ work → "+gh/myorg", gh → "https://github.com" }`:

```
Step 1: input = "+work/myproject"
        → alias_name = "work", alias_path = "myproject"
        → resolve("work") = "+gh/myorg"
        → expanded = "+gh/myorg" + "/" + "myproject" = "+gh/myorg/myproject"
        → result starts with "+", recurse

Step 2: input = "+gh/myorg/myproject"
        → alias_name = "gh", alias_path = "myorg/myproject"
        → resolve("gh") = "https://github.com"
        → expanded = "https://github.com" + "/" + "myorg/myproject"
        → result does not start with "+"
        → return Expanded { alias: "work", url: "https://github.com/myorg/myproject" }
```

## Verification

**Verification methods:**

- `rustc` — Rust type system; if code compiles, constraint holds
- `cargo-dep` — Cargo.toml dependency audit; verified by `cargo check`
- `unit-test` — deterministic test in isolation
- `agent-check` — agent self-verification (weakest guarantee)

| Constraint               | Method      | Result   | Detail                                       | Phase |
| :----------------------- | :---------- | :------- | :------------------------------------------- | :---- |
| sigil-required           | unit-test   | pending  | `+` prefix → alias, else → raw               | 2     |
| sigil-unambiguous        | agent-check | **pass** | No valid URL/SCP/path starts with `+`        | —     |
| alias-name-validated     | unit-test   | pending  | UAX #31 validation on alias name             | 2     |
| path-opaque              | unit-test   | pending  | Path passed through without modification     | 2     |
| expansion-deterministic  | unit-test   | pending  | Same input + resolver → same output          | 2     |
| resolution-terminates    | unit-test   | pending  | Cycle detection terminates all chains        | 2     |
| recursive-transparent    | unit-test   | pending  | Stacked aliases resolve fully                | 2     |
| raw-preserves-input      | unit-test   | pending  | Raw output equals input exactly              | 2     |
| expanded-preserves-alias | unit-test   | pending  | Original alias name preserved in Expanded    | 2     |
| zero-deps                | cargo-dep   | pending  | Cargo.toml has only unicode-ident (if any)   | 2     |
| no-io                    | rustc       | pending  | No std::fs, std::net in source               | 2     |
| no-silent-fallback       | unit-test   | pending  | Invalid alias → error, not Raw               | 2     |
| no-partial-expansion     | unit-test   | pending  | No `+` prefix in Expanded.url                | 2     |
| no-alias-in-metadata     | rustc       | pending  | AliasedUrl not Serialize                     | 2     |
| resolution-complexity    | agent-check | pending  | O(d) bounded by config size                  | 2     |
| error-diagnostic         | unit-test   | pending  | Error types carry diagnostic info            | 2     |
| expansion-concatenation  | unit-test   | pending  | resolve(name) + "/" + path, no normalization | 2     |
| trailing-slash-warning   | unit-test   | pending  | Warning on alias value ending with /         | 2     |

**Coverage:** 2 agent-check, 13 unit-test, 1 cargo-dep, 2 rustc = **18 total**.

## Implications

### Scope Boundaries

This specification explicitly does NOT define:

- **URL parsing**: scheme detection, SCP classification, credential
  extraction — all handled by downstream URL parsers (e.g., gix-url)
- **Application grammars**: delimiters like `::` or `@` — handled by
  consumers (e.g., atom-uri)
- **Alias storage**: TOML configs, environment variables, hardcoded
  maps — the `AliasResolver` trait abstracts this away
- **Policy decisions**: whether to warn on non-HTTPS expansions,
  whether to allow credentials in expanded URLs — consumer's concern

### Integration Points

- **atom-uri** consumes alurl to resolve the source component of
  `[source \"::\"] label [\"@\" version]`
- **ion-resolve** provides an `AliasResolver` implementation backed
  by user configuration
- The `AliasedUrl::Expanded.alias` field enables ion-cli to provide
  diagnostic messages like "resolved +gh/owner/repo via alias 'gh'"
