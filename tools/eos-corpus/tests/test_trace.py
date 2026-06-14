"""Tests for trace.py: emit_trace produces valid simulator-loadable JSON."""

import json
import pytest
from eos_corpus.graph import parse_drv_closure
from eos_corpus.trace import emit_trace, emit_cache_variants


def _simple_closure():
    store = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-"
    return parse_drv_closure({
        store + "top.drv":  {"inputDrvs": {store + "dep.drv": ["out"]}, "name": "top"},
        store + "dep.drv":  {"inputDrvs": {}, "name": "dep"},
    })


def _atom_path(nodes):
    return next(k for k in nodes if "top" in k)


class TestEmitTrace:
    def test_required_fields_on_nodes(self):
        nodes = _simple_closure()
        atom = _atom_path(nodes)
        durations = {k: 10.0 for k in nodes}
        measured = {k: True for k in nodes}
        trace = emit_trace(nodes, durations, measured, atom_path=atom)

        for node in trace["nodes"]:
            assert "id" in node
            assert "duration" in node
            assert "measured" in node
            assert "is_atom" in node
            assert "peak_mem" in node

    def test_atom_flag_set_correctly(self):
        nodes = _simple_closure()
        atom = _atom_path(nodes)
        trace = emit_trace(nodes, {}, {}, atom_path=atom)
        atom_nodes = [n for n in trace["nodes"] if n["is_atom"]]
        non_atom = [n for n in trace["nodes"] if not n["is_atom"]]
        assert len(atom_nodes) == 1
        assert atom_nodes[0]["id"] == atom
        assert len(non_atom) == 1

    def test_edges_reference_valid_ids(self):
        nodes = _simple_closure()
        trace = emit_trace(nodes, {}, {})
        node_ids = {n["id"] for n in trace["nodes"]}
        for edge in trace["edges"]:
            assert edge["from"] in node_ids
            assert edge["to"] in node_ids

    def test_workers_default_eight(self):
        nodes = _simple_closure()
        trace = emit_trace(nodes, {}, {})
        assert len(trace["workers"]) == 8

    def test_store_cached_default_empty(self):
        nodes = _simple_closure()
        trace = emit_trace(nodes, {}, {})
        assert trace["store_cached"] == []

    def test_measured_false_for_fallback(self):
        nodes = _simple_closure()
        trace = emit_trace(nodes, {}, {})  # no measured flags → all False
        for n in trace["nodes"]:
            assert n["measured"] is False

    def test_serializes_to_valid_json(self):
        nodes = _simple_closure()
        trace = emit_trace(nodes, {k: 5.0 for k in nodes}, {k: True for k in nodes})
        txt = json.dumps(trace)
        reparsed = json.loads(txt)
        assert len(reparsed["nodes"]) == 2


class TestCacheVariants:
    def _big_trace(self, n=600):
        store = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-"
        raw = {}
        prev = None
        for i in range(n):
            name = f"pkg-{i}"
            path = store + name + ".drv"
            raw[path] = {"inputDrvs": {prev: ["out"]} if prev else {}, "name": name}
            prev = path
        nodes = parse_drv_closure(raw)
        durations = {k: 1.0 for k in nodes}
        measured = {k: False for k in nodes}
        return emit_trace(nodes, durations, measured)

    def test_requires_cold_partial_warm(self):
        base = self._big_trace(600)
        variants = emit_cache_variants(base, n=600)
        assert set(variants.keys()) == {"cold", "partial", "warm"}

    def test_cold_has_empty_store_cached(self):
        base = self._big_trace(600)
        variants = emit_cache_variants(base, n=600)
        assert variants["cold"]["store_cached"] == []

    def test_partial_caches_roughly_half(self):
        n = 600
        base = self._big_trace(n)
        variants = emit_cache_variants(base, n=n)
        cached = variants["partial"]["store_cached"]
        assert len(cached) > 0
        assert len(cached) < n

    def test_warm_caches_more_than_partial(self):
        n = 600
        base = self._big_trace(n)
        variants = emit_cache_variants(base, n=n)
        assert len(variants["warm"]["store_cached"]) >= len(variants["partial"]["store_cached"])

    def test_variants_share_same_nodes(self):
        base = self._big_trace(600)
        variants = emit_cache_variants(base, n=600)
        cold_ids = {n["id"] for n in variants["cold"]["nodes"]}
        warm_ids = {n["id"] for n in variants["warm"]["nodes"]}
        assert cold_ids == warm_ids
