//! Atom content bridge trait.
//!
//! Defines [`AtomContentBridge`], the interface for transferring atom content
//! from an atom source backend into the build engine's content-addressed store.
//!
//! This trait is an L2 (eos) concern — it sits at the boundary between the atom
//! protocol's content representation and the engine's store format. Different
//! atom backends (git, future Cyphr) require different bridge implementations.
//!
//! The trait is deliberately NOT part of [`AtomSource`](atom_core::AtomSource).
//! `AtomSource` is a *forgetful functor* — it observes identity and metadata.
//! Content transfer is a *transport concern*, orthogonal to observation.
//! See the formal model §2.6 for the categorical justification.

use atom_id::AtomId;
use trait_variant::make;

use crate::digest::Digest;
use crate::eval::ResolvedInput;

/// Bridge between atom content storage and the build engine's artifact store.
///
/// Implementations transfer atom source trees from an [`AtomContent`] representation
/// into the engine's content-addressed store (e.g., snix castore) where the evaluator
/// and builder can access them.
///
/// # Construction
///
/// Bridges are constructed at the *wiring site* (e.g., the scheduler) where
/// the concrete atom backend type is known. This avoids `as_any()` downcasts
/// inside the build pipeline — the bridge carries the backend knowledge that
/// the orchestrator needs without leaking it through trait interfaces.
///
/// # Lifecycle
///
/// A bridge instance is typically scoped to a single build job. The scheduler
/// constructs it alongside the [`AtomSource`](atom_core::AtomSource) and passes
/// both to the orchestrator.
#[make(Send)]
pub trait AtomContentBridge: Send + Sync {
    /// The digest algorithm used by this bridge's target store.
    type Digest: Digest;

    /// Backend-specific error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Ingest an atom's content tree into the engine's store.
    ///
    /// Transfers the content identified by `dig` (the backend-specific content
    /// snapshot digest, e.g., a git tree OID) into the content-addressed store,
    /// registers a store path, and returns the resolved input metadata.
    ///
    /// # Arguments
    ///
    /// * `id` — the atom's identity (used for provenance, not content lookup)
    /// * `label` — human-readable name, used as the store path name component
    /// * `dig` — backend-specific content snapshot digest (e.g., 20-byte SHA-1 git OID). The bridge
    ///   interprets these bytes according to its backend.
    ///
    /// # Errors
    ///
    /// Returns an error if the content cannot be located in the backend,
    /// the blob upload fails, or the store path cannot be registered.
    async fn ingest_atom(
        &self,
        id: &AtomId,
        label: &str,
        dig: &[u8],
    ) -> Result<ResolvedInput<Self::Digest>, Self::Error>;
}
