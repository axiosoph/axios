module surety_no_f1

open surety_core

// ===========================================================================
// F1 DELIBERATELY ABSENT -- the differential half of the acyclicity
// demonstration (v2 Sec. 2, model-fidelity axiom). Same core, no
// `no a: Atom | a in a.^input`. The bare condition-(v) recursion then
// admits circular-justification fixed points that hash-reference
// construction makes unreachable in reality.
// ===========================================================================

// EXPECTED: instance found -- m1 -> m2 -> m1, both ReproducibleCASource,
// each justified by the other. The identical predicate is UNSAT in
// surety_classification.als (F1 present).
run CircularJustificationAdmitted {
  some disj m1, m2: Atom {
    m2 in m1.input and m1 in m2.input
    m1.classify = ReproducibleCASource
    m2.classify = ReproducibleCASource
  }
} for 6

// EXPECTED: instance found -- worse, the ungrounded cycle PRESENTS AS
// TOTAL: empty residue, no genesis seed anywhere in the closure. This is
// exactly the spurious fixed point F1 exists to exclude.
run CircularSelfJustifyingTotal {
  some disj m1, m2: Atom {
    m2 in m1.input and m1 in m2.input
    Total[m1]
    m1 not in GenesisSeed
    no (depclosure[m1] & GenesisSeed)
  }
} for 6
