# Specification Audit Report

- **Quadrant:** Explanation
- **Audience:** Axios core architects and engineers

## Executive Summary

The Axios specification suite contains three structural conflicts between architectural layers, three unaddressed edge cases in protocol flows, and three opportunities for algebraic optimization. Resolving these issues secures the trust boundaries between the atom, eos, and ion workspaces.

## Structural Conflicts between Layers

### 1. Lock File Schema Version Discrepancy

Two specifications at Layer 3 (ion) contradict each other regarding the lock file schema version. `lock-file-schema.md` defines the schema version as `0`. Conversely, `ion-resolution.md` defines the schema version as `1`. This inconsistency breaks parser validation.

### 2. Dependency Dispatch Ownership

A boundary conflict exists between Layer 2 (eos) and Layer 3 (ion) regarding dependency type tag interpretation. `lock-file-schema.md` states that Eos uses the `type` field in the lock file to dispatch backend selection. However, `layer-boundaries.md` and `ion-eos-contract.md` dictate that Eos never processes lock files directly. Instead, the `ion-eos` bridge crate translates lock entries into structured `eos-core` types before RPC submission. The `ion-eos` bridge crate must dispatch execution via structured variants in `eos-core`, not via string parsing of lock file tags in Eos.

### 3. Cryptographic Verification Boundaries

A logical contradiction exists regarding which layer verifies cryptographic transactions. `ion-eos-contract.md` asserts that Eos must trust atoms resolved from its `AtomSource` and must not perform redundant verification. In contrast, `eos-build-engine.md` dictates that Eos must fetch atom snapshots and verify that the owner validly signed and authorized the publish transaction according to the claim chain. This violation leaks Layer 1 protocol details into the Layer 2 engine.

## Unaddressed Edge Cases and Missing Constraints

### 1. Peer-Assisted Resolution Deadlocks

The peer-assisted resolution sequence blocks the client thread while Eos queries remote mirrors. If a remote mirror hangs, the resolver blocks indefinitely. The specifications lack constraints defining timeouts for remote mirror queries and session liveness checks for peer-assisted transfers.

### 2. Fork Collisions and Claim Chain Divergence

The sourcing pipeline rejects resolutions where two mirrors serve the same version of an atom with identical snapshot digests but different claim digests (`czd`). If the claims belong to distinct owners, the resolver halts. The specification lacks a trust policy constraint allowing users to whitelist or select preferred owners to resolve fork collisions.

### 3. Signature Freshness in Web of Trust

`eos-network-protocol.md` enforces a five-minute freshness window on all signatures to prevent replay attacks. This constraint incorrectly applies to `OriginAttestation` records. Attestations for static outputs must remain valid indefinitely to ensure long-term reproducibility, even if the builder's clock has drifted.

## Algebraic Formulations of Protocol Constraints

### 1. Web of Trust Policies as a Lattice

The Web of Trust policy space forms a bounded join-semilattice $(P, \sqsubseteq, \sqcup, \bot)$.

- $P$ denotes the set of trust policy predicates $p: \text{Set(Attestation)} \to \mathbb{B}$.
- The partial order $p_1 \sqsubseteq p_2$ defines strictness: $p_2(A) \implies p_1(A)$.
- The bottom element $\bot$ represents Trust-On-First-Use, which accepts any valid signature.
- Stricter policies compose via the join operator $\sqcup$:
  $$(p_1 \sqcup p_2)(A) \equiv p_1(A) \land p_2(A)$$
  This algebraic representation simplifies policy composition in the daemon configuration.

### 2. Ingestion as Monoidal Accumulation

The `AtomStore` state space forms a bounded semilattice $(S, \cup, \emptyset)$.

- Ingestion $\cup: S \times S \to S$ satisfies associativity, commutativity, and idempotence.
- The resolution function $\text{resolve}_s: \text{AtomId} \to \text{Option(Entry)}$ exhibits monotonicity:
  $$s_1 \sqsubseteq s_2 \implies \forall i \in \text{AtomId}, \ \text{resolve}_{s_1}(i) \le \text{resolve}_{s_2}(i)$$
  This formulation guarantees that concurrent ingestion remains safe and order-independent.

### 3. Version Resolution Satisfiability

Version resolution operates as constraint satisfaction over a totally ordered set of versions $(V, \le)$.

- A version requirement $R$ defines a subset $R \subseteq V$.
- For a dependency graph $G = (D, E)$ where each edge $(u, v) \in E$ carries requirement $R_{uv}$, a valid resolution assigns a version mapping $f: D \to V$ satisfying:
  $$\forall (u, v) \in E, \ f(v) \in R_{uv}$$
  This algebraic structure clarifies the completeness and determinism bounds of the SAT resolver.
