"""Tests for hydra.py: schema parsing, duration extraction, commit resolution.

All tests use monkeypatched _get; no live network calls.
"""

import pytest
from unittest.mock import patch, MagicMock
from eos_corpus.hydra import HydraClient


@pytest.fixture
def client():
    c = HydraClient(delay=0)  # no rate-limit delay in tests
    return c


# ---------------------------------------------------------------------------
# nixpkgs_commit extraction
# ---------------------------------------------------------------------------

class TestNixpkgsCommit:
    def test_flake_uri_github(self, client):
        ev = {"flake": "github:NixOS/nixpkgs/00e16e88fac4?narHash=sha256-xxx"}
        assert client.nixpkgs_commit(ev) == "00e16e88fac4"

    def test_flake_uri_long_sha(self, client):
        sha = "a" * 40
        ev = {"flake": f"github:NixOS/nixpkgs/{sha}"}
        assert client.nixpkgs_commit(ev) == sha

    def test_legacy_input_revision(self, client):
        ev = {
            "flake": None,
            "jobsetevalinputs": {
                "nixpkgs": {"revision": "00e16e88fac4", "type": "git", "uri": None}
            },
        }
        assert client.nixpkgs_commit(ev) == "00e16e88fac4"

    def test_returns_none_when_absent(self, client):
        ev = {"flake": None, "jobsetevalinputs": {}}
        assert client.nixpkgs_commit(ev) is None

    def test_flake_without_commit(self, client):
        ev = {"flake": "github:NixOS/nixpkgs/nixos-24.11"}
        # "nixos-24.11" is not hex-only → should return None
        assert client.nixpkgs_commit(ev) is None


# ---------------------------------------------------------------------------
# build_duration
# ---------------------------------------------------------------------------

class TestBuildDuration:
    def test_succeeded_build(self, client):
        build = {"buildstatus": 0, "starttime": 1000, "stoptime": 1060}
        assert client.build_duration(build) == 60.0

    def test_cache_hit_no_duration(self, client):
        # buildstatus 6 = cached
        build = {"buildstatus": 6, "starttime": 1000, "stoptime": 1000}
        assert client.build_duration(build) is None

    def test_failed_build_no_duration(self, client):
        build = {"buildstatus": 1, "starttime": 1000, "stoptime": 1020}
        assert client.build_duration(build) is None

    def test_zero_times_no_duration(self, client):
        build = {"buildstatus": 0, "starttime": 0, "stoptime": 0}
        assert client.build_duration(build) is None

    def test_stoptime_equals_starttime(self, client):
        build = {"buildstatus": 0, "starttime": 1000, "stoptime": 1000}
        assert client.build_duration(build) is None


# ---------------------------------------------------------------------------
# schema discovery
# ---------------------------------------------------------------------------

class TestSchemaDiscovery:
    def test_records_eval_schema_on_first_call(self, client):
        fake_eval = {
            "id": 1234,
            "timestamp": 1700000000,
            "flake": "github:NixOS/nixpkgs/abc123",
            "builds": [100, 200],
            "jobsetevalinputs": {},
        }
        with patch.object(client, "_get", return_value=fake_eval):
            client.get_eval(1234)
        assert "eval" in client.discovered_schema
        assert "id" in client.discovered_schema["eval"]
        assert client.discovered_schema["eval"]["id"] == "int"

    def test_only_records_once(self, client):
        fake_eval1 = {"id": 1, "timestamp": 1000, "flake": None, "builds": [], "jobsetevalinputs": {}}
        fake_eval2 = {"id": 2, "timestamp": 2000, "flake": None, "builds": [], "jobsetevalinputs": {}}
        with patch.object(client, "_get", return_value=fake_eval1):
            client.get_eval(1)
        first_schema = client.discovered_schema.get("eval")
        with patch.object(client, "_get", return_value=fake_eval2):
            client.get_eval(2)
        assert client.discovered_schema["eval"] is first_schema  # same object, not replaced


# ---------------------------------------------------------------------------
# find_latest_eval_id
# ---------------------------------------------------------------------------

class TestFindLatestEvalId:
    def test_parses_eval_id_from_redirect(self, client):
        mock_resp = MagicMock()
        mock_resp.headers = {"Location": "https://hydra.nixos.org/eval/1826247?name=unstable"}
        with patch.object(client._session, "get", return_value=mock_resp):
            result = client.find_latest_eval_id()
        assert result == 1826247

    def test_raises_on_missing_location(self, client):
        mock_resp = MagicMock()
        mock_resp.headers = {"Location": "https://hydra.nixos.org/"}
        with patch.object(client._session, "get", return_value=mock_resp):
            with pytest.raises(RuntimeError):
                client.find_latest_eval_id()
