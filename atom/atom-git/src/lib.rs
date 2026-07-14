//! Git backend for the Atom protocol.
//!
//! Implements [`AtomRegistry`] and [`AtomStore`] using git object storage.

pub mod charter_store;
pub mod error;
pub mod gix_util;
pub mod registry;
pub mod source;
pub mod store;

pub use error::GitError;
pub use registry::GitRegistry;
pub use source::{GitEntry, GitSource};
pub use store::GitStore;
