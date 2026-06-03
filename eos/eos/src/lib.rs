//! The runtime engine.
//!
//! Evaluation, building, cache management, and scheduling.
//! Connects [`BuildEngine`] with [`ArtifactStore`] to turn locked
//! dependencies into build outputs.
pub mod bridge;
pub mod fetch;
pub mod index;
pub mod orchestrator;
