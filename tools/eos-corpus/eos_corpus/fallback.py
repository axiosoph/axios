"""Two-tier duration fallback for drv nodes not matched in Hydra.

Tier 1: historical Hydra data (caller responsibility — pass a resolved value or None).
Tier 2: heuristic by output name pattern.
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
_DEFAULT_DURATION = 75.0  # 30–120 s → midpoint 75 s


def tier2_duration(drv_name: str) -> float:
    """Return heuristic duration (seconds) for a drv by its name.

    Evaluated in priority order; first match wins.
    """
    for pattern, value in _PATTERNS:
        if pattern.search(drv_name):
            return value
    return _DEFAULT_DURATION


def resolve_duration(
    drv_name: str,
    tier1: Optional[float] = None,
) -> tuple[float, bool]:
    """Return ``(duration_seconds, measured)`` for a drv node.

    ``tier1`` is the Hydra-measured duration (``stoptime - starttime``), or
    ``None`` when no build record was found (cache hit or missing).

    Returns ``(tier1, True)`` when tier1 is provided and positive.
    Falls back to tier2 heuristic (``measured=False``) otherwise.
    """
    if tier1 is not None and tier1 > 0:
        return (tier1, True)
    return (tier2_duration(drv_name), False)
