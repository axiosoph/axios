"""P10 analysis: reads sweep JSONL and produces the recommendation memo."""
from __future__ import annotations

import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any


# ---------------------------------------------------------------------------
# Pure analysis primitives
# ---------------------------------------------------------------------------

def pareto_frontier(points: list[dict], x: str, y: str) -> list[dict]:
    """Return the Pareto-optimal subset of *points* minimising both *x* and *y*.

    A point P dominates Q iff P[x] <= Q[x] and P[y] <= Q[y] with at least
    one strict inequality.
    """
    frontier: list[dict] = []
    for p in points:
        dominated = False
        for q in points:
            if q is p:
                continue
            if q[x] <= p[x] and q[y] <= p[y] and (q[x] < p[x] or q[y] < p[y]):
                dominated = True
                break
        if not dominated:
            frontier.append(p)
    return frontier


def dominant_fraction(
    results: list[dict],
    winner: str,
    loser: str,
    metric: str,
    seeding: str = "from-scratch",
    delta: float = 0.0,
    gamma: float = 0.0,
    worker_pool: str = "medium_homogeneous",
) -> float:
    """Fraction of traces where *winner* beats *loser* on *metric* (lower is better).

    Filters to the given seeding/delta/gamma/worker_pool slice.  Returns the
    fraction of traces in which winner's metric value is strictly less than
    loser's.
    """
    filtered = [
        r for r in results
        if r["seeding"] == seeding
        and r["delta"] == delta
        and r["gamma"] == gamma
        and r["lambda"] == 1.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
        and r.get("worker_pool", "medium_homogeneous") == worker_pool
    ]
    by_trace: dict[str, dict[str, float]] = defaultdict(dict)
    for r in filtered:
        if r["variant"] in (winner, loser):
            by_trace[r["trace"]][r["variant"]] = r["metrics"][metric]

    wins = 0
    total = 0
    for trace_scores in by_trace.values():
        if winner in trace_scores and loser in trace_scores:
            total += 1
            if trace_scores[winner] < trace_scores[loser]:
                wins += 1
    return wins / total if total > 0 else 0.0


def rank_within_trace(
    results: list[dict],
    metric: str,
    seeding: str = "from-scratch",
    delta: float = 0.0,
    gamma: float = 0.0,
    worker_pool: str = "medium_homogeneous",
) -> list[str]:
    """Rank variants by median relative performance within each trace.

    For each trace, compute each variant's ratio to the best variant on that
    trace (so the best is 1.0, worse variants are > 1.0).  Then rank variants
    by their median ratio across all traces (ascending = better).

    Filters to the default θ, λ=1, no duration scaling, given seeding/δ/γ.
    """
    filtered = [
        r for r in results
        if r["seeding"] == seeding
        and r["delta"] == delta
        and r["gamma"] == gamma
        and r["lambda"] == 1.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
        and r.get("worker_pool", "medium_homogeneous") == worker_pool
    ]

    # Group by trace
    by_trace: dict[str, dict[str, float]] = defaultdict(dict)
    for r in filtered:
        by_trace[r["trace"]][r["variant"]] = r["metrics"][metric]

    # Per-trace relative scores
    variant_ratios: dict[str, list[float]] = defaultdict(list)
    for trace_scores in by_trace.values():
        if not trace_scores:
            continue
        best = min(trace_scores.values())
        if best <= 0:
            continue
        for v, score in trace_scores.items():
            variant_ratios[v].append(score / best)

    # Rank by median ratio
    medians = {v: statistics.median(ratios) for v, ratios in variant_ratios.items()}
    return sorted(medians, key=lambda v: medians[v])


def relative_effect(results: list[dict], v1: str, v2: str, metric: str,
                    seeding: str = "from-scratch", delta: float = 0.0,
                    gamma: float = 0.0,
                    worker_pool: str = "medium_homogeneous") -> dict:
    """Compute median and IQR of the (v1−v2)/v2 relative difference across traces.

    Positive = v1 is worse; negative = v1 is better.
    """
    filtered = [
        r for r in results
        if r["seeding"] == seeding
        and r["delta"] == delta
        and r["gamma"] == gamma
        and r["lambda"] == 1.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
        and r.get("worker_pool", "medium_homogeneous") == worker_pool
        and r["variant"] in (v1, v2)
    ]
    by_trace: dict[str, dict[str, float]] = defaultdict(dict)
    for r in filtered:
        by_trace[r["trace"]][r["variant"]] = r["metrics"][metric]

    diffs: list[float] = []
    for scores in by_trace.values():
        if v1 in scores and v2 in scores and scores[v2] > 0:
            diffs.append((scores[v1] - scores[v2]) / scores[v2])

    if not diffs:
        return {"n": 0, "median": None, "q1": None, "q3": None}
    diffs.sort()
    n = len(diffs)
    return {
        "n": n,
        "median": statistics.median(diffs),
        "q1": diffs[n // 4],
        "q3": diffs[(3 * n) // 4],
    }


def ep_count_stats(results: list[dict], variant: str,
                   seeding: str = "from-scratch",
                   worker_pool: str = "medium_homogeneous") -> dict:
    """Median EP count for a variant across all non-large base traces."""
    filtered = [
        r for r in results
        if r["variant"] == variant
        and r["seeding"] == seeding
        and r["delta"] == 0.0
        and r["gamma"] == 0.0
        and r["lambda"] == 1.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
        and r.get("worker_pool", "medium_homogeneous") == worker_pool
    ]
    counts = [r["metrics"]["ep_count"] for r in filtered]
    if not counts:
        return {"n": 0, "median": None}
    return {"n": len(counts), "median": statistics.median(counts)}


def gamma_starvation_table(results: list[dict]) -> list[dict]:
    """Max dispatch wait vs γ for the starvation sweep."""
    starvation = [r for r in results if r["trace"].startswith("synthetic_starvation")]
    by_k_gamma: dict[tuple, list[float]] = defaultdict(list)
    for r in starvation:
        k = r["trace"]  # e.g. synthetic_starvation_k10.json
        g = r["gamma"]
        by_k_gamma[(k, g)].append(r["metrics"]["max_dispatch_wait"])
    rows = []
    for (k, g), waits in sorted(by_k_gamma.items()):
        rows.append({"trace": k, "gamma": g, "max_wait": statistics.mean(waits)})
    return rows


def _dominant_fraction_at_scale(
    results: list[dict], winner: str, loser: str, metric: str,
    other_scale: float, compiler_scale: float,
) -> float:
    """Like dominant_fraction but filters on explicit scale values."""
    filtered = [
        r for r in results
        if r["variant"] in (winner, loser)
        and r["seeding"] == "from-scratch"
        and r["delta"] == 0.0
        and r["gamma"] == 0.0
        and r["lambda"] == 1.0
        and abs(r["other_scale"] - other_scale) < 1e-9
        and abs(r["compiler_scale"] - compiler_scale) < 1e-9
    ]
    by_trace: dict[str, dict[str, float]] = defaultdict(dict)
    for r in filtered:
        by_trace[r["trace"]][r["variant"]] = r["metrics"][metric]
    wins = 0
    total = 0
    for scores in by_trace.values():
        if winner in scores and loser in scores:
            total += 1
            if scores[winner] < scores[loser]:
                wins += 1
    return wins / total if total > 0 else 0.0


def ablation_rank_stability(results: list[dict]) -> dict:
    """For each (other_scale, compiler_scale) pair, re-rank H1 vs H4.

    Returns a table of (scale_config → fraction_H1_wins_makespan).
    """
    ablation_results = [
        r for r in results
        if (r["other_scale"] != 1.0 or r["compiler_scale"] != 1.0)
        and r["variant"] in ("H1", "H4")
    ]
    by_config: dict[tuple, float] = {}
    seen: set[tuple] = set()
    for r in ablation_results:
        key = (r["other_scale"], r["compiler_scale"])
        if key in seen:
            continue
        seen.add(key)
        frac = _dominant_fraction_at_scale(
            ablation_results, "H1", "H4", "makespan",
            other_scale=key[0], compiler_scale=key[1],
        )
        label = f"other×{key[0]}_compiler×{key[1]}"
        by_config[label] = frac
    return dict(sorted(by_config.items()))


def lambda_pareto_table(results: list[dict], variant: str) -> list[dict]:
    """Aggregate (median_makespan, median_redundant) per λ value for *variant*."""
    subset = [
        r for r in results
        if r["variant"] == variant
        and r["seeding"] == "from-scratch"
        and r["delta"] == 0.0
        and r["gamma"] == 0.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
    ]
    by_lambda: dict[float, list[tuple]] = defaultdict(list)
    for r in subset:
        lam = r["lambda"]
        ms = r["metrics"]["makespan"]
        rw = r["metrics"]["redundant_work"]
        by_lambda[lam].append((ms, rw))

    rows = []
    for lam in sorted(by_lambda):
        pairs = by_lambda[lam]
        rows.append({
            "lambda": lam,
            "median_makespan": statistics.median(p[0] for p in pairs),
            "median_redundant": statistics.median(p[1] for p in pairs),
        })
    return rows


def h0_speedup_summary(
    results: list[dict],
    baseline: str = "H0",
    target: str = "H1",
    metric: str = "makespan",
) -> dict:
    """Speedup of *target* over *baseline* across matched traces.

    Filters to from-scratch, δ=0, γ=0, λ=1, no scaling.  Returns
    per-trace ratios and aggregate statistics.
    """
    filtered = [
        r for r in results
        if r["seeding"] == "from-scratch"
        and r["delta"] == 0.0
        and r["gamma"] == 0.0
        and r["lambda"] == 1.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
        and r["variant"] in (baseline, target)
    ]
    by_trace: dict[str, dict[str, float]] = defaultdict(dict)
    for r in filtered:
        trace = r["trace"]
        by_trace[trace][r["variant"]] = r["metrics"][metric]

    ratios: dict[str, float] = {}
    for trace, scores in by_trace.items():
        if baseline in scores and target in scores and scores[target] > 0:
            ratios[trace] = scores[baseline] / scores[target]

    if not ratios:
        return {"n": 0, "ratios": {}}

    vals = list(ratios.values())
    return {
        "n": len(vals),
        "median": statistics.median(vals),
        "mean": statistics.mean(vals),
        "min": min(vals),
        "max": max(vals),
        "ratios": ratios,
    }


def xlarge_detail(
    results: list[dict],
    pkg_prefix: str = "chromium",
) -> list[dict]:
    """Per-variant summary for xlarge traces (base and cold cache states).

    Returns rows with variant, cache, makespan, ep_count, and redundant_work.
    """
    subset = [
        r for r in results
        if r["trace"].startswith(pkg_prefix)
        and r["seeding"] == "from-scratch"
        and r["delta"] == 0.0
        and r["gamma"] == 0.0
        and r["lambda"] == 1.0
        and r["other_scale"] == 1.0
        and r["compiler_scale"] == 1.0
        and not any(s in r["trace"] for s in (".warm.", ".partial."))
    ]
    rows = []
    for r in sorted(subset, key=lambda x: (x["variant"], x["trace"])):
        cache = "cold" if ".cold." in r["trace"] else "base"
        rows.append({
            "variant": r["variant"],
            "cache": cache,
            "makespan": r["metrics"]["makespan"],
            "ep_count": r["metrics"]["ep_count"],
            "redundant_work": r["metrics"].get("redundant_work", 0.0),
        })
    return rows


def unified_dag_comparison(results: list[dict]) -> list[dict]:
    """Per-variant comparison on the unified 13-package DAG trace.

    Filters to unified.* traces, from-scratch seeding, no δ/γ/λ/scale
    overrides.  Returns rows with variant, cache_state, makespan,
    ep_count, and makespan relative to H1 on the same cache state.
    """
    subset = [
        r for r in results
        if r["trace"].startswith("unified")
        and r.get("seeding", "from-scratch") == "from-scratch"
        and r.get("delta", 0.0) == 0.0
        and r.get("gamma", 0.0) == 0.0
        and r.get("lambda", 1.0) == 1.0
        and r.get("other_scale", 1.0) == 1.0
        and r.get("compiler_scale", 1.0) == 1.0
    ]

    # H1 makespan per cache state
    h1_by_cache: dict[str, float] = {}
    for r in subset:
        if r["variant"] == "H1" and r.get("theta_combined", 60.0) == 60.0:
            cache = _cache_label(r["trace"])
            h1_by_cache[cache] = r["metrics"]["makespan"]

    rows = []
    for r in sorted(subset, key=lambda x: (x["trace"], x["metrics"]["makespan"])):
        cache = _cache_label(r["trace"])
        variant_label = r["variant"]
        if r["variant"] == "H2":
            variant_label = f"H2(θ={r.get('theta_combined', 60.0):.0f})"
        h1_ref = h1_by_cache.get(cache)
        vs_h1 = (r["metrics"]["makespan"] - h1_ref) / h1_ref if h1_ref else None
        rows.append({
            "variant": variant_label,
            "cache": cache,
            "makespan": r["metrics"]["makespan"],
            "ep_count": r["metrics"]["ep_count"],
            "vs_h1": vs_h1,
        })
    return rows


def _cache_label(trace_name: str) -> str:
    if ".cold." in trace_name:
        return "cold"
    if ".warm." in trace_name:
        return "warm"
    if ".partial." in trace_name:
        return "partial"
    return "base"


def load_results(jsonl_path: Path) -> list[dict]:
    records = []
    with open(jsonl_path) as f:
        for line in f:
            line = line.strip()
            if line:
                records.append(json.loads(line))
    return records
