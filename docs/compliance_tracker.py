#!/usr/bin/env python3
"""
Axios Specification Compliance Tracker

- Quadrant: Reference (Utility tool for compliance tracking)
- Audience: Axios developers and compliance auditors

This script parses all specifications in docs/specs/*.md to extract constraint IDs
from their verification tables AND from inline `**[id]**:` prose definitions,
scans the codebase for compliance annotations, matches them, and outputs
compliance status to docs/compliance.json and www/content/compliance.md. It also
emits docs/on_path_constraints.json, an on-path constraint manifest recording
each constraint's spec-stated verification method where one exists, leaving a
gap genuinely open (not auto-justified) otherwise, checked by
check_constraint_coverage.py.
"""

import os
import re
import json
import glob

def normalize_id(cid):
    # Remove markdown links: [text](url) -> text
    cid = re.sub(r'\[([^\]]+)\]\([^)]+\)', r'\1', cid)
    # Strip brackets, backticks, spaces, colons, stars
    cid = cid.strip("[]` \t\r\n:*")
    return cid.lower()

# An inline prose definition site: a line beginning with a bold-bracketed ID,
# e.g. `**[charter-typ]**: ...` or `**[anchor-immutable]** _(amended ...)_: ...`.
# Bracketed IDs referenced (not defined) elsewhere in prose use single
# backticks (`` `[id]` ``), never this bold form, so matching at line-start is
# unambiguous across the corpus (verified by survey: the sole non-definition
# occurrence, atom-sourcing.md's bulleted `- **[no-unpublished-dependency]**`,
# restates an ID already defined by its own `**[id]**:` paragraph elsewhere in
# the same file, so re-discovering it here is harmless).
_INLINE_DEF_RE = re.compile(r'^\*\*\[([^\]]+)\]\*\*')
_VERIFIED_RE = re.compile(r'`VERIFIED:\s*([^`]*)`')
_HEADING_RE = re.compile(r'^#')
# A spec-stated evaluator counts as "unnamed" only when the text is the bare
# status word "unverified" (with or without a parenthetical explanation) —
# every other observed form (`machine (TLC)`, `rustc (...)`, `agent-check`,
# `pass — some_test`, `TLA+ ModelName — ...`) names a concrete mechanism.
_BARE_UNVERIFIED_RE = re.compile(r'^unverified\b', re.IGNORECASE)

def _has_named_evaluator(evaluator_text):
    if not evaluator_text:
        return False
    return not _BARE_UNVERIFIED_RE.match(evaluator_text.strip())

def extract_constraint_records_from_spec(file_path):
    """Extract every on-path constraint occurrence from a spec file.

    Returns a list of dicts: {norm_id, display_id, line_number, evaluator,
    source}, covering both markdown-table rows (pre-existing style) and
    inline `**[id]**:` prose definitions (widened style). `evaluator` is the
    verification-method text the spec itself states for that occurrence — a
    table's `Method`-like column value, or the nearest inline
    `` `VERIFIED: ...` `` trailer found before the next definition, heading,
    or table — or None if the spec names none.
    """
    records = []
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            lines = f.readlines()
    except Exception as e:
        print(f"Error reading spec file {file_path}: {e}")
        return records

    # ---- table-style extraction (pre-existing "Constraint" column state
    # machine), widened to also capture a "Method"-like column when present ----
    in_table = False
    constraint_col_idx = -1
    method_col_idx = -1
    skip_next = False

    for i, line in enumerate(lines):
        line_stripped = line.strip()
        if not line_stripped.startswith('|'):
            in_table = False
            constraint_col_idx = -1
            method_col_idx = -1
            continue

        # Split row by '|' and strip spaces
        cells = [c.strip() for c in line_stripped.split('|')[1:-1]]

        if not in_table:
            # Check if this is a header row containing 'Constraint'
            normalized_cells = [c.lower().strip("[]` \t") for c in cells]
            if 'constraint' in normalized_cells:
                in_table = True
                constraint_col_idx = normalized_cells.index('constraint')
                method_col_idx = normalized_cells.index('method') if 'method' in normalized_cells else -1
                skip_next = True
            continue

        if skip_next:
            skip_next = False
            continue

        # Extract the constraint ID from the recorded column
        if 0 <= constraint_col_idx < len(cells):
            cell_val = cells[constraint_col_idx].strip()
            # Skip separator line or empty cell
            if not cell_val or all(char in '-: \t' for char in cell_val):
                continue
            # Extract and clean
            cid_normalized = normalize_id(cell_val)
            # Find the display ID (remove brackets/backticks/spaces, keep casing)
            cid_display = cell_val.strip("[]` \t")
            if cid_normalized:
                evaluator = None
                if 0 <= method_col_idx < len(cells):
                    method_val = cells[method_col_idx].strip()
                    if method_val and not all(ch in '-: \t' for ch in method_val):
                        evaluator = method_val
                records.append({
                    "norm_id": cid_normalized,
                    "display_id": cid_display,
                    "line_number": i + 1,
                    "evaluator": evaluator,
                    "source": "table",
                })

    # ---- inline `**[id]**:` prose-style extraction (widened) ----
    n = len(lines)
    for idx in range(n):
        line_stripped = lines[idx].strip()
        m = _INLINE_DEF_RE.match(line_stripped)
        if not m:
            continue
        cell_val = m.group(1)
        cid_normalized = normalize_id(cell_val)
        cid_display = cell_val.strip("[]` \t")
        if not cid_normalized:
            continue

        # Scan forward for the nearest VERIFIED trailer, stopping at the
        # next definition, a heading, or a table — never borrowing a
        # trailer that belongs to a later, different definition.
        evaluator = None
        j = idx + 1
        while j < n:
            nxt_stripped = lines[j].strip()
            if _INLINE_DEF_RE.match(nxt_stripped) or _HEADING_RE.match(nxt_stripped) or nxt_stripped.startswith('|'):
                break
            vm = _VERIFIED_RE.search(nxt_stripped)
            if vm:
                evaluator = vm.group(1).strip()
                break
            j += 1

        records.append({
            "norm_id": cid_normalized,
            "display_id": cid_display,
            "line_number": idx + 1,
            "evaluator": evaluator,
            "source": "inline",
        })

    return records

def extract_constraints_from_spec(file_path):
    """Backward-compatible (norm_id, display_id) view used by main() to
    build docs/compliance.json. Deduped per spec file — a constraint
    surfaced by both a table row and its own inline definition (e.g.
    atom-sourcing.md's no-unpublished-dependency) must contribute exactly
    one row, not a doubled entry in the compliance matrix.
    """
    seen = set()
    constraints = []
    for r in extract_constraint_records_from_spec(file_path):
        if r["norm_id"] in seen:
            continue
        seen.add(r["norm_id"])
        constraints.append((r["norm_id"], r["display_id"]))
    return constraints

def build_constraint_manifest(repo_root):
    """Build the on-path constraint manifest: one entry per (spec_file, id)
    pair across docs/specs/*.md.

    Each entry carries a spec-stated named evaluator when one exists.
    `residue` is NEVER auto-generated: it is a reserved slot for a
    deliberate, reviewed justification of why a *specific* constraint
    needs no machine evaluator (mirrors findings_apply's
    resolved-requires-evaluator rule) — manufacturing one here for every
    unnamed-evaluator constraint would make check_constraint_coverage.py
    unable to ever fail on real output, which defeats the coverage check
    entirely. A constraint with neither is left with both fields empty so
    the coverage-check surfaces it as a genuine, uncovered gap rather than
    silently laundering it into "covered". `spec_status` is purely
    informational context (whatever verification-status text, if any, the
    spec itself states — even the bare word "unverified") for a human
    triaging the gap; it is never consulted by the coverage-check.

    Disambiguation rule: entries are keyed by (spec_file, id), never by id
    alone — a genuine cross-file same-name ID (e.g. lock-schema-version,
    independently defined in both lock-file-schema.md and
    ion-resolution.md) surfaces as two distinct entries, one per defining
    spec, rather than being silently collapsed into one. Within a single
    file, an id repeated across styles (table + inline) is merged into one
    entry, preferring whichever occurrence names a real evaluator.
    """
    spec_dir = os.path.join(repo_root, "docs", "specs")
    spec_files = glob.glob(os.path.join(spec_dir, "*.md"))

    manifest = []
    for spec_file in sorted(spec_files):
        rel_spec_path = os.path.relpath(spec_file, repo_root).replace(os.sep, '/')
        by_id = {}
        for r in extract_constraint_records_from_spec(spec_file):
            existing = by_id.get(r["norm_id"])
            if existing is None:
                by_id[r["norm_id"]] = r
            elif not _has_named_evaluator(existing["evaluator"]) and _has_named_evaluator(r["evaluator"]):
                by_id[r["norm_id"]] = r

        for norm_id in sorted(by_id):
            r = by_id[norm_id]
            evaluator_text = r["evaluator"]
            if _has_named_evaluator(evaluator_text):
                manifest.append({
                    "id": r["display_id"],
                    "spec_file": rel_spec_path,
                    "line": r["line_number"],
                    "evaluator": evaluator_text,
                    "residue": "",
                    "spec_status": "",
                })
            else:
                manifest.append({
                    "id": r["display_id"],
                    "spec_file": rel_spec_path,
                    "line": r["line_number"],
                    "evaluator": "",
                    "residue": "",
                    "spec_status": evaluator_text or "",
                })

    return manifest

def scan_codebase_for_annotations(repo_root):
    annotations = {}
    directories = ['atom', 'eos', 'ion', 'alurl']
    
    comp_pattern = re.compile(r'//\s*@spec-compliance\s*\[\s*([^\]]+?)\s*\]')
    mech_pattern = re.compile(r'//\s*Mechanism:\s*(.*)')
    verified_pattern = re.compile(r'//\s*Verified-By:\s*(.*)')

    for directory in directories:
        dir_path = os.path.join(repo_root, directory)
        if not os.path.exists(dir_path):
            continue

        for root, _, files in os.walk(dir_path):
            # Ignore target directories and other build caches
            if 'target' in root.split(os.sep):
                continue

            for file in files:
                if not file.endswith('.rs'):
                    continue

                file_path = os.path.join(root, file)
                rel_path = os.path.relpath(file_path, repo_root)

                try:
                    with open(file_path, 'r', encoding='utf-8', errors='replace') as f:
                        lines = f.readlines()
                except Exception as e:
                    print(f"Error reading file {file_path}: {e}")
                    continue

                idx = 0
                while idx < len(lines):
                    line = lines[idx]
                    comp_match = comp_pattern.search(line)
                    if comp_match:
                        constraint_raw = comp_match.group(1)
                        constraint_id = normalize_id(constraint_raw)
                        mechanism = ""
                        test_path = ""
                        line_number = idx + 1

                        # Look ahead for Mechanism and Verified-By
                        curr_idx = idx + 1
                        while curr_idx < len(lines):
                            next_line = lines[curr_idx]
                            # Stop scanning if we exit the comment block
                            if not next_line.strip().startswith('//'):
                                break

                            mech_match = mech_pattern.search(next_line)
                            ver_match = verified_pattern.search(next_line)

                            if mech_match:
                                mechanism = mech_match.group(1).strip()
                            elif ver_match:
                                test_path = ver_match.group(1).strip()

                            if mechanism and test_path:
                                break
                            if curr_idx > idx + 5:  # Scan limit to prevent infinite loops
                                break
                            curr_idx += 1

                        if constraint_id:
                            if constraint_id not in annotations:
                                annotations[constraint_id] = []
                            annotations[constraint_id].append({
                                "code_path": rel_path.replace(os.sep, '/'),
                                "line_number": line_number,
                                "mechanism": mechanism,
                                "test_path": test_path
                            })
                            # Fast forward past the comment block
                            idx = max(idx, curr_idx)
                    idx += 1

    return annotations

def main():
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    print(f"Running compliance tracker in repository root: {repo_root}")

    # Step 1: Walk specs and extract constraints
    spec_dir = os.path.join(repo_root, "docs", "specs")
    spec_files = glob.glob(os.path.join(spec_dir, "*.md"))
    
    spec_constraints = {}
    spec_to_constraints = {} # group for output grouping
    
    for spec_file in sorted(spec_files):
        rel_spec_path = os.path.relpath(spec_file, repo_root).replace(os.sep, '/')
        constraints = extract_constraints_from_spec(spec_file)
        
        spec_name = os.path.basename(spec_file)
        spec_to_constraints[spec_name] = {
            "rel_path": rel_spec_path,
            "constraints": []
        }
        
        for norm_id, display_id in constraints:
            spec_constraints[norm_id] = {
                "display_id": display_id,
                "specification_file": rel_spec_path,
                "spec_name": spec_name
            }
            spec_to_constraints[spec_name]["constraints"].append(norm_id)
            
    print(f"Extracted {len(spec_constraints)} constraints from specs.")

    # Step 2: Scan codebase for annotations
    annotations = scan_codebase_for_annotations(repo_root)
    print(f"Found codebase annotations for {len(annotations)} distinct constraints.")

    # Step 3: Match and generate JSON structure
    compliance_data = {
        "constraints": {}
    }
    
    for norm_id, info in spec_constraints.items():
        display_id = info["display_id"]
        spec_file = info["specification_file"]
        
        if norm_id in annotations:
            # Combine mechanisms
            mechs = [ann["mechanism"] for ann in annotations[norm_id] if ann["mechanism"]]
            mechanism = " | ".join(sorted(list(set(mechs)))) if mechs else "Verified implementation."
            
            # Format verification paths
            v_paths = []
            for ann in annotations[norm_id]:
                v_paths.append({
                    "code_path": ann["code_path"],
                    "line_number": ann["line_number"],
                    "test_path": ann["test_path"] if ann["test_path"] else None
                })
                
            compliance_data["constraints"][display_id] = {
                "status": "VERIFIED",
                "specification_file": spec_file,
                "mechanism": mechanism,
                "verification_paths": v_paths
            }
        else:
            compliance_data["constraints"][display_id] = {
                "status": "UNVERIFIED",
                "specification_file": spec_file,
                "mechanism": None,
                "verification_paths": []
            }

    # Warn about annotations not matching any spec constraint
    for norm_id, ann_list in annotations.items():
        if norm_id not in spec_constraints:
            print(f"WARNING: Codebase contains annotation for unknown constraint ID '{norm_id}' in {ann_list[0]['code_path']}:{ann_list[0]['line_number']}.")

    # Step 4: Write docs/compliance.json
    json_path = os.path.join(repo_root, "docs", "compliance.json")
    with open(json_path, "w", encoding="utf-8") as f:
        json.dump(compliance_data, f, indent=2)
    print(f"Wrote compliance JSON to {json_path}")

    # Step 4b: Write the on-path constraint manifest (coverage-check input)
    manifest = build_constraint_manifest(repo_root)
    manifest_path = os.path.join(repo_root, "docs", "on_path_constraints.json")
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump({"constraints": manifest}, f, indent=2)
        f.write("\n")
    unnamed = sum(1 for e in manifest if not e["evaluator"] and not e["residue"])
    print(f"Wrote on-path constraint manifest ({len(manifest)} entries, {unnamed} unnamed-evaluator) to {manifest_path}")

    # Step 5: Compile markdown compliance matrix at www/content/compliance.md
    verified_count = sum(1 for c in compliance_data["constraints"].values() if c["status"] == "VERIFIED")
    total_count = len(compliance_data["constraints"])
    unverified_count = total_count - verified_count
    compliance_rate = (verified_count / total_count * 100.0) if total_count > 0 else 0.0

    markdown_content = f"""+++
title = "Spec Compliance Matrix"
description = "Automated compliance status of codebase implementations against specifications"
tags = ["compliance"]
+++

**Metadata:**
- **Quadrant:** Reference
- **Audience:** Developers, integrators, and auditors of the Axios stack

# Spec Compliance Matrix

This page tracks the compliance of the Axios codebase (`atom/`, `eos/`, `ion/`, `alurl/`) with the system specifications defined in `docs/specs/`.

## Compliance Summary

- **Total Constraints:** {total_count}
- **Verified Constraints:** {verified_count}
- **Unverified Constraints:** {unverified_count}
- **Compliance Rate:** {compliance_rate:.2f}%

---

## Specifications Matrix

"""

    for spec_name in sorted(spec_to_constraints.keys()):
        spec_info = spec_to_constraints[spec_name]
        rel_path = spec_info["rel_path"]
        c_ids = spec_info["constraints"]
        
        if not c_ids:
            continue
            
        # Format a clean title from the spec name (e.g. layer-boundaries.md -> Layer Boundaries)
        clean_title = spec_name.replace(".md", "").replace("-", " ").title()
        
        markdown_content += f"### {clean_title}\n\n"
        markdown_content += f"Specification file: [`{rel_path}`](/reference/{spec_name.replace('.md', '.html')})\n\n"
        markdown_content += "| Constraint ID | Status | Mechanism | Verification Path |\n"
        markdown_content += "| :--- | :--- | :--- | :--- |\n"
        
        for norm_id in c_ids:
            info = spec_constraints[norm_id]
            display_id = info["display_id"]
            c_data = compliance_data["constraints"][display_id]
            
            if c_data["status"] == "VERIFIED":
                status_str = "✅ VERIFIED"
                mechanism_str = c_data["mechanism"]
                # Format verification paths nicely
                paths_str = "<br>".join([f"`{p['code_path']}:{p['line_number']}`" + (f" (Test: `{p['test_path']}`)" if p['test_path'] else "") for p in c_data["verification_paths"]])
            else:
                status_str = "❌ UNVERIFIED"
                mechanism_str = "*No implementation annotations found. Verification is pending.*"
                paths_str = "-"
                
            markdown_content += f"| `{display_id}` | {status_str} | {mechanism_str} | {paths_str} |\n"
            
        markdown_content += "\n"

    md_path = os.path.join(repo_root, "www", "content", "reference", "compliance.md")
    os.makedirs(os.path.dirname(md_path), exist_ok=True)
    with open(md_path, "w", encoding="utf-8") as f:
        f.write(markdown_content)
    print(f"Wrote compliance markdown matrix to {md_path}")

if __name__ == "__main__":
    main()
