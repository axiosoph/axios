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
# The output is partitioned into CURRENT and HISTORICAL/SUPERSEDED sections
# (2026-08-05).  A green run over the historical section is regression cover
# on a design of record this repo no longer has -- it is not, and must never
# be read as, live protocol assurance.  This partition exists because a
# well-formed, green model of an abolished mechanism reads as current if
# nothing says otherwise: exactly what let the repudiated pair-as-identity
# doctrine (2026-07-18) and a design ADR-0007 SS3 abolished ("genesis-once,
# facts-forever") both stay green for weeks.  See tla/AtomCharter.tla's own
# header banner and docs/architecture/atom-sad.md Appendix E for the same
# distinction stated where each superseded artifact lives.
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

run_tlc_checks() {
    # $1 = array name (nameref), holding "<module> <config>" entries
    local -n entries="$1"
    for entry in "${entries[@]}"; do
        # shellcheck disable=SC2086
        set -- $entry
        { echo "============================================================"
          echo "== TLC: $1  (config $2)"
          echo "============================================================"
        } | tee -a "$LOG"
        ( cd tla && tlc -config "$2" "$1.tla" ) 2>&1 | tee -a "$LOG"
    done
}

# ===========================================================================
# CURRENT -- checks the design of record as it stands today.
# ===========================================================================
{ echo "############################################################"
  echo "# CURRENT -- design of record"
  echo "############################################################"
} | tee -a "$LOG"

# --- TLA+ / TLC : claim/publish subchain ----------------------------------
CURRENT_TLC_CHECKS=(
    "AtomTransactions AtomTransactions_Fork.cfg"
    "AtomTransactions AtomTransactions_Distinct.cfg"
)
run_tlc_checks CURRENT_TLC_CHECKS

# --- Alloy : headless SimpleCLI, SAT4J solver ----------------------------
# atom_structure.als's Charter succession chain (prior-chained charters,
# the founding/successor anchor-derivation split, and the assertion that
# backed it) was abolished 2026-08-05 -- ADR-0007 SS3 ("genesis-once,
# facts-forever") forecloses that shape. See the file's own
# charter_fork_distinction retirement comment and
# docs/specs/atom-transactions.md's Verification section for the full
# argument. The whole file's Charter/claim/identity modeling is current
# as of that date; the mixed-status caveat this comment used to carry no
# longer applies, and this file no longer belongs among ADR-0007's list
# of stale formal models (docs/architecture/atom-sad.md Appendix E) --
# only AtomCharter.tla's Succession/Rotation configs, HISTORICAL below,
# still do.
#
# What a green run below does NOT certify: the fact-based owner-change
# mechanism ADR-0007 SS3 requires ([charter-owner-fact],
# [charter-owner-fact-linear], [claim-owner-fact]) has no model in this
# file at all -- it is graded `evaluator: none` in atom-transactions.md
# because nothing in this repository checks it yet, this file included.
{ echo "============================================================"
  echo "== Alloy: atom_structure.als (SimpleCLI, SAT4J)"
  echo "== Charter/claim/identity modeling current as of 2026-08-05;"
  echo "==   no owner-fact mechanism modeled (see comment above)."
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

# ===========================================================================
# HISTORICAL / SUPERSEDED -- regression cover on an abolished design, not a
# live protocol guarantee.  AtomCharter.tla's Succession and Rotation
# configs machine-check charter/claim SUCCESSION: ownership change by
# writing a new charter or claim record that replaces the last one.
# ADR-0007 SS3 ("One record shape: genesis-once, facts-forever") abolished
# that design -- charter, claim, and publish are each a permanent,
# single genesis record, never reissued; ownership change is now a FACT on
# the one eternal record, authorized by its current owner-fold.  ADR-0007's
# own header names both configs explicitly as stale.  A green run below
# confirms the OLD design was internally consistent; it says nothing about
# the current protocol.  See tla/AtomCharter.tla's header banner for which
# of its actions this affects (four of seven) and which it does not.
# ===========================================================================
{ echo "############################################################"
  echo "# HISTORICAL / SUPERSEDED -- ADR-0007 SS3 genesis-once abolished"
  echo "# this design. Regression cover only, not a live guarantee."
  echo "############################################################"
} | tee -a "$LOG"

HISTORICAL_TLC_CHECKS=(
    "AtomCharter AtomCharter_Succession.cfg"
    "AtomCharter AtomCharter_Rotation.cfg"
)
run_tlc_checks HISTORICAL_TLC_CHECKS

echo
echo "Combined log written to $LOG"
