//! The **execution primitive** — skeleton types for
//! `docs/models/execution-model.md`.
//!
//! The load-bearing invariants, encoded structurally where possible:
//!
//! - **Policy stratification is derived, never declared** (model §2.3): there is no `is_test` flag
//!   anywhere. [`Policy::stratum`] computes action vs. trial from channel states alone.
//! - **The command is opaque** (model §2.1): [`Command`] is argv/env/ cwd. Nothing in this crate —
//!   and nothing in any conforming executor — interprets it. There is no interpreted language in
//!   the trusted core (ADR-0006 §3).
//! - **Pin payloads are data** (model §1.3): a pinned channel carries canonical bytes that MUST
//!   enter the request digest (obligation P5); the type makes forgetting the payload impossible,
//!   and [`ExecutionRequest::req_digest`] is left `unimplemented!` precisely because its canonical
//!   serialization *is* P5's deliverable — do not ship a casual serialization here.
//! - **Records accumulate** (model §2.2): nothing in this crate is a cache slot. A record is a
//!   signed fact; multiplicity per request is legitimate; equality claims go through
//!   [`RecordCore`], never the full record (model §3.1, F4).

pub mod facts;

use std::collections::BTreeMap;

pub use htc_comp::{CPath, Digest};

/// An ambient channel — anything that can influence a process and is
/// not in its view (model §1.3). `#[non_exhaustive]`: the set is
/// extensible but honest — add variants as reality demands, and extend
/// the conformance table (model §6.3) in the same change.
#[non_exhaustive]
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Channel {
    Net,
    Clock,
    Entropy,
    Ids,
    LocaleTz,
    CpuFeatures,
    Nproc,
    ProcFs,
}

/// Canonical bytes that determine a pinned channel's content as a
/// function of the request (replay-map digest, `SOURCE_DATE_EPOCH`
/// value, uid/gid mapping). MUST be inside the digested request — two
/// executions differing in pin content must never share a cache slot
/// (model §2.1, obligation P5).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PinPayload(pub Vec<u8>);

/// Per-channel, per-execution state (model §1.3).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ChannelState {
    /// The channel does not exist for the process (e.g. empty netns).
    Closed,
    /// The channel exists; its content is a declared function of the
    /// request, carried by the payload.
    Pinned(PinPayload),
    /// The channel passes through to the real world. Any `Open` channel
    /// makes the request a trial.
    Open,
}

/// Whether file access is observed — an execution-policy axis, NOT a
/// mount tier (ADR-0006 §2). ptrace+seccomp is the ratified instrument;
/// instrument coverage is an empirical property recorded with every
/// observation (model §6.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Observe {
    #[default]
    None,
    Trace,
}

/// A policy: a state for every channel, plus the observation axis
/// (model §1.3). Channels absent from the map default to `Closed` —
/// fail-closed is the only sound default.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Policy {
    pub channels: BTreeMap<Channel, ChannelState>,
    pub observe: Observe,
}

/// The two strata (model §2.3). A property of the policy — never of
/// the workload kind. There is deliberately no way to construct this
/// except through [`Policy::stratum`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stratum {
    /// No channel open: world-independent; the record is a cacheable
    /// fact. Builds and hermetic tests live here.
    Action,
    /// Some channel open: world-dependent; the record is a signed
    /// attestation, never a cache value.
    Trial,
}

impl Policy {
    /// Membership in the deterministic stratum, derived (model §1.3:
    /// `Det = { p | no channel open under p }`).
    pub fn stratum(&self) -> Stratum {
        if self
            .channels
            .values()
            .any(|s| matches!(s, ChannelState::Open))
        {
            Stratum::Trial
        } else {
            Stratum::Action
        }
    }
}

/// Opaque command: argv, env, cwd (model §2.1). The executor is a
/// universal machine runner, not an evaluator — nothing interprets
/// this, ever.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Command {
    pub argv: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: CPath,
}

/// The root digest of the composition a request runs against. Under any
/// deterministic policy, `⟦view⟧` plus pinned channel content is the
/// process's entire observable universe (model §2.1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CompositionRoot(pub Digest);

/// The identity of a concrete execution — the executor-level cache key
/// (model §2.4). Constructed only by [`ExecutionRequest::req_digest`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ReqDigest(pub [u8; 32]);

/// The one request shape (model §2.1). Build, test, fetch-discovery,
/// and closure capture are all this type under different policies —
/// there are no other request kinds.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExecutionRequest {
    /// The entire visible universe of the process.
    pub view: CompositionRoot,
    pub command: Command,
    /// Declared scratch/output paths to ingest into the store.
    pub outputs: Vec<CPath>,
    pub policy: Policy,
}

impl ExecutionRequest {
    /// The canonical request digest.
    ///
    /// **Deliberately unimplemented.** The canonical serialization is
    /// obligation P5's deliverable: every field the executor consults —
    /// pin payloads included — must be reachable from the digest
    /// preimage, audited as an inventory, versioned as a format. A
    /// casual `serde` serialization here would silently become a cache
    /// identity; do not add one without discharging P5.
    pub fn req_digest(&self) -> ReqDigest {
        unimplemented!(
            "P5: canonical request serialization is a specified deliverable, not a default — see \
             docs/models/execution-model.md §2.1/§8"
        )
    }
}

/// Executor identity + signature over the record. Rides atom's signing
/// machinery (coz); opaque bytes at this layer. Whose signatures count
/// is the trust layer's judgment (model §3.4), never this crate's.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Signature(pub Vec<u8>);

/// Digest of an observed read-set, present iff `policy.observe` was
/// `Trace` (model §3.1). Coverage limits of the instrument used are
/// recorded alongside upstream — an observed read-set is a lower bound
/// relative to the instrument's trapped surface (model §6.3).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ObservationDigest(pub Digest);

/// World summary attached to trial records only (model §3.1): executor
/// id, time, world summary. Actions carry no context by construction —
/// they have no world to summarize.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TrialContext {
    pub executor_id: String,
    /// Seconds since epoch at execution; trials are evidence about a
    /// moment in the world, so the moment is part of the evidence.
    pub time: u64,
    pub world_summary: String,
}

/// The output of every execution (model §3.1). For an action this is a
/// fact (cache-usable relative to signer trust); for a trial, an
/// attestation (evidence under acceptance policy). Same shape — the
/// epistemic difference lives in the policy that produced it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ExecutionRecord {
    pub req_digest: ReqDigest,
    pub exit_code: i32,
    /// Digests of ingested output trees, in `outputs` order.
    pub outputs: Vec<Digest>,
    /// stdout/stderr blob digests. NOT part of record equality — stdio
    /// is not bit-stable even for reproducible builds (model §3.1).
    pub stdout: Digest,
    pub stderr: Digest,
    pub observed: Option<ObservationDigest>,
    pub context: Option<TrialContext>,
    pub signature: Signature,
}

/// `record_core(r)` — the tuple ALL record-equality claims quantify
/// over (model §3.1, correction F4): reproducibility counting,
/// k-record-equal attestations, golden-request conformance. Never
/// compare full records.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct RecordCore {
    pub req_digest: ReqDigest,
    pub exit_code: i32,
    pub outputs: Vec<Digest>,
}

impl ExecutionRecord {
    pub fn core(&self) -> RecordCore {
        RecordCore {
            req_digest: self.req_digest,
            exit_code: self.exit_code,
            outputs: self.outputs.clone(),
        }
    }
}

/// Why an executor refuses a request (the session type's `Refused`
/// branch, model §6.2): the policy is unsatisfiable *here* — e.g. a pin
/// mechanism this executor lacks. Refusal is not failure; a failed
/// execution returns a record with `exit_code != 0`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PolicyError(pub String);

/// One reply of the `ExecuteSession` (model §6.2).
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ExecuteReply {
    /// Action: a trust-acceptable witness existed (cache hit — any
    /// acceptable witness, there is no canonical one). Trial: existing
    /// evidence accepted under acceptance policy.
    Known(ExecutionRecord),
    /// Executed now.
    Scheduled(ExecutionRecord),
    /// Policy unsatisfiable on this executor.
    Refused(PolicyError),
}

/// The executor: the substrate's single dynamic operation
/// (`execute : Request × World → Record`, model §2.2). Conformance is
/// bisimilarity on the action stratum (golden request → core-equal
/// record) plus valid attestations on trials (model §6.1); the
/// per-channel enforcement table is model §6.3.
///
/// Implementations own sandbox construction; they MUST materialize
/// exactly `⟦view⟧` and MUST NOT interpret `command`. Sync signature
/// for skeleton simplicity; the wire protocol (session type, model
/// §6.2) is where async/streaming lives.
pub trait Executor {
    fn execute(&self, request: &ExecutionRequest) -> ExecuteReply;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_policy() -> Policy {
        Policy::default() // empty map: everything closed, fail-closed
    }

    #[test]
    fn empty_policy_is_action() {
        assert_eq!(base_policy().stratum(), Stratum::Action);
    }

    #[test]
    fn pinned_channels_stay_in_det() {
        let mut p = base_policy();
        p.channels
            .insert(Channel::Net, ChannelState::Pinned(PinPayload(vec![1, 2])));
        p.channels.insert(
            Channel::Clock,
            ChannelState::Pinned(PinPayload(b"1725148800".to_vec())),
        );
        assert_eq!(p.stratum(), Stratum::Action);
    }

    #[test]
    fn any_open_channel_makes_a_trial() {
        let mut p = base_policy();
        p.channels.insert(Channel::Clock, ChannelState::Closed);
        p.channels.insert(Channel::Net, ChannelState::Open);
        assert_eq!(p.stratum(), Stratum::Trial);
    }

    #[test]
    fn observation_does_not_change_stratum() {
        // Observation is an axis, not a policy hole (ADR-0006 §2).
        let mut p = base_policy();
        p.observe = Observe::Trace;
        assert_eq!(p.stratum(), Stratum::Action);
    }
}
