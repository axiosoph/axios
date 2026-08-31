module surety_core

// ===========================================================================
// Surety-of-source: shared definitional core -- Track B structural model.
//
// Realizes the classification law of the committed surety-of-source model,
// docs/models/surety-of-source.md (companion prose; every sig / fact / pred /
// fun below is the "Alloy shape" of a definition stated there). Labels of the
// form "v2 Sec. N" index the ratified definitional core that model is drawn
// from, kept as stable fine-grained cross-references. This module carries
// every sig / fact / pred / fun EXCEPT the F1 acyclicity axiom, which lives in
// the two entry modules so its effect is demonstrable differentially:
//   surety_classification.als -- F1 asserted (the real model; all checks)
//   surety_no_f1.als          -- F1 absent (circular-justification demo)
//
// The evidence snapshot sigma IS the model instance (a retracted vouch is
// simply absent); the policy P is the pair of free subset sigs
// AdmittedBuilder / AdmittedVoucher, whose valuations the checker explores
// (v2 Sec. 8).
// ===========================================================================

// ---- Buckets and modes (v2 Sec. 2, Sec. 7) --------------------------------

abstract sig Bucket {}
one sig ReproducibleCASource, AttestationResidue,
        TrustImport, SourceClassResidue extends Bucket {}

abstract sig Mode {}
one sig Reproducible, Witnessed extends Mode {}

sig Signer {}
sig ClassName {}

// ---- Artifact sort: atoms + raw fetched byte-payloads (v2 Sec. 1.2) -------

abstract sig Artifact {
  classify: one Bucket   // the total classification function (v2 Sec. 2)
}

sig Atom extends Artifact {
  input:         set Artifact,   // direct build inputs, named by the build record
  declaredClass: one ClassName,  // ClaimPayload.pkg self-declaration (v2 Sec. 4.2)
  declaredMode:  one Mode        // signed reproducibility-mode declaration (v2 Sec. 7)
}

sig RawPayload extends Artifact {}  // promoted fetch pins, adopted-lockfile pins

// ---- Free structural attributes (the closure walk's observables) ----------

sig HasBuildRecord   in Atom {}  // v2 Sec. 1.1: build record present
sig CAOutput         in Atom {}  // condition (iii): output content-addressed
sig PassesFormatGate in Atom {}  // gate (a), hard fail (v2 Sec. 4.1)
sig PassesParseGate  in Atom {}  // gate (b), hard fail (v2 Sec. 4.1)
sig FlaggedByOpacity in Atom {}  // gate (c), SOFT (ruled nrd #4, v2 Sec. 4.1):
                                 // deliberately unconstrained -- it feeds no
                                 // classification fact; its absence from every
                                 // fact below IS the ruling, structurally.

sig GenesisSeed in Artifact {}   // v2 Sec. 3.3: a set, never a singleton

sig GateExecutable in ClassName {}  // verifier adapter coverage (v2 Sec. 4.1)

// ---- Policy P: free sets; the checker quantifies over valuations ----------

sig AdmittedBuilder in Signer {}
sig AdmittedVoucher in Signer {}

// ---- Evidence: sigma is the instance; retraction = omission (v2 Sec. 4.5) --
// Derived/asserted split (v2 Sec. 3.1): a Corroboration is a re-runnable
// execution record; a Vouch is pure keyed judgment.

abstract sig Evidence { signer: one Signer }
sig Corroboration extends Evidence { rec: one Atom }
sig Vouch extends Evidence { target: one Atom, class: one ClassName }

// ---- Well-formedness -------------------------------------------------------

fact BuildRecordNamesInputs {
  // inputs are named BY the build record; no record, no inputs (v2 Sec. 1.2)
  all m: Atom - HasBuildRecord | no m.input
}

fact CorroborationTargetsBuildRecords {
  // a corroboration is a record_core-equal rebuild OF a build record
  all c: Corroboration | c.rec in HasBuildRecord
}

fact SeedsHaveNoBuildRecord {
  // the genesis seed is a permanent trust-import (v2 Sec. 3.3)
  no GenesisSeed & HasBuildRecord
}

// ---- Dependency closure (v2 Sec. 1.2): reflexive-transitive input image ----

fun depclosure[a: Atom]: set Artifact { a.*input }

// ---- Corroboration (v2 Sec. 7, ruled empirical, nrd #3) --------------------
// Independence floor abstracted to ">= 1 policy-admitted corroborating
// record"; the distinct-thumbprint / keys-vs-parties caveat (v2 Sec. 7-8)
// is below the model's granularity and noted in the report.

pred corroborated[m: Atom] {
  some c: Corroboration | c.rec = m and c.signer in AdmittedBuilder
}

// ---- Established sourcehood: the vouch mechanism (v2 Sec. 4.5, cond. (iv)) --

pred srcEstablished[m: Atom] {
  m.declaredClass in GateExecutable
  m in PassesFormatGate
  m in PassesParseGate
  some v: Vouch {
    v.target = m
    v.class = m.declaredClass
    v.signer in AdmittedVoucher
  }
}

// ---- Established_ReproducibleCASource: the five conditions (v2 Sec. 2) -----

pred EstablishedRCAS[m: Atom] {
  m in HasBuildRecord                                  // (i)
  m.declaredMode = Reproducible and corroborated[m]    // (ii)
  m in CAOutput                                        // (iii)
  srcEstablished[m]                                    // (iv)
  all n: m.^input |
    n in GenesisSeed or n.classify = ReproducibleCASource  // (v)
}

// ---- The classification law: biconditional + fail-closed residue
//      precedence cascade (v2 Sec. 2). The model's load-bearing fact. -------

fact ClassificationLaw {
  // raw payloads carry no build record: precedence clause 1
  all p: RawPayload | p.classify = TrustImport

  // the single function both proofs build from (the Sec. 2 biconditional)
  all m: Atom | m.classify = ReproducibleCASource iff EstablishedRCAS[m]

  // residue precedence: the deepest defect books the member
  all m: Atom | not EstablishedRCAS[m] implies {
    m not in HasBuildRecord
      implies m.classify = TrustImport                    // clause 1
      else (not srcEstablished[m]
        implies m.classify = SourceClassResidue           // clause 2
        else m.classify = AttestationResidue)             // clause 3
  }
}

// ---- Trust surface T(a), assumption basis B(a), Total (v2 Sec. 3) ----------

fun trustSurface[a: Atom]: set Artifact {
  { m: depclosure[a] |
      m.classify in AttestationResidue + TrustImport + SourceClassResidue }
}

// B(a): the policy-admitted evidence the non-residue classifications rest on.
// (Its third component, the seed identities, is depclosure[a] & GenesisSeed;
// seeds are artifacts, not evidence records, so they stay out of this sort.)
fun basis[a: Atom]: set Evidence {
  { c: Corroboration |
      c.rec in depclosure[a] and c.signer in AdmittedBuilder
      and c.rec.classify = ReproducibleCASource }
  +
  { v: Vouch |
      v.signer in AdmittedVoucher
      and some m: depclosure[a] & Atom {
        v.target = m and v.class = m.declaredClass and srcEstablished[m]
      } }
}

// Total, ratified non-genesis form (v2 Sec. 3.2, ruling #1)
pred Total[a: Atom] { trustSurface[a] in GenesisSeed }

// ---- Laundering shapes (v2 Sec. 6.1/6.2) -----------------------------------
// Ground-truth "really a built binary" is not machine-representable
// (sourceness fails Rice-invariance, v2 Sec. 5.2); the model renders
// Laundered by its structural/evidential signature -- exactly the two
// escape paths Sec. 6.1 names (C1, C3). The one boundary NOT crossed is
// Sec. 6.2's policy boundary: an admitted voucher's false vouch is pinned
// in B(a), never prevented; see RealTotalCarriesVouchInBasis.

pred launderedShape[m: Artifact] {
  m not in GenesisSeed
  and (
    m not in HasBuildRecord                     // C1: outside build accounting
    or (m in Atom and not srcEstablished[m])    // C3: self-declared sourcehood
  )
}
