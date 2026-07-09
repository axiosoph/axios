#!/usr/bin/env python3
"""
Axios Specification Compliance Tracker — extraction tests

- Quadrant: Reference (test harness for the constraint extractor)
- Audience: Axios developers verifying compliance_tracker.py's constraint
  extraction against docs/specs/*.md.

Standalone assertion-based tests (no pytest dependency in this environment):
run directly with `python3 docs/test_compliance_tracker.py`. Exits non-zero
if any test fails.
"""

import json
import os
import re
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import compliance_tracker as ct
import check_constraint_coverage as cc

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# The 13 charter constraint IDs named at atom-transactions.md's
# 2026-07-08 charter-amendment coverage note (:988-996): the 10 explicitly
# listed IDs plus the "three amended anchor invariants".
CHARTER_IDS = {
    "charter-typ", "charter-anchor", "claim-chains-charter",
    "claim-charter-authorization", "claim-replacement-authority",
    "charter-ancestry", "charter-succession", "charter-succession-linear",
    "chain-monotonicity", "charter-fork-distinction",
    "anchor-immutable", "anchor-content-addressed", "anchor-resolvable",
}


def _extracted_ids(spec_relpath):
    path = os.path.join(REPO_ROOT, spec_relpath)
    return {norm_id for norm_id, _ in ct.extract_constraints_from_spec(path)}


def test_lock_ids_discovered():
    path = os.path.join(REPO_ROOT, "docs/specs/lock-file-schema.md")
    with open(path, encoding="utf-8") as f:
        text = f.read()
    expected = {m.strip("[]").lower() for m in re.findall(r"\[lock-[a-z-]+\]", text)}
    assert len(expected) >= 36, f"expected lock-ID population itself shrank: {len(expected)}"
    extracted = _extracted_ids("docs/specs/lock-file-schema.md")
    missing = expected - extracted
    assert not missing, f"lock IDs missing from extraction: {sorted(missing)}"


def test_charter_ids_discovered():
    assert len(CHARTER_IDS) == 13
    extracted = _extracted_ids("docs/specs/atom-transactions.md")
    missing = CHARTER_IDS - extracted
    assert not missing, f"charter IDs missing from extraction: {sorted(missing)}"


def test_no_duplicate_ids_per_spec():
    # atom-sourcing.md defines constraints inline AND re-summarizes some of
    # them in a trailing Verification table (e.g. no-unpublished-dependency).
    # The widened extractor must dedupe within one spec, not double-count.
    path = os.path.join(REPO_ROOT, "docs/specs/atom-sourcing.md")
    pairs = ct.extract_constraints_from_spec(path)
    ids = [norm_id for norm_id, _ in pairs]
    assert len(ids) == len(set(ids)), "duplicate constraint ids extracted from one spec"


def test_cross_file_same_id_stays_distinct():
    # lock-schema-version is genuinely (re)defined in both specs; the
    # on-path manifest must keep them as two entries, not collapse them.
    manifest = ct.build_constraint_manifest(REPO_ROOT)
    hits = [e for e in manifest if e["id"] == "lock-schema-version"]
    spec_files = {e["spec_file"] for e in hits}
    assert len(hits) >= 2 and len(spec_files) >= 2, (
        f"expected lock-schema-version to appear once per defining spec, got {hits}"
    )


def test_manifest_never_fabricates_residue():
    # charter-anchor's spec text states only the bare status
    # 'unverified (models require extension — see Verification note)' — no
    # mechanism is named. The manifest must leave this gap genuinely open
    # (evaluator AND residue both empty), never launder it into a passing
    # entry by manufacturing residue text from the absence itself.
    manifest = ct.build_constraint_manifest(REPO_ROOT)
    hits = [e for e in manifest if e["id"] == "charter-anchor" and e["spec_file"] == "docs/specs/atom-transactions.md"]
    assert len(hits) == 1, f"expected exactly one charter-anchor entry, got {hits}"
    entry = hits[0]
    assert entry["evaluator"] == "", f"charter-anchor should have no named evaluator, got {entry}"
    assert entry["residue"] == "", f"residue must not be auto-fabricated, got {entry}"
    assert entry["spec_status"], "spec_status should still carry the raw spec-stated text for triage"


def test_coverage_check_has_teeth_on_real_manifest():
    # The defect this guards against: a manifest generator that always
    # fills residue for every gap makes check_constraint_coverage.py
    # incapable of ever failing on real output. Build the real manifest,
    # write it exactly as compliance_tracker.py does, and confirm the
    # checker actually flags the genuine on-path coverage gaps.
    manifest = ct.build_constraint_manifest(REPO_ROOT)
    true_gaps = [e for e in manifest if not e["evaluator"] and not e["residue"]]
    assert len(true_gaps) >= 50, (
        f"expected the known ~63 genuine coverage gaps to still be present, got {len(true_gaps)}"
    )

    fd, path = tempfile.mkstemp(suffix=".json")
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump({"constraints": manifest}, f)
        entries, violations = cc.check_coverage(path)
        assert len(entries) == len(manifest)
        assert len(violations) == len(true_gaps), (
            f"coverage-check violation count ({len(violations)}) should match "
            f"the genuine gap count ({len(true_gaps)}) — a mismatch means the "
            f"checker is silently passing over real gaps again"
        )
    finally:
        os.remove(path)


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
