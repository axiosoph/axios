+++
title = "Running the spec compliance tracker"
description = "How to run the automated spec-compliance tracking script on the Axios codebase"
quadrant = "How-To"
audience = "Developers and CI/CD engineers running or extending the compliance tracker"
+++

Run the compliance tracker to check specification coverage across the codebase.

## Prerequisites

- Python 3.8 or higher
- The Axios repository cloned locally

## Step 1: Run the tracker

From the repository root:

```bash
python3 docs/compliance_tracker.py
```

The script:

1. Scans `docs/specs/*.md` for constraints declared in verification tables.
2. Recursively searches `atom/`, `eos/`, `ion/`, and `alurl/` for annotations matching `// @spec-compliance[constraint-id]`.
3. Matches annotations against spec constraints.
4. Writes `docs/compliance.json` (machine-readable status database).
5. Writes `www/content/reference/compliance.md` (human-readable matrix).

## Step 2: Check the output

Verify that both files were generated:

1. `docs/compliance.json` — Maps each constraint to its status (`VERIFIED` or `UNVERIFIED`), describes the verification mechanism, and lists code paths.
2. `www/content/reference/compliance.md` — The rendered compliance matrix.

## Step 3: Rebuild the site

After updating compliance status, rebuild from the `www/` directory:

```bash
python3 process_docs.py
sukr
```
