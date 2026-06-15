"""eos-corpus CLI — four subcommands: find-anchor, metrics, extract, validate."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Optional

import click

from .fallback import resolve_duration
from .graph import (
    ALL_COVERAGE_CELLS,
    StructuralMetrics,
    assign_coverage_cells,
    parse_drv_closure,
)
from .hydra import HydraClient
from .nix import at_commit, derivation_show
from .trace import emit_cache_variants, emit_trace, write_trace


# ---------------------------------------------------------------------------
# Shared helpers
# ---------------------------------------------------------------------------

def _echo_table(rows: list[dict], columns: list[str]) -> None:
    widths = {c: len(c) for c in columns}
    for row in rows:
        for c in columns:
            widths[c] = max(widths[c], len(str(row.get(c, ""))))
    header = "  ".join(c.ljust(widths[c]) for c in columns)
    sep = "  ".join("-" * widths[c] for c in columns)
    click.echo(header)
    click.echo(sep)
    for row in rows:
        click.echo("  ".join(str(row.get(c, "")).ljust(widths[c]) for c in columns))


def _coverage_matrix_table(filled: set[str]) -> str:
    lines = []
    lines.append(f"{'':20s} | {'Small (<1.1k)':13s} | {'Medium (1.1–3k)':15s} | {'Large (≥3k)':11s}")
    lines.append("-" * 72)
    for cpr_label, cpr_key in [("Low CPR  ≤0.60", "low"), ("Mid CPR  0.60–1.48", "mid"), ("High CPR >1.48", "high")]:
        cells = [
            "✓" if f"{sz}_{cpr_key}_cpr" in filled else "·"
            for sz in ("small", "medium", "large")
        ]
        lines.append(f"{cpr_label:20s} | {cells[0]:12s} | {cells[1]:15s} | {cells[2]:12s}")
    lines.append("")
    low_conv = "✓" if "low_convergence" in filled else "·"
    high_conv = "✓" if "high_convergence" in filled else "·"
    lines.append(f"{'Low convergence':20s} | {low_conv}")
    lines.append(f"{'High convergence':20s} | {high_conv}")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# CLI group
# ---------------------------------------------------------------------------

@click.group()
def main() -> None:
    """eos-corpus — nixpkgs trace corpus extraction toolkit."""


# ---------------------------------------------------------------------------
# find-anchor
# ---------------------------------------------------------------------------

@main.command("find-anchor")
@click.option("--nixpkgs", required=True, type=click.Path(exists=True),
              help="Local nixpkgs checkout (used to verify the commit exists locally).")
@click.option("--max-lookback", default=50, show_default=True,
              help="Max eval IDs to walk backward from latest.")
@click.option("--delay", default=2.0, show_default=True, help="Seconds between Hydra API calls.")
def find_anchor(nixpkgs: str, max_lookback: int, delay: float) -> None:
    """Find a suitable Hydra eval for nixpkgs/unstable and derive the anchor commit.

    Walks backward from the latest Hydra eval ID, picks the most-built
    nixpkgs eval whose commit exists in the local checkout, and prints:
    anchor commit SHA, Hydra eval ID, build count.

    This is the correct direction: start at Hydra, derive the commit,
    then use it in the local nixpkgs checkout.
    """
    import subprocess as _sp

    client = HydraClient(delay=delay)

    click.echo("Resolving latest eval ID …")
    try:
        latest = client.find_latest_eval_id()
    except Exception as exc:
        click.echo(f"ERROR: cannot resolve latest eval: {exc}", err=True)
        sys.exit(1)
    click.echo(f"Latest Hydra eval: {latest}")

    click.echo(f"Walking backward (max {max_lookback} evals) to find a nixpkgs/unstable eval …")
    best_eval_id: Optional[int] = None
    best_commit: Optional[str] = None
    best_build_count = 0

    for offset in range(max_lookback):
        eid = latest - offset
        if eid <= 0:
            break
        try:
            ev = client.get_eval(eid)
        except Exception as exc:
            click.echo(f"  skip {eid}: {exc}", err=True)
            continue

        sha = client.nixpkgs_commit(ev)
        if sha is None:
            click.echo(f"  {eid}: not a nixpkgs eval, skipping", err=True)
            continue

        build_count = len(ev.get("builds", []))
        click.echo(f"  {eid}: commit={sha[:16]}… builds={build_count}")

        # Verify commit exists in local checkout.
        try:
            _sp.run(
                ["git", "-C", nixpkgs, "cat-file", "-e", f"{sha}^{{commit}}"],
                check=True, capture_output=True,
            )
        except _sp.CalledProcessError:
            click.echo(f"  {eid}: commit not in local checkout, skipping", err=True)
            continue

        # Prefer the eval with the most builds (proxy for most succeeded).
        if build_count > best_build_count:
            best_eval_id = eid
            best_commit = sha
            best_build_count = build_count
            click.echo(f"  → new best: eval={eid} builds={build_count}")

        # After finding at least one candidate, scan a few more then stop.
        if best_eval_id is not None and offset >= 5:
            break

    if best_eval_id is None or best_commit is None:
        click.echo("ERROR: no suitable nixpkgs eval found within lookback window", err=True)
        sys.exit(1)

    click.echo(f"\nanchor_commit={best_commit}")
    click.echo(f"hydra_eval_id={best_eval_id}")
    click.echo(f"build_count={best_build_count}")
    click.echo("\nDiscovered API schema snippet:")
    click.echo(json.dumps(client.discovered_schema.get("eval", {}), indent=2))


# ---------------------------------------------------------------------------
# metrics
# ---------------------------------------------------------------------------

@main.command("metrics")
@click.option("--nixpkgs", required=True, type=click.Path(exists=True))
@click.option("--anchor", required=True, help="Anchor commit SHA.")
@click.option("--packages", required=True, multiple=True, metavar="PKG")
@click.option("--min-cells", default=6, show_default=True,
              help="Exit nonzero if fewer than this many cells can be filled.")
def metrics(nixpkgs: str, anchor: str, packages: tuple[str, ...], min_cells: int) -> None:
    """Compute structural metrics for each package at the anchor commit.

    Prints per-package table and coverage matrix.
    Exits nonzero if fewer than min-cells coverage matrix cells can be filled.
    """
    rows = []
    all_filled: set[str] = set()

    with at_commit(nixpkgs, anchor):
        for pkg in packages:
            click.echo(f"  evaluating {pkg} …", err=True)
            try:
                closure_json = derivation_show(nixpkgs, pkg)
            except subprocess.CalledProcessError as exc:
                click.echo(f"  SKIP {pkg}: nix derivation show failed: {exc.stderr[:200]}", err=True)
                continue

            nodes = parse_drv_closure(closure_json)
            m = StructuralMetrics.compute(nodes)
            all_filled.update(m.coverage_cells)
            rows.append({
                "package": pkg,
                "N": m.n,
                "depth": m.depth,
                "max_fanin": m.max_fanin,
                "conv_density": f"{m.convergence_density:.3f}",
                "CPR": f"{m.cpr:.2f}",
                "cells": ", ".join(m.coverage_cells),
            })

    click.echo("")
    _echo_table(rows, ["package", "N", "depth", "max_fanin", "conv_density", "CPR", "cells"])
    click.echo("")
    click.echo("Coverage matrix:")
    click.echo(_coverage_matrix_table(all_filled))
    click.echo(f"\nFilled {len(all_filled)} / {len(ALL_COVERAGE_CELLS)} cells")

    if len(all_filled) < min_cells:
        click.echo(f"\nERROR: only {len(all_filled)} cells filled; need at least {min_cells}", err=True)
        sys.exit(1)


# ---------------------------------------------------------------------------
# extract
# ---------------------------------------------------------------------------

@main.command("extract")
@click.option("--nixpkgs", required=True, type=click.Path(exists=True))
@click.option("--anchor", required=True)
@click.option("--hydra-eval", required=True, type=int, metavar="EVAL_ID")
@click.option("--packages", required=True, multiple=True, metavar="PKG")
@click.option("--out", required=True, type=click.Path(), metavar="DIR")
@click.option("--delay", default=2.0, show_default=True)
@click.option("--trim-at", default=2000, show_default=True,
              help="Trim closures larger than this; retain ≥500 nodes and all fan-in≥3.")
def extract(
    nixpkgs: str,
    anchor: str,
    hydra_eval: int,
    packages: tuple[str, ...],
    out: str,
    delay: float,
    trim_at: int,
) -> None:
    """Extract eos-sim trace files from the nixpkgs closure + Hydra timing.

    For each package, emits one trace JSON to OUT/.  Packages with N>500 also
    get cold/partial/warm cache-state variants.
    """
    out_dir = Path(out)
    out_dir.mkdir(parents=True, exist_ok=True)

    client = HydraClient(delay=delay)

    # Probe and record Hydra eval schema on first call.
    click.echo(f"Probing Hydra eval {hydra_eval} for schema discovery …", err=True)
    ev = client.get_eval(hydra_eval)
    anchor_ts = ev.get("timestamp", 0)
    click.echo(f"Discovered eval schema: {json.dumps(client.discovered_schema.get('eval', {}))}", err=True)
    click.echo(f"Anchor eval timestamp: {anchor_ts}", err=True)
    click.echo(
        "NOTE: Using per-package /api/latestbuilds lookup (unstable then staging-next).\n"
        "      /eval/{id}/builds is NOT used — it returns ~100 MB of JSON and times out.\n"
        "      Hydra /build/{id} has no 'buildsteps' field; only top-level timing is\n"
        "      available. Transitive dep timing is not available from any Hydra endpoint.",
        err=True,
    )

    with at_commit(nixpkgs, anchor):
        for pkg in packages:
            click.echo(f"\n[{pkg}] evaluating closure …", err=True)
            try:
                closure_json = derivation_show(nixpkgs, pkg)
            except subprocess.CalledProcessError as exc:
                click.echo(f"  SKIP {pkg}: {exc.stderr[:200]}", err=True)
                continue

            nodes = parse_drv_closure(closure_json)
            n_orig = len(nodes)

            # Trim oversized closures.
            trim_depth: Optional[int] = None
            conv_before = sum(1 for p in nodes.values() if len(p.deps) >= 3)
            if n_orig > trim_at:
                nodes, trim_depth = _trim_closure(nodes, target_min=500)
                conv_after = sum(1 for p in nodes.values() if len(p.deps) >= 3)
                click.echo(
                    f"  trimmed {n_orig}→{len(nodes)} nodes at depth {trim_depth}; "
                    f"convergence nodes {conv_before}→{conv_after}", err=True
                )

            # Identify the top-level drv (atom).
            atom_path = _find_atom(nodes, pkg)

            # Look up the best Hydra build for this package.
            # Per-package lookup avoids the 100 MB /eval/{id}/builds download.
            # Only the root/atom node can be matched to a Hydra build; all
            # transitive dependencies are cache hits in Hydra and have no
            # per-node timing available from any endpoint.
            click.echo(f"  fetching Hydra build for {pkg} (unstable → staging-next) …", err=True)
            pkg_build = client.find_package_build(pkg, anchor_ts=anchor_ts, nr=10)
            atom_tier1: Optional[float] = None
            if pkg_build:
                atom_tier1 = client.build_duration(pkg_build)
                jobset_used = pkg_build.get("jobset", "?")
                nixname = pkg_build.get("nixname", "?")
                diff = pkg_build.get("stoptime", 0) - pkg_build.get("starttime", 0)
                click.echo(
                    f"  found build id={pkg_build.get('id')} jobset={jobset_used} "
                    f"nixname={nixname} diff={diff}s "
                    f"{'(real timing)' if atom_tier1 else '(cache hit → no timing)'}",
                    err=True,
                )
            else:
                click.echo(f"  no Hydra build found for {pkg}; atom uses tier-2 heuristic", err=True)

            # Resolve durations: only the atom node gets tier-1 if available.
            durations: Dict[str, float] = {}
            measured_flags: Dict[str, bool] = {}
            measured_count = 0

            for path in nodes:
                # Only the atom (root) node gets a tier-1 duration from Hydra.
                # Transitive deps are not available in Hydra's API — they are
                # all cache hits in nixpkgs/unstable evals.
                tier1 = atom_tier1 if (path == atom_path and atom_tier1 is not None) else None
                dur, meas = resolve_duration(
                    nodes[path].name,
                    tier1=tier1,
                    input_count=len(nodes[path].deps),
                )
                durations[path] = dur
                measured_flags[path] = meas
                if meas:
                    measured_count += 1

            n = len(nodes)
            ratio = measured_count / n if n else 0.0
            flag = " [BELOW 40% MEASURED]" if ratio < 0.40 else ""
            click.echo(
                f"  measured {measured_count}/{n} nodes ({ratio:.1%}){flag}", err=True
            )
            if ratio < 0.40:
                click.echo(
                    f"  cause: only the root (atom) derivation has Hydra timing; "
                    f"all {n - measured_count} transitive deps are binary-cache hits "
                    f"with no per-node timing in any Hydra endpoint",
                    err=True,
                )

            base = emit_trace(nodes, durations, measured_flags, atom_path=atom_path, pkg_name=pkg)

            # Emit base trace.
            safe = pkg.replace(".", "_").replace("/", "_")
            out_path = out_dir / f"{safe}.json"
            write_trace(str(out_path), base)
            click.echo(f"  wrote {out_path}", err=True)

            # Emit cache-state variants for large closures.
            if n > 500:
                variants = emit_cache_variants(base, n=n)
                for variant_name, variant_trace in variants.items():
                    vpath = out_dir / f"{safe}.{variant_name}.json"
                    write_trace(str(vpath), variant_trace)
                    click.echo(f"  wrote {vpath}", err=True)

    click.echo("\nExtraction complete.", err=True)
    click.echo(f"Discovered API schemas: {json.dumps(client.discovered_schema, indent=2)}")


def _find_atom(nodes, pkg: str) -> Optional[str]:
    """Identify the top-level drv path (atom) for a package attribute.

    The atom is the node with in-degree 0 in the dependency graph
    (nothing else depends on it) — the root of the closure, which
    corresponds to the top-level pkgs attribute.
    """
    from .graph import compute_in_degrees
    in_deg = compute_in_degrees(nodes)
    roots = [p for p, d in in_deg.items() if d == 0]
    if len(roots) == 1:
        return roots[0]
    # Multiple roots: pick the one whose name most closely matches pkg attr.
    pkg_base = pkg.split(".")[-1]
    for r in roots:
        if pkg_base in nodes[r].name:
            return r
    return roots[0] if roots else None


def _trim_closure(nodes, target_min: int = 500):
    """Trim a closure to retain ≥target_min nodes and all fan-in≥3 nodes.

    Returns (trimmed_nodes_dict, trim_depth).
    The strategy: compute depth of each node; cut at the depth where
    node count first exceeds target_min, preserving all convergence nodes.
    """
    from .graph import compute_in_degrees, compute_depth, _topo_order

    in_deg = compute_in_degrees(nodes)
    convergence_paths = {p for p, d in in_deg.items() if d >= 3}

    topo = _topo_order(nodes)
    # depth[p] = longest chain from any source to p
    depth_map: Dict[str, int] = {p: 0 for p in nodes}
    children: Dict[str, list] = {p: [] for p in nodes}
    for path, node in nodes.items():
        for dep in node.deps:
            if dep in nodes:
                children[dep].append(path)
    for p in topo:
        for child in children[p]:
            depth_map[child] = max(depth_map[child], depth_map[p] + 1)

    max_depth = max(depth_map.values()) if depth_map else 0

    # Try cutting at increasing depths until we retain ≥ target_min nodes.
    for cut in range(max_depth, -1, -1):
        kept = {p for p, d in depth_map.items() if d <= cut}
        kept |= convergence_paths  # always keep convergence structure
        if len(kept) >= target_min:
            trimmed = {p: nodes[p] for p in kept if p in nodes}
            # Relink deps to only kept nodes.
            for n in trimmed.values():
                n.deps = [d for d in n.deps if d in trimmed]
            return trimmed, cut

    # Fallback: return all nodes.
    return nodes, max_depth


# ---------------------------------------------------------------------------
# validate
# ---------------------------------------------------------------------------

@main.command("validate")
@click.option("--corpus", required=True, type=click.Path(exists=True), metavar="DIR")
@click.option("--sim-bin", default=None, help="Path to eos-sim binary (auto-detected if absent).")
def validate(corpus: str, sim_bin: Optional[str]) -> None:
    """Validate every trace in CORPUS and print a coverage report.

    Exits 0 only when all PASS gates are met:
      - total nodes ≥ 100 (gate)
      - total nodes ≥ 500 (target)
      - ≥ 8 coverage matrix cells filled
    """
    corpus_dir = Path(corpus)
    traces = sorted(corpus_dir.glob("*.json"))

    # Exclude variant files from primary analysis (they share nodes with base).
    base_traces = [t for t in traces if not any(
        t.stem.endswith(f".{v}") for v in ("cold", "partial", "warm")
    )]

    if not base_traces:
        click.echo("ERROR: no trace JSON files found in corpus directory", err=True)
        sys.exit(1)

    # Resolve eos-sim binary.
    sim = _find_sim(sim_bin)

    total_nodes = 0
    all_filled: set[str] = set()
    rows = []
    any_failure = False

    for trace_path in base_traces:
        try:
            trace = json.loads(trace_path.read_text())
        except json.JSONDecodeError as exc:
            click.echo(f"  INVALID JSON {trace_path.name}: {exc}", err=True)
            any_failure = True
            continue

        nodes = trace.get("nodes", [])
        edges = trace.get("edges", [])
        n = len(nodes)
        e = len(edges)
        total_nodes += n

        measured = sum(1 for nd in nodes if nd.get("measured"))
        ratio = measured / n if n else 0.0
        flag = " ← BELOW 40% MEASURED" if ratio < 0.40 else ""

        # Verify loadable by simulator.
        sim_ok = _sim_load_check(sim, trace_path) if sim else None
        sim_status = "OK" if sim_ok else ("FAIL" if sim_ok is False else "skip")

        # Compute coverage cells using unit durations (structural proxy).
        # Rationale: the CPR thresholds in graph.py are calibrated for the unit-
        # duration CPR distribution of nixpkgs closures (D/N ratio).  Tier-2
        # heuristic durations homogenise CPR via the shared bootstrap chain and
        # map every package into mid-CPR regardless of structural variation.
        # Using unit durations restores the structural discriminability the
        # coverage matrix is meant to capture, consistent with the spec's
        # "uniform unit durations as structural proxy when Hydra timing is absent".
        from .graph import parse_drv_closure, StructuralMetrics, DrvNode
        node_map = {nd["id"]: DrvNode(path=nd["id"], name=nd.get("plan_name", nd["id"])) for nd in nodes}
        for edge in edges:
            frm = edge.get("from")
            to = edge.get("to")
            if frm in node_map and to in node_map:
                node_map[frm].deps.append(to)
        m = StructuralMetrics.compute(node_map, durations=None)  # unit durations
        all_filled.update(m.coverage_cells)

        rows.append({
            "file": trace_path.name,
            "N": n,
            "E": e,
            "measured%": f"{ratio:.0%}",
            "cells": ", ".join(m.coverage_cells),
            "sim": sim_status,
            "note": flag.strip(),
        })

    click.echo(f"\nCorpus: {corpus}")
    click.echo(f"Base traces: {len(base_traces)}  |  Variant files: {len(traces) - len(base_traces)}")
    click.echo("")
    _echo_table(rows, ["file", "N", "E", "measured%", "cells", "sim", "note"])
    click.echo("")
    click.echo("Coverage matrix:")
    click.echo(_coverage_matrix_table(all_filled))
    click.echo(f"\nCells filled: {len(all_filled)} / {len(ALL_COVERAGE_CELLS)}")
    click.echo(f"Total nodes:  {total_nodes}")
    click.echo("")

    # Gate checks.
    gates = [
        ("total nodes ≥ 100 (gate)",    total_nodes >= 100),
        ("total nodes ≥ 500 (target)",  total_nodes >= 500),
        ("≥ 8 coverage cells filled",   len(all_filled) >= 8),
    ]
    all_pass = True
    for label, ok in gates:
        status = "PASS" if ok else "FAIL"
        click.echo(f"  {status}  {label}")
        if not ok:
            all_pass = False

    if any_failure:
        click.echo("\nFAIL: one or more traces could not be parsed", err=True)
        sys.exit(1)

    if not all_pass:
        sys.exit(1)

    click.echo("\nPASS")


def _find_sim(explicit: Optional[str]) -> Optional[Path]:
    """Locate the eos-sim binary."""
    if explicit:
        return Path(explicit)
    # Try relative to this file (tools/eos-corpus → tools/eos-sim).
    here = Path(__file__).parent.parent
    candidates = [
        here.parent / "eos-sim" / "target" / "debug" / "eos-sim",
        here.parent / "eos-sim" / "target" / "release" / "eos-sim",
        Path("eos-sim"),
    ]
    for c in candidates:
        if c.exists():
            return c
    return None


def _sim_load_check(sim: Path, trace_path: Path) -> bool:
    """Run the simulator in load-only mode; return True on success."""
    try:
        result = subprocess.run(
            [str(sim), "--trace", str(trace_path), "--variant", "H1", "--seed", "42"],
            capture_output=True, text=True, timeout=120,
        )
        return result.returncode == 0
    except Exception:
        return False
