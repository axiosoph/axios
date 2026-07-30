import html
import os
import re

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


def process_file(src_path, dst_dir, section, description_tpl, default_title="Document", tags=None):
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

    frontmatter = f'+++\ntitle = "{title}"\ndescription = "{description_tpl.format(title=title)}"\nquadrant = "Explanation"\naudience = "Developers and architects of the Axios stack"\n{tags_str}+++\n\n'

    dst_path = os.path.join(dst_dir, filename)
    with open(dst_path, "w", encoding="utf-8") as f:
        f.write(frontmatter + body_content)
    print(f"  {filename} -> {title}")


def process_section(src_dir, dst_dir, section, description_tpl, default_title="Document", tags=None):
    """Process all .md files from src_dir into dst_dir with frontmatter."""
    if not os.path.isdir(src_dir):
        return

    for filename in sorted(os.listdir(src_dir)):
        if not filename.endswith(".md"):
            continue
        process_file(
            os.path.join(src_dir, filename), dst_dir, section,
            description_tpl, default_title, tags,
        )


if __name__ == "__main__":
    os.chdir(os.path.dirname(os.path.abspath(__file__)))

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
    )

    print("adrs:")
    process_section(
        "../docs/adr", "content/architecture", "architecture",
        "Architecture decision record: {title}", "Architecture Decision Record",
        tags=["adr"],
    )

    print("architecture:")
    process_section(
        "../docs/architecture", "content/architecture", "architecture",
        "System architecture: {title}", "Architecture Document",
        tags=["sad"],
    )

    print("models:")
    process_section(
        "../docs/models", "content/models", "models",
        "Formal model: {title}", "Formal Model",
        tags=["model"],
    )

    print("glossary:")
    process_file(
        "../docs/glossary.md", "content/reference", "reference",
        "Canonical definitions for the project's novel terminology",
        "Glossary",
        tags=["glossary"],
    )

    # spec-audit.md is superseded (see banner in the source doc) and is no
    # longer published to the public site.
