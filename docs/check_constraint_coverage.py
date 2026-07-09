#!/usr/bin/env python3
"""
Axios On-Path Constraint Coverage Check

- Quadrant: Reference (CI/gate check)
- Audience: Axios developers and the campaign coverage gate

Reads docs/on_path_constraints.json (produced by compliance_tracker.py) and
fails if any on-path constraint entry lacks BOTH a named evaluator and a
residue justification. This is the coverage-check overlay for AC1: it makes
the on-path constraint join real by refusing to pass silently over any
constraint the widened extractor discovered.

Usage: python3 docs/check_constraint_coverage.py [manifest_path]
Exit code: 0 if every entry is covered, 1 otherwise (violations printed).
"""

import json
import os
import sys


def check_coverage(manifest_path):
    with open(manifest_path, "r", encoding="utf-8") as f:
        data = json.load(f)

    entries = data.get("constraints", [])
    violations = []
    for entry in entries:
        evaluator = entry.get("evaluator", "")
        residue = entry.get("residue", "")
        if not evaluator and not residue:
            violations.append(entry)

    return entries, violations


def main():
    repo_root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    default_path = os.path.join(repo_root, "docs", "on_path_constraints.json")
    manifest_path = sys.argv[1] if len(sys.argv) > 1 else default_path

    entries, violations = check_coverage(manifest_path)

    if violations:
        print(f"FAIL: {len(violations)}/{len(entries)} on-path constraints lack both a named evaluator and a residue justification:")
        for v in violations:
            print(f"  - {v.get('id')} ({v.get('spec_file')}:{v.get('line')})")
        return 1

    print(f"PASS: all {len(entries)} on-path constraints have a named evaluator or a residue justification.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
