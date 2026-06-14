"""Rate-limited Hydra CI REST client.

API reference: https://github.com/NixOS/hydra/blob/master/hydra-api.yaml

Key endpoints used:
  GET /eval/{eval-id}
      → {id, timestamp, flake, builds: [int], jobsetevalinputs: {name: {revision, uri, type}}}
  GET /eval/{eval-id}/builds
      → [{id, drvpath, job, starttime, stoptime, buildstatus, nixname, ...}]
  GET /build/{build-id}
      → {id, drvpath, job, starttime, stoptime, buildstatus, nixname, ...}
  GET /api/latestbuilds?project=P&jobset=J&job=JOB&nr=N
      → [{id, drvpath, job, starttime, stoptime, buildstatus, ...}]

Duration per build: stoptime - starttime (seconds).
buildstatus==0 means succeeded; other values mean failed/cached/queued.
Cache hits show starttime==stoptime (or stoptime==0).

Schema discovered on first live call and stored in self.discovered_schema.
"""

from __future__ import annotations

import time
from typing import Any, Dict, List, Optional

import requests


BASE = "https://hydra.nixos.org"
_DEFAULT_DELAY = 2.0  # seconds between API calls (rate-limit courtesy)


class HydraClient:
    """Thin, rate-limited wrapper around the Hydra JSON API."""

    def __init__(self, delay: float = _DEFAULT_DELAY, timeout: int = 30) -> None:
        self.delay = delay
        self.timeout = timeout
        self._last_call: float = 0.0
        self.discovered_schema: Dict[str, Any] = {}
        self._session = requests.Session()
        self._session.headers.update({"Accept": "application/json"})

    def _get(self, url: str, params: Optional[dict] = None) -> Any:
        """Issue a rate-limited GET and return parsed JSON."""
        elapsed = time.monotonic() - self._last_call
        if elapsed < self.delay:
            time.sleep(self.delay - elapsed)
        resp = self._session.get(url, params=params, timeout=self.timeout)
        self._last_call = time.monotonic()
        resp.raise_for_status()
        return resp.json()

    # ------------------------------------------------------------------
    # Schema discovery
    # ------------------------------------------------------------------

    def _record_schema(self, key: str, obj: Any) -> None:
        """Store a representative schema snippet on first encounter."""
        if key not in self.discovered_schema and isinstance(obj, dict):
            self.discovered_schema[key] = {k: type(v).__name__ for k, v in obj.items()}
        elif key not in self.discovered_schema and isinstance(obj, list) and obj:
            self.discovered_schema[key] = {
                k: type(v).__name__ for k, v in obj[0].items()
                if isinstance(obj[0], dict)
            }

    # ------------------------------------------------------------------
    # Eval endpoints
    # ------------------------------------------------------------------

    def get_eval(self, eval_id: int) -> dict:
        """Return the eval object for a given ID."""
        data = self._get(f"{BASE}/eval/{eval_id}")
        self._record_schema("eval", data)
        return data

    def get_eval_builds(self, eval_id: int) -> List[dict]:
        """Return all builds for an eval (may be a large list for nixpkgs)."""
        data = self._get(f"{BASE}/eval/{eval_id}/builds")
        self._record_schema("eval_builds", data)
        if isinstance(data, list):
            return data
        # Some Hydra versions wrap in an object
        return data.get("builds", [])

    # ------------------------------------------------------------------
    # Build endpoints
    # ------------------------------------------------------------------

    def get_build(self, build_id: int) -> dict:
        """Return full build details."""
        data = self._get(f"{BASE}/build/{build_id}")
        self._record_schema("build", data)
        return data

    def latest_builds(
        self,
        project: str,
        jobset: str,
        job: str,
        nr: int = 10,
    ) -> List[dict]:
        """Return recent builds for a specific job attribute."""
        data = self._get(
            f"{BASE}/api/latestbuilds",
            params={"project": project, "jobset": jobset, "job": job, "nr": nr},
        )
        self._record_schema("latestbuilds", data)
        return data if isinstance(data, list) else []

    # ------------------------------------------------------------------
    # Higher-level helpers
    # ------------------------------------------------------------------

    def build_duration(self, build: dict) -> Optional[float]:
        """Extract build duration in seconds; returns None for cache hits / failures."""
        status = build.get("buildstatus")
        start = build.get("starttime", 0)
        stop = build.get("stoptime", 0)
        # buildstatus 0 = succeeded; 6 = cached/not built
        if status not in (0,):
            return None
        if not start or not stop or stop <= start:
            return None
        return float(stop - start)

    def nixpkgs_commit(self, eval_obj: dict) -> Optional[str]:
        """Extract the nixpkgs commit SHA from an eval object.

        For flake-based evals the commit is embedded in the flake URI.
        For legacy evals it lives in jobsetevalinputs['nixpkgs']['revision'].
        """
        # Flake path: "github:NixOS/nixpkgs/COMMIT?..."
        flake = eval_obj.get("flake") or ""
        if flake:
            parts = flake.split("/")
            for part in reversed(parts):
                clean = part.split("?")[0]
                if len(clean) >= 12 and all(c in "0123456789abcdef" for c in clean):
                    return clean

        # Legacy input path
        inputs = eval_obj.get("jobsetevalinputs") or {}
        nixpkgs_input = inputs.get("nixpkgs") or {}
        revision = nixpkgs_input.get("revision")
        if revision:
            return str(revision)

        return None

    def find_eval_for_commit(
        self,
        commit: str,
        start_eval: int,
        max_lookback: int = 200,
        project: str = "nixpkgs",
        jobset: str = "unstable",
    ) -> Optional[int]:
        """Walk backwards from ``start_eval`` to find the eval containing ``commit``.

        Checks up to ``max_lookback`` consecutive eval IDs.  Returns the eval
        ID of the first match, or None if not found.

        The nixpkgs/unstable jobset evaluates roughly once per day, so 200
        evals covers ~6 months of history.
        """
        for offset in range(max_lookback):
            eid = start_eval - offset
            if eid <= 0:
                break
            try:
                ev = self.get_eval(eid)
            except requests.HTTPError as exc:
                if exc.response is not None and exc.response.status_code == 404:
                    continue
                raise
            sha = self.nixpkgs_commit(ev)
            if sha and commit in sha:
                return eid
        return None

    def find_latest_eval_id(self) -> int:
        """Resolve the latest eval ID via the latest-eval redirect."""
        resp = self._session.get(
            f"{BASE}/jobset/nixpkgs/unstable/latest-eval",
            timeout=self.timeout,
            allow_redirects=False,
        )
        self._last_call = time.monotonic()
        location = resp.headers.get("Location", "")
        # Location: https://hydra.nixos.org/eval/1826247?name=unstable
        for part in location.split("/"):
            part = part.split("?")[0]
            if part.isdigit():
                return int(part)
        raise RuntimeError(f"Cannot parse latest eval ID from Location: {location!r}")
