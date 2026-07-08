# HTC — the substrate workspace (L2)

Skeleton crates translating the ratified formal models into compilable
types, so implementation work starts from the model rather than
re-deriving it:

| Crate      | Model                                                    | Contents                                                                                                                                 |
| :--------- | :------------------------------------------------------- | :--------------------------------------------------------------------------------------------------------------------------------------- |
| `htc-comp` | [Composition Model](../docs/models/composition-model.md) | Composition values, the merge monoid `⊕` (working, law-tested; conservative graft-conflict rule pending P1), interface/certificate types |
| `htc-exec` | [Execution Model](../docs/models/execution-model.md)     | Requests, channel policies, derived action/trial strata, records + `record_core`, the `Executor` trait and session reply                 |

Ground rules encoded in the types (do not regress them):

- Stratum (action vs. trial) is **derived from the policy** — no
  workload-kind flags, ever.
- `Command` is opaque. Nothing interprets it (ADR-0006 §3).
- `ExecutionRequest::req_digest` is `unimplemented!` **on purpose**: the
  canonical serialization is proof obligation P5, an audited deliverable,
  not a serde default.
- Record equality goes through `RecordCore` only.
- No trust, signing, or storage-backend code here — those are the atom
  layer and the castore respectively.
