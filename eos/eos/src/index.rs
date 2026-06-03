//! In-memory AtomIndex implementation.

use std::collections::HashMap;
use std::sync::Mutex;

use atom_id::AtomId;
use eos_core::index::{AtomIndex, AtomMeta, AtomQuery};

/// A basic, in-memory implementation of [`AtomIndex`].
#[derive(Debug, Default)]
pub struct RequestIndex {
    atoms: Mutex<HashMap<AtomId, AtomMeta>>,
}

impl RequestIndex {
    /// Creates a new `RequestIndex`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            atoms: Mutex::new(HashMap::new()),
        }
    }
}

impl AtomIndex for RequestIndex {
    type Error = std::convert::Infallible;

    async fn resolve(&self, id: &AtomId) -> Result<Option<AtomMeta>, Self::Error> {
        let guard = self.atoms.lock().expect("mutex poisoned");
        Ok(guard.get(id).cloned())
    }

    async fn contains(&self, id: &AtomId) -> Result<bool, Self::Error> {
        let guard = self.atoms.lock().expect("mutex poisoned");
        Ok(guard.contains_key(id))
    }

    async fn search(&self, query: &AtomQuery) -> Result<Vec<AtomMeta>, Self::Error> {
        let guard = self.atoms.lock().expect("mutex poisoned");
        let mut results = Vec::new();

        for atom in guard.values() {
            // Match label pattern (case-insensitive substring)
            if !atom
                .label
                .to_lowercase()
                .contains(&query.label_pattern.to_lowercase())
            {
                continue;
            }

            // Check set filter if provided
            if let Some(ref set_filter) = query.set_filter
                && !atom.sets.contains(set_filter)
            {
                continue;
            }

            results.push(atom.clone());
            if results.len() >= query.limit as usize {
                break;
            }
        }

        Ok(results)
    }

    async fn ingest(&self, meta: AtomMeta) -> Result<(), Self::Error> {
        let mut guard = self.atoms.lock().expect("mutex poisoned");
        guard.insert(meta.id.clone(), meta);
        Ok(())
    }
}
