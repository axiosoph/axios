"""Two-tier duration fallback for drv nodes not matched in Hydra.

Tier 1: historical Hydra data (caller responsibility — pass a resolved value or None).
Tier 2: heuristic by output name pattern, refined by direct input count (heft).

When input_count is provided, the "other" bucket uses a heft model derived from
the derivation's direct dependency count (fan-out in the build graph). This
stratifies durations around the scheduler's θ_cost threshold so that nodes with
fewer dependencies fall below the threshold while heavier packages remain above,
enabling heuristics that gate on build cost to discriminate meaningfully.

Name-pattern overrides (compilers, fetchers, hooks) always take priority over heft.
"""

from __future__ import annotations

import re
from typing import Optional


# Tier-2 heuristic midpoints (seconds), derived from spec ranges.
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

# Heft tiers: (max_input_count_inclusive, duration_seconds).
# Calibrated so that the dominant 3–8 input bucket (≈76% of corpus nodes) sits
# below the scheduler's default θ_cost=60s, while medium/heavy builds exceed it.
# Derived from Hydra spot checks: jq=20s (simple), curl=33s (simple),
# openssh=73s (medium), bat=356s (heavy).
_HEFT_TIERS: list[tuple[int, float]] = [
    (0,  10.0),   # 0 inputs: trivial (patch, stub, fixed-output)
    (2,  20.0),   # 1–2 inputs: minimal build
    (8,  40.0),   # 3–8 inputs: simple package — below θ_cost=60s
    (25, 80.0),   # 9–25 inputs: medium package — above θ_cost=60s
    (60, 160.0),  # 26–60 inputs: heavy package
]
_DEFAULT_HEFT = 300.0   # 61+ inputs: very complex (libreoffice-scale)
_DEFAULT_DURATION = 75.0  # used only when input_count is unavailable


def _heft_duration(input_count: int) -> float:
    for threshold, value in _HEFT_TIERS:
        if input_count <= threshold:
            return value
    return _DEFAULT_HEFT


def tier2_duration(drv_name: str, input_count: Optional[int] = None) -> float:
    """Return heuristic duration (seconds) for a drv by its name.

    Name patterns are evaluated first; first match wins.
    When no pattern matches, falls back to heft model if input_count is
    provided, otherwise returns _DEFAULT_DURATION.
    """
    for pattern, value in _PATTERNS:
        if pattern.search(drv_name):
            return value
    if input_count is not None:
        return _heft_duration(input_count)
    return _DEFAULT_DURATION


def resolve_duration(
    drv_name: str,
    tier1: Optional[float] = None,
    input_count: Optional[int] = None,
) -> tuple[float, bool]:
    """Return ``(duration_seconds, measured)`` for a drv node.

    ``tier1`` is the Hydra-measured duration (``stoptime - starttime``), or
    ``None`` when no build record was found (cache hit or missing).
    ``input_count`` is the number of direct build inputs (deps) this drv has.

    Returns ``(tier1, True)`` when tier1 is provided and positive.
    Falls back to tier2 heuristic (``measured=False``) otherwise.
    """
    if tier1 is not None and tier1 > 0:
        return (tier1, True)
    return (tier2_duration(drv_name, input_count=input_count), False)
