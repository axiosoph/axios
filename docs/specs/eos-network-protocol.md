# SPEC: Eos Network Protocol

<!--
  SPEC documents are normative specification artifacts produced by the /spec workflow.
  They declare behavioral contracts that constrain implementation — what MUST be true,
  what MUST NEVER be true, and what transitions are permitted.

  The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD",
  "SHOULD NOT", "RECOMMENDED", "NOT RECOMMENDED", "MAY", and "OPTIONAL" in this
  document are to be interpreted as described in BCP 14 (RFC 2119, RFC 8174) when,
  and only when, they appear in all capitals, as shown here.

  See: workflows/spec.md for the full protocol specification.
  See: docs/models/publishing-stack-layers.md for the algebraic domain model.
-->

## Domain

**Problem Domain:** The Eos Network Protocol defines the communication contracts, wire APIs, binary cache distribution, and trust verification mechanisms between Ion frontends, Eos schedulers, worker nodes, and remote binary substituters (artifact caches). 

Because Eos operates in a decentralized and potentially untrusted environment, it must treat the network as a trust boundary. Worker nodes cannot blindly execute build commands, nor can client machines blindly import built binaries from caches. This specification establishes the cryptographic guarantees that ensure absolute reproducibility and validation of build origins without relying on central Certificate Authorities.

**Model Reference:**
- [ion-eos-contract.md](ion-eos-contract.md) — Handoff boundaries and capability advertisement
- [atom-transactions.md](atom-transactions.md) — Cryptographic claims and verify operations

**Criticality Tier:** Medium — correctness preserves the security boundary of the publishing stack, protecting hosts from executing unverified binaries.

---

## Constraints

### Type Declarations

We define the following type signatures to represent network communication and cryptographic state:

```
TYPE NodeId = PrincipalRoot                             -- Cryptographic sovereign identity (Cyphr PR)
TYPE SessionToken = Signature                           -- Signed payload verifying node authentication
TYPE PlanSignature = Signature                          -- Signature over tuple (EnginePlanHash, OutputDigest)
TYPE ExpectedOutput = (StorePath, Blake3Digest)         -- Expected store path and its content hash

TYPE SubstitutionRequest = {
    plan_hash: Blake3Digest,
    expected_outputs: Vec<StorePath>
}

TYPE SubstitutionResponse = {
    outputs: Vec<(StorePath, Blake3Digest)>,
    signatures: Set<(NodeId, PlanSignature)>
}

TYPE HandshakePayload = {
    node_id: NodeId,
    timestamp: UnixTime,
    supported_backends: Set<String>,
    supported_plugins: Set<String>
}
```

---

### Invariants

**[eos-network-sovereign-auth]**: All API endpoints and inter-node wire connections MUST require authentication using sovereign identities defined at Layer 1 (Cyphr Principal Roots and signed challenge-response payloads). Eos MUST NOT trust connections authenticated solely by traditional web-PKI TLS certificates.
`VERIFIED: unverified`

**[eos-trustless-substitution]**: When fetching a pre-built output from a remote substituter (binary cache) at a given `StorePath`, Eos MUST verify that the content digest of the fetched artifact matches the expected hash computed from the verified `EnginePlan`. Eos MUST NOT accept substituted outputs that fail this check.
`VERIFIED: unverified`

**[eos-origin-attestation]**: A build output written to a shared binary cache MUST be accompanied by an origin attestation: a signature from the worker node (`NodeId`) that executed the build, signing the tuple `(EnginePlanHash, OutputDigest)`.
`VERIFIED: unverified`

**[eos-protocol-capability-matching]**: Eos nodes MUST negotiate and verify compatibility of their supported backends and capabilities (e.g. `nix`, `guix`) during connection handshake. If a mismatch is detected, the connection MUST be closed.
`VERIFIED: unverified`

**[eos-signature-freshness]**: Handshake signatures and API calls MUST carry a timestamp that is checked for freshness against the receiving node's system clock. Payload signatures older than a pre-defined window (e.g. 5 minutes) MUST be rejected to prevent replay attacks.
`VERIFIED: unverified`

---

### Transitions

**[negotiate-session]**: Establish an authenticated connection between peers.
- **PRE**: A client or peer initiates a handshake, presenting its `HandshakePayload` and a valid signature.
- **POST**: The payload is validated for capability matching and timestamp freshness. If valid, a secure session is opened and a `SessionToken` is established. Otherwise, the handshake is aborted and connection closed.
`VERIFIED: unverified`

**[request-substitute]**: Query remote caches for pre-built outputs.
- **PRE**: An Eos coordinator has a plan in the `NeedsBuild` state.
- **POST**: Sends a `SubstitutionRequest` to configured remote caches. If a valid `SubstitutionResponse` is returned containing verified output hashes, Eos bypasses the build.
`VERIFIED: unverified`

---

### Forbidden States

**[no-unattested-substitution]**: Eos MUST NOT accept binary caches or substituters that serve pre-built binaries without valid origin attestations matching a trusted worker whitelist, if strict policy is enabled.
`VERIFIED: unverified`

**[no-unencrypted-secrets]**: Worker nodes MUST NOT transmit private keys or plaintext credentials over the network during build execution.
`VERIFIED: unverified`

**[no-unauthorized-handshake]**: A node MUST NOT transition to an authenticated session state if the handshake signature does not match its declared `NodeId` (Principal Root).
`VERIFIED: unverified`

---

### Behavioral Properties

**[eventual-cache-consistency]**: If a build output is successfully pushed to a remote binary cache, subsequent queries for that output's hash MUST return the artifact within a bounded propagation delay.
- **Type**: Liveness
`VERIFIED: unverified`

**[reproducible-build-consensus]**: For high-security environments, Eos MAY schedule the same `EnginePlan` on $N$ independent, distrusted worker nodes and verify that the resulting output digests are identical (majority consensus) before committing the output.
- **Type**: Safety
`VERIFIED: unverified`

---

## Verification

| Constraint | Method | Result | Detail |
| :--------- | :----- | :----- | :----- |
| `eos-network-sovereign-auth` | Unit tests | UNVERIFIED | Challenge-response verification tests |
| `eos-trustless-substitution` | Integration test | UNVERIFIED | Inject corrupted binary into cache simulation |
| `eos-origin-attestation` | Signature check | UNVERIFIED | Verify worker signature validation logic |
| `eos-protocol-capability-matching` | Handshake test | UNVERIFIED | Handshake capability mismatch test |
| `eos-signature-freshness` | Replay test | UNVERIFIED | Replay expired payload verification test |
| `negotiate-session` | Unit test | UNVERIFIED | Verify handshake transitions |
| `request-substitute` | Unit test | UNVERIFIED | Verify cache query transitions |
| `no-unattested-substitution` | Policy audit | UNVERIFIED | Verify whitelist rejection policy |
| `no-unencrypted-secrets` | Code audit | UNVERIFIED | Scan codebase for secret leaks in logs/payloads |
| `no-unauthorized-handshake` | Signature check | UNVERIFIED | Signature mismatch rejection test |
| `eventual-cache-consistency` | Integration test | UNVERIFIED | Cache propagation delay measurement |
| `reproducible-build-consensus` | Consensus test | UNVERIFIED | Consensus mismatch injection simulation |

---

## Implications

1. **Sovereign Cryptography Integration**:
   API endpoint security relies completely on Cyphr/Coz cryptography. Eos nodes must implement ed25519 signature checks on all incoming messages, aligning with the cryptography libraries verified in L1.

2. **Decentralized Binary Cache Networks**:
   Since caches are content-addressed and verified via plan-to-output mapping, binary cache distribution can be entirely peer-to-peer (P2P). Worker nodes can act as substituters for one another without central registration.

3. **Reproductibility Audits**:
   By recording origin attestations, Eos creates a cryptographic audit trail. If a malicious binary is ever detected (e.g. by rebuilding the plan locally and finding a hash mismatch), the signature identifies the malicious worker node (`NodeId`), allowing immediate eviction from the trust group.
