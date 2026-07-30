import fnmatch
import html
import json
import os
import re
import subprocess
import sys

# Regex for markdown links: [text](path.md) or [text](path.md#hash)
# Uses [^\[\]]* for link text to avoid catastrophic backtracking on
# files with heavy bracket usage (e.g. Mermaid diagrams).
_LINK_PATTERN = re.compile(
    r"\[([^\[\]]*)\]\(([^)#?\s]+\.md)(#[^)\s]*)?\)"
)

# --------------------------------------------------------------------------
# Claim blocks (github.com/axiosoph/axios/issues/103)
#
# A claim block is a fenced code block whose info string is exactly `claim`
# (docket's MVP.md §1.1), holding a small flat YAML payload: `kind`
# (requirement/invariant/constraint), `evaluator` (how firmly it is
# discharged, a preference-ordered hierarchy from `proof` down to `none`),
# and an optional `cites` list. sukr's markdown backend has no highlighter
# for the `claim` language, so left alone it renders as a raw, unstyled
# code fence — a bare normative statement with no indication of how
# strongly it is held, which reads as MORE authoritative than the source.
# Rendering the kind and evaluator as visible badges instead publishes that
# self-assessment rather than hiding it.
# --------------------------------------------------------------------------

_CLAIM_FENCE = re.compile(r"```claim\r?\n(.*?)```", re.DOTALL)
_CLAIM_KEY_LINE = re.compile(r"^([a-zA-Z_][\w-]*)\s*:\s*(.*)$")
_CLAIM_FLOW_LIST = re.compile(r"^\[(.*)\]$", re.DOTALL)

# Schema per docket's contracts/claim.ncl: `kind` and `evaluator` are closed
# enums, `evaluator` doubling as a preference order (proof strongest, none
# weakest) that the CSS strength gradient below mirrors.
_CLAIM_KINDS = {"requirement", "invariant", "constraint"}
_CLAIM_EVALUATORS = {
    "proof", "model-check", "type", "property-test", "test", "example", "none",
}


def _parse_claim_flow_list(value):
    """Parse a YAML flow-sequence `[a, b, c]` of bare/quoted scalars.

    Not a general YAML parser — `cites` is the only field using this shape
    (docket MVP.md §1.3), so only that shape is supported. Anything else
    is reported as an error rather than guessed at.
    """
    match = _CLAIM_FLOW_LIST.match(value.strip())
    if not match:
        return [], [f"cites: expected a flow list like `[a, b]`, got {value!r}"]
    inner = match.group(1).strip()
    if not inner:
        return [], []
    items = []
    for raw_item in inner.split(","):
        item = raw_item.strip()
        if len(item) >= 2 and item[0] == item[-1] and item[0] in "\"'":
            item = item[1:-1]
        if not item:
            return [], [f"cites: empty entry in {value!r}"]
        items.append(item)
    return items, []


def parse_claim_block(raw_yaml):
    """Parse and validate one claim block's YAML payload.

    A hand-rolled parser scoped to the claim schema's three flat keys, not
    a general YAML parser — this file has no third-party dependency today
    and the schema (docket contracts/claim.ncl) is a closed, flat shape,
    so adding one is not justified for it.

    Returns `(fields, errors)`. `fields` holds whatever recognized keys
    were found; `errors` lists every deviation found, in encounter order.
    An empty `errors` means the block is well-formed.
    """
    fields = {}
    errors = []
    seen = set()
    for lineno, line in enumerate(raw_yaml.splitlines(), start=1):
        stripped = line.strip()
        if not stripped:
            continue
        match = _CLAIM_KEY_LINE.match(stripped)
        if not match:
            errors.append(f"line {lineno}: not a `key: value` line: {stripped!r}")
            continue
        key, value = match.group(1), match.group(2).strip()
        if key in seen:
            errors.append(f"duplicate field {key!r}")
        seen.add(key)

        if key == "kind":
            if value not in _CLAIM_KINDS:
                errors.append(f"kind: {value!r} is not one of {sorted(_CLAIM_KINDS)}")
            fields["kind"] = value
        elif key == "evaluator":
            if value not in _CLAIM_EVALUATORS:
                errors.append(
                    f"evaluator: {value!r} is not one of {sorted(_CLAIM_EVALUATORS)}"
                )
            fields["evaluator"] = value
        elif key == "cites":
            cites, cite_errors = _parse_claim_flow_list(value)
            errors.extend(cite_errors)
            fields["cites"] = cites
        else:
            errors.append(f"unknown field {key!r}")

    if "kind" not in fields:
        errors.append("missing required field `kind`")
    if "evaluator" not in fields:
        errors.append("missing required field `evaluator`")
    fields.setdefault("cites", [])
    return fields, errors


def _render_claim_html(fields):
    """Render a well-formed claim as a badge callout.

    `cites` are rendered as plain `<code>` text, not links: resolving a
    citation (claim id or `doc#anchor`) to a URL needs corpus-wide
    knowledge of every other document's derived path, which is the
    quadrant/frontmatter derivation's job (out of this surface) — turning
    a citation into a possibly-broken link here would be inventing scope,
    not rendering one.
    """
    kind = fields["kind"]
    evaluator = fields["evaluator"]
    cites = fields["cites"]

    parts = [
        f'<aside class="claim-block" data-claim-kind="{kind}" '
        f'data-claim-evaluator="{evaluator}">',
        '<p class="claim-badges">'
        f'<span class="claim-kind-badge">{kind}</span>'
        f'<span class="claim-evaluator-badge">evaluator: {evaluator}</span>'
        "</p>",
    ]
    if cites:
        cite_html = ", ".join(f"<code>{html.escape(c)}</code>" for c in cites)
        parts.append(f'<p class="claim-cites">cites: {cite_html}</p>')
    parts.append("</aside>")
    return "".join(parts)


def _render_malformed_claim_html(raw_yaml, errors):
    """Render a malformed claim block as a flagged error callout.

    Silently dropping it is the one option ruled out by the brief: that
    would hide exactly the defect the claim register exists to surface.
    So it stays visible — flagged, with its errors and raw source shown —
    rather than reverting to an unstyled code fence, which would look like
    ordinary (if odd) documentation rather than a build-time defect.
    """
    error_items = "".join(f"<li>{html.escape(e)}</li>" for e in errors)
    raw_html = html.escape(raw_yaml)
    return (
        '<aside class="claim-block claim-block-malformed" data-claim-kind="malformed">'
        '<p class="claim-badges">'
        '<span class="claim-kind-badge claim-kind-malformed">malformed claim block</span>'
        "</p>"
        f'<ul class="claim-errors">{error_items}</ul>'
        f"<pre class=\"claim-raw\"><code>{raw_html}</code></pre>"
        "</aside>"
    )


def render_claim_blocks(body_content):
    """Replace every ```claim fenced block with rendered HTML.

    Runs BEFORE `rewrite_links()`: a raw `cites: [a, b]` line uses bracket
    syntax that the markdown-link regex could otherwise walk into.

    The output is emitted as inline raw HTML rather than left as markdown,
    because sukr's markdown backend (pulldown-cmark) passes HTML blocks
    through verbatim (see `Event::Html` in sukr's src/content.rs) — the
    only hook available from this side of the pipeline without changing
    sukr itself, which is out of this surface.
    """
    def repl(match):
        raw_yaml = match.group(1)
        fields, errors = parse_claim_block(raw_yaml)
        if errors:
            print(f"  warning: malformed claim block ({'; '.join(errors)})")
            block_html = _render_malformed_claim_html(raw_yaml, errors)
        else:
            block_html = _render_claim_html(fields)
        # A blank line inside the raw HTML block would end it early for
        # pulldown-cmark's HTML-block scanner, letting the remainder fall
        # through as ordinary markdown. Only the malformed path's raw-YAML
        # dump can carry one; collapse defensively either way.
        return re.sub(r"\n\s*\n", "\n", block_html)

    return _CLAIM_FENCE.sub(repl, body_content)

# --------------------------------------------------------------------------
# Quadrant derivation (github.com/axiosoph/axios/issues/103)
#
# docket.ncl is the single source of truth for which of the four
# documentation quadrants each genre serves (see its header comment). The
# `quadrant` frontmatter emitted for every document is DERIVED from it by
# matching the document's source path against each genre's `path` glob —
# never hardcoded here, which is what let every genre carry the same
# ("Explanation") value regardless of its actual kind. A source path
# matching zero or more than one genre stops the build rather than
# guessing: docket.ncl's header requires genre patterns to be disjoint, and
# a silent default is the defect this replaces.
# --------------------------------------------------------------------------

# docket.ncl's quadrant names are lowercase; the site's frontmatter
# convention (see the hand-authored content/how-to/_index.md,
# content/reference/_index.md) is capitalized/hyphenated.
_QUADRANT_DISPLAY = {
    "reference": "Reference",
    "explanation": "Explanation",
    "how-to": "How-To",
    "tutorial": "Tutorial",
}


def load_docket_genres(repo_root):
    """Load and parse docket.ncl's genre list via `nickel export`.

    No fallback: a missing `nickel` binary, a non-zero exit, or unparseable
    output all stop the build. Guessing a genre list is the same defect
    class as guessing a quadrant.
    """
    docket_path = os.path.join(repo_root, "docket.ncl")
    try:
        result = subprocess.run(
            ["nickel", "export", "--format", "json", docket_path],
            capture_output=True, text=True, check=False,
        )
    except FileNotFoundError as exc:
        sys.exit(f"fatal: cannot run `nickel` to read {docket_path}: {exc}")
    if result.returncode != 0:
        sys.exit(
            f"fatal: `nickel export` failed on {docket_path} "
            f"(exit {result.returncode}):\n{result.stderr}"
        )
    try:
        docket = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        sys.exit(f"fatal: {docket_path} did not export valid JSON: {exc}")
    genres = docket.get("genres")
    if not isinstance(genres, list) or not genres:
        sys.exit(f"fatal: {docket_path} exported no `genres` list")
    return genres


def quadrant_for_source(src_path, repo_root, genres):
    """Derive the display quadrant for one source path via docket.ncl.

    `src_path` may be a directory (a process_section source) or a single
    file (a process_file source); both are matched against each genre's
    glob the same way docket.ncl matches real corpus paths — a directory is
    probed with a synthetic child so a `docs/specs/**`-style pattern
    matches it the same way it matches any file beneath it. Zero or
    multiple matches both stop the build: either is a docket.ncl
    configuration error, never a precedence question for this script to
    resolve on its own.
    """
    rel = os.path.relpath(os.path.abspath(src_path), repo_root)
    probe = rel if rel.endswith(".md") else f"{rel}/__probe__"
    matches = [g for g in genres if fnmatch.fnmatch(probe, g["path"])]
    if not matches:
        sys.exit(
            f"fatal: {rel!r} matches no genre in docket.ncl "
            f"(checked: {[g['path'] for g in genres]}); "
            "add a genre row rather than assuming a default quadrant"
        )
    if len(matches) > 1:
        sys.exit(
            f"fatal: {rel!r} matches multiple genres in docket.ncl "
            f"({[g['path'] for g in matches]}); genre patterns must be disjoint"
        )
    quadrant = matches[0]["quadrant"]
    if quadrant not in _QUADRANT_DISPLAY:
        sys.exit(
            f"fatal: docket.ncl genre for {rel!r} declares unknown quadrant "
            f"{quadrant!r} (known: {sorted(_QUADRANT_DISPLAY)})"
        )
    return _QUADRANT_DISPLAY[quadrant]


def rewrite_links(text, section):
    def repl(match):
        text_part = match.group(1)
        url_part = match.group(2)
        hash_part = match.group(3) or ""

        if url_part.startswith(("http://", "https://", "mailto:")):
            return match.group(0)

        # Map uncompiled directories to GitHub to avoid broken links
        if "models/tla/" in url_part or "models/lean/" in url_part:
            clean_part = url_part
            while clean_part.startswith("../"):
                clean_part = clean_part[3:]
            gh_url = f"https://github.com/axiosoph/axios/tree/main/docs/{clean_part}"
            return f"[{text_part}]({gh_url}{hash_part})"

        if url_part.startswith("../specs/"):
            norm_url = "reference/" + url_part[9:]
        elif url_part.startswith("../adr/"):
            norm_url = "architecture/" + url_part[7:]
        elif url_part.startswith("../models/"):
            norm_url = "models/" + url_part[10:]
        elif url_part.startswith("../architecture/"):
            norm_url = "architecture/" + url_part[16:]
        elif url_part.startswith("../"):
            norm_url = url_part[3:]
        else:
            norm_url = f"{section}/{url_part}"

        if norm_url.endswith(".md"):
            norm_url = norm_url[:-3] + ".html"

        if not norm_url.startswith("/"):
            norm_url = "/" + norm_url

        return f"[{text_part}]({norm_url}{hash_part})"

    return _LINK_PATTERN.sub(repl, text)


def process_file(src_path, dst_dir, section, description_tpl, default_title="Document", tags=None, *, quadrant):
    """Process one .md file into dst_dir with frontmatter."""
    os.makedirs(dst_dir, exist_ok=True)
    filename = os.path.basename(src_path)
    with open(src_path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    title = default_title
    body_start = 0
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("#"):
            title_match = re.match(r"^#+\s*(?:(?:SPEC|MODEL):\s*)?(.*)$", stripped)
            if title_match:
                title = title_match.group(1).strip()
            body_start = i + 1
            break

    body_content = "".join(lines[body_start:])
    body_content = render_claim_blocks(body_content)
    body_content = rewrite_links(body_content, section)

    file_tags = tags
    if callable(tags):
        file_tags = tags(filename)

    tags_str = ""
    if file_tags:
        tags_json = "[" + ", ".join(f'"{t}"' for t in file_tags) + "]"
        tags_str = f"tags = {tags_json}\n"

    frontmatter = f'+++\ntitle = "{title}"\ndescription = "{description_tpl.format(title=title)}"\nquadrant = "{quadrant}"\naudience = "Developers and architects of the Axios stack"\n{tags_str}+++\n\n'

    dst_path = os.path.join(dst_dir, filename)
    with open(dst_path, "w", encoding="utf-8") as f:
        f.write(frontmatter + body_content)
    print(f"  {filename} -> {title}")


def process_section(src_dir, dst_dir, section, description_tpl, default_title="Document", tags=None, *, quadrant):
    """Process all .md files from src_dir into dst_dir with frontmatter."""
    if not os.path.isdir(src_dir):
        return

    for filename in sorted(os.listdir(src_dir)):
        if not filename.endswith(".md"):
            continue
        process_file(
            os.path.join(src_dir, filename), dst_dir, section,
            description_tpl, default_title, tags, quadrant=quadrant,
        )


if __name__ == "__main__":
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
    REPO_ROOT = os.path.abspath("..")
    GENRES = load_docket_genres(REPO_ROOT)

    print("specs:")
    def get_spec_tags(filename):
        if filename in ("atom-sourcing.md", "atom-transactions.md", "git-storage-format.md"):
            return ["atom", "layer1"]
        elif filename in ("eos-build-engine.md", "eos-network-protocol.md", "eos-sandboxing.md", "eos-scheduler.md"):
            return ["eos", "layer2"]
        elif filename in ("ion-manifest.md", "ion-resolution.md", "lock-file-schema.md"):
            return ["ion", "layer3"]
        elif filename in ("aliased-url-resolution.md", "ion-eos-contract.md", "layer-boundaries.md"):
            return ["cross-cutting"]
        return []

    process_section(
        "../docs/specs", "content/reference", "reference",
        "Behavioral specification for {title}", "Specification",
        tags=get_spec_tags,
        quadrant=quadrant_for_source("../docs/specs", REPO_ROOT, GENRES),
    )

    print("adrs:")
    process_section(
        "../docs/adr", "content/architecture", "architecture",
        "Architecture decision record: {title}", "Architecture Decision Record",
        tags=["adr"],
        quadrant=quadrant_for_source("../docs/adr", REPO_ROOT, GENRES),
    )

    print("architecture:")
    process_section(
        "../docs/architecture", "content/architecture", "architecture",
        "System architecture: {title}", "Architecture Document",
        tags=["sad"],
        quadrant=quadrant_for_source("../docs/architecture", REPO_ROOT, GENRES),
    )

    print("models:")
    process_section(
        "../docs/models", "content/models", "models",
        "Formal model: {title}", "Formal Model",
        tags=["model"],
        quadrant=quadrant_for_source("../docs/models", REPO_ROOT, GENRES),
    )

    print("glossary:")
    process_file(
        "../docs/glossary.md", "content/reference", "reference",
        "Canonical definitions for the project's novel terminology",
        "Glossary",
        tags=["glossary"],
        quadrant=quadrant_for_source("../docs/glossary.md", REPO_ROOT, GENRES),
    )

    # spec-audit.md is superseded (see banner in the source doc) and is no
    # longer published to the public site.
