#!/usr/bin/env bash
#
# Reproducible TLC model-check runner for the Eos scheduling models.
#
# Checks every topology configuration plus the MultiRequest model at the
# bounded dispatch window (Delta=2) and the degenerate strict-immediacy
# window (Delta=0), teeing the combined output — with a header per
# configuration — to the campaign log. A run is clean iff every
# configuration prints:
#
#     Model checking completed. No error has been found.
#
# Usage (from anywhere): docs/models/tla/run_model_check.sh
# The script re-enters the pinned TLA+ toolchain via shell.nix when needed,
# so it works from a clean checkout with only Nix installed.

set -euo pipefail

cd "$(dirname "$0")"

# Re-exec inside the pinned TLA+ environment if tlc is not already on PATH.
if ! command -v tlc >/dev/null 2>&1; then
    exec nix-shell shell.nix --run "$(printf '%q ' "$(pwd)/run_model_check.sh" "$@")"
fi

LOG_DIR="${LOG_DIR:-/tmp/eos-scheduler-validation}"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/tla_model_check.log"

# Each entry is "<module> <config>".
CHECKS=(
    "LinearModel LinearModel.cfg"
    "DiamondModel DiamondModel.cfg"
    "ConvergenceModel ConvergenceModel.cfg"
    "IndependentModel IndependentModel.cfg"
    "MultiRequestModel MultiRequestModel.cfg"
    "MultiRequestModel MultiRequestModel_Delta0.cfg"
    "StarvationModel StarvationModel.cfg"
)

: > "$LOG"
for entry in "${CHECKS[@]}"; do
    # shellcheck disable=SC2086
    set -- $entry
    module="$1"
    config="$2"
    {
        echo "============================================================"
        echo "== TLC: $module  (config $config)"
        echo "============================================================"
    } | tee -a "$LOG"
    tlc -config "$config" "$module.tla" 2>&1 | tee -a "$LOG"
done

echo
echo "Combined log written to $LOG"
