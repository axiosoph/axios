+++
title = "Supply chain security"
description = "How Atom's Surety of Source model replaces registry trust with cryptographic proofs of repository origin"
quadrant = "Explanation"
audience = "Security engineers, package manager designers, and developers evaluating Atom's trust guarantees"
+++

Every `npm install`, `cargo build`, or `pip install` delegates trust to a centralized registry. The registry decides which bytes you get. If it's wrong, you're compromised. This is the fundamental weakness of the current package supply chain.

## The centralized registry problem

In a traditional ecosystem, a package's integrity depends on the registry (`npmjs.com`, `crates.io`, etc.). The registry manages credentials, access tokens, and namespace ownership.

Two things make this fragile:

1. **Credential-based security** — Trust is tied to account authentication. Compromise a maintainer's credentials or CI tokens and you can publish anything. The [Red Hat `@redhat-cloud-services` npm compromise (RHSB-2026-006)](https://access.redhat.com/security/vulnerabilities/RHSB-2026-006) showed this clearly: attackers hijacked a developer's GitHub account, injected the "Miasma" credential-harvesting malware into ~32 packages across 96 versions, and the packages carried authentic provenance signatures because they went through legitimate OIDC publishing workflows.
2. **Opaque tarballs** — Registries distribute pre-packaged source files or built artifacts. There is no cryptographic proof that the tarball on the registry matches any specific commit in the developer's repository. The downstream user cannot verify lineage; they just trust the registry.

## Surety of source

Atom inverts this. Instead of trusting credentials on a central server, Atom binds published packages directly to their origin repository's history through cryptographic proofs.

Under this model, mirrors, registries, and stores are just transport. Authenticity is verified locally by the consumer:

$$\text{Genesis Commit} \to \text{Claim Transaction} \to \text{Publish Transaction} \to \text{Content Snapshot}$$

The chain has three links:

1. **Anchor** — Package identity is bound to the repository's genesis commit hash. You can't fake this without creating a different repository entirely.
2. **Claim** — The owner publishes a signed `claim` transaction containing their public key, the anchor, and the package label. This establishes ownership via Trust-On-First-Use (TOFU).
3. **Publish** — Each version release is signed in a `publish` transaction that cryptographically binds to:
   - The authorizing `claim` digest.
   - The exact source commit (`src`).
   - The relative path of the package in the repository.
   - The content-addressed hash of the deterministic snapshot (`dig`).

## Local verification and DAG validation

Verification happens in two phases. The first (8 steps) runs locally with zero network access and is mandatory. The second (4 steps) optionally checks content provenance by fetching minimal source metadata.

Both phases validate the Git DAG using a temporal ancestry check:

$$\text{genesis} \to \text{claim.src} \to \text{publish.src}$$

The client confirms that the genesis commit is an ancestor of the claim's source commit, which is an ancestor of the publish's source commit. This temporal floor prevents backdating: even if an attacker steals a publisher's key, they cannot publish a version and pretend it predates the compromise.

Because the content snapshot is deterministic, the client can also download the source tree at `publish.src`, navigate to `path`, regenerate the snapshot, and confirm the hash matches `dig`.

If the signatures hold, the DAG ordering is valid, and the content hash matches, the atom is verified. The consumer knows the code came from the legitimate repository owner without trusting any registry, mirror, or network.
