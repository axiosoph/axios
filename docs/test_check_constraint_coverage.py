#!/usr/bin/env python3
"""
Axios On-Path Constraint Coverage Check — tests

- Quadrant: Reference (test harness for check_constraint_coverage.py)
- Audience: Axios developers verifying the AC1 coverage-check overlay.

Standalone assertion-based tests (no pytest dependency in this environment):
run directly with `python3 docs/test_check_constraint_coverage.py`. Exits
non-zero if any test fails. Demonstrates the required red/green pair: a
manifest entry with neither evaluator nor residue fails; removing it (or
supplying either field) passes.
"""

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
CHECKER = os.path.join(HERE, "check_constraint_coverage.py")

sys.path.insert(0, HERE)
import check_constraint_coverage as cc


def _write_manifest(entries):
    fd, path = tempfile.mkstemp(suffix=".json")
    with os.fdopen(fd, "w", encoding="utf-8") as f:
        json.dump({"constraints": entries}, f)
    return path


def test_all_covered_passes():
    path = _write_manifest([
        {"id": "a", "spec_file": "x.md", "line": 1, "evaluator": "unit-test", "residue": ""},
        {"id": "b", "spec_file": "x.md", "line": 2, "evaluator": "", "residue": "no VERIFIED annotation found in spec prose"},
    ])
    try:
        entries, violations = cc.check_coverage(path)
        assert len(entries) == 2
        assert violations == []
    finally:
        os.remove(path)


def test_unnamed_evaluator_entry_fails():
    path = _write_manifest([
        {"id": "a", "spec_file": "x.md", "line": 1, "evaluator": "unit-test", "residue": ""},
        {"id": "seeded-bad", "spec_file": "x.md", "line": 3, "evaluator": "", "residue": ""},
    ])
    try:
        entries, violations = cc.check_coverage(path)
        assert len(entries) == 2
        assert len(violations) == 1
        assert violations[0]["id"] == "seeded-bad"
    finally:
        os.remove(path)


def test_cli_exit_codes_red_then_green():
    bad_path = _write_manifest([
        {"id": "seeded-bad", "spec_file": "x.md", "line": 3, "evaluator": "", "residue": ""},
    ])
    good_path = _write_manifest([
        {"id": "a", "spec_file": "x.md", "line": 1, "evaluator": "unit-test", "residue": ""},
    ])
    try:
        bad_result = subprocess.run([sys.executable, CHECKER, bad_path], capture_output=True, text=True)
        assert bad_result.returncode != 0, f"expected non-zero exit, got {bad_result.returncode}: {bad_result.stdout}"

        good_result = subprocess.run([sys.executable, CHECKER, good_path], capture_output=True, text=True)
        assert good_result.returncode == 0, f"expected zero exit, got {good_result.returncode}: {good_result.stdout}"
    finally:
        os.remove(bad_path)
        os.remove(good_path)


if __name__ == "__main__":
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_") and callable(v)]
    failures = []
    for t in tests:
        try:
            t()
            print(f"PASS {t.__name__}")
        except AssertionError as e:
            failures.append(t.__name__)
            print(f"FAIL {t.__name__}: {e}")
        except Exception as e:
            failures.append(t.__name__)
            print(f"ERROR {t.__name__}: {e!r}")
    if failures:
        print(f"\n{len(failures)}/{len(tests)} tests failed: {failures}")
        sys.exit(1)
    print(f"\nAll {len(tests)} tests passed.")
