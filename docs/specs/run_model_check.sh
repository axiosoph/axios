#!/usr/bin/env bash
#
# Reproducible checker runner for the atom-transactions formal models.
#
# Runs every TLC configuration and the Alloy model headlessly, teeing a
# combined log.  A run is clean iff:
#   * every TLC configuration prints "Model checking completed. No error has
#     been found."; and
#   * every Alloy `check` prints "No counterexample found" (assertion valid)
#     and every Alloy `run` prints "Instance found" (predicate consistent).
#
# Usage (from anywhere): docs/specs/run_model_check.sh
# Re-enters the pinned toolchain via docs/specs/shell.nix when tlc/java are
# not already on PATH, so it works from a clean checkout with only Nix.

set -euo pipefail

cd "$(dirname "$0")"

# Re-exec inside the pinned TLC+Alloy environment if the tools are absent.
if ! command -v tlc >/dev/null 2>&1 || ! command -v java >/dev/null 2>&1; then
    exec nix-shell shell.nix --run "$(printf '%q ' "$(pwd)/run_model_check.sh" "$@")"
fi

: "${ALLOY_JAR:?ALLOY_JAR must be set (provided by shell.nix)}"

LOG_DIR="${LOG_DIR:-/tmp/atom-transactions-validation}"
mkdir -p "$LOG_DIR"
LOG="$LOG_DIR/model_check.log"
: > "$LOG"

# --- TLA+ / TLC : "<module> <config>" ------------------------------------
TLC_CHECKS=(
    "AtomTransactions AtomTransactions_Fork.cfg"
    "AtomTransactions AtomTransactions_Distinct.cfg"
    "AtomCharter AtomCharter_Succession.cfg"
    "AtomCharter AtomCharter_Rotation.cfg"
)
for entry in "${TLC_CHECKS[@]}"; do
    # shellcheck disable=SC2086
    set -- $entry
    { echo "============================================================"
      echo "== TLC: $1  (config $2)"
      echo "============================================================"
    } | tee -a "$LOG"
    ( cd tla && tlc -config "$2" "$1.tla" ) 2>&1 | tee -a "$LOG"
done

# --- Alloy : headless SimpleCLI, SAT4J solver ----------------------------
{ echo "============================================================"
  echo "== Alloy: atom_structure.als (SimpleCLI, SAT4J)"
  echo "============================================================"
} | tee -a "$LOG"
( cd alloy
  rm -f .alloy.tmp
  java -Dsat4j=yes -cp "$ALLOY_JAR" \
      edu.mit.csail.sdg.alloy4whole.SimpleCLI atom_structure.als >/dev/null 2>&1 || true
  # SimpleCLI writes per-command verdicts to .alloy.tmp; surface them.
  grep -E 'Executing|counterexample|Counterexample|Instance found|No instance' .alloy.tmp
  rm -f .alloy.tmp
) 2>&1 | tee -a "$LOG"

echo
echo "Combined log written to $LOG"
