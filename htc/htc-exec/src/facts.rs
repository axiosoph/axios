//! The fact-set seam — the substrate's only state
//! (composition model §6; execution model §2.2).
//!
//! Execution writes it, composition and the scheduler read it, and its
//! laws are what make the whole design tractable under
//! decentralization: **monotone, commutative, idempotent accumulation
//! of signed records, with no canonical witness and no reconciliation
//! operation anywhere in the API.**
//!
//! The production realization is atom metadata (appended records,
//! htc-sad §6.10) — per-atom, decentralized, no global store. The
//! [`FactChannel`] trait is what that realization implements; the
//! in-memory [`MemFacts`] exists so schedulers and tests can run
//! against the laws today, and so the laws have executable witnesses
//! (the tests below) before the atom-layer implementation lands.
//!
//! Deliberate API absences (regressions if added):
//! - no `remove` — the fact-set only grows (repair happens by adding evidence or changing intent,
//!   never by erasing history);
//! - no `canonical(record_set) -> record` — witness selection is a *consumer* choice under *its*
//!   trust anchors, recorded at request formation, not a store operation;
//! - no trust filtering here — whose facts count is the trust layer's read-time judgment (execution
//!   model §3.4). The store keeps evidence from any signer.

use std::collections::{BTreeMap, BTreeSet};

use crate::{ExecutionRecord, ReqDigest};

/// A record's content identity inside the store: the signed record
/// object's digest. Two byte-identical records are one fact —
/// insertion is idempotent by this key.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct RecordId(pub [u8; 32]);

/// Append-only, multi-witness record store keyed by request identity.
///
/// Laws (tested on [`MemFacts`]; every implementation owes them):
/// 1. **Monotone**: `witnesses(r)` after any `append` is a superset of before.
/// 2. **Commutative + idempotent**: any interleaving and duplication of the same appends yields the
///    same store (set-union CRDT shape) — this is what makes concurrent completion safe with no
///    reconciliation (execution model §5.1, P4).
/// 3. **Multi-valued**: several records for one `ReqDigest` is a legitimate state, surfaced as-is.
pub trait FactChannel {
    type Error;

    /// Append a signed record. Duplicate appends (same `RecordId`) are
    /// no-ops, not errors.
    fn append(&mut self, id: RecordId, record: ExecutionRecord) -> Result<(), Self::Error>;

    /// Every witness for a request, in `RecordId` order (deterministic
    /// enumeration; NOT a preference order — there is no preference).
    fn witnesses(&self, req: ReqDigest) -> Result<Vec<(RecordId, ExecutionRecord)>, Self::Error>;
}

/// In-memory reference implementation — the laws' executable witness.
#[derive(Default, Debug)]
pub struct MemFacts {
    by_req: BTreeMap<ReqDigest, BTreeMap<RecordId, ExecutionRecord>>,
    seen: BTreeSet<RecordId>,
}

impl FactChannel for MemFacts {
    type Error = std::convert::Infallible;

    fn append(&mut self, id: RecordId, record: ExecutionRecord) -> Result<(), Self::Error> {
        if self.seen.insert(id) {
            self.by_req
                .entry(record.req_digest)
                .or_default()
                .insert(id, record);
        }
        Ok(())
    }

    fn witnesses(&self, req: ReqDigest) -> Result<Vec<(RecordId, ExecutionRecord)>, Self::Error> {
        Ok(self
            .by_req
            .get(&req)
            .map(|m| m.iter().map(|(k, v)| (*k, v.clone())).collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Digest, Signature};

    fn record(req: u8, out: u8) -> ExecutionRecord {
        ExecutionRecord {
            req_digest: ReqDigest([req; 32]),
            exit_code: 0,
            outputs: vec![Digest([out; 32])],
            stdout: Digest([0; 32]),
            stderr: Digest([0; 32]),
            observed: None,
            context: None,
            signature: Signature(vec![out]),
        }
    }

    fn ids_for(store: &MemFacts, req: u8) -> Vec<RecordId> {
        store
            .witnesses(ReqDigest([req; 32]))
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    #[test]
    fn accumulates_multiple_witnesses_without_reconciling() {
        // Two trusted builders, two distinct output digests for one
        // request: a legitimate state, both surfaced.
        let mut s = MemFacts::default();
        s.append(RecordId([1; 32]), record(7, 10)).unwrap();
        s.append(RecordId([2; 32]), record(7, 11)).unwrap();
        assert_eq!(ids_for(&s, 7).len(), 2);
    }

    #[test]
    fn append_order_is_irrelevant_and_duplicates_are_noops() {
        let appends = [
            (RecordId([1; 32]), record(7, 10)),
            (RecordId([2; 32]), record(7, 11)),
            (RecordId([3; 32]), record(8, 12)),
        ];
        // forward, reversed, and with every append duplicated
        let mut a = MemFacts::default();
        let mut b = MemFacts::default();
        let mut c = MemFacts::default();
        for (id, r) in appends.iter() {
            a.append(*id, r.clone()).unwrap();
        }
        for (id, r) in appends.iter().rev() {
            b.append(*id, r.clone()).unwrap();
        }
        for (id, r) in appends.iter().chain(appends.iter()) {
            c.append(*id, r.clone()).unwrap();
        }
        for req in [7, 8] {
            assert_eq!(ids_for(&a, req), ids_for(&b, req));
            assert_eq!(ids_for(&a, req), ids_for(&c, req));
        }
    }

    #[test]
    fn growth_is_monotone() {
        let mut s = MemFacts::default();
        s.append(RecordId([1; 32]), record(7, 10)).unwrap();
        let before = ids_for(&s, 7);
        s.append(RecordId([2; 32]), record(7, 11)).unwrap();
        let after = ids_for(&s, 7);
        assert!(before.iter().all(|id| after.contains(id)));
    }
}
