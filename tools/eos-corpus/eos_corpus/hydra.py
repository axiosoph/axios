"""Rate-limited Hydra CI REST client.

API reference: https://github.com/NixOS/hydra/blob/master/hydra-api.yaml

Key endpoints used:
  GET /eval/{eval-id}
      → {id, timestamp, flake, builds: [int], jobsetevalinputs: {name: {revision, uri, type}}}
  GET /eval/{eval-id}/builds
      → [{id, drvpath, job, starttime, stoptime, buildstatus, nixname, ...}]
  GET /build/{build-id}
      → {id, drvpath, job, starttime, stoptime, buildstatus, nixname, buildproducts,
         buildoutputs, buildmetrics, releasename, jobsetevals, priority, finished,
         project, system}
      NOTE: no 'buildsteps' field exists in the Hydra API (confirmed via live API probe
      and hydra-api.yaml schema inspection on 2026-06-14). Per-dependency timing is not
      available from Hydra. Only top-level package starttime/stoptime is available.
  GET /api/latestbuilds?project=P&jobset=J&job=JOB&nr=N
      → [{id, job, project, finished, jobset, buildstatus, nixname, timestamp, system}]
      NOTE: the latestbuilds list response does NOT include starttime/stoptime or drvpath;
      you must call GET /build/{id} to retrieve those fields.

Duration per build: stoptime - starttime (seconds).
buildstatus==0 means succeeded; other values mean failed/cached/queued.
Cache hits show starttime==stoptime (or stoptime==0).
Builds fetched from nixpkgs/unstable are nearly always cache hits (diff=0) because
packages already exist in the binary cache. nixpkgs/staging-next rebuilds from scratch
and typically has real non-zero durations.

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

    def get_eval_builds(self, eval_id: int, extended_timeout: int = 600) -> List[dict]:
        """Return all builds for an eval.

        nixpkgs evals have 284k build records (~100 MB JSON).  Uses streaming
        to avoid read-buffer timeout on slow connections; the connect timeout
        is 30 s, read timeout is ``extended_timeout`` (default 600 s = 10 min).
        """
        elapsed = time.monotonic() - self._last_call
        if elapsed < self.delay:
            time.sleep(self.delay - elapsed)
        resp = self._session.get(
            f"{BASE}/eval/{eval_id}/builds",
            stream=True,
            timeout=(30, extended_timeout),
        )
        self._last_call = time.monotonic()
        resp.raise_for_status()
        chunks: list[bytes] = []
        for chunk in resp.iter_content(chunk_size=1024 * 1024):
            chunks.append(chunk)
        data = __import__("json").loads(b"".join(chunks))
        self._record_schema("eval_builds", data)
        if isinstance(data, list):
            return data
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

    def find_package_build(
        self,
        pkg_attr: str,
        anchor_ts: int,
        nr: int = 10,
    ) -> Optional[dict]:
        """Find the best Hydra build record for a nixpkgs package attribute.

        Strategy:
        1. Try nixpkgs/unstable first (evals are close to anchor commit).
           Most unstable builds are cache hits (starttime==stoptime); a real
           duration here is rare but preferred because the drv is from the same
           eval as the anchor.
        2. Fall back to nixpkgs/staging-next, which rebuilds from scratch and
           nearly always has real non-zero durations.  The drvpath differs from
           the anchor commit's closure, so duration is used as a proxy only —
           it cannot be matched by drvpath.

        Returns the build dict from GET /build/{id} for the best candidate, or
        None if no reachable build with real timing is found.

        The returned dict has keys:
            id, drvpath, nixname, starttime, stoptime, buildstatus,
            jobset, jobsetevals, project, system, ...
        No 'buildsteps' key exists in the Hydra API.
        """
        job = f"{pkg_attr}.x86_64-linux"
        best: Optional[dict] = None

        for jobset in ("unstable", "staging-next"):
            try:
                candidates = self.latest_builds(
                    project="nixpkgs", jobset=jobset, job=job, nr=nr
                )
            except Exception:
                continue

            # Sort by closeness to anchor timestamp, prefer succeeded builds.
            succeeded = [b for b in candidates if b.get("buildstatus") == 0]
            ordered = sorted(
                succeeded,
                key=lambda b: abs(b.get("timestamp", 0) - anchor_ts),
            )

            for candidate in ordered:
                build_id = candidate.get("id")
                if not build_id:
                    continue
                try:
                    full = self.get_build(build_id)
                except Exception:
                    continue

                dur = self.build_duration(full)
                if dur is not None and dur > 0:
                    # Real timing found — prefer this over a cache-hit build.
                    return full

                # Cache hit: keep as fallback in case nothing better exists.
                if best is None:
                    best = full

        return best

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
