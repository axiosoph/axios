"""Merge per-package trace JSONs into a unified multi-package DAG."""
from __future__ import annotations

from typing import Sequence


def merge_traces(traces: Sequence[dict]) -> dict:
    """Merge per-package traces into a unified multi-package DAG.

    Node IDs are nixpkgs store-path hashes (globally unique per corpus
    anchor).  Deduplication by ID is therefore semantically correct:
    shared dependencies appear exactly once in the merged graph, and
    their fan-in reflects all downstream packages that require them.

    Collision policy:
    - duration / measured / peak_mem: first-occurrence wins (same anchor
      guarantees identical values across per-package traces).
    - is_atom: True from any source takes precedence, preserving every
      requested package's root as an atom in the unified graph.
    - plan_name: first-occurrence wins.

    The returned trace has store_cached=[] (base / cold-start state).
    Cache-state variants should be generated via emit_cache_variants().
    """
    seen: dict[str, dict] = {}
    edge_set: set[tuple[str, str]] = set()
    edges: list[dict] = []

    for trace in traces:
        for node in trace["nodes"]:
            nid = node["id"]
            if nid not in seen:
                seen[nid] = dict(node)
            elif node.get("is_atom"):
                seen[nid]["is_atom"] = True

        for edge in trace["edges"]:
            key = (edge["from"], edge["to"])
            if key not in edge_set:
                edge_set.add(key)
                edges.append({"from": edge["from"], "to": edge["to"]})

    workers = (
        list(traces[0]["workers"])
        if traces
        else [{"id": f"w{i}", "speed": 1.0} for i in range(8)]
    )

    return {
        "nodes": list(seen.values()),
        "edges": edges,
        "workers": workers,
        "store_cached": [],
    }
