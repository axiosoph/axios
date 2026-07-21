import os
import re

# Regex for markdown links: [text](path.md) or [text](path.md#hash)
# Uses [^\[\]]* for link text to avoid catastrophic backtracking on
# files with heavy bracket usage (e.g. Mermaid diagrams).
_LINK_PATTERN = re.compile(
    r"\[([^\[\]]*)\]\(([^)#?\s]+\.md)(#[^)\s]*)?\)"
)

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
