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
`github.com/owner/repo`). This pattern recurs across package managers,
build systems, and source reference formats. Alurl is a standalone library
that provides structure-preserving alias detection and expansion as a
generic, reusable concern — decoupled from any specific URL parser,
transport layer, or application grammar.

**Model Reference:** None. Alurl's behavior is a pure function
(string → string) with no hidden state or protocol ordering. Formal
modeling is not warranted; normative constraints are sufficient.

**Criticality Tier:** Medium — alurl's output feeds into URL resolution
pipelines. Incorrect expansion could direct users to unintended sources.
Alias stacking introduces termination and cycle risks.

## Scope

Alurl owns **alias detection and expansion**. It understands enough URL
structure to locate aliases at the host position, but does not classify,
validate, or construct URLs.

**In scope:**

- The `+` sigil convention for marking aliases
- URL structure awareness: scheme, credentials, host position
- Alias name extraction and path splitting
- Structure-preserving substitution (prefix + resolved + separator + suffix)
- The `AliasMap` type (concrete alias mapping with resolution logic)
- The `AliasSource` trait (abstract config-loading interface)
- Recursive resolution (alias values that are themselves aliases)
- Cycle detection

**Out of scope:**

- Scheme inference or injection — alurl preserves the input structure
  as-is; if no scheme was provided, none is added
- URL validation, normalization, or construction (e.g., gix-url)
- Path interpretation (absolute, relative) — consumer's concern
- Application-specific delimiters (e.g., atom's `::`) — consumer's
  concern
- Alias storage, configuration format, or persistence — consumer
  provides an `AliasSource` implementation to load aliases into an
  `AliasMap`
- Network access or I/O — alurl is a pure computation library

## Constraints

### Type Declarations

```
TYPE  AliasName  = String { UAX #31 Identifier — XID_Start + XID_Continue }   (alurl)
  -- The alias key. Uses UAX #31 rules, validated inline via
  -- `unicode-ident` (no dependency on atom-id).
  -- Examples: gh, nixpkgs, work, myOrg
  -- Non-examples: my.alias (dots), my-alias (hyphens), 123 (digit start)

TYPE  AliasSuffix = Option<(char, String)>                                    (alurl)
  -- The separator character ("/" or ":") and everything after it.
  -- Absent for bare aliases like `+gh`. The separator is preserved
  -- in expansion output to maintain the input's transport semantics.
  -- The string portion MAY be empty (e.g., `+gh/` → separator="/",
  -- suffix=""). Opaque to alurl — no validation, no normalization.
  -- Examples: ("/", "owner/repo"), (":", "owner/repo"), ("/", "")

TYPE  AliasedUrl = Expanded { alias: AliasName, url: String }                 (alurl)
               | Raw(String)
  -- EITHER the input contained an alias at a valid host position
  -- and has been expanded via structure-preserving substitution,
  -- OR the input contained no alias and is passed through as-is.

TYPE  AliasMap = struct(HashMap<String, String>)                              (alurl)
  -- Newtype wrapper around a hash map. Keys are alias names, values
  -- are host strings (NOT full URLs — values SHOULD NOT contain
  -- schemes). Alurl owns all resolution logic against this map:
  -- lookup, recursive expansion, and cycle detection.
  -- Primary method: AliasMap::resolve(&self, input: &str)
  --   -> Result<AliasedUrl, ResolveError>

TYPE  AliasSource = trait {                                                   (alurl)
        type Error: std::error::Error;
        fn load(&self) -> Result<AliasMap, Self::Error>;
      }
  -- Abstract configuration loading interface. The implementor reads
  -- aliases from whatever source (TOML, JSON, env vars, hardcoded)
  -- into an AliasMap. Resolution logic is NOT the implementor's
  -- concern — alurl handles that once it has the map.

TYPE  ResolveError = AliasNotFound(String)                                    (alurl)
                   | InvalidAliasName(String)
                   | CycleDetected { chain: Vec<String> }
  -- Errors that can occur during alias resolution.
  -- InvalidAliasName: `+` at host position but name fails UAX #31.
  -- Cycle detection is the primary termination guarantee. A depth
  -- limit MAY be added as defense-in-depth but is not mandated.
  -- Loading errors are AliasSource's concern, not alurl's.
```

### Grammar

The alias grammar with URL structure awareness, in ABNF (RFC 5234):

```abnf
aliased-input  = [prefix] alias [separator suffix]
               / raw-input

prefix         = scheme "://"                        ; explicit scheme
               / scheme "://" credentials "@"        ; scheme + credentials
               / credentials "@"                     ; credentials only

scheme         = ALPHA *(ALPHA / DIGIT / "+" / "-" / ".")
credentials    = user [":" pass]
user           = 1*(VCHAR)                           ; visible characters
pass           = 1*(VCHAR)

alias          = "+" alias-name
alias-name     = XID_Start *XID_Continue             ; UAX #31 Identifier

separator      = "/" / ":"                           ; URL-style or SCP-style

suffix         = *(%x01-FF)                          ; opaque, MAY be empty

raw-input      = <any input not matching aliased-input>
```

### Invariants

**[sigil-required]**: An input string MUST be treated as containing an
alias if and only if a `+` character (U+002B) appears at a valid host
position (see `[host-position-only]`). If no `+` appears at a host
position, the input MUST be passed through as `AliasedUrl::Raw` without
modification.
`VERIFIED: unverified`

**[host-position-only]**: The `+` sigil is valid ONLY at a host
position within the input. To locate the host position, the parser
MUST:
(1) check for a scheme (`://`) and skip past it if present;
(2) scan for the last `@` within the authority block (everything
before the first `/` or `:` boundary) to skip past credentials;
(3) the host position is immediately after the last `@`, or at the
start of the authority if no `@` is found.
A `+` appearing at any other position (e.g., mid-path, as part of
a username) MUST NOT be treated as an alias sigil.
`VERIFIED: unverified`

**[alias-name-validated]**: The alias name (characters after `+` until
the first `/`, `:`, or end of input) MUST be a valid UAX #31 Identifier
(XID_Start followed by zero or more XID_Continue characters). An
invalid alias name MUST produce an `InvalidAliasName` error, not a
fallback to raw.
`VERIFIED: unverified`

**[separator-opaque-suffix]**: The character immediately following
the alias name determines the separator:
(a) `/` or `:` → separator; everything after is the opaque suffix;
(b) end of input → bare alias, no separator, no suffix.
Alurl MUST NOT interpret the suffix or the choice of separator.
The separator is preserved in the output to maintain the input's
transport semantics (e.g., `:` for SCP, `/` for URL-style).
`VERIFIED: unverified`

**[structure-preserving]**: Alias expansion MUST preserve the input's
structure. Alurl substitutes ONLY the alias name with the resolved
value. The prefix (scheme, credentials), separator, and suffix are
preserved exactly as provided. No scheme is injected. No normalization
is performed. The output is:
`prefix + resolved_alias + separator + suffix`
`VERIFIED: unverified`

**[suffix-opaque]**: The suffix (everything after the separator)
MUST be treated as an opaque string. Alurl MUST NOT validate,
normalize, or interpret it. The suffix MAY be empty.
`VERIFIED: unverified`

**[expansion-deterministic]**: Given the same input and the same
`AliasMap`, `AliasMap::resolve()` MUST produce the same output.
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

**[recursive-transparent]**: If alias expansion produces a result
that contains a `+` at a valid host position, alurl MUST re-resolve
it as a new alias. The caller MUST NOT need to loop — recursive
resolution is alurl's responsibility, not the caller's.
`VERIFIED: unverified`

**[raw-preserves-input]**: When the input does not contain a `+` at a
valid host position, the returned `AliasedUrl::Raw` MUST contain the
exact input string with no modifications.
`VERIFIED: unverified`

**[expanded-preserves-alias]**: When the input is an alias, the
returned `AliasedUrl::Expanded` MUST include the **original** alias
name (the first alias in the chain, before recursive resolution).
This enables downstream consumers to provide diagnostic messages
referencing what the user typed.
`VERIFIED: unverified`

**[zero-deps]**: Alurl MUST have zero non-std external dependencies.
Alias detection and expansion is pure string processing. Unicode
validation MAY use `unicode-ident` as the sole permitted dependency.
`VERIFIED: unverified`

**[no-io]**: Alurl MUST NOT perform any I/O (filesystem, network,
environment variables). All external state is provided through the
`AliasMap` (populated by an `AliasSource` implementor).
`VERIFIED: unverified`

### Transitions

**[classify-transition]**: Given an input string, alurl MUST locate
the host position and determine whether an alias is present.

- **PRE**: Input MUST be a non-empty string.
- **POST**: Parse any prefix (scheme, credentials). Check if the
  character at the host position is `+`. If yes, extract the alias
  name and separator. If no, return `Raw(input)`.
  `VERIFIED: unverified`

**[resolve-transition]**: Given an alias input and an `AliasMap`,
`AliasMap::resolve()` MUST expand the alias and reconstruct the output.

- **PRE**: The input MUST contain a valid alias at a host position.
  The alias name MUST be a valid Identifier.
- **POST**: The alias name is looked up in the map. The resolved
  value replaces the alias name in the output. Prefix, separator, and
  suffix are preserved. If the result contains another `+` at a host
  position, recursive resolution is applied (subject to cycle guards).
  The final result is returned as `AliasedUrl::Expanded`.
  `VERIFIED: unverified`

### Forbidden States

**[no-silent-fallback]**: If an input contains `+` at a valid host
position but the alias name is invalid or the resolver returns an
error, alurl MUST NOT silently fall back to `Raw`. It MUST return an
error. A `+` at a host position is an unambiguous declaration of alias
intent — failure to resolve is an error, not a suggestion.
`VERIFIED: unverified`

**[no-partial-expansion]**: Alurl MUST NOT return an `AliasedUrl::Expanded`
whose `url` field still contains an unresolved `+`-prefixed alias at a
host position. Recursive resolution MUST complete fully or fail entirely.
`VERIFIED: unverified`

**[no-scheme-injection]**: Alurl MUST NOT add, remove, or modify the
scheme component. If the input has no scheme, the output has no scheme.
If the input has `ssh://`, the output has `ssh://`. Scheme inference is
the consumer's responsibility.
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
O(n) for prefix parsing (where n is input length, dominated by
scanning for `://` and `@`) and O(1) for resolver lookup (assuming
the resolver is hash-based). Recursive resolution MUST be O(d × n)
where d is the chain depth, bounded by the alias configuration size.

- **Type**: Performance
  `VERIFIED: unverified`

**[error-diagnostic]**: All error types MUST carry sufficient
information for diagnostic messages. `CycleDetected` MUST include
the full chain of alias names that formed the cycle.
`AliasNotFound` MUST include the alias name that failed.

- **Type**: Usability
  `VERIFIED: unverified`

## Verification Pipeline

### Resolution Examples

| Input                        | Alias Config       | Result                                                               |
| :--------------------------- | :----------------- | :------------------------------------------------------------------- |
| `+gh/owner/repo`             | `gh → github.com`  | `Expanded { alias: "gh", url: "github.com/owner/repo" }`             |
| `+gh`                        | `gh → github.com`  | `Expanded { alias: "gh", url: "github.com" }`                        |
| `+gh:owner/repo`             | `gh → github.com`  | `Expanded { alias: "gh", url: "github.com:owner/repo" }`             |
| `ssh://+gh/owner/repo`       | `gh → github.com`  | `Expanded { alias: "gh", url: "ssh://github.com/owner/repo" }`       |
| `git@+gh:owner/repo`         | `gh → github.com`  | `Expanded { alias: "gh", url: "git@github.com:owner/repo" }`         |
| `git@+gh/owner/repo`         | `gh → github.com`  | `Expanded { alias: "gh", url: "git@github.com/owner/repo" }`         |
| `https://user:pass@+gh/repo` | `gh → github.com`  | `Expanded { alias: "gh", url: "https://user:pass@github.com/repo" }` |
| `+gh:8080/owner/repo`        | `gh → github.com`  | `Expanded { alias: "gh", url: "github.com:8080/owner/repo" }`        |
| `+work/myproject`            | `work → +gh/myorg` | `Expanded { alias: "work", url: "github.com/myorg/myproject" }`      |
| `+a`                         | `a → +b`, `b → +a` | `Err(CycleDetected { chain: ["a", "b", "a"] })`                      |
| `+unknown/repo`              | (empty)            | `Err(AliasNotFound("unknown"))`                                      |
| `https://example.com/foo`    | (any)              | `Raw("https://example.com/foo")`                                     |
| `/tmp/local/repo`            | (any)              | `Raw("/tmp/local/repo")`                                             |
| `git@host:path`              | (any)              | `Raw("git@host:path")`                                               |

### Recursive Resolution Trace

For `+work/myproject` with config `{ work → "+gh/myorg", gh → "github.com" }`:

```
Step 1: input = "+work/myproject"
        → prefix = (none), alias = "work", sep = "/", path = "myproject"
        → resolve("work") = "+gh/myorg"
        → expanded = "+gh/myorg" + "/" + "myproject" = "+gh/myorg/myproject"
        → result has "+" at host position, recurse

Step 2: input = "+gh/myorg/myproject"
        → prefix = (none), alias = "gh", sep = "/", path = "myorg/myproject"
        → resolve("gh") = "github.com"
        → expanded = "github.com" + "/" + "myorg/myproject"
        → no "+" at host position
        → return Expanded { alias: "work", url: "github.com/myorg/myproject" }
```

### SCP Resolution Trace

For `git@+gh:owner/repo` with config `{ gh → "github.com" }`:

```
Step 1: input = "git@+gh:owner/repo"
        → scan: no "://", find last "@" → prefix = "git@"
        → host position: "+gh:owner/repo"
        → alias = "gh", sep = ":", suffix = "owner/repo"
        → resolve("gh") = "github.com"
        → expanded = "git@" + "github.com" + ":" + "owner/repo"
        → return Expanded { alias: "gh", url: "git@github.com:owner/repo" }
```

### Port Trace (No Disambiguation Needed)

For `+gh:8080/owner/repo` with config `{ gh → "github.com" }`:

```
Step 1: input = "+gh:8080/owner/repo"
        → prefix = (none), alias = "gh", sep = ":", suffix = "8080/owner/repo"
        → resolve("gh") = "github.com"
        → expanded = "github.com" + ":" + "8080/owner/repo"
        → return Expanded { alias: "gh", url: "github.com:8080/owner/repo" }
        (downstream parser determines that :8080 is a port)
```

## Verification

**Verification methods:**

- `rustc` — Rust type system; if code compiles, constraint holds
- `cargo-dep` — Cargo.toml dependency audit; verified by `cargo check`
- `unit-test` — deterministic test in isolation
- `agent-check` — agent self-verification (weakest guarantee)

| Constraint               | Method      | Result  | Detail                                     | Phase |
| :----------------------- | :---------- | :------ | :----------------------------------------- | :---- |
| sigil-required           | unit-test   | pending | `+` at host position → alias, else → raw   | 2     |
| host-position-only       | unit-test   | pending | `+` mid-path / in creds is NOT an alias    | 2     |
| alias-name-validated     | unit-test   | pending | UAX #31 validation, InvalidAliasName error | 2     |
| separator-opaque-suffix  | unit-test   | pending | `/` or `:` → separator, rest is opaque     | 2     |
| structure-preserving     | unit-test   | pending | prefix + resolved + sep + suffix           | 2     |
| suffix-opaque            | unit-test   | pending | Suffix passed through without modification | 2     |
| expansion-deterministic  | unit-test   | pending | Same input + AliasMap → same output        | 2     |
| resolution-terminates    | unit-test   | pending | Cycle detection terminates all chains      | 2     |
| recursive-transparent    | unit-test   | pending | Stacked aliases resolve fully              | 2     |
| raw-preserves-input      | unit-test   | pending | Raw output equals input exactly            | 2     |
| expanded-preserves-alias | unit-test   | pending | Original alias name preserved in Expanded  | 2     |
| zero-deps                | cargo-dep   | pending | Cargo.toml has only unicode-ident (if any) | 2     |
| no-io                    | rustc       | pending | No std::fs, std::net in source             | 2     |
| no-silent-fallback       | unit-test   | pending | Invalid alias → error, not Raw             | 2     |
| no-partial-expansion     | unit-test   | pending | No `+` at host position in Expanded.url    | 2     |
| no-scheme-injection      | unit-test   | pending | Bare alias output has no scheme            | 2     |
| no-alias-in-metadata     | rustc       | pending | AliasedUrl not Serialize                   | 2     |
| resolution-complexity    | agent-check | pending | O(d × n) bounded by config size            | 2     |
| error-diagnostic         | unit-test   | pending | Error types carry diagnostic info          | 2     |

**Coverage:** 1 agent-check, 15 unit-test, 1 cargo-dep, 2 rustc = **19 total**.

## Implications

### Scope Boundaries

This specification explicitly does NOT define:

- **Scheme inference**: If input is `+gh/owner/repo`, alurl outputs
  `github.com/owner/repo` — no `https://` is added. The consumer
  (e.g., gix-url) infers the scheme.
- **URL construction**: alurl outputs a string, not a parsed URL type.
  Constructing `gix::Url` or equivalent is the consumer's concern.
- **Application grammars**: delimiters like `::` or `@version` —
  handled by consumers (e.g., atom-uri)
- **Alias storage**: TOML configs, environment variables — the
  `AliasSource` trait loads these into an `AliasMap`
- **Policy decisions**: whether to warn on credential-bearing URLs,
  whether to allow specific schemes — consumer's concern

### Integration Points

- **atom-uri** splits `::` and `@version` first, then passes the
  source component to `AliasMap::resolve()` for alias expansion
- **ion-resolve** provides an `AliasSource` implementation that
  reads user configuration into an `AliasMap`
- **gix-url** classifies and validates alurl's output, inferring
  scheme when none is present
- The `AliasedUrl::Expanded.alias` field enables ion-cli to provide
  diagnostic messages like "resolved +gh/owner/repo via alias 'gh'"

### Alias Configuration Guidance

> [!NOTE]
> Alias values SHOULD be hostnames or host+path prefixes, NOT full URLs
> with schemes. This allows one alias to work with multiple transports:
>
> ```toml
> [aliases]
> gh = "github.com"           # Good: scheme-free
> gl = "gitlab.com"           # Good: scheme-free
> work = "+gh/my-org"         # Good: alias chaining
> bad = "https://github.com"  # Avoid: locks to HTTPS
> ```
>
> With `gh = "github.com"`:
>
> - `+gh/owner/repo` → `github.com/owner/repo` (consumer infers HTTPS)
> - `ssh://+gh/owner/repo` → `ssh://github.com/owner/repo`
> - `git@+gh:owner/repo` → `git@github.com:owner/repo` (SCP)

> [!TIP]
> Alias values SHOULD NOT end with a trailing `/`. A trailing slash
> combined with a `/` separator produces a double slash in the output
> (e.g., `github.com//repo`). `AliasSource` implementations SHOULD
> validate this at load time and warn the user.
