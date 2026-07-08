+++
title = "Supply chain security"
description = "How Atom's Surety of Source model replaces registry trust with cryptographic proofs of repository origin"
quadrant = "Explanation"
tags = ["general"]
audience = "Security engineers, package manager designers, and developers evaluating Atom's trust guarantees"
+++

Every `npm install`, `cargo build`, or `pip install` delegates trust to a centralized registry. The registry decides which bytes you get. If it's wrong, you're compromised. This is the fundamental weakness of the current package supply chain.

## The centralized registry problem

In a traditional ecosystem, a package's integrity depends on the registry (`npmjs.com`, `crates.io`, etc.). The registry manages credentials, access tokens, and namespace ownership.

Two things make this fragile:

1. **Credential-based security** — Trust is tied to account authentication. Compromise a maintainer's credentials or CI tokens and you can publish anything. The [Red Hat `@redhat-cloud-services` npm compromise (RHSB-2026-006)](https://access.redhat.com/security/vulnerabilities/RHSB-2026-006) showed this clearly: attackers hijacked a developer's GitHub account, injected the "Miasma" credential-harvesting malware into ~32 packages across 96 versions, and the packages carried authentic provenance signatures because they went through legitimate OIDC publishing workflows.
2. **Opaque tarballs** — Registries distribute pre-packaged source files or built artifacts. There is no cryptographic proof that the tarball on the registry matches any specific commit in the developer's repository. The downstream user cannot verify lineage; they just trust the registry.

## Surety of source

Atom inverts this. Instead of trusting credentials on a central server, Atom binds published packages directly to a signed declaration of ownership over their origin repository, and then to that repository's history, through cryptographic proofs.

Under this model, mirrors, registries, and stores are just transport. Authenticity is verified locally by the consumer:

$$\text{Charter Transaction} \to \text{Claim Transaction} \to \text{Publish Transaction} \to \text{Content Snapshot}$$

The chain has three links:

1. **Charter** — Before anyone can claim a package, the repository's owner signs a **founding charter**: a transaction that says, in effect, "this repository publishes packages, starting here, under this key." Package identity is bound to the cryptographic digest of that signed transaction, not to the repository's raw genesis commit hash. This is a deliberate change from an earlier design: a commit hash is unowned data — anyone holding a copy of the repository can compute it, so anchoring identity there meant the _first_ claim to show up won the name, with nothing stopping a second party from doing the same over a copy of the same history. A charter is instead a specific, signed act by whoever is establishing the package, so the chain roots in something owned rather than something merely observed. The genesis commit isn't discarded — the charter's own history pointer transitively pins it — it just stops being what identity is anchored to. Because the anchor is now a digest of a signed transaction rather than a git-specific hash, the same scheme works no matter what version-control system holds the source.

   Rooting identity in a signed charter also settles what happens when the same source history is published by more than one party — a fork. Two charters signed over identical history are, by construction, two distinct packages: each fork mints its own charter, so there's no shared identity to contend over. And ownership can change hands without the package's identity changing: a _successor_ charter, signed by the outgoing owner and chained back to the founding charter, records a key rotation or a transfer to a new owner. A transfer to a new owner also carries the incoming owner's own signature, proving they agreed to take it on — an owner can't be handed a package they never asked for. The anchor never moves; only who controls it does. Because a charter marks a specific point in the repository's history, everything before that point remains visible in the repository, but it is not part of the package's owned history until someone charters or claims it after the fact.

2. **Claim** — The owner publishes a signed `claim` transaction, authorized by the charter's owner, containing their public key, a reference back to the charter, and the package label. Trust-on-first-use now happens once, at the charter — a claim itself is owner-authorized, not a race to be first.
3. **Publish** — Each version release is signed in a `publish` transaction that cryptographically binds to:
   - The authorizing `claim` digest.
   - The exact source commit (`src`).
   - The relative path of the package in the repository.
   - The content-addressed hash of the deterministic snapshot (`dig`).

## Local verification and DAG validation

Verification happens in two phases. The first (13 steps, covering the charter, claim, and publish signatures and their chain of authorization) runs locally with zero network access and is mandatory. The second (5 steps) optionally checks content provenance by fetching minimal source metadata.

Both phases validate the Git DAG using a temporal ancestry check:

$$\text{charter.src} \to \text{claim.src} \to \text{publish.src}$$

The client confirms that the charter's source revision is an ancestor of the claim's source commit, which is an ancestor of the publish's source commit. This temporal floor prevents backdating: even if an attacker steals a publisher's key, they cannot publish a version and pretend it predates the compromise.

Because the content snapshot is deterministic, the client can also download the source tree at `publish.src`, navigate to `path`, regenerate the snapshot, and confirm the hash matches `dig`.

If the signatures hold, the DAG ordering is valid, and the content hash matches, the atom is verified. The consumer knows the code came from the legitimate repository owner without trusting any registry, mirror, or network.
