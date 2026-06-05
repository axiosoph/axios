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
        if any(d in url_part for d in ("models/", "plans/", ".sketches/", "charters/")):
            clean_part = url_part
            while clean_part.startswith("../"):
                clean_part = clean_part[3:]
            if clean_part.startswith(".sketches/"):
                gh_url = f"https://github.com/axiosoph/axios/tree/main/{clean_part}"
            else:
                gh_url = f"https://github.com/axiosoph/axios/tree/main/docs/{clean_part}"
            return f"[{text_part}]({gh_url}{hash_part})"

        if url_part.startswith("../specs/"):
            norm_url = "reference/" + url_part[9:]
        elif url_part.startswith("../adr/"):
            norm_url = "architecture/adr/" + url_part[7:]
        elif url_part.startswith("../architecture/"):
            norm_url = "architecture/documents/" + url_part[16:]
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


def process_section(src_dir, dst_dir, section, description_tpl, default_title="Document"):
    """Process all .md files from src_dir into dst_dir with frontmatter."""
    os.makedirs(dst_dir, exist_ok=True)

    if not os.path.isdir(src_dir):
        return

    for filename in sorted(os.listdir(src_dir)):
        if not filename.endswith(".md"):
            continue

        src_path = os.path.join(src_dir, filename)
        with open(src_path, "r", encoding="utf-8") as f:
            lines = f.readlines()

        title = default_title
        body_start = 0
        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("#"):
                title_match = re.match(r"^#+\s*(?:SPEC:\s*)?(.*)$", stripped)
                if title_match:
                    title = title_match.group(1).strip()
                body_start = i + 1
                break

        body_content = "".join(lines[body_start:])
        body_content = rewrite_links(body_content, section)

        frontmatter = f'+++\ntitle = "{title}"\ndescription = "{description_tpl.format(title=title)}"\nquadrant = "Explanation"\naudience = "Developers and architects of the Axios stack"\n+++\n\n'

        dst_path = os.path.join(dst_dir, filename)
        with open(dst_path, "w", encoding="utf-8") as f:
            f.write(frontmatter + body_content)
        print(f"  {filename} -> {title}")


def process_single(src_path, dst_path, section, description, default_title="Document"):
    """Process a single .md file with frontmatter."""
    os.makedirs(os.path.dirname(dst_path), exist_ok=True)

    with open(src_path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    title = default_title
    body_start = 0
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("#"):
            title_match = re.match(r"^#+\s*(.*)$", stripped)
            if title_match:
                title = title_match.group(1).strip()
            body_start = i + 1
            break

    body_content = "".join(lines[body_start:])
    body_content = rewrite_links(body_content, section)

    frontmatter = f'+++\ntitle = "{title}"\ndescription = "{description}"\nquadrant = "Explanation"\naudience = "Developers and architects of the Axios stack"\n+++\n\n'

    with open(dst_path, "w", encoding="utf-8") as f:
        f.write(frontmatter + body_content)
    print(f"  {os.path.basename(src_path)} -> {title}")


if __name__ == "__main__":
    os.chdir(os.path.dirname(os.path.abspath(__file__)))

    print("specs:")
    process_section(
        "../docs/specs", "content/reference", "reference",
        "Behavioral specification for {title}", "Specification",
    )

    print("adrs:")
    process_section(
        "../docs/adr", "content/architecture/adr", "architecture/adr",
        "Architecture decision record: {title}", "Architecture Decision Record",
    )

    print("architecture:")
    process_section(
        "../docs/architecture", "content/architecture/documents", "architecture/documents",
        "System architecture: {title}", "Architecture Document",
    )

    print("explanations:")
    process_single(
        "../docs/spec-audit.md", "content/explanation/spec-audit.md", "explanation",
        "Completeness and coherence audit of the Axios specifications",
        "Specification Audit Report",
    )
