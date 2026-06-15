"""P10 sweep harness: corpus × parameter matrix execution for eos-sim."""
from __future__ import annotations

import copy
import json
import os
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

# ---------------------------------------------------------------------------
# Duration bucket constants (tier-2 heuristic midpoints, PROVENANCE.md)
# ---------------------------------------------------------------------------
COMPILER_DURATION = 510.0   # gcc/clang/rustc/llvm
FETCHER_DURATION = 6.0      # *-source, *-src, fetch*
DOC_DURATION = 17.0         # doc/man
HOOK_DURATION = 3.0         # hook/setup/wrapper
OTHER_DURATION = 75.0       # everything else

# Tolerance for duration bucket matching (floating-point safety)
_EPSILON = 1e-6


def _is_other(d: float) -> bool:
    return abs(d - OTHER_DURATION) < _EPSILON


def _is_compiler(d: float) -> bool:
    return abs(d - COMPILER_DURATION) < _EPSILON


# ---------------------------------------------------------------------------
# Trace mutation (pure; returns a deep copy)
# ---------------------------------------------------------------------------

def scale_durations(trace: dict, other_scale: float, compiler_scale: float) -> dict:
    """Return a new trace with tier-2 'other' and 'compiler' durations scaled.

    Fetcher (6 s), hook (3 s), and doc (17 s) buckets are left unchanged.
    """
    t = copy.deepcopy(trace)
    for node in t["nodes"]:
        d = node["duration"]
        if _is_other(d):
            node["duration"] = d * other_scale
        elif _is_compiler(d):
            node["duration"] = d * compiler_scale
    return t


def inject_workers(trace: dict, workers: list[dict]) -> dict:
    """Return a new trace with the worker pool replaced.

    ``store_cached`` (global build cache) is preserved unchanged.
    """
    t = copy.deepcopy(trace)
    t["workers"] = copy.deepcopy(workers)
    return t


def build_starvation_trace(k_high: int, d_high: float, d_hub: float) -> dict:
    """Synthetic sustained-contention trace (mirrors starvation.rs fixture).

    One low-priority EP ``L`` (OCT 0), a hub ``F``, and ``k_high`` staggered
    high-priority EPs ``H0..H{k-1}`` each depending on ``F``.  Single worker
    = maximal contention.  Staggering: ``H_k`` arrives at ``t = k * d_high``
    so each freed slot meets a fresh high-priority request.
    """
    nodes: list[dict] = [
        {"id": "L", "duration": d_high, "arrival": 0.0},
        {"id": "F", "duration": d_hub,  "arrival": 0.0},
    ]
    edges: list[dict] = []
    for k in range(k_high):
        nodes.append({"id": f"H{k}", "duration": d_high, "arrival": d_high * k})
        edges.append({"from": "F", "to": f"H{k}"})
    return {
        "nodes": nodes,
        "edges": edges,
        "workers": [{"id": "w0", "speed": 1.0}],
    }


# ---------------------------------------------------------------------------
# Sweep cell definition
# ---------------------------------------------------------------------------

@dataclass
class SweepCell:
    trace_path: Path
    variant: str           # H1 | H2 | H3 | H4
    seeding: str           # from-scratch | atom-seeded
    delta: float = 0.0
    gamma: float = 0.0
    lam: float = 1.0       # redundant-work weight λ
    theta_critical: float = 30.0
    theta_redundancy: float = 20.0
    theta_cost: float = 60.0
    theta_scale: float = 1.0   # 0 = conf-gating off
    worker_pool: str = "medium_homogeneous"
    other_scale: float = 1.0   # duration ablation
    compiler_scale: float = 1.0
    seed: int = 0
    # synthetic trace overrides the path (set when trace is built in-memory)
    _trace_override: dict | None = field(default=None, repr=False)

    def label(self) -> str:
        parts = [
            self.trace_path.stem,
            self.variant,
            self.seeding,
            f"d{self.delta}",
            f"g{self.gamma}",
            f"l{self.lam}",
            f"tc{self.theta_critical}",
            f"tr{self.theta_redundancy}",
            f"ts{self.theta_scale}",
            self.worker_pool,
            f"os{self.other_scale}",
            f"cs{self.compiler_scale}",
        ]
        return "|".join(str(p) for p in parts)


# ---------------------------------------------------------------------------
# Simulator invocation
# ---------------------------------------------------------------------------

_SIM_BIN = Path(__file__).parent.parent.parent / "eos-sim" / "target" / "release" / "eos-sim"


def _sim_binary() -> Path:
    if _SIM_BIN.exists():
        return _SIM_BIN
    # Fallback: debug build
    debug = _SIM_BIN.parent.parent / "target" / "debug" / "eos-sim"
    if debug.exists():
        return debug
    raise RuntimeError(f"eos-sim binary not found at {_SIM_BIN}; run `cargo build --release -p eos-sim`")


def run_cell(cell: SweepCell, log_fh=None) -> dict | None:
    """Run a single sweep cell; return the result record or None on failure."""
    trace_data = cell._trace_override or json.loads(cell.trace_path.read_text())

    # Apply duration scaling
    if cell.other_scale != 1.0 or cell.compiler_scale != 1.0:
        trace_data = scale_durations(trace_data, cell.other_scale, cell.compiler_scale)

    # Apply worker injection for heterogeneous pool
    if cell.worker_pool == "small_heterogeneous":
        het_workers = [
            {"id": "w0", "speed": 0.5},
            {"id": "w1", "speed": 1.0},
            {"id": "w2", "speed": 2.0},
        ]
        trace_data = inject_workers(trace_data, het_workers)

    with tempfile.NamedTemporaryFile(suffix=".json", mode="w", delete=False) as tf:
        json.dump(trace_data, tf)
        tmp_path = tf.name

    try:
        cmd = [
            str(_sim_binary()),
            "--trace", tmp_path,
            "--variant", cell.variant,
            "--seeding", cell.seeding,
            "--delta", str(cell.delta),
            "--gamma", str(cell.gamma),
            "--lambda", str(cell.lam),
            "--theta-critical", str(cell.theta_critical),
            "--theta-redundancy", str(cell.theta_redundancy),
            "--theta-cost", str(cell.theta_cost),
            "--theta-scale", str(cell.theta_scale),
            "--seed", str(cell.seed),
            "--json",
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)

        stdout = result.stdout
        if log_fh is not None:
            log_fh.write(stdout)
            log_fh.flush()

        if result.returncode != 0:
            print(f"[WARN] cell {cell.label()} failed: {result.stderr.strip()}", file=sys.stderr)
            return None

        # Parse the JSON metrics line
        metrics_line = None
        for line in stdout.splitlines():
            line = line.strip()
            if line.startswith("{"):
                metrics_line = line
        if metrics_line is None:
            print(f"[WARN] no metrics line in output for {cell.label()}", file=sys.stderr)
            return None

        metrics = json.loads(metrics_line)
        return {
            "trace": str(cell.trace_path.name),
            "variant": cell.variant,
            "seeding": cell.seeding,
            "delta": cell.delta,
            "gamma": cell.gamma,
            "lambda": cell.lam,
            "theta_critical": cell.theta_critical,
            "theta_redundancy": cell.theta_redundancy,
            "theta_cost": cell.theta_cost,
            "theta_scale": cell.theta_scale,
            "worker_pool": cell.worker_pool,
            "other_scale": cell.other_scale,
            "compiler_scale": cell.compiler_scale,
            "seed": cell.seed,
            "metrics": metrics,
        }
    finally:
        os.unlink(tmp_path)


# ---------------------------------------------------------------------------
# Sweep matrix generators
# ---------------------------------------------------------------------------

CORPUS_DIR = Path(__file__).parent.parent.parent / "eos-sim-traces"

# All 48 corpus traces grouped by size class (node count)
_SMALL_PKGS  = ["jq", "python3"]
_MEDIUM_PKGS = ["curl", "ripgrep", "bat", "fd", "openssh", "rustc", "git", "linux"]
_LARGE_PKGS  = ["ffmpeg", "libreoffice"]
_ALL_PKGS    = _SMALL_PKGS + _MEDIUM_PKGS + _LARGE_PKGS

_CACHE_VARIANTS = ["json", "cold.json", "warm.json", "partial.json"]


def _trace(pkg: str, variant: str = "json") -> Path:
    if variant == "json":
        return CORPUS_DIR / f"{pkg}.json"
    return CORPUS_DIR / f"{pkg}.{variant}"


def generate_core_matrix() -> list[SweepCell]:
    """Full variant × seeding × cache × δ/γ sweep.

    Non-large traces: all 4 cache variants.
    Large traces: base (.json) only, H1+H4, from-scratch only.
    This limits large-trace runs to ~16 cells × ~9 s each ≈ 2 min.
    """
    cells: list[SweepCell] = []

    # Non-large packages: full matrix
    for pkg in _SMALL_PKGS + _MEDIUM_PKGS:
        for cv in _CACHE_VARIANTS:
            tp = _trace(pkg, cv)
            for variant in ["H1", "H2", "H3", "H4"]:
                for seeding in ["from-scratch", "atom-seeded"]:
                    for delta in [0.0, 30.0]:
                        for gamma in [0.0, 0.5]:
                            cells.append(SweepCell(
                                trace_path=tp, variant=variant,
                                seeding=seeding, delta=delta, gamma=gamma,
                            ))

    # Large packages: limited matrix to control runtime
    for pkg in _LARGE_PKGS:
        tp = _trace(pkg)
        for variant in ["H1", "H4"]:
            for delta in [0.0, 30.0]:
                for gamma in [0.0, 0.5]:
                    cells.append(SweepCell(
                        trace_path=tp, variant=variant,
                        seeding="from-scratch", delta=delta, gamma=gamma,
                    ))
        # atom-seeded for large too (H1, H4 only)
        for variant in ["H1", "H4"]:
            cells.append(SweepCell(
                trace_path=tp, variant=variant,
                seeding="atom-seeded", delta=0.0, gamma=0.0,
            ))

    return cells


def generate_threshold_matrix() -> list[SweepCell]:
    """θ sensitivity sweep: H1 and H4 × non-large base traces × θ grid."""
    cells: list[SweepCell] = []
    for pkg in _SMALL_PKGS + _MEDIUM_PKGS:
        tp = _trace(pkg)
        for variant in ["H1", "H4"]:
            for tc in [15.0, 30.0, 60.0]:
                for tr in [10.0, 20.0, 40.0]:
                    for ts in [0.0, 1.0]:  # confidence gating off/on
                        cells.append(SweepCell(
                            trace_path=tp, variant=variant,
                            seeding="from-scratch", delta=0.0, gamma=0.0,
                            theta_critical=tc, theta_redundancy=tr, theta_scale=ts,
                        ))
    return cells


def generate_lambda_matrix() -> list[SweepCell]:
    """λ Pareto sweep: H1 and H4 × non-large base traces × λ values."""
    cells: list[SweepCell] = []
    lambdas = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0]
    for pkg in _SMALL_PKGS + _MEDIUM_PKGS:
        tp = _trace(pkg)
        for variant in ["H1", "H4"]:
            for lam in lambdas:
                cells.append(SweepCell(
                    trace_path=tp, variant=variant,
                    seeding="from-scratch", delta=0.0, gamma=0.0,
                    lam=lam,
                ))
    return cells


def generate_ablation_matrix() -> list[SweepCell]:
    """Duration-sensitivity ablation: H1, H4 × non-large base × scale configs.

    Per P10 §3e: perturb 'other' (75 s) and 'compiler' (510 s) separately.
    """
    cells: list[SweepCell] = []
    # Perturb 'other' bucket only (compiler fixed at 1.0)
    other_scales = [0.5, 2.0, 5.0]
    # Perturb 'compiler' bucket only (other fixed at 1.0)
    compiler_scales = [0.5, 2.0, 5.0]
    for pkg in _SMALL_PKGS + _MEDIUM_PKGS:
        tp = _trace(pkg)
        for variant in ["H1", "H4"]:
            for os_ in other_scales:
                cells.append(SweepCell(
                    trace_path=tp, variant=variant,
                    seeding="from-scratch", delta=0.0, gamma=0.0,
                    other_scale=os_, compiler_scale=1.0,
                    worker_pool="medium_homogeneous",
                ))
            for cs in compiler_scales:
                cells.append(SweepCell(
                    trace_path=tp, variant=variant,
                    seeding="from-scratch", delta=0.0, gamma=0.0,
                    other_scale=1.0, compiler_scale=cs,
                    worker_pool="medium_homogeneous",
                ))
    return cells


def generate_starvation_matrix() -> list[SweepCell]:
    """Starvation/fairness sweep: synthetic contention traces × γ values.

    Uses theta_cost=0 to force per-node EPs (matching the starvation.rs
    fixture pattern), which is necessary to preserve the L vs H_k contention
    structure.  Without this, H1 default thresholds absorb H_k into the hub
    EP's scope, collapsing the contention to a 2-EP schedule.
    """
    cells: list[SweepCell] = []
    k_values = [5, 10, 20]
    gamma_values = [0.0, 0.1, 0.5, 1.0, 2.0]
    for k in k_values:
        trace = build_starvation_trace(k_high=k, d_high=2.0, d_hub=11.0)
        for gamma in gamma_values:
            cells.append(SweepCell(
                trace_path=Path(f"synthetic_starvation_k{k}.json"),
                variant="H1",
                seeding="from-scratch",
                delta=0.0,
                gamma=gamma,
                # theta_cost=0 forces all nodes to be promoted to EPs,
                # preserving the hub+stagger contention structure.
                theta_cost=0.0,
                theta_critical=1e9,
                theta_redundancy=1e9,
                theta_scale=0.0,
                _trace_override=trace,
            ))
    return cells


def generate_small_het_matrix() -> list[SweepCell]:
    """Small heterogeneous worker pool: H1, H4 × non-large base traces."""
    cells: list[SweepCell] = []
    for pkg in _SMALL_PKGS + _MEDIUM_PKGS:
        tp = _trace(pkg)
        for variant in ["H1", "H4"]:
            for delta in [0.0, 30.0]:
                cells.append(SweepCell(
                    trace_path=tp, variant=variant,
                    seeding="from-scratch", delta=delta, gamma=0.0,
                    worker_pool="small_heterogeneous",
                ))
    return cells


def generate_full_matrix() -> dict[str, list[SweepCell]]:
    return {
        "core":      generate_core_matrix(),
        "threshold": generate_threshold_matrix(),
        "lambda":    generate_lambda_matrix(),
        "ablation":  generate_ablation_matrix(),
        "starvation": generate_starvation_matrix(),
        "small_het": generate_small_het_matrix(),
    }


# ---------------------------------------------------------------------------
# Main sweep runner
# ---------------------------------------------------------------------------

def run_sweep(
    out_jsonl: Path,
    log_path: Path,
    max_workers: int = 8,
    matrix: dict[str, list[SweepCell]] | None = None,
) -> int:
    """Run the full sweep; write JSONL results and tee to log.  Returns run count."""
    if matrix is None:
        matrix = generate_full_matrix()

    all_cells: list[SweepCell] = []
    for cells in matrix.values():
        all_cells.extend(cells)

    out_jsonl.parent.mkdir(parents=True, exist_ok=True)
    total = len(all_cells)
    done = 0
    errors = 0

    with open(out_jsonl, "w") as jf, open(log_path, "a") as lf:
        with ThreadPoolExecutor(max_workers=max_workers) as pool:
            futures = {pool.submit(run_cell, cell, None): cell for cell in all_cells}
            for fut in as_completed(futures):
                cell = futures[fut]
                done += 1
                try:
                    rec = fut.result()
                except Exception as exc:
                    print(f"[ERROR] {cell.label()}: {exc}", file=sys.stderr)
                    errors += 1
                    rec = None
                if rec is not None:
                    jf.write(json.dumps(rec) + "\n")
                    jf.flush()
                    # Write contract lines to log
                    n = rec["metrics"].get("ep_count", 0)
                    lf.write(f"Loaded {n} plans\n")
                    lf.write("Simulation completed\n")
                    lf.flush()
                if done % 50 == 0 or done == total:
                    print(f"[sweep] {done}/{total} done, {errors} errors", file=sys.stderr)

    return done - errors


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser(description="P10 sweep runner")
    ap.add_argument("--out", type=Path, default=Path(".scratch/eos-scheduler-validation/results/sweep.jsonl"))
    ap.add_argument("--log", type=Path, default=Path(".scratch/eos-scheduler-validation/simulator.log"))
    ap.add_argument("--workers", type=int, default=8)
    args = ap.parse_args()

    n = run_sweep(args.out, args.log, max_workers=args.workers)
    print(f"Sweep complete: {n} successful runs → {args.out}")
