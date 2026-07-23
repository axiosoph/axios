module surety_classification

open surety_core

// ===========================================================================
// The real model: surety_core + the F1 acyclicity axiom, with every check
// and probe. Companion prose: docs/models/surety-of-source.md. Labels of the
// form "v2 Sec. N" index the ratified definitional core that model is drawn
// from.
// ===========================================================================

// F1 ACYCLICITY AXIOM (v2 Sec. 2, model-fidelity axiom -- REQUIRED):
// a build input is a czd/hash reference to an already-fixed artifact, so
// dependency closures are acyclic by construction. Without this, the bare
// condition-(v) recursion admits spurious circular-justification fixed
// points -- demonstrated in surety_no_f1.als.
fact F1Acyclic {
  no a: Atom | a in a.^input
}

// ==== SAFETY, sense 1 (v2 Sec. 3.4, Sec. 6.2) ==============================
// These must hold with AdmittedVoucher unconstrained -- including empty,
// and including adversary vouches from non-admitted signers.

// No laundered member ever classifies into the closed bucket.
assert NoSilentLaundering {
  all m: Artifact | launderedShape[m] implies
    m.classify in TrustImport + AttestationResidue + SourceClassResidue
}
check NoSilentLaundering for 6 but 8 Artifact, 8 Evidence

// A closure containing any laundered member can never present as Total.
assert LaunderedNeverPresentsAsTotal {
  all a: Atom |
    (some m: depclosure[a] | launderedShape[m]) implies not Total[a]
}
check LaunderedNeverPresentsAsTotal for 6 but 8 Artifact, 8 Evidence

// C1 fetch-pin: a no-build-record member is FORCED into TrustImport
// (v2 Sec. 6.1, closing attack-c1) ...
assert C1FetchPinForcedToTrustImport {
  all m: Artifact | m not in HasBuildRecord implies m.classify = TrustImport
}
check C1FetchPinForcedToTrustImport for 6 but 8 Artifact, 8 Evidence

// ... and therefore any non-seed fetch pin in the closure defeats Total.
assert C1FetchPinBlocksTotal {
  all a: Atom |
    (some m: depclosure[a] | m not in GenesisSeed and m not in HasBuildRecord)
      implies not Total[a]
}
check C1FetchPinBlocksTotal for 6 but 8 Artifact, 8 Evidence

// No testimony-only path into the closed bucket (rulings #2 and #3):
// membership is decided by what independently happened, never declaration.
assert DeclarationAloneNeverCloses {
  all m: Atom | m.classify = ReproducibleCASource implies
    (corroborated[m] and srcEstablished[m])
}
check DeclarationAloneNeverCloses for 6 but 8 Artifact, 8 Evidence

// v2 Sec. 6.2 differential (ii): vouches from non-admitted signers are inert.
assert NonAdmittedVouchesInert {
  all m: Atom |
    (no v: Vouch | v.target = m and v.class = m.declaredClass
                   and v.signer in AdmittedVoucher)
      implies m.classify != ReproducibleCASource
}
check NonAdmittedVouchesInert for 6 but 8 Artifact, 8 Evidence

// Seeds are permanent trust-imports (v2 Sec. 3.3).
assert SeedsAreTrustImports {
  all s: GenesisSeed | s.classify = TrustImport
}
check SeedsAreTrustImports for 6 but 8 Artifact, 8 Evidence

// The forced-attributability half of the policy boundary (v2 Sec. 4.5):
// every real Total verdict carries at least one admitted vouch, enumerated
// in the assumption basis B(a) -- the trust is located, never erased.
assert RealTotalCarriesVouchInBasis {
  all a: Atom | (Total[a] and a not in GenesisSeed) implies
    some (basis[a] & Vouch)
}
check RealTotalCarriesVouchInBasis for 6 but 8 Artifact, 8 Evidence

// Deeper-scope margin on the two load-bearing safety assertions.
check NoSilentLaundering for 8 but 10 Artifact, 10 Evidence
check LaunderedNeverPresentsAsTotal for 8 but 10 Artifact, 10 Evidence

// ==== SATISFIABILITY, sense 2 (v2 Sec. 3.4) ================================

// A real (non-seed) atom CAN be Total: the predicate is not vacuous.
// EXPECTED: instance found.
run RealAtomCanBeTotal {
  some a: Atom | Total[a] and a not in GenesisSeed
} for 6

// ... including grounded through a genuine seed at the closure base.
// EXPECTED: instance found.
run RealAtomTotalGroundedInSeed {
  some a: Atom | Total[a] and a not in GenesisSeed
    and some (a.^input & GenesisSeed)
} for 6

// Sense-2 differential (v2 Sec. 3.4, Sec. 6.2 differential (i)): with NO
// admitted vouchers, no real atom is ever Total -- the vouch mechanism is
// exactly what makes Total non-vacuous.
// EXPECTED: no instance.
run TotalWithoutVouchers {
  no AdmittedVoucher
  some a: Atom | Total[a] and a not in GenesisSeed
} for 6

// The v2 Sec. 5.3 gen.c witness's classification fate: gate-evading,
// corroborated, CA -- but unvouched: it sits in SourceClassResidue and
// never presents as Total. EXPECTED: instance found.
run GenCWitnessSitsInResidue {
  some m: Atom {
    m in HasBuildRecord
    m in PassesFormatGate and m in PassesParseGate
    m.declaredClass in GateExecutable
    m.declaredMode = Reproducible and corroborated[m]
    m in CAOutput
    no v: Vouch | v.target = m and v.class = m.declaredClass
                  and v.signer in AdmittedVoucher
    m.classify = SourceClassResidue
  }
} for 6

// Vacuity guard: the facts jointly admit instances inhabiting all four
// buckets at once (the checks above are not green over an empty theory).
// EXPECTED: instance found.
run AllBucketsInhabited {
  all b: Bucket | some classify.b
} for 6 but 8 Artifact, 8 Evidence

// ==== F1 bites: with the axiom, circular justification is EXCLUDED ========
// EXPECTED: no instance. The identical predicate is satisfiable in
// surety_no_f1.als (F1 absent).
run CircularJustification {
  some disj m1, m2: Atom {
    m2 in m1.input and m1 in m2.input
    m1.classify = ReproducibleCASource
    m2.classify = ReproducibleCASource
  }
} for 6
