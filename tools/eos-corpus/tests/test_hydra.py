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


# ---------------------------------------------------------------------------
# find_package_build
# ---------------------------------------------------------------------------

class TestFindPackageBuild:
    """Tests for find_package_build: per-package Hydra lookup."""

    def _make_latestbuilds_response(self, build_ids_and_ts):
        """Return a latestbuilds-style list (no starttime/stoptime)."""
        return [
            {"id": bid, "buildstatus": 0, "timestamp": ts, "finished": 1, "job": "jq.x86_64-linux"}
            for bid, ts in build_ids_and_ts
        ]

    def _make_full_build(self, build_id, start, stop, jobset="unstable"):
        return {
            "id": build_id,
            "drvpath": f"/nix/store/HASH-pkg.drv",
            "nixname": "jq-1.8.1",
            "starttime": start,
            "stoptime": stop,
            "buildstatus": 0,
            "jobset": jobset,
            "jobsetevals": [1826247],
            "project": "nixpkgs",
            "system": "x86_64-linux",
            "finished": 1,
        }

    def test_returns_real_timing_build_over_cache_hit(self, client):
        """When unstable has a cache hit but staging-next has real timing, returns staging-next build."""
        anchor_ts = 1779235200

        cache_hit = self._make_full_build(1001, start=1000, stop=1000, jobset="unstable")
        real_build = self._make_full_build(2001, start=1000, stop=1060, jobset="staging-next")

        call_count = {"n": 0}

        def fake_latest_builds(project, jobset, job, nr=10):
            if jobset == "unstable":
                return self._make_latestbuilds_response([(1001, anchor_ts - 100)])
            elif jobset == "staging-next":
                return self._make_latestbuilds_response([(2001, anchor_ts - 200)])
            return []

        def fake_get_build(build_id):
            if build_id == 1001:
                return cache_hit
            if build_id == 2001:
                return real_build
            return {}

        with patch.object(client, "latest_builds", side_effect=fake_latest_builds), \
             patch.object(client, "get_build", side_effect=fake_get_build):
            result = client.find_package_build("jq", anchor_ts=anchor_ts)

        assert result is not None
        assert result["id"] == 2001
        assert result["jobset"] == "staging-next"
        assert client.build_duration(result) == 60.0

    def test_returns_none_when_no_builds_found(self, client):
        """Returns None when latestbuilds returns empty for all jobsets."""
        with patch.object(client, "latest_builds", return_value=[]):
            result = client.find_package_build("jq", anchor_ts=1779235200)
        assert result is None

    def test_prefers_build_closest_to_anchor(self, client):
        """Among builds with real timing, selects the one closest to anchor timestamp."""
        anchor_ts = 1779235200

        build_far = {"id": 3001, "buildstatus": 0, "timestamp": anchor_ts - 10000, "finished": 1}
        build_close = {"id": 3002, "buildstatus": 0, "timestamp": anchor_ts - 500, "finished": 1}

        full_far = self._make_full_build(3001, start=100, stop=160, jobset="staging-next")
        full_close = self._make_full_build(3002, start=200, stop=280, jobset="staging-next")

        def fake_latest_builds(project, jobset, job, nr=10):
            if jobset == "unstable":
                return []
            return [build_far, build_close]

        def fake_get_build(build_id):
            return {3001: full_far, 3002: full_close}.get(build_id, {})

        with patch.object(client, "latest_builds", side_effect=fake_latest_builds), \
             patch.object(client, "get_build", side_effect=fake_get_build):
            result = client.find_package_build("jq", anchor_ts=anchor_ts)

        # Should pick build_close (diff=500) over build_far (diff=10000)
        assert result is not None
        assert result["id"] == 3002

    def test_skips_failed_builds(self, client):
        """Only buildstatus==0 builds are considered."""
        anchor_ts = 1779235200
        failed = {"id": 4001, "buildstatus": 1, "timestamp": anchor_ts, "finished": 1}
        succeeded = {"id": 4002, "buildstatus": 0, "timestamp": anchor_ts - 100, "finished": 1}

        full_succeeded = self._make_full_build(4002, start=100, stop=160, jobset="staging-next")

        def fake_latest_builds(project, jobset, job, nr=10):
            if jobset == "unstable":
                return [failed]
            return [succeeded]

        with patch.object(client, "latest_builds", side_effect=fake_latest_builds), \
             patch.object(client, "get_build", return_value=full_succeeded):
            result = client.find_package_build("jq", anchor_ts=anchor_ts)

        assert result is not None
        assert result["id"] == 4002

    def test_falls_back_to_cache_hit_when_no_real_timing(self, client):
        """If only cache hits exist, returns one rather than None."""
        anchor_ts = 1779235200
        cache_build = {"id": 5001, "buildstatus": 0, "timestamp": anchor_ts, "finished": 1}
        full_cache = self._make_full_build(5001, start=1000, stop=1000, jobset="unstable")

        def fake_latest_builds(project, jobset, job, nr=10):
            if jobset == "unstable":
                return [cache_build]
            return []

        with patch.object(client, "latest_builds", side_effect=fake_latest_builds), \
             patch.object(client, "get_build", return_value=full_cache):
            result = client.find_package_build("jq", anchor_ts=anchor_ts)

        # Cache hit build is returned as fallback; build_duration returns None
        assert result is not None
        assert client.build_duration(result) is None

    def test_handles_api_exceptions_gracefully(self, client):
        """If latestbuilds raises, tries next jobset; returns None if all fail."""
        with patch.object(client, "latest_builds", side_effect=Exception("network error")):
            result = client.find_package_build("jq", anchor_ts=1779235200)
        assert result is None
