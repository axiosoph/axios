"""P10 sweep harness unit tests (TDD)."""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent))

from eos_sweep.sweep import (
    COMPILER_DURATION,
    OTHER_DURATION,
    build_starvation_trace,
    inject_workers,
    scale_durations,
)
from eos_sweep.analyze import dominant_fraction, pareto_frontier, rank_within_trace


# ---------------------------------------------------------------------------
# scale_durations
# ---------------------------------------------------------------------------

def _base_trace():
    return {
        "nodes": [
            {"id": "a", "duration": 75.0},   # other bucket
            {"id": "b", "duration": 510.0},  # compiler bucket
            {"id": "c", "duration": 6.0},    # fetcher – unchanged
            {"id": "d", "duration": 3.0},    # hook – unchanged
            {"id": "e", "duration": 17.0},   # doc – unchanged
        ],
        "edges": [],
        "workers": [{"id": "w0", "speed": 1.0}],
    }


def test_scale_durations_other_bucket():
    t = scale_durations(_base_trace(), other_scale=2.0, compiler_scale=1.0)
    durations = {n["id"]: n["duration"] for n in t["nodes"]}
    assert durations["a"] == 150.0


def test_scale_durations_compiler_bucket():
    t = scale_durations(_base_trace(), other_scale=1.0, compiler_scale=0.5)
    durations = {n["id"]: n["duration"] for n in t["nodes"]}
    assert durations["b"] == 255.0


def test_scale_durations_fetcher_unchanged():
    t = scale_durations(_base_trace(), other_scale=5.0, compiler_scale=5.0)
    durations = {n["id"]: n["duration"] for n in t["nodes"]}
    assert durations["c"] == 6.0
    assert durations["d"] == 3.0
    assert durations["e"] == 17.0


def test_scale_durations_identity():
    orig = _base_trace()
    scaled = scale_durations(orig, other_scale=1.0, compiler_scale=1.0)
    assert scaled["nodes"] == orig["nodes"]


def test_scale_durations_does_not_mutate():
    orig = _base_trace()
    scale_durations(orig, other_scale=2.0, compiler_scale=2.0)
    assert orig["nodes"][0]["duration"] == 75.0


def test_scale_durations_constants():
    assert OTHER_DURATION == 75.0
    assert COMPILER_DURATION == 510.0


# ---------------------------------------------------------------------------
# inject_workers
# ---------------------------------------------------------------------------

def test_inject_workers_replaces_pool():
    trace = _base_trace()
    new_workers = [
        {"id": "x0", "speed": 0.5},
        {"id": "x1", "speed": 1.0},
        {"id": "x2", "speed": 2.0},
    ]
    result = inject_workers(trace, new_workers)
    assert len(result["workers"]) == 3
    assert result["workers"][0]["id"] == "x0"


def test_inject_workers_preserves_store_cached():
    trace = {**_base_trace(), "store_cached": ["plan-abc", "plan-def"]}
    result = inject_workers(trace, [{"id": "w0", "speed": 1.0}])
    assert result["store_cached"] == ["plan-abc", "plan-def"]


def test_inject_workers_does_not_mutate():
    trace = _base_trace()
    orig_workers = trace["workers"].copy()
    inject_workers(trace, [{"id": "new", "speed": 1.0}])
    assert trace["workers"] == orig_workers


# ---------------------------------------------------------------------------
# build_starvation_trace
# ---------------------------------------------------------------------------

def test_starvation_trace_structure():
    t = build_starvation_trace(k_high=5, d_high=2.0, d_hub=11.0)
    ids = {n["id"] for n in t["nodes"]}
    assert "L" in ids
    assert "F" in ids
    for k in range(5):
        assert f"H{k}" in ids


def test_starvation_trace_single_worker():
    t = build_starvation_trace(k_high=3, d_high=2.0, d_hub=11.0)
    assert len(t["workers"]) == 1


def test_starvation_trace_hub_edges():
    t = build_starvation_trace(k_high=4, d_high=2.0, d_hub=11.0)
    # Every H_k depends on F (edge from F to H_k means F is dependency of H_k)
    hub_edges = [(e["from"], e["to"]) for e in t["edges"]]
    for k in range(4):
        assert ("F", f"H{k}") in hub_edges


# ---------------------------------------------------------------------------
# pareto_frontier
# ---------------------------------------------------------------------------

def test_pareto_frontier_basic():
    points = [
        {"makespan": 10, "redundant": 5},
        {"makespan": 8,  "redundant": 8},
        {"makespan": 5,  "redundant": 12},
        {"makespan": 7,  "redundant": 7},  # dominated by (8,8) is not; actually (7,7) dominates (8,8)
        {"makespan": 12, "redundant": 3},
    ]
    frontier = pareto_frontier(points, x="makespan", y="redundant")
    # (7,7) dominates (8,8) and (10,5) is not dominated, (5,12) is not dominated
    # (7,7) not dominated by anything smaller on both axes
    # (10,5) not dominated — no point beats it on redundant
    # (12,3) not dominated — no point beats it on redundant
    # (5,12) not dominated — no point beats it on makespan
    # (8,8) dominated by (7,7) on both axes
    ids_in_frontier = {(p["makespan"], p["redundant"]) for p in frontier}
    assert (8, 8) not in ids_in_frontier
    assert (5, 12) in ids_in_frontier
    assert (12, 3) in ids_in_frontier


def test_pareto_frontier_single_optimal():
    points = [
        {"makespan": 1, "redundant": 1},
        {"makespan": 2, "redundant": 2},
        {"makespan": 3, "redundant": 3},
    ]
    frontier = pareto_frontier(points, x="makespan", y="redundant")
    assert len(frontier) == 1
    assert frontier[0]["makespan"] == 1


def test_pareto_frontier_all_optimal():
    points = [
        {"makespan": 1, "redundant": 3},
        {"makespan": 2, "redundant": 2},
        {"makespan": 3, "redundant": 1},
    ]
    frontier = pareto_frontier(points, x="makespan", y="redundant")
    assert len(frontier) == 3


# ---------------------------------------------------------------------------
# dominant_fraction
# ---------------------------------------------------------------------------

def _make_results(rows):
    """rows: list of (trace, variant, makespan)"""
    return [
        {"trace": t, "variant": v, "seeding": "from-scratch",
         "delta": 0.0, "gamma": 0.0, "lambda": 1.0,
         "other_scale": 1.0, "compiler_scale": 1.0,
         "metrics": {"makespan": ms, "redundant_work": 0.0, "ep_count": 10,
                     "mean_utilization": 1.0, "critical_path_accuracy": 1.0,
                     "max_dispatch_wait": 0.0, "objective": ms}}
        for t, v, ms in rows
    ]


def test_dominant_fraction_h1_always_better():
    results = _make_results([
        ("trace_a.json", "H1", 100.0),
        ("trace_a.json", "H4", 120.0),
        ("trace_b.json", "H1", 200.0),
        ("trace_b.json", "H4", 250.0),
    ])
    frac = dominant_fraction(results, winner="H1", loser="H4",
                             metric="makespan", seeding="from-scratch",
                             delta=0.0, gamma=0.0)
    assert frac == 1.0


def test_dominant_fraction_h1_sometimes_better():
    results = _make_results([
        ("trace_a.json", "H1", 100.0),
        ("trace_a.json", "H4", 90.0),   # H4 wins on a
        ("trace_b.json", "H1", 200.0),
        ("trace_b.json", "H4", 250.0),  # H1 wins on b
    ])
    frac = dominant_fraction(results, winner="H1", loser="H4",
                             metric="makespan", seeding="from-scratch",
                             delta=0.0, gamma=0.0)
    assert frac == 0.5


# ---------------------------------------------------------------------------
# rank_within_trace
# ---------------------------------------------------------------------------

def test_rank_within_trace_h1_best():
    results = _make_results([
        ("t.json", "H1", 80.0),
        ("t.json", "H2", 90.0),
        ("t.json", "H3", 95.0),
        ("t.json", "H4", 100.0),
    ])
    ranked = rank_within_trace(results, metric="makespan",
                               seeding="from-scratch", delta=0.0, gamma=0.0)
    assert ranked[0] == "H1"
    assert ranked[-1] == "H4"


def test_rank_within_trace_multiple_traces():
    results = _make_results([
        ("t1.json", "H1", 80.0),
        ("t1.json", "H4", 100.0),
        ("t2.json", "H1", 90.0),
        ("t2.json", "H4", 85.0),  # H4 wins on t2
    ])
    # H1 wins 1/2, H4 wins 1/2 — median relative perf should be close
    ranked = rank_within_trace(results, metric="makespan",
                               seeding="from-scratch", delta=0.0, gamma=0.0)
    # Both variants present; result is a list of 2 variants
    assert set(ranked) == {"H1", "H4"}
