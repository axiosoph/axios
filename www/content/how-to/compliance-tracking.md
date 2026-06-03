+++
title = "Running the Spec Compliance Tracker"
description = "How to run the automated spec-compliance tracking script on the Axios codebase"
quadrant = "How-To"
audience = "Developers and CI/CD engineers running or extending the Axios compliance tracker"
+++

# Running the Spec Compliance Tracker

This guide shows you how to run the automated compliance tracking tool to verify specification compliance across the Axios codebase.

## Prerequisites

Ensure you have the following installed on your system:

- **Python 3.8** or higher
- The Axios repository cloned locally

## Step 1: Execute the Compliance Tracker

Run the Python script located in the `docs/` directory from the root of the repository:

```bash
python3 docs/compliance_tracker.py
```

The script performs the following actions:

1. Scans all specification files in `docs/specs/*.md` for constraints declared in their verification tables.
2. Recursively scans the codebase directories (`atom/`, `eos/`, `ion/`, `alurl/`) for compliance annotations of the form `// @spec-compliance[constraint-id]`.
3. Matches the annotations against the specification constraints.
4. Generates the status database `docs/compliance.json`.
5. Compiles the markdown compliance matrix at `www/content/compliance.md`.

## Step 2: Verify the Output Files

Verify that the following files have been generated or updated:

1. **`docs/compliance.json`** — The machine-readable database mapping each constraint to its status (`VERIFIED` or `UNVERIFIED`), describing the mechanism, and listing all code paths.
2. **`www/content/compliance.md`** — The human-readable spec compliance matrix.

## Step 3: Run the Website Build Process

After updating the compliance status, rebuild the documentation site using `sukr` (run from the `www/` directory):

```bash
# Process raw documentation and specs
python3 process_docs.py

# Build the static site
sukr
```
