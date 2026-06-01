//! The runtime engine.
//!
//! Evaluation, building, cache management, and scheduling.
//! Connects [`BuildEngine`] with [`ArtifactStore`] to turn locked
//! dependencies into build outputs.
pub mod fetch;
pub mod lock;
pub mod orchestrator;
