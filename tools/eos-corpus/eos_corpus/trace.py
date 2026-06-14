"""Emit simulator-compatible trace JSON from a drv closure with duration data."""

from __future__ import annotations

import json
from typing import Dict, List, Optional

from .graph import DrvNode


_DEFAULT_WORKERS = 8


def _make_workers(n: int = _DEFAULT_WORKERS) -> List[dict]:
    return [{"id": f"w{i}", "speed": 1.0} for i in range(n)]


def emit_trace(
    nodes: Dict[str, DrvNode],
    durations: Dict[str, float],
    measured_flags: Dict[str, bool],
    atom_path: Optional[str] = None,
    store_cached: Optional[List[str]] = None,
    workers: Optional[List[dict]] = None,
    pkg_name: str = "",
) -> dict:
    """Build a trace dict consumable by eos-sim.

    Parameters
    ----------
    nodes:
        Parsed drv closure (path → DrvNode).
    durations:
        Duration in seconds per drv path.
    measured_flags:
        True = Hydra buildstep timing; False = fallback estimate.
    atom_path:
        The top-level drv path for this package.  Nodes whose path equals
        ``atom_path`` get ``is_atom=True``; all others get ``is_atom=False``.
        This is the proxy rule documented in provenance: the top-level
        ``pkgs``-attribute derivation is the atom boundary.
    store_cached:
        List of drv-path ids already in the global store (cold/partial/warm
        variants set this differently).
    workers:
        Worker pool spec.  Defaults to 8 homogeneous workers.
    pkg_name:
        Human-readable package name stored in each node's ``plan_name``.
    """
    if workers is None:
        workers = _make_workers()
    if store_cached is None:
        store_cached = []

    trace_nodes = []
    for path, node in nodes.items():
        duration = durations.get(path, 1.0)
        measured = measured_flags.get(path, False)
        is_atom = path == atom_path
        trace_nodes.append({
            "id": path,
            "duration": duration,
            "measured": measured,
            "is_atom": is_atom,
            "peak_mem": None,
            "plan_name": f"{pkg_name}:{node.name}" if pkg_name else node.name,
        })

    trace_edges = []
    for path, node in nodes.items():
        for dep in node.deps:
            if dep in nodes:
                trace_edges.append({"from": path, "to": dep})

    return {
        "nodes": trace_nodes,
        "edges": trace_edges,
        "workers": workers,
        "store_cached": store_cached,
    }


def emit_cache_variants(
    base_trace: dict,
    n: int,
) -> Dict[str, dict]:
    """Produce cold / partial / warm cache-state variants for traces with N > 500.

    cold   — store_cached: []
    partial — all nodes at depth ≥ N/2 marked cached
    warm    — all nodes outside the top-20%-by-node-count (by id sort) marked cached

    The base_trace already has store_cached=[] (cold state); this function
    derives the other two.
    """
    import copy

    all_ids = [n["id"] for n in base_trace["nodes"]]

    # partial: simulate that deeper dependencies are already cached.
    # We approximate depth by position in the edges graph: nodes with
    # out-degree 0 are sinks (leaves); we use a simple heuristic —
    # the second half of the topological order (already encoded in edge
    # structure) represents deeper deps.
    # For simplicity here, mark the last N/2 node IDs (by position in list)
    # as cached — callers should pass nodes already in topo order.
    half = len(all_ids) // 2
    partial_cached = all_ids[half:]

    # warm: only top-20% (first 20%) by node count NOT cached
    top_20pct = max(1, len(all_ids) // 5)
    warm_cached = all_ids[top_20pct:]

    cold = copy.deepcopy(base_trace)
    cold["store_cached"] = []

    partial = copy.deepcopy(base_trace)
    partial["store_cached"] = partial_cached

    warm = copy.deepcopy(base_trace)
    warm["store_cached"] = warm_cached

    return {"cold": cold, "partial": partial, "warm": warm}


def write_trace(path: str, trace: dict) -> None:
    """Write a trace dict to a JSON file."""
    with open(path, "w") as fh:
        json.dump(trace, fh, indent=2)
    # terminate with newline
    with open(path, "a") as fh:
        fh.write("\n")
