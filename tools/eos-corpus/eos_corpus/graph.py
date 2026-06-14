"""DAG utilities: parse drv closures, compute structural metrics, assign coverage cells."""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from typing import Dict, List, Optional, Set, Tuple


@dataclass
class DrvNode:
    path: str
    name: str  # basename without hash and .drv
    deps: List[str] = field(default_factory=list)  # paths this node depends on


def _drv_name(path: str) -> str:
    """Extract human name from a drv path: /nix/store/HASH-NAME.drv → NAME."""
    base = path.rsplit("/", 1)[-1]
    # strip .drv suffix, then strip leading hash-
    base = re.sub(r"\.drv$", "", base)
    base = re.sub(r"^[a-z0-9]{32}-", "", base)
    return base


def parse_drv_closure(json_data: str | dict) -> Dict[str, DrvNode]:
    """Parse the JSON output of `nix derivation show --recursive`.

    The input is an object mapping drv paths to derivation descriptors.
    Each descriptor has an ``inputDrvs`` key (object mapping dep-path → outputs list).
    Returns a dict mapping drv-path → DrvNode.
    """
    if isinstance(json_data, str):
        raw = json.loads(json_data)
    else:
        raw = json_data

    nodes: Dict[str, DrvNode] = {}
    for path, desc in raw.items():
        deps = list(desc.get("inputDrvs", {}).keys())
        nodes[path] = DrvNode(path=path, name=_drv_name(path), deps=deps)
    return nodes


# ---------------------------------------------------------------------------
# Graph metrics
# ---------------------------------------------------------------------------

def _topo_order(nodes: Dict[str, DrvNode]) -> List[str]:
    """Return nodes in topological order (dependencies before dependents).

    Build order: deps first, top-level last.
    """
    in_degree: Dict[str, int] = {p: 0 for p in nodes}
    children: Dict[str, List[str]] = {p: [] for p in nodes}  # dep → dependents

    for path, node in nodes.items():
        for dep in node.deps:
            if dep in nodes:
                in_degree[path] += 1
                children[dep].append(path)

    queue = [p for p, d in in_degree.items() if d == 0]
    order: List[str] = []
    while queue:
        cur = queue.pop()
        order.append(cur)
        for child in children[cur]:
            in_degree[child] -= 1
            if in_degree[child] == 0:
                queue.append(child)
    return order


def compute_in_degrees(nodes: Dict[str, DrvNode]) -> Dict[str, int]:
    """Return in-degree for each node (number of nodes that depend on it)."""
    in_deg: Dict[str, int] = {p: 0 for p in nodes}
    for node in nodes.values():
        for dep in node.deps:
            if dep in in_deg:
                in_deg[dep] += 1
    return in_deg


def compute_depth(nodes: Dict[str, DrvNode]) -> int:
    """Longest path (edge count) from any source to any sink in build order.

    Build-order sources: nodes with no dependencies (leaves).
    Build-order sinks: nodes that nothing else depends on (top-level).
    Longest path = critical-path hop count.
    """
    if not nodes:
        return 0

    topo = _topo_order(nodes)
    # dist[p] = longest path (in hops) from any source to p
    dist: Dict[str, int] = {p: 0 for p in nodes}
    for p in topo:
        for dep in nodes[p].deps:
            if dep in dist:
                dist[p] = max(dist[p], dist[dep] + 1)
    return max(dist.values()) if dist else 0


def compute_cpr(
    nodes: Dict[str, DrvNode],
    durations: Optional[Dict[str, float]] = None,
    parallelism: int = 8,
) -> float:
    """Critical-path ratio: CP_duration / (sum_durations / P).

    When ``durations`` is None or a node is absent, unit duration (1.0) is used
    as a structural proxy (per spec).
    """
    if not nodes:
        return 0.0

    def dur(p: str) -> float:
        if durations and p in durations:
            return max(durations[p], 1e-9)
        return 1.0

    topo = _topo_order(nodes)
    cp: Dict[str, float] = {p: 0.0 for p in nodes}
    for p in topo:
        node_dur = dur(p)
        best_pred = max((cp[dep] for dep in nodes[p].deps if dep in cp), default=0.0)
        cp[p] = best_pred + node_dur

    critical_path = max(cp.values()) if cp else 0.0
    total = sum(dur(p) for p in nodes)
    avg = total / parallelism
    if avg < 1e-9:
        return 0.0
    return critical_path / avg


# ---------------------------------------------------------------------------
# Coverage matrix
# ---------------------------------------------------------------------------

# Cell names follow the spec table.
# Size thresholds are calibrated for nixpkgs full recursive derivation closures,
# where even "simple" packages (ripgrep, jq) have N ≈ 1,000 due to the bootstrap
# chain.  The spec's original (50, 500) thresholds assumed synthetic or
# per-package-only traces; the calibrated values are (1_100, 3_000).
SIZE_CELLS = {
    "small":  (None, 1_100),
    "medium": (1_100, 3_000),
    "large":  (3_000, None),
}
CPR_CELLS = {
    "low":  (None, 0.5),
    "mid":  (0.5, 2.0),
    "high": (2.0, None),
}


def size_bucket(n: int) -> str:
    for name, (lo, hi) in SIZE_CELLS.items():
        if (lo is None or n >= lo) and (hi is None or n < hi):
            return name
    return "large"


def cpr_bucket(cpr: float) -> str:
    if cpr <= 0.5:
        return "low"
    if cpr <= 2.0:
        return "mid"
    return "high"


def assign_coverage_cells(
    n: int,
    cpr: float,
    max_fanin: int,
) -> List[str]:
    """Return the coverage matrix cells this trace fills.

    Returns a list of cell ID strings.  A single trace may fill multiple cells.
    """
    cells: List[str] = []

    # Size × CPR grid (9 cells)
    sz = size_bucket(n)
    cr = cpr_bucket(cpr)
    cells.append(f"{sz}_{cr}_cpr")

    # Convergence axis (2 cells, independent of size/CPR)
    if max_fanin < 3:
        cells.append("low_convergence")
    if max_fanin >= 5:
        cells.append("high_convergence")

    return cells


ALL_COVERAGE_CELLS: List[str] = [
    f"{sz}_{cr}_cpr"
    for sz in ("small", "medium", "large")
    for cr in ("low", "mid", "high")
] + ["low_convergence", "high_convergence"]


@dataclass
class StructuralMetrics:
    n: int
    depth: int
    max_fanin: int
    convergence_density: float
    cpr: float
    coverage_cells: List[str]

    @classmethod
    def compute(
        cls,
        nodes: Dict[str, DrvNode],
        durations: Optional[Dict[str, float]] = None,
        parallelism: int = 8,
    ) -> "StructuralMetrics":
        n = len(nodes)
        in_deg = compute_in_degrees(nodes)
        max_fanin = max(in_deg.values()) if in_deg else 0
        convergence_density = (
            sum(1 for d in in_deg.values() if d >= 3) / n if n else 0.0
        )
        depth = compute_depth(nodes)
        cpr = compute_cpr(nodes, durations, parallelism)
        cells = assign_coverage_cells(n, cpr, max_fanin)
        return cls(
            n=n,
            depth=depth,
            max_fanin=max_fanin,
            convergence_density=convergence_density,
            cpr=cpr,
            coverage_cells=cells,
        )
