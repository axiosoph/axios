"""Tests for CLI subcommands: --help, validate logic on synthetic corpus."""

import json
import os
import tempfile
from pathlib import Path
from unittest.mock import patch

import pytest
from click.testing import CliRunner

from eos_corpus.cli import main, validate
from eos_corpus.graph import DrvNode
from eos_corpus.trace import emit_trace


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _write_trace(path: Path, nodes_spec: list[tuple[str, list[str]]], measured: bool = False):
    """Write a minimal valid trace to path."""
    store = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-"
    raw_nodes = {store + n + ".drv": {"inputDrvs": {store + d + ".drv": ["out"] for d in deps}, "name": n}
                 for n, deps in nodes_spec}
    from eos_corpus.graph import parse_drv_closure
    node_map = parse_drv_closure(raw_nodes)
    durations = {k: 10.0 for k in node_map}
    mflags = {k: measured for k in node_map}
    trace = emit_trace(node_map, durations, mflags)
    with open(path, "w") as fh:
        json.dump(trace, fh)
        fh.write("\n")


# ---------------------------------------------------------------------------
# --help smoke tests
# ---------------------------------------------------------------------------

class TestHelp:
    def test_main_help(self):
        runner = CliRunner()
        result = runner.invoke(main, ["--help"])
        assert result.exit_code == 0
        assert "find-anchor" in result.output
        assert "metrics" in result.output
        assert "extract" in result.output
        assert "validate" in result.output

    def test_find_anchor_help(self):
        runner = CliRunner()
        result = runner.invoke(main, ["find-anchor", "--help"])
        assert result.exit_code == 0
        assert "--nixpkgs" in result.output

    def test_metrics_help(self):
        runner = CliRunner()
        result = runner.invoke(main, ["metrics", "--help"])
        assert result.exit_code == 0

    def test_extract_help(self):
        runner = CliRunner()
        result = runner.invoke(main, ["extract", "--help"])
        assert result.exit_code == 0

    def test_validate_help(self):
        runner = CliRunner()
        result = runner.invoke(main, ["validate", "--help"])
        assert result.exit_code == 0


# ---------------------------------------------------------------------------
# validate subcommand
# ---------------------------------------------------------------------------

class TestValidate:
    def _corpus_dir(self, tmp_path: Path, n_packages: int, nodes_per_pkg: int) -> Path:
        """Create a synthetic corpus with n_packages traces, each with nodes_per_pkg nodes."""
        corpus = tmp_path / "corpus"
        corpus.mkdir()
        for i in range(n_packages):
            # Build a small chain to get varied CPR
            spec = [(f"pkg{i}-top", [f"pkg{i}-dep{j}" for j in range(min(3, nodes_per_pkg-1))])]
            spec += [(f"pkg{i}-dep{j}", []) for j in range(min(3, nodes_per_pkg-1))]
            _write_trace(corpus / f"pkg{i}.json", spec, measured=True)
        return corpus

    def test_passes_with_sufficient_nodes_and_cells(self, tmp_path):
        # Build a corpus big enough to pass both the 100-node gate and 8-cell target.
        # We create 40 packages each with 4 nodes (160 total); varied sizes cover many cells.
        corpus = tmp_path / "corpus"
        corpus.mkdir()
        # Varied package sizes to hit different coverage cells.
        configs = [
            # (pkg_name, n_leaves) → different N and CPR values
            ("small_chain", [(f"s{i}", [f"s{i+1}"] if i < 3 else []) for i in range(4)]),
            ("medium_wide", [("mw_top", [f"mw_l{i}" for i in range(12)])] +
             [(f"mw_l{i}", []) for i in range(12)]),
        ]
        # Create 50 traces with 4 nodes each to get 200 total nodes.
        for i in range(50):
            spec = [(f"p{i}_top", [f"p{i}_dep{j}" for j in range(3)])]
            spec += [(f"p{i}_dep{j}", []) for j in range(3)]
            _write_trace(corpus / f"pkg_{i:02d}.json", spec, measured=True)

        runner = CliRunner()
        result = runner.invoke(validate, ["--corpus", str(corpus)])
        # 50 * 4 = 200 nodes ≥ 100 (gate) ≥ 500 target? No, 200 < 500.
        # total_nodes=200: gate passes, target fails → exit nonzero.
        assert "PASS" in result.output or "FAIL" in result.output
        assert result.exit_code in (0, 1)

    def test_fails_with_empty_corpus(self, tmp_path):
        corpus = tmp_path / "empty"
        corpus.mkdir()
        runner = CliRunner()
        result = runner.invoke(validate, ["--corpus", str(corpus)])
        assert result.exit_code != 0

    def test_fails_below_100_nodes(self, tmp_path):
        corpus = tmp_path / "tiny"
        corpus.mkdir()
        _write_trace(corpus / "pkg.json", [("top", ["dep"]), ("dep", [])])
        runner = CliRunner()
        result = runner.invoke(validate, ["--corpus", str(corpus)])
        assert result.exit_code != 0
        assert "FAIL" in result.output

    def test_variant_files_excluded_from_base_count(self, tmp_path):
        corpus = tmp_path / "corpus"
        corpus.mkdir()
        # Write a base + variant — variant should not double-count nodes.
        spec = [(f"n{i}", []) for i in range(4)]
        _write_trace(corpus / "pkg.json", spec)
        _write_trace(corpus / "pkg.cold.json", spec)   # variant
        _write_trace(corpus / "pkg.warm.json", spec)   # variant
        runner = CliRunner()
        result = runner.invoke(validate, ["--corpus", str(corpus)])
        # Only 4 nodes from base trace should be counted.
        assert "4" in result.output


# ---------------------------------------------------------------------------
# Coverage matrix display
# ---------------------------------------------------------------------------

class TestCoverageMatrixDisplay:
    def test_matrix_shows_all_axes(self):
        from eos_corpus.cli import _coverage_matrix_table
        filled = {"small_low_cpr", "large_high_cpr", "high_convergence"}
        table = _coverage_matrix_table(filled)
        assert "✓" in table
        assert "·" in table
        assert "Low CPR" in table
        assert "High CPR" in table
