import os
import re

def rewrite_links(text):
    # Regex for markdown links: [text](path.md) or [text](path.md#hash)
    def repl(match):
        text_part = match.group(1)
        url_part = match.group(2)
        hash_part = match.group(3) or ""
        
        if url_part.startswith(("http://", "https://", "mailto:")):
            return match.group(0)
            
        if url_part.endswith(".md"):
            new_url = url_part[:-3] + ".html"
        else:
            new_url = url_part
        return f"[{text_part}]({new_url}{hash_part})"

    pattern = r"\[([^\]]+)\]\(([^)#?\s]+\.md)(#[^)\s]*)?\)"
    return re.sub(pattern, repl, text)

def process_specs():
    src_dir = "../docs/specs"
    dst_dir = "content/specs"
    os.makedirs(dst_dir, exist_ok=True)
    
    for filename in os.listdir(src_dir):
        if not filename.endswith(".md"):
            continue
        
        src_path = os.path.join(src_dir, filename)
        dst_path = os.path.join(dst_dir, filename)
        
        with open(src_path, "r", encoding="utf-8") as f:
            lines = f.readlines()
            
        # Parse the title from the first heading line
        title = "Specification"
        body_start = 0
        for i, line in enumerate(lines):
            if line.strip().startswith("#"):
                title_match = re.match(r"^#+\s*(?:SPEC:\s*)?(.*)$", line.strip())
                if title_match:
                    title = title_match.group(1).strip()
                body_start = i + 1
                break
                
        body_content = "".join(lines[body_start:])
        body_content = rewrite_links(body_content)
        
        # Formulate TOML frontmatter and classification block
        frontmatter = f"""+++
title = "{title}"
description = "Behavioral specification and requirements for {title}"
quadrant = "Reference"
audience = "Developers and integrators of the Axios stack layers"
+++

"""
        with open(dst_path, "w", encoding="utf-8") as f:
            f.write(frontmatter + body_content)
        print(f"Processed spec: {filename} -> {title}")

def process_adrs():
    src_dir = "../docs/adr"
    dst_dir = "content/adr"
    os.makedirs(dst_dir, exist_ok=True)
    
    for filename in os.listdir(src_dir):
        if not filename.endswith(".md"):
            continue
        
        src_path = os.path.join(src_dir, filename)
        dst_path = os.path.join(dst_dir, filename)
        
        with open(src_path, "r", encoding="utf-8") as f:
            lines = f.readlines()
            
        title = "Architecture Decision Record"
        body_start = 0
        for i, line in enumerate(lines):
            if line.strip().startswith("#"):
                title_match = re.match(r"^#+\s*(.*)$", line.strip())
                if title_match:
                    title = title_match.group(1).strip()
                body_start = i + 1
                break
                
        body_content = "".join(lines[body_start:])
        body_content = rewrite_links(body_content)
        
        frontmatter = f"""+++
title = "{title}"
description = "Architectural decision record tracking design choices and rationale for {title}"
quadrant = "Explanation"
audience = "Contributors, developers, and maintainers tracking Axios system design evolution"
+++

"""
        with open(dst_path, "w", encoding="utf-8") as f:
            f.write(frontmatter + body_content)
        print(f"Processed ADR: {filename} -> {title}")

def process_explanations():
    src_path = "../docs/spec-audit.md"
    dst_dir = "content/explanation"
    os.makedirs(dst_dir, exist_ok=True)
    dst_path = os.path.join(dst_dir, "spec-audit.md")
    
    with open(src_path, "r", encoding="utf-8") as f:
        lines = f.readlines()
        
    title = "Specification Audit Report"
    body_start = 0
    for i, line in enumerate(lines):
        if line.strip().startswith("#"):
            title_match = re.match(r"^#+\s*(.*)$", line.strip())
            if title_match:
                title = title_match.group(1).strip()
            body_start = i + 1
            break
            
    body_content = "".join(lines[body_start:])
    body_content = rewrite_links(body_content)
    
    frontmatter = f"""+++
title = "{title}"
description = "Completeness and coherence audit of the Axios specifications"
quadrant = "Explanation"
audience = "Contributors and architects tracking Axios system design and specs completeness"
+++

"""
    with open(dst_path, "w", encoding="utf-8") as f:
        f.write(frontmatter + body_content)
    print(f"Processed explanation: spec-audit.md -> {title}")

if __name__ == "__main__":
    # Change Cwd to this script's directory for safety
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
    process_specs()
    process_adrs()
    process_explanations()

