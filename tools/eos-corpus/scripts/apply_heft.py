#!/usr/bin/env python3
"""Post-process existing trace files to apply the full tier-2 duration model.

Reads each *.json trace in the given directory, computes per-node direct
input count from the edges array, and updates `duration` for every
non-measured node using fallback.tier2_duration(name, input_count=N, drv_id=ID).
Measured nodes (from Hydra) are left unchanged.

Usage:
    python3 apply_heft.py [trace_dir]

trace_dir defaults to tools/eos-sim-traces relative to the repo root.
"""

from __future__ import annotations

import json
import sys
from collections import Counter
from pathlib import Path

# Allow import from sibling package without installation.
sys.path.insert(0, str(Path(__file__).parent.parent))
from eos_corpus.fallback import tier2_duration


def apply_heft(path: Path) -> tuple[int, int]:
    """Update non-measured durations in a trace file. Returns (total, updated)."""
    with path.open() as f:
        trace = json.load(f)

    fan_out: Counter[str] = Counter()
    for edge in trace.get("edges", []):
        fan_out[edge["from"]] += 1

    updated = 0
    for node in trace["nodes"]:
        if node.get("measured"):
            continue
        input_count = fan_out.get(node["id"], 0)
        # plan_name is "pkg:output-name"; DrvNode.name is output-name without .drv
        name = node.get("plan_name", "").split(":")[-1] or node["id"].split("-", 1)[-1]
        # drv_id: the full basename (hash-name.drv) used as jitter seed
        drv_id = node["id"]
        new_dur = tier2_duration(name, input_count=input_count, drv_id=drv_id)
        if abs(new_dur - node["duration"]) > 0.01:
            node["duration"] = round(new_dur, 2)
            updated += 1

    with path.open("w") as f:
        json.dump(trace, f, indent=2)
        f.write("\n")

    return len(trace["nodes"]), updated


def main() -> None:
    repo_root = Path(__file__).parent.parent.parent.parent
    trace_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else repo_root / "tools/eos-sim-traces"

    files = sorted(trace_dir.glob("*.json"))
    if not files:
        print(f"No JSON files found in {trace_dir}", file=sys.stderr)
        sys.exit(1)

    total_nodes = total_updated = 0
    for p in files:
        nodes, updated = apply_heft(p)
        total_nodes += nodes
        total_updated += updated
        print(f"  {p.name}: {updated}/{nodes} nodes updated")

    print(f"\nDone: {total_updated}/{total_nodes} nodes updated across {len(files)} files")


if __name__ == "__main__":
    main()
