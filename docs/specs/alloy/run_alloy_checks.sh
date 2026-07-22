#!/usr/bin/env bash
#
# Alloy model-check gate for the structural safety models whose committed
# "machine-checked" claims would otherwise have no in-CI evaluator:
#
#   * atom_backend_seam.als      -- the typed backend-seam law
#   * surety_classification.als  -- surety-of-source, F1 present (the real model)
#   * surety_no_f1.als           -- surety-of-source, F1 absent (differential)
#
# It complements docs/specs/run_model_check.sh (TLC configs + atom_structure.als):
# both run the same pinned Alloy 5.1.0 toolchain from docs/specs/shell.nix.
#
# A run is clean iff, for each model, every Alloy `check` reports "No
# counterexample found" and the model's expected `run` verdicts hold:
#   * seam: every run is SAT (an instance is found);
#   * surety_classification: the two load-bearing UNSAT probes stay UNSAT
#     (TotalWithoutVouchers -- Total is vouch-dependent; CircularJustification
#     -- the F1 acyclicity axiom bites);
#   * surety_no_f1: BOTH circular-justification runs are SAT, proving the F1
#     acyclicity axiom is load-bearing (the identical predicate is UNSAT once
#     F1 is present, above).
#
# Exits non-zero (failing CI) on any deviation. Re-enters the pinned toolchain
# via docs/specs/shell.nix when java/ALLOY_JAR are absent, so it runs
# identically from a clean checkout and in CI.

set -euo pipefail

cd "$(dirname "$0")"          # docs/specs/alloy

# Re-exec inside the pinned Alloy environment if the tools are absent.
if [ -z "${ALLOY_JAR:-}" ] || ! command -v java >/dev/null 2>&1; then
    exec nix-shell ../shell.nix --run "$(printf '%q ' "$(pwd)/run_alloy_checks.sh" "$@")"
fi

fail() { echo "ALLOY MODEL-CHECK FAILED: $*" >&2; exit 1; }

# Per-command verdict lines for one .als entry module. SimpleCLI writes them to
# .alloy.tmp and executes each command twice, so verdict lines are doubled --
# an analyzer artifact, present identically in run_model_check.sh's logs.
verdicts() {
    rm -f .alloy.tmp
    java -Dsat4j=yes -cp "$ALLOY_JAR" \
        edu.mit.csail.sdg.alloy4whole.SimpleCLI "$1" >/dev/null 2>&1 || true
    grep -E 'Executing|counterexample|Counterexample|Instance found|No instance' .alloy.tmp
    rm -f .alloy.tmp
}

# The first verdict line following a named command (single awk pass -- no pipe,
# so `set -o pipefail` cannot misfire on an early-closed reader).
verdict_of() {
    awk -v name="$2" '/^Executing/ && index($0, name) { getline; print; exit }' <<<"$1"
}

count_exec() { grep -c '^Executing' <<<"$1" || true; }

# A failing `check` prints "Counterexample found." (capital C); a passing one
# prints "No counterexample found." -- so a case-sensitive match on the former
# never matches the success line.
assert_no_counterexample() {
    if grep -q 'Counterexample found' <<<"$1"; then
        fail "$2: a check found a counterexample"
    fi
}

# ---- atom_backend_seam.als : every check passes, every run is SAT ----------
SEAM="$(verdicts atom_backend_seam.als)"
echo "== atom_backend_seam.als =="; echo "$SEAM"
assert_no_counterexample "$SEAM" atom_backend_seam.als
[ "$(count_exec "$SEAM")" -eq 5 ] \
    || fail "atom_backend_seam.als: expected 5 commands, ran $(count_exec "$SEAM")"
if grep -q 'No instance found' <<<"$SEAM"; then
    fail "atom_backend_seam.als: a run found no instance"
fi

# ---- surety_classification.als : safety holds, UNSAT probes stay UNSAT -----
CLASS="$(verdicts surety_classification.als)"
echo "== surety_classification.als =="; echo "$CLASS"
assert_no_counterexample "$CLASS" surety_classification.als
[ "$(count_exec "$CLASS")" -eq 16 ] \
    || fail "surety_classification.als: expected 16 commands, ran $(count_exec "$CLASS")"
[[ "$(verdict_of "$CLASS" 'Run TotalWithoutVouchers')" == *'No instance found'* ]] \
    || fail "surety_classification.als: TotalWithoutVouchers must be UNSAT (Total is vouch-dependent)"
[[ "$(verdict_of "$CLASS" 'Run CircularJustification for')" == *'No instance found'* ]] \
    || fail "surety_classification.als: CircularJustification must be UNSAT (F1 acyclicity bites)"

# ---- surety_no_f1.als : the differential must find BOTH instances ----------
NOF1="$(verdicts surety_no_f1.als)"
echo "== surety_no_f1.als =="; echo "$NOF1"
[ "$(count_exec "$NOF1")" -eq 2 ] \
    || fail "surety_no_f1.als: expected 2 commands, ran $(count_exec "$NOF1")"
if grep -q 'No instance found' <<<"$NOF1"; then
    fail "surety_no_f1.als: differential must find both circular-justification instances"
fi

echo
echo "All Alloy structural safety checks passed."
