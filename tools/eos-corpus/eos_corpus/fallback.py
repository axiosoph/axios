"""Two-tier duration fallback for drv nodes not matched in Hydra.

Tier 1: historical Hydra data (caller responsibility — pass a resolved value or None).
Tier 2: heuristic with three sub-levels in priority order:
    2a. Named-package table — well-known packages whose build times are documented
        or have been measured via Hydra; match on output name prefix.
    2b. Name-pattern overrides — structural roles (compilers, fetchers, hooks, docs).
    2c. Heft model — direct input count as a build-complexity proxy.

All tier-2 estimates are multiplied by a deterministic log-normal jitter factor
seeded from the derivation ID, so the same drv always gets the same duration but
different drvs with the same bucket get distinct values.  This breaks the
degenerate case where every "other" node is identical, letting cost-gated
heuristics discriminate within the same structural class.

σ_jitter = 0.4 (log-scale std dev) gives a ≈ 1.5× half-range:
    68% of values fall within [0.67×, 1.49×] of the midpoint.
"""

from __future__ import annotations

import math
import random
import re
from typing import Optional

# ---------------------------------------------------------------------------
# Tier-2a: named-package table
# ---------------------------------------------------------------------------
# Estimates sourced from Hydra live measurements (annotated) or community
# build-time surveys.  Values are midpoints; jitter is applied on top.
# Listed in lookup-priority order: first match wins.
_NAMED: list[tuple[re.Pattern, float]] = [
    # Hydra-measured corpus packages (real stoptime-starttime values)
    (re.compile(r"^linux-\d"),        1477.0),  # Hydra: 1477s
    (re.compile(r"^ffmpeg-\d"),        776.0),  # Hydra: 776s
    (re.compile(r"^git-\d"),           677.0),  # Hydra: 677s
    (re.compile(r"^bat-\d"),           356.0),  # Hydra: 356s
    (re.compile(r"^fd-\d"),            130.0),  # Hydra: 130s
    (re.compile(r"^openssh-\d"),        73.0),  # Hydra: 73s
    (re.compile(r"^ripgrep-\d"),        68.0),  # Hydra: 68s
    (re.compile(r"^curl-\d"),           33.0),  # Hydra: 33s
    (re.compile(r"^jq-\d"),             20.0),  # Hydra: 20s
    # Community-documented approximate build times
    (re.compile(r"^chromium-\d"),    10800.0),  # ~3h on fast hardware
    (re.compile(r"^firefox-\d"),      3600.0),  # ~1h
    (re.compile(r"^llvm-\d"),         2400.0),  # ~40min
    (re.compile(r"^clang-\d"),        2400.0),  # same toolchain as llvm
    (re.compile(r"^gcc-\d"),          1800.0),  # ~30min
    (re.compile(r"^boost-\d"),         600.0),  # ~10min
    (re.compile(r"^qt[56]"),           900.0),  # qt base ~15min
    (re.compile(r"^glibc-\d"),         300.0),  # ~5min
    (re.compile(r"^python3-\d"),       180.0),  # python interpreter build
    (re.compile(r"^nodejs-\d"),        300.0),
    (re.compile(r"^perl-\d"),          150.0),
    (re.compile(r"^ruby-\d"),          120.0),
    (re.compile(r"^cmake-\d"),         120.0),
    (re.compile(r"^glib-\d"),          120.0),  # glib (not glibc)
    (re.compile(r"^openssl-\d"),        90.0),
    (re.compile(r"^sqlite-\d"),         30.0),
    (re.compile(r"^xz-\d"),             10.0),
    (re.compile(r"^bzip2-\d"),           5.0),
    (re.compile(r"^zlib-\d"),            8.0),
]

# ---------------------------------------------------------------------------
# Tier-2b: structural name patterns
# ---------------------------------------------------------------------------
_PATTERNS: list[tuple[re.Pattern, float]] = [
    # compilers: 120–900 s → midpoint 510 s
    (re.compile(r"(^|[-_])(gcc|clang|rustc|llvm)(\d|$|-)", re.I), 510.0),
    # fetch/source: 2–10 s → midpoint 6 s
    (re.compile(r"(^|-)(source|src|fetch[a-z]*)(\.drv)?$", re.I), 6.0),
    # docs/manuals: 5–30 s → midpoint 17 s
    (re.compile(r"(^|-)(doc|docs|man|manual)(s)?(\.drv)?(-|$)", re.I), 17.0),
    # hooks/wrappers/setup: 1–5 s → midpoint 3 s
    (re.compile(r"(^|-)(hook|setup[a-z-]*|wrapper[a-z-]*)(\.drv)?(-|$)", re.I), 3.0),
]

# ---------------------------------------------------------------------------
# Tier-2c: heft tiers (input count → build complexity proxy)
# ---------------------------------------------------------------------------
# Calibrated so that the dominant 3–8 input bucket (≈76% of corpus nodes)
# sits below the scheduler's default θ_cost=60s before jitter.
_HEFT_TIERS: list[tuple[int, float]] = [
    (0,  10.0),   # 0 inputs: trivial (patch, stub, fixed-output)
    (2,  20.0),   # 1–2 inputs: minimal build
    (8,  40.0),   # 3–8 inputs: simple package
    (25, 80.0),   # 9–25 inputs: medium package
    (60, 160.0),  # 26–60 inputs: heavy package
]
_DEFAULT_HEFT = 300.0   # 61+ inputs: very complex
_DEFAULT_DURATION = 75.0  # fallback when input_count is unavailable

# ---------------------------------------------------------------------------
# Log-normal jitter
# ---------------------------------------------------------------------------
_JITTER_SIGMA = 0.4  # log-scale std dev; 68% of values within [0.67×, 1.49×]


def _jitter(base: float, drv_id: str) -> float:
    """Apply deterministic log-normal jitter seeded from the drv store-path hash.

    Nix store paths use base32 (not hex), so we hash the string directly.
    Python's built-in hash() is seeded per-process; use a stable alternative.
    We XOR the ordinal values of the first 16 chars with prime multipliers for
    a simple stable seed that gives good distribution across drv basenames.
    """
    seed = 0
    for i, ch in enumerate(drv_id[:16]):
        seed ^= ord(ch) * (1000003 + i * 37)
    seed &= 0xFFFFFFFF
    rng = random.Random(seed)
    shift = rng.gauss(0.0, _JITTER_SIGMA)
    return max(1.0, base * math.exp(shift))


# ---------------------------------------------------------------------------
# Public API
# ---------------------------------------------------------------------------

def _heft_duration(input_count: int) -> float:
    for threshold, value in _HEFT_TIERS:
        if input_count <= threshold:
            return value
    return _DEFAULT_HEFT


def tier2_duration(
    drv_name: str,
    input_count: Optional[int] = None,
    drv_id: Optional[str] = None,
) -> float:
    """Return heuristic duration (seconds) for a drv.

    Evaluation order:
      1. Named-package table (2a) — precise known values.
      2. Structural name patterns (2b) — role-based heuristics (no jitter applied
         to role-based estimates; their variance is minimal and already low).
      3. Heft model (2c) — input-count buckets.

    Jitter is applied on top of 2a and 2c values when drv_id is provided.
    Role patterns (2b) are NOT jittered — hooks, fetchers, and docs have
    very low real-world variance.
    """
    # 2a: named package
    for pattern, value in _NAMED:
        if pattern.search(drv_name):
            return _jitter(value, drv_id) if drv_id else value

    # 2b: structural role (no jitter — low real-world variance)
    for pattern, value in _PATTERNS:
        if pattern.search(drv_name):
            return value

    # 2c: heft model
    base = _heft_duration(input_count) if input_count is not None else _DEFAULT_DURATION
    return _jitter(base, drv_id) if drv_id else base


def resolve_duration(
    drv_name: str,
    tier1: Optional[float] = None,
    input_count: Optional[int] = None,
    drv_id: Optional[str] = None,
) -> tuple[float, bool]:
    """Return ``(duration_seconds, measured)`` for a drv node.

    ``tier1``       — Hydra-measured duration, or None (cache hit / not found).
    ``input_count`` — number of direct build inputs (deps) this drv has.
    ``drv_id``      — drv store-path basename (hash prefix used as jitter seed).

    Returns ``(tier1, True)`` when tier1 is positive.
    Falls back to tier-2 heuristic (``measured=False``) otherwise.
    """
    if tier1 is not None and tier1 > 0:
        return (tier1, True)
    return (tier2_duration(drv_name, input_count=input_count, drv_id=drv_id), False)
