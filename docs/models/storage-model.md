# MODEL: Storage — the First Primitive

_2026-07-07. Status: v0.1 — authored to complete the substrate
trichotomy (storage, composition, execution; see the
[Composition Model](composition-model.md) §0). The substrate's founding
analysis named content-addressed storage as its first primitive and
treated it as solved by the snix castore; that judgment stands. This
model exists because the other two primitives' laws quantify over
storage's — digest identity underpins every equality claim in the
[Composition Model](composition-model.md) and the
[Execution Model](execution-model.md) — so the axioms must be stated,
even though the implementation is outsourced._

---

## 1. The one-sentence model

> **Storage is a content-addressed map from digests to immutable values,
> `store : Bytes → Digest` and `load : Digest ⇀ Bytes`, whose single law —
> identity is digest — is what every other component of the substrate
> borrows when it claims two things are "the same."**

## 2. Values and axioms

- **Blob** `b` — bytes. Identity `H(b)`.
- **Tree** `t` — a Merkle tree of named blobs/trees (castore-shaped).
  Identity: root digest over the **canonical serialization**, in which
  entries are name-sorted.

Axioms (assumed, not proven — they are the trust base):

```
A1 (retrieval)      load(store(b)) = b
A2 (identity)       dig₁ = dig₂  ⟺  content₁ = content₂
                    — up to collision resistance of H; every identity
                    claim in the substrate is conditional on A2
A3 (canonicity)     one content, one serialization: distinct trees never
                    share a digest, and a tree's name-sorted entry order
                    is part of its identity (the composition denotation's
                    enumeration-order requirement, Composition Model §2,
                    is A3 surfaced at the view layer)
```

## 3. The substitution principle

Because verification is re-hashing, **the transport is untrusted by
construction**: a blob fetched from a mirror, a CDN, a peer, or a hostile
network either matches its digest or is rejected. Nothing about the
channel enters the trust argument. This single principle is what makes
the substrate's distribution story free — substitution of remote content
for local content is sound with zero protocol trust — and it is why
installation ([Composition Model](composition-model.md) §8) needs no
trial machinery: fetch-by-digest has no identity to corrupt.

## 4. Persistence, GC, rollback

Liveness is **reachability over digests** from a set of retained roots
(composition roots, promoted intent, retained records). GC drops
unreachable blobs; rollback is retaining an old root. Neither operation
has any semantics beyond graph reachability — transactional update
(Composition Model §8) is a root swap precisely because storage never
mutates.

## 5. What storage deliberately does not own

- **Atoms** — this model covers the **artifact** store: the outputs
  executions produce and compositions arrange. Atoms (source intent)
  have their own storage primitive — the git object store per the
  [git storage format](../specs/git-storage-format.md) — with its own
  identity discipline (coz CZDs, the signing layer's identifiers). The
  two stores share the content-addressing idea, not a namespace, an
  implementation, or an identifier type. They do meet at one seam: build
  inputs enter a view only through the artifact store, so an atom's
  **source** is ingested here at request formation — and that crossing
  re-derives the source's identity under this store's hash. Since git
  still defaults to SHA-1 (not collision-resistant) while the artifact
  store is blake3, recording the ingested source's artifact digest in
  atom metadata is a cheap integrity upgrade — an implementation
  opportunity at the seam, noted for framing, not a model obligation.
- **Naming** — mapping human names/versions to digests is the atom
  protocol and ion's resolution, layered above.
- **Trust** — signatures are themselves stored content; whose signature
  makes content _usable_ is the trust layer
  ([Execution Model](execution-model.md) §3.4).
- **Query** — the fact-set (Composition Model §6) is an _index over_
  stored records, owned at the composition/execution seam, not a storage
  concern.

## 6. Implementation and obligation

The implementation is the snix castore (blake3-keyed; ADR-0005). The one
proof obligation contributed to the substrate queue:

- **P10 — canonical-serialization injectivity.** The concrete tree
  serialization is canonical (A3): serialization is a bijection between
  tree values and their byte forms, entry order is enforced sorted, and
  no two distinct values share a preimage. An audit obligation with a
  checkable inventory, like P5 — not mathematical news, but every
  digest-equality argument in the sibling models silently rests on it.
