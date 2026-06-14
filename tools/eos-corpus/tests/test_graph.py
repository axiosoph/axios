"""Tests for graph.py: drv closure parsing, structural metrics, coverage cells."""

import json
import pytest
from eos_corpus.graph import (
    DrvNode,
    StructuralMetrics,
    ALL_COVERAGE_CELLS,
    _drv_name,
    assign_coverage_cells,
    compute_cpr,
    compute_depth,
    compute_in_degrees,
    parse_drv_closure,
    size_bucket,
    cpr_bucket,
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def make_closure(*edges: tuple[str, list[str]]) -> dict:
    """Build a synthetic drv closure dict from (name, [dep_names]) pairs.

    Wraps each in a fake nix store path so parse_drv_closure can see it.
    """
    store = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-"
    raw = {}
    for name, deps in edges:
        path = store + name + ".drv"
        raw[path] = {
            "inputDrvs": {store + d + ".drv": ["out"] for d in deps},
            "name": name,
        }
    return raw


def nodes_from_edges(*edges: tuple[str, list[str]]) -> dict:
    return parse_drv_closure(make_closure(*edges))


# ---------------------------------------------------------------------------
# parse_drv_closure
# ---------------------------------------------------------------------------

class TestParseDrvClosure:
    def test_parses_dict_input(self):
        raw = make_closure(("pkg-1.0", []), ("dep-1.0", []))
        nodes = parse_drv_closure(raw)
        assert len(nodes) == 2

    def test_parses_json_string(self):
        raw = make_closure(("pkg-1.0", []))
        nodes = parse_drv_closure(json.dumps(raw))
        assert len(nodes) == 1

    def test_dep_links_resolved(self):
        raw = make_closure(("top", ["dep"]), ("dep", []))
        nodes = parse_drv_closure(raw)
        top_path = [k for k in nodes if "top" in k][0]
        dep_path = [k for k in nodes if "dep" in k][0]
        assert dep_path in nodes[top_path].deps

    def test_empty_closure(self):
        assert parse_drv_closure({}) == {}

    def test_drv_name_extraction(self):
        assert _drv_name("/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-ripgrep-14.1.1.drv") == "ripgrep-14.1.1"
        assert _drv_name("/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-source.drv") == "source"


# ---------------------------------------------------------------------------
# in-degree
# ---------------------------------------------------------------------------

class TestInDegrees:
    def test_single_node(self):
        nodes = nodes_from_edges(("a", []))
        assert compute_in_degrees(nodes) == {list(nodes.keys())[0]: 0}

    def test_chain(self):
        # a → b → c  (a depends on b depends on c)
        nodes = nodes_from_edges(("a", ["b"]), ("b", ["c"]), ("c", []))
        in_deg = compute_in_degrees(nodes)
        # c is depended on by b (in_deg=1), b by a (in_deg=1), a by nobody (in_deg=0)
        names = {n.name: k for k, n in nodes.items()}
        assert in_deg[names["a"]] == 0
        assert in_deg[names["b"]] == 1
        assert in_deg[names["c"]] == 1

    def test_diamond_fanin(self):
        # top → a, top → b; a → leaf, b → leaf
        nodes = nodes_from_edges(
            ("top", ["a", "b"]), ("a", ["leaf"]), ("b", ["leaf"]), ("leaf", [])
        )
        in_deg = compute_in_degrees(nodes)
        names = {n.name: k for k, n in nodes.items()}
        assert in_deg[names["leaf"]] == 2
        assert in_deg[names["a"]] == 1
        assert in_deg[names["top"]] == 0


# ---------------------------------------------------------------------------
# depth
# ---------------------------------------------------------------------------

class TestDepth:
    def test_single_node_depth_zero(self):
        nodes = nodes_from_edges(("a", []))
        assert compute_depth(nodes) == 0

    def test_chain_depth(self):
        # a→b→c: depth = 2
        nodes = nodes_from_edges(("a", ["b"]), ("b", ["c"]), ("c", []))
        assert compute_depth(nodes) == 2

    def test_diamond_depth(self):
        # top→a→leaf, top→b→leaf: depth = 2
        nodes = nodes_from_edges(
            ("top", ["a", "b"]), ("a", ["leaf"]), ("b", ["leaf"]), ("leaf", [])
        )
        assert compute_depth(nodes) == 2

    def test_wide_graph_depth_one(self):
        # top depends on 10 leaves: depth = 1
        nodes = nodes_from_edges(
            ("top", [f"leaf{i}" for i in range(10)]),
            *[(f"leaf{i}", []) for i in range(10)],
        )
        assert compute_depth(nodes) == 1


# ---------------------------------------------------------------------------
# CPR
# ---------------------------------------------------------------------------

class TestCPR:
    def test_single_node_cpr(self):
        nodes = nodes_from_edges(("a", []))
        # CP=1, sum/8=1/8, CPR=8
        cpr = compute_cpr(nodes, parallelism=8)
        assert abs(cpr - 8.0) < 1e-9

    def test_chain_cpr(self):
        # 3-node chain, all unit duration: CP=3, sum/8=3/8, CPR=8
        nodes = nodes_from_edges(("a", ["b"]), ("b", ["c"]), ("c", []))
        cpr = compute_cpr(nodes, parallelism=8)
        assert abs(cpr - 8.0) < 1e-9

    def test_wide_low_cpr(self):
        # top (dur=1) + 15 leaves (dur=1 each): CP=2, sum=16, sum/8=2, CPR=1.0
        nodes = nodes_from_edges(
            ("top", [f"l{i}" for i in range(15)]),
            *[(f"l{i}", []) for i in range(15)],
        )
        cpr = compute_cpr(nodes, parallelism=8)
        # CP=2, total=16, avg=2, CPR=1.0
        assert abs(cpr - 1.0) < 1e-9

    def test_custom_durations(self):
        nodes = nodes_from_edges(("top", ["dep"]), ("dep", []))
        names = {n.name: k for k, n in nodes.items()}
        durations = {names["top"]: 10.0, names["dep"]: 100.0}
        cpr = compute_cpr(nodes, durations, parallelism=8)
        # CP = 100+10=110, sum=110, avg=110/8, CPR=8.0
        assert abs(cpr - 8.0) < 1e-9

    def test_empty_graph(self):
        assert compute_cpr({}) == 0.0


# ---------------------------------------------------------------------------
# Coverage matrix
# ---------------------------------------------------------------------------

class TestCoverageMatrix:
    def test_all_cells_defined(self):
        assert len(ALL_COVERAGE_CELLS) == 11  # 9 size×CPR + 2 convergence

    def test_size_buckets(self):
        assert size_bucket(0) == "small"
        assert size_bucket(49) == "small"
        assert size_bucket(50) == "medium"
        assert size_bucket(500) == "medium"
        assert size_bucket(501) == "large"

    def test_cpr_buckets(self):
        assert cpr_bucket(0.0) == "low"
        assert cpr_bucket(0.5) == "low"
        assert cpr_bucket(0.51) == "mid"
        assert cpr_bucket(2.0) == "mid"
        assert cpr_bucket(2.01) == "high"

    def test_assign_cells_small_high_cpr(self):
        cells = assign_coverage_cells(n=10, cpr=3.0, max_fanin=2)
        assert "small_high_cpr" in cells
        assert "low_convergence" in cells  # max_fanin=2 < 3

    def test_assign_cells_large_low_cpr_high_conv(self):
        cells = assign_coverage_cells(n=1000, cpr=0.3, max_fanin=8)
        assert "large_low_cpr" in cells
        assert "high_convergence" in cells
        assert "low_convergence" not in cells

    def test_assign_cells_can_fill_multiple(self):
        # A single large trace can fill both large_xxx and a convergence cell
        cells = assign_coverage_cells(n=600, cpr=1.0, max_fanin=5)
        assert "large_mid_cpr" in cells
        assert "high_convergence" in cells


# ---------------------------------------------------------------------------
# StructuralMetrics.compute
# ---------------------------------------------------------------------------

class TestStructuralMetrics:
    def test_diamond(self):
        nodes = nodes_from_edges(
            ("top", ["a", "b"]), ("a", ["leaf"]), ("b", ["leaf"]), ("leaf", [])
        )
        m = StructuralMetrics.compute(nodes)
        assert m.n == 4
        assert m.depth == 2
        assert m.max_fanin == 2  # leaf has in-degree 2
        assert m.convergence_density == 0.0  # no node has fanin≥3
        assert "small" in m.coverage_cells[0]

    def test_convergence_density(self):
        # star: top depends on all leaves; one shared leaf depended on by 3 others
        nodes = nodes_from_edges(
            ("top", ["a", "b", "c"]),
            ("a", ["shared"]),
            ("b", ["shared"]),
            ("c", ["shared"]),
            ("shared", []),
        )
        m = StructuralMetrics.compute(nodes)
        assert m.max_fanin == 3
        # shared has fanin=3; 1 of 5 nodes → density=0.2
        assert abs(m.convergence_density - 0.2) < 1e-9
        assert "high_convergence" not in m.coverage_cells  # need fanin≥5
